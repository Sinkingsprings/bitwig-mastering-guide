/// Process-global multi-instance registry.
///
/// All MasteringGuide CLAP instances running in the same host process share
/// a single static slot table. Track instances write their analysis frame
/// each GUI repaint. The master instance reads all track frames on demand.
///
/// This works because CLAP plugins are loaded into the host's process on
/// Linux (Bitwig does not sandbox plugins). No OS-level IPC is required.

use crate::analysis::frame::TrackFrame;
use crate::params::TrackRole;
use std::sync::{Mutex, OnceLock};

const MAX_SLOTS: usize = 32;

fn global() -> &'static Mutex<Vec<Option<TrackSlot>>> {
    static GLOBAL: OnceLock<Mutex<Vec<Option<TrackSlot>>>> = OnceLock::new();
    GLOBAL.get_or_init(|| Mutex::new(vec![None; MAX_SLOTS]))
}

#[derive(Clone)]
struct TrackSlot {
    track_name: String,
    role: TrackRole,
    frame: TrackFrame,
    /// 0 = Track analyzer, 1 = Master bus
    mode: u8,
    heartbeat_ms: u64,
}

/// One row in `read_tracks` output: the track's name, role, and latest frame.
#[derive(Clone)]
pub struct TrackEntry {
    pub name: String,
    pub role: TrackRole,
    pub frame: TrackFrame,
}

/// Handle to one claimed slot in the global registry.
/// Releases the slot automatically on drop.
pub struct Registry {
    slot_idx: usize,
}

impl Registry {
    /// Claim a free slot. Returns `None` if all 32 slots are occupied.
    pub fn new(mode: u8, name: String) -> Option<Self> {
        let mut slots = global().lock().ok()?;
        let idx = slots.iter().position(|s| s.is_none())?;
        slots[idx] = Some(TrackSlot {
            track_name: name,
            role: TrackRole::Auto,
            frame: TrackFrame::default(),
            mode,
            heartbeat_ms: 0,
        });
        Some(Self { slot_idx: idx })
    }

    /// Write the current analysis frame plus role into our slot.
    /// Called from the GUI/main thread ~30× per second.
    pub fn write(&self, name: &str, role: TrackRole, frame: &TrackFrame) {
        if let Ok(mut slots) = global().lock() {
            if let Some(Some(ref mut slot)) = slots.get_mut(self.slot_idx) {
                slot.track_name = name.to_string();
                slot.role = role;
                slot.frame = frame.clone();
                slot.heartbeat_ms = now_ms();
            }
        }
    }

    /// Return all active non-master track entries.
    /// Slots whose heartbeat is older than 3 s are considered stale and skipped.
    pub fn read_tracks(&self) -> Vec<TrackEntry> {
        let now = now_ms();
        let slots = match global().lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        slots
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|s| s.mode == 0 && now.saturating_sub(s.heartbeat_ms) < 3_000)
            .map(|s| TrackEntry {
                name: s.track_name.clone(),
                role: s.role,
                frame: s.frame.clone(),
            })
            .collect()
    }

    pub fn slot_index(&self) -> usize {
        self.slot_idx
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        if let Ok(mut slots) = global().lock() {
            if let Some(slot) = slots.get_mut(self.slot_idx) {
                *slot = None;
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
