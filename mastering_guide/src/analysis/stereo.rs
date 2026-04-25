/// Stereo correlation and M/S analysis.
pub struct StereoAnalyzer {
    sum_ll: f64,
    sum_rr: f64,
    sum_lr: f64,
    count: u64,
    window_samples: usize,
    pub correlation: f32,
    pub stereo_width: f32,
}

impl StereoAnalyzer {
    pub fn new() -> Self {
        Self {
            sum_ll: 0.0,
            sum_rr: 0.0,
            sum_lr: 0.0,
            count: 0,
            window_samples: 13230,
            correlation: 1.0,
            stereo_width: 0.0,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.window_samples = (sample_rate * 0.3) as usize;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.sum_ll = 0.0;
        self.sum_rr = 0.0;
        self.sum_lr = 0.0;
        self.count = 0;
        self.correlation = 1.0;
        self.stereo_width = 0.0;
    }

    pub fn process(&mut self, left: &[f32], right: &[f32]) {
        let len = left.len().min(right.len());
        for i in 0..len {
            let l = left[i] as f64;
            let r = right[i] as f64;
            self.sum_ll += l * l;
            self.sum_rr += r * r;
            self.sum_lr += l * r;
            self.count += 1;
        }
        // Slide window
        if self.count as usize >= self.window_samples {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let denom = (self.sum_ll * self.sum_rr).sqrt();
        if denom > 1e-10 {
            self.correlation = (self.sum_lr / denom).clamp(-1.0, 1.0) as f32;
        } else {
            self.correlation = 1.0;
        }
        self.stereo_width = (1.0 - self.correlation) / 2.0;
        self.sum_ll = 0.0;
        self.sum_rr = 0.0;
        self.sum_lr = 0.0;
        self.count = 0;
    }
}
