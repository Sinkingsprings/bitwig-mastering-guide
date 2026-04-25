/// PSR (Peak to Short-term loudness Ratio) tracker.
/// Tracks the minimum PSR seen over a rolling window — the minimum is the
/// most useful value for detecting squeeze on the loudest sections.
pub struct DynamicsAnalyzer {
    psr_history: Vec<f32>,
    history_head: usize,
    pub psr_min: f32,
}

impl DynamicsAnalyzer {
    pub fn new() -> Self {
        // 10 seconds of 3-second short-term updates = ~3 history slots needed,
        // but we keep 20 for robustness.
        Self {
            psr_history: vec![f32::INFINITY; 20],
            history_head: 0,
            psr_min: f32::INFINITY,
        }
    }

    pub fn reset(&mut self) {
        self.psr_history.fill(f32::INFINITY);
        self.history_head = 0;
        self.psr_min = f32::INFINITY;
    }

    /// Called whenever we have a fresh true peak and short-term LUFS reading.
    pub fn update(&mut self, true_peak_dbtp: f32, lufs_short_term: f32) {
        if lufs_short_term.is_finite() && true_peak_dbtp.is_finite() {
            let psr = true_peak_dbtp - lufs_short_term;
            self.psr_history[self.history_head] = psr;
            self.history_head = (self.history_head + 1) % self.psr_history.len();
            self.psr_min = self
                .psr_history
                .iter()
                .cloned()
                .filter(|v| v.is_finite())
                .fold(f32::INFINITY, f32::min);
        }
    }
}

/// Compute PLR (Peak to Loudness Ratio) from stored values.
pub fn compute_plr(true_peak_dbtp: f32, lufs_integrated: f32) -> f32 {
    if true_peak_dbtp.is_finite() && lufs_integrated.is_finite() {
        true_peak_dbtp - lufs_integrated
    } else {
        f32::INFINITY
    }
}
