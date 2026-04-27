use atomic_refcell::AtomicRefCell;
use std::sync::Arc;

pub const NUM_BANDS: usize = 10;

/// Snapshot of all analysis metrics for one track/bus at a point in time.
/// Written by the audio thread via AnalysisState, read by main thread and IPC.
#[derive(Clone, Debug)]
pub struct TrackFrame {
    pub lufs_momentary: f32,
    pub lufs_short_term: f32,
    pub lufs_integrated: f32,
    /// Integrated LUFS of the mono-summed (L+R)/2 signal. Compared with
    /// `lufs_integrated` to gauge real mono-compatibility loss in LU.
    pub lufs_integrated_mono: f32,
    pub true_peak_dbtp: f32,
    pub sample_peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub plr: f32,
    pub psr_min: f32,
    /// p95-p5 spread of short-term LUFS over the last ~60 s, in LU. NaN
    /// until enough history has accumulated.
    pub macrodynamics_lu: f32,
    pub correlation: f32,
    #[allow(dead_code)]
    pub stereo_width: f32,
    pub bands_dbfs: [f32; NUM_BANDS],
    /// Per-band correlation in [-1, 1] derived from mid/side band power ratios.
    /// 1.0 = mono/in-phase, 0.0 = uncorrelated, <0 = anti-phase.
    pub bands_corr: [f32; NUM_BANDS],
    /// Linear regression slope of `bands_dbfs` over octave index (dB/oct).
    /// Negative = darker at top, positive = brighter at top. Pink-noise = ~-3.
    pub spectral_tilt_db_per_oct: f32,
    pub dc_offset: f32,
    #[allow(dead_code)]
    pub timestamp_ms: u64,
}

impl Default for TrackFrame {
    fn default() -> Self {
        Self {
            lufs_momentary: 0.0,
            lufs_short_term: 0.0,
            lufs_integrated: 0.0,
            lufs_integrated_mono: 0.0,
            true_peak_dbtp: 0.0,
            sample_peak_dbfs: 0.0,
            rms_dbfs: 0.0,
            plr: 0.0,
            psr_min: 0.0,
            macrodynamics_lu: f32::NAN,
            correlation: 1.0,
            stereo_width: 0.0,
            bands_dbfs: [f32::NEG_INFINITY; NUM_BANDS],
            // Default to fully correlated so silence/uninitialised state never
            // fires the "bass too wide" rule on its own.
            bands_corr: [1.0; NUM_BANDS],
            spectral_tilt_db_per_oct: 0.0,
            dc_offset: 0.0,
            timestamp_ms: 0,
        }
    }
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
