/// True peak estimator using 4x linear interpolation (fast approximation).
/// A full polyphase FIR oversampler would be more accurate; this gives a
/// conservative estimate sufficient for flagging ceiling violations.
pub struct PeakAnalyzer {
    sample_peak: f32,
    true_peak: f32,
    /// Previous samples per channel for inter-sample interpolation
    prev: [f32; 2],
    /// Running DC offset accumulator
    dc_sum: f64,
    dc_count: u64,
    /// Short RMS window (300ms worth of samples)
    rms_sum: f64,
    rms_count: u64,
    rms_window_samples: usize,
}

impl PeakAnalyzer {
    pub fn new() -> Self {
        Self {
            sample_peak: f32::NEG_INFINITY,
            true_peak: f32::NEG_INFINITY,
            prev: [0.0; 2],
            dc_sum: 0.0,
            dc_count: 0,
            rms_sum: 0.0,
            rms_count: 0,
            rms_window_samples: 13230, // 300ms at 44.1kHz; updated on init
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.rms_window_samples = (sample_rate * 0.3) as usize;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.sample_peak = f32::NEG_INFINITY;
        self.true_peak = f32::NEG_INFINITY;
        self.prev = [0.0; 2];
        self.dc_sum = 0.0;
        self.dc_count = 0;
        self.rms_sum = 0.0;
        self.rms_count = 0;
    }

    /// Process one block. `channels` is a slice of per-channel sample slices.
    pub fn process(&mut self, channels: &[&[f32]]) {
        let num_channels = channels.len().min(2);
        if num_channels == 0 {
            return;
        }
        let num_samples = channels[0].len();

        for i in 0..num_samples {
            let mut sum_sq = 0.0f32;
            for ch in 0..num_channels {
                let s = channels[ch][i];
                let abs_s = s.abs();

                // Sample peak
                if abs_s > self.sample_peak.max(0.0) {
                    self.sample_peak = abs_s;
                }

                // True peak: linear interpolation between prev and current (4 sub-samples)
                let prev = self.prev[ch];
                for k in 1..=4u32 {
                    let t = k as f32 / 4.0;
                    let interp = prev + t * (s - prev);
                    if interp.abs() > self.true_peak.max(0.0) {
                        self.true_peak = interp.abs();
                    }
                }
                self.prev[ch] = s;

                // DC and RMS accumulation
                self.dc_sum += s as f64;
                self.dc_count += 1;
                sum_sq += s * s;
            }
            self.rms_sum += (sum_sq / num_channels as f32) as f64;
            self.rms_count += 1;
            // Slide RMS window
            if self.rms_count as usize > self.rms_window_samples {
                self.rms_sum = 0.0;
                self.rms_count = 0;
            }
        }
    }

    pub fn sample_peak_dbfs(&self) -> f32 {
        if self.sample_peak <= 0.0 {
            return f32::NEG_INFINITY;
        }
        20.0 * self.sample_peak.log10()
    }

    pub fn true_peak_dbtp(&self) -> f32 {
        if self.true_peak <= 0.0 {
            return f32::NEG_INFINITY;
        }
        20.0 * self.true_peak.log10()
    }

    pub fn rms_dbfs(&self) -> f32 {
        if self.rms_count == 0 {
            return f32::NEG_INFINITY;
        }
        let mean_sq = self.rms_sum / self.rms_count as f64;
        if mean_sq <= 0.0 {
            return f32::NEG_INFINITY;
        }
        10.0 * (mean_sq.log10() as f32)
    }

    pub fn dc_offset(&self) -> f32 {
        if self.dc_count == 0 {
            return 0.0;
        }
        (self.dc_sum / self.dc_count as f64) as f32
    }
}
