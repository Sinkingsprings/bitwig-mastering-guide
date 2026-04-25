use ebur128::{EbuR128, Mode};

pub struct LufsAnalyzer {
    inner: Option<EbuR128>,
}

impl LufsAnalyzer {
    pub fn new() -> Self {
        Self { inner: None }
    }

    pub fn initialize(&mut self, sample_rate: f32) -> bool {
        match EbuR128::new(2, sample_rate as u32, Mode::all()) {
            Ok(mut meter) => {
                meter.set_max_history(10_000).ok();
                self.inner = Some(meter);
                true
            }
            Err(_) => false,
        }
    }

    pub fn reset(&mut self) {
        if let Some(ref mut m) = self.inner {
            m.reset();
        }
    }

    pub fn process_interleaved(&mut self, samples: &[f32]) {
        if let Some(ref mut m) = self.inner {
            m.add_frames_f32(samples).ok();
        }
    }

    pub fn momentary_lufs(&self) -> f32 {
        self.inner
            .as_ref()
            .and_then(|m| m.loudness_momentary().ok())
            .map(|v| v as f32)
            .unwrap_or(f32::NEG_INFINITY)
    }

    pub fn short_term_lufs(&self) -> f32 {
        self.inner
            .as_ref()
            .and_then(|m| m.loudness_shortterm().ok())
            .map(|v| v as f32)
            .unwrap_or(f32::NEG_INFINITY)
    }

    pub fn integrated_lufs(&self) -> f32 {
        self.inner
            .as_ref()
            .and_then(|m| m.loudness_global().ok())
            .map(|v| v as f32)
            .unwrap_or(f32::NEG_INFINITY)
    }
}
