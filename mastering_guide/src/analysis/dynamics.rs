/// Length of the macrodynamics short-term LUFS ring. Sized for 60 s of
/// history at the 100 ms publish cadence in `analysis::AnalysisState`.
const MACRO_HISTORY_LEN: usize = 600;

/// PSR (Peak to Short-term loudness Ratio) tracker plus a short-term LUFS
/// ring buffer used to derive macrodynamics (verse-vs-chorus contrast).
pub struct DynamicsAnalyzer {
    psr_history: Vec<f32>,
    history_head: usize,
    pub psr_min: f32,

    /// Ring buffer of recent short-term LUFS readings (slot count = entries
    /// over ~60 s at 100 ms publish cadence). Macrodynamics range is the
    /// p95-p5 spread over this buffer.
    macro_history: Vec<f32>,
    macro_head: usize,
    /// Pre-allocated scratch used to sort the macro history without
    /// allocating on the audio thread.
    macro_scratch: Vec<f32>,
    /// p95-p5 of the short-term LUFS history, in LU. NaN until we have
    /// enough samples.
    pub macrodynamics_lu: f32,
}

impl DynamicsAnalyzer {
    pub fn new() -> Self {
        // 10 seconds of 3-second short-term updates = ~3 history slots needed,
        // but we keep 20 for robustness.
        Self {
            psr_history: vec![f32::INFINITY; 20],
            history_head: 0,
            psr_min: f32::INFINITY,
            macro_history: vec![f32::NAN; MACRO_HISTORY_LEN],
            macro_head: 0,
            macro_scratch: Vec::with_capacity(MACRO_HISTORY_LEN),
            macrodynamics_lu: f32::NAN,
        }
    }

    pub fn reset(&mut self) {
        self.psr_history.fill(f32::INFINITY);
        self.history_head = 0;
        self.psr_min = f32::INFINITY;
        self.macro_history.fill(f32::NAN);
        self.macro_head = 0;
        self.macrodynamics_lu = f32::NAN;
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

        if lufs_short_term.is_finite() && lufs_short_term > -70.0 {
            self.macro_history[self.macro_head] = lufs_short_term;
            self.macro_head = (self.macro_head + 1) % self.macro_history.len();
            self.recompute_macrodynamics();
        }
    }

    /// Sort a copy of finite history values and read p95 - p5. The scratch
    /// vector is pre-allocated to MACRO_HISTORY_LEN, so this never grows.
    fn recompute_macrodynamics(&mut self) {
        self.macro_scratch.clear();
        for &v in &self.macro_history {
            if v.is_finite() {
                self.macro_scratch.push(v);
            }
        }
        // Need a meaningful population before reporting a range. ~10 s.
        if self.macro_scratch.len() < 100 {
            self.macrodynamics_lu = f32::NAN;
            return;
        }
        self.macro_scratch
            .sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = self.macro_scratch.len();
        let p5_idx = (n * 5) / 100;
        let p95_idx = ((n * 95) / 100).min(n - 1);
        self.macrodynamics_lu = self.macro_scratch[p95_idx] - self.macro_scratch[p5_idx];
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
