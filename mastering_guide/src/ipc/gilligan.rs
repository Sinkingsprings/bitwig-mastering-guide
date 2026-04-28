/// Skipper-side client for the Gilligan IPC bridge.
///
/// Runs a single background thread that connects to Gilligan's Unix domain
/// socket, reads length-prefixed JSON `track_table` messages, and stores the
/// parsed metadata in an `Arc<Mutex<GilliganState>>` shared with the GUI.
///
/// The audio thread never touches this module.

use serde::Deserialize;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WireMsg {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    tracks: Vec<WireTrack>,
}

#[derive(Debug, Deserialize)]
struct WireTrack {
    idx: i32,
    name: String,
    #[serde(rename = "type")]
    track_type: String,
    is_group: bool,
    position: i32,
    color_r: u8,
    color_g: u8,
    color_b: u8,
    vu_l: f32,
    vu_r: f32,
}

// ── Public types ──────────────────────────────────────────────────────────────

/// One row of Gilligan metadata, available to the GUI.
#[derive(Clone, Debug)]
pub struct GilliganTrack {
    /// Bitwig-assigned track index (−1 = master).
    pub idx: i32,
    pub name: String,
    pub track_type: String,
    pub is_group: bool,
    pub position: i32,
    pub color: [u8; 3],
    pub vu_l: f32,
    pub vu_r: f32,
}

/// Shared state written by the IPC thread, read by the GUI thread.
#[derive(Default)]
pub struct GilliganState {
    pub tracks: Vec<GilliganTrack>,
    /// True while the IPC thread is connected to Gilligan.
    pub connected: bool,
}

// ── Client ────────────────────────────────────────────────────────────────────

/// Spawn the background IPC thread.  Returns the shared state handle.
/// Call once from `create_editor()` (GUI thread, before the editor opens).
pub fn spawn(state: Arc<Mutex<GilliganState>>) {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string());
    let path = format!("/tmp/skipper-gilligan-{}.sock", user);

    thread::Builder::new()
        .name("skipper-gilligan-ipc".into())
        .spawn(move || client_loop(&path, state))
        .expect("failed to spawn Gilligan IPC thread");
}

fn client_loop(socket_path: &str, state: Arc<Mutex<GilliganState>>) {
    loop {
        match UnixStream::connect(socket_path) {
            Ok(stream) => {
                {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.connected = true;
                }
                serve_connection(stream, &state);
                {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    s.connected = false;
                    s.tracks.clear();
                }
            }
            Err(_) => {
                // Gilligan not running yet — retry after a short pause.
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn serve_connection(mut stream: UnixStream, state: &Arc<Mutex<GilliganState>>) {
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();

    let mut header = [0u8; 4];
    loop {
        match stream.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock
                || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        }

        let body_len = u32::from_be_bytes(header) as usize;
        if body_len == 0 || body_len > 1 << 20 {
            break; // sanity check
        }

        let mut body = vec![0u8; body_len];
        if stream.read_exact(&mut body).is_err() {
            break;
        }

        if let Ok(msg) = serde_json::from_slice::<WireMsg>(&body) {
            if msg.kind == "track_table" {
                let parsed: Vec<GilliganTrack> = msg
                    .tracks
                    .into_iter()
                    .map(|t| GilliganTrack {
                        idx: t.idx,
                        name: t.name,
                        track_type: t.track_type,
                        is_group: t.is_group,
                        position: t.position,
                        color: [t.color_r, t.color_g, t.color_b],
                        vu_l: t.vu_l,
                        vu_r: t.vu_r,
                    })
                    .collect();
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.tracks = parsed;
            }
        }
    }

    // Send any queued outbound messages (Phase 12 fix actions) — placeholder.
    let _ = stream.flush();
}
