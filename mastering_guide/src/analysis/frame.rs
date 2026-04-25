use atomic_refcell::AtomicRefCell;
use std::sync::Arc;

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

/// Shared between audio thread (writer) and GUI thread (reader).
/// AtomicRefCell gives lock-free non-blocking access on both sides:
/// try_borrow_mut() on the audio thread and try_borrow() on the GUI thread
/// both return immediately if contended, so neither thread ever waits.
#[derive(Clone)]
pub struct FrameReader(pub Arc<AtomicRefCell<TrackFrame>>);

pub struct FrameWriter(pub Arc<AtomicRefCell<TrackFrame>>);

pub fn frame_channel() -> (FrameWriter, FrameReader) {
    let shared = Arc::new(AtomicRefCell::new(TrackFrame::default()));
    (FrameWriter(shared.clone()), FrameReader(shared))
}

impl FrameWriter {
    pub fn update(&self, frame: TrackFrame) {
        if let Ok(mut guard) = self.0.try_borrow_mut() {
            *guard = frame;
        }
    }
}

impl FrameReader {
    pub fn read(&self) -> TrackFrame {
        self.0.try_borrow().map(|g| g.clone()).unwrap_or_default()
    }
}
