use num_complex::Complex;
use realfft::RealFftPlanner;
use std::sync::Arc;

pub const FFT_SIZE: usize = 4096;
pub const NUM_BANDS: usize = 10;

/// IEC 61260 octave band center frequencies (Hz)
const BAND_CENTERS: [f32; NUM_BANDS] = [
    31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

pub struct SpectrumAnalyzer {
    planner: RealFftPlanner<f32>,
    fft: Option<Arc<dyn realfft::RealToComplex<f32>>>,
    window: Vec<f32>,
    /// Raw audio samples for overlap-add. Never touched by the FFT, so the
    /// overlap shift operates on valid audio data even after fft.process()
    /// destroys input_buf.
    audio_buf: Vec<f32>,
    /// Windowed copy fed into the FFT; overwritten by realfft during process().
    input_buf: Vec<f32>,
    output_buf: Vec<Complex<f32>>,
    hop_count: usize,
    hop_size: usize,
    sample_rate: f32,
    /// Smoothed band energies (exponential moving average)
    pub bands_dbfs: [f32; NUM_BANDS],
    /// Same band energies in linear power, smoothed in lockstep with the dB
    /// values. Exposed so the audio thread can compute mid/side band ratios
    /// without round-tripping through dB.
    pub bands_power_linear: [f32; NUM_BANDS],
    /// Bin ranges per band
    band_bins: [(usize, usize); NUM_BANDS],
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        Self {
            planner: RealFftPlanner::new(),
            fft: None,
            window: Vec::new(),
            audio_buf: Vec::new(),
            input_buf: Vec::new(),
            output_buf: Vec::new(),
            hop_count: 0,
            hop_size: FFT_SIZE / 2,
            sample_rate: 44100.0,
            bands_dbfs: [f32::NEG_INFINITY; NUM_BANDS],
            bands_power_linear: [0.0; NUM_BANDS],
            band_bins: [(0, 0); NUM_BANDS],
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let fft = self.planner.plan_fft_forward(FFT_SIZE);
        self.window = hann_window(FFT_SIZE);
        self.audio_buf = vec![0.0; FFT_SIZE];
        self.input_buf = fft.make_input_vec();
        self.output_buf = fft.make_output_vec();
        self.band_bins = compute_band_bins(sample_rate, FFT_SIZE);
        self.fft = Some(fft);
        self.hop_count = 0;
        self.bands_dbfs = [f32::NEG_INFINITY; NUM_BANDS];
    }

    pub fn reset(&mut self) {
        self.hop_count = 0;
        self.bands_dbfs = [f32::NEG_INFINITY; NUM_BANDS];
        self.bands_power_linear = [0.0; NUM_BANDS];
        self.audio_buf.fill(0.0);
        if let Some(ref fft) = self.fft.clone() {
            self.input_buf = fft.make_input_vec();
        }
    }

    /// Feed mono-mixed samples (L+R)/2 from one render block.
    pub fn process_mono(&mut self, mono: &[f32]) {
        let Some(ref fft) = self.fft.clone() else {
            return;
        };
        for &s in mono {
            if self.hop_count < FFT_SIZE {
                self.audio_buf[self.hop_count] = s;
                self.hop_count += 1;
            }
            if self.hop_count >= FFT_SIZE {
                // Apply window into input_buf; audio_buf is preserved for overlap.
                for i in 0..FFT_SIZE {
                    self.input_buf[i] = self.audio_buf[i] * self.window[i];
                }
                let _ = fft.process(&mut self.input_buf, &mut self.output_buf);
                self.update_bands();
                // Overlap: shift audio_buf left by hop_size, zero the tail.
                self.audio_buf.copy_within(self.hop_size..FFT_SIZE, 0);
                for i in (FFT_SIZE - self.hop_size)..FFT_SIZE {
                    self.audio_buf[i] = 0.0;
                }
                self.hop_count = FFT_SIZE - self.hop_size;
            }
        }
    }

    fn update_bands(&mut self) {
        const ALPHA: f32 = 0.1;
        let scale = 2.0 / FFT_SIZE as f32;
        for b in 0..NUM_BANDS {
            let (lo, hi) = self.band_bins[b];
            if lo >= hi {
                continue;
            }
            let power: f32 = self.output_buf[lo..hi]
                .iter()
                .map(|c: &Complex<f32>| {
                    let mag = c.norm() * scale;
                    mag * mag
                })
                .sum::<f32>()
                / (hi - lo) as f32;
            let db = if power > 1e-10 {
                10.0 * power.log10()
            } else {
                -120.0
            };
            if self.bands_dbfs[b] <= -119.0 {
                self.bands_dbfs[b] = db;
                self.bands_power_linear[b] = power;
            } else {
                self.bands_dbfs[b] = self.bands_dbfs[b] * (1.0 - ALPHA) + db * ALPHA;
                self.bands_power_linear[b] =
                    self.bands_power_linear[b] * (1.0 - ALPHA) + power * ALPHA;
            }
        }
    }
}

/// Linear regression of `bands` (dB) over octave index. Bands are octave-spaced
/// so x = 0..NUM_BANDS gives a slope in dB per octave. Bands at or below
/// `silence_floor_db` are excluded so silent low frequencies do not skew the
/// result. Returns 0.0 if fewer than 3 bands have usable energy.
pub fn band_slope_db_per_oct(bands: &[f32; NUM_BANDS], silence_floor_db: f32) -> f32 {
    let mut count = 0u32;
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;
    let mut sum_xy = 0.0_f64;
    let mut sum_xx = 0.0_f64;
    for (i, &y) in bands.iter().enumerate() {
        if y.is_finite() && y > silence_floor_db {
            let x = i as f64;
            let yf = y as f64;
            sum_x += x;
            sum_y += yf;
            sum_xy += x * yf;
            sum_xx += x * x;
            count += 1;
        }
    }
    if count < 3 {
        return 0.0;
    }
    let n = count as f64;
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-9 {
        return 0.0;
    }
    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    slope as f32
}

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (size - 1) as f32).cos())
        })
        .collect()
}

fn compute_band_bins(sample_rate: f32, fft_size: usize) -> [(usize, usize); NUM_BANDS] {
    let bin_hz = sample_rate / fft_size as f32;
    let nyquist = fft_size / 2 + 1;
    let mut bins = [(0usize, 0usize); NUM_BANDS];
    for (b, &center) in BAND_CENTERS.iter().enumerate() {
        let lo_hz = center / 2.0_f32.sqrt();
        let hi_hz = center * 2.0_f32.sqrt();
        let lo_bin = ((lo_hz / bin_hz).floor() as usize).max(1);
        let hi_bin = ((hi_hz / bin_hz).ceil() as usize + 1).min(nyquist);
        bins[b] = (lo_bin, hi_bin);
    }
    bins
}
