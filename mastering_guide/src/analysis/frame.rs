use std::sync::{Arc, RwLock};

pub const NUM_BANDS: usize = 10;

/// Snapshot of all analysis metrics for one track/bus at a point in time.
/// Written by the audio thread via AnalysisState, read by main thread and IPC.
#[derive(Clone, Debug, Default)]
pub struct TrackFrame {
    pub lufs_momentary: f32,
    pub lufs_short_term: f32,
    pub lufs_integrated: f32,
    pub true_peak_dbtp: f32,
    pub sample_peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub plr: f32,
    pub psr_min: f32,
    pub correlation: f32,
    #[allow(dead_code)]
    pub stereo_width: f32,
    pub bands_dbfs: [f32; NUM_BANDS],
    pub dc_offset: f32,
    #[allow(dead_code)]
    pub timestamp_ms: u64,
}

/// Shared between audio thread (writer) and main thread (reader).
/// Uses RwLock for simplicity at this stage; no allocation on audio thread
/// since we only write a plain Copy-able struct.
#[derive(Clone)]
pub struct FrameReader(pub Arc<RwLock<TrackFrame>>);

pub struct FrameWriter(pub Arc<RwLock<TrackFrame>>);

pub fn frame_channel() -> (FrameWriter, FrameReader) {
    let shared = Arc::new(RwLock::new(TrackFrame::default()));
    (FrameWriter(shared.clone()), FrameReader(shared))
}

impl FrameWriter {
    pub fn update(&self, frame: TrackFrame) {
        if let Ok(mut guard) = self.0.try_write() {
            *guard = frame;
        }
        // If the lock is contended we just skip this update — stale data is fine
    }
}

impl FrameReader {
    pub fn read(&self) -> TrackFrame {
        self.0.read().map(|g| g.clone()).unwrap_or_default()
    }
}
