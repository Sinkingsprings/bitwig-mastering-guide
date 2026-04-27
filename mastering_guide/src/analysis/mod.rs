pub mod dynamics;
pub mod frame;
pub mod lufs;
pub mod peak;
pub mod spectrum;
pub mod stereo;

use frame::{FrameReader, FrameWriter, TrackFrame, frame_channel};
use nih_plug::prelude::Buffer;

use crate::analysis::frame::NUM_BANDS;

pub struct AnalysisState {
    lufs: lufs::LufsAnalyzer,
    peak: peak::PeakAnalyzer,
    /// FFT of the mid signal (L+R)/2.
    spectrum: spectrum::SpectrumAnalyzer,
    /// FFT of the side signal (L-R)/2 — used together with the mid spectrum
    /// to derive a per-band correlation in [-1, 1].
    spectrum_side: spectrum::SpectrumAnalyzer,
    stereo: stereo::StereoAnalyzer,
    dynamics: dynamics::DynamicsAnalyzer,
    writer: FrameWriter,
    /// Cloneable — the editor can call reader() multiple times safely.
    reader: FrameReader,
    /// Pre-allocated scratch buffers sized to max_buffer_size in initialize().
    interleaved: Vec<f32>,
    mono: Vec<f32>,
    side: Vec<f32>,
    left_buf: Vec<f32>,
    right_buf: Vec<f32>,
    sample_rate: f32,
    publish_counter: usize,
    publish_interval: usize,
}

impl AnalysisState {
    pub fn new() -> Self {
        let (writer, reader) = frame_channel();
        Self {
            lufs: lufs::LufsAnalyzer::new(),
            peak: peak::PeakAnalyzer::new(),
            spectrum: spectrum::SpectrumAnalyzer::new(),
            spectrum_side: spectrum::SpectrumAnalyzer::new(),
            stereo: stereo::StereoAnalyzer::new(),
            dynamics: dynamics::DynamicsAnalyzer::new(),
            writer,
            reader,
            interleaved: Vec::new(),
            mono: Vec::new(),
            side: Vec::new(),
            left_buf: Vec::new(),
            right_buf: Vec::new(),
            sample_rate: 44100.0,
            publish_counter: 0,
            publish_interval: 4410,
        }
    }

    /// Returns false if the host provides an unsupported sample rate.
    pub fn initialize(&mut self, sample_rate: f32, max_buffer_size: usize) -> bool {
        self.sample_rate = sample_rate;
        self.publish_interval = (sample_rate * 0.1) as usize;
        if !self.lufs.initialize(sample_rate) {
            return false;
        }
        self.peak.initialize(sample_rate);
        self.spectrum.initialize(sample_rate);
        self.spectrum_side.initialize(sample_rate);
        self.stereo.initialize(sample_rate);
        // Pre-allocate to the host's declared maximum block size so that
        // process() never allocates on the audio thread.
        self.interleaved.resize(max_buffer_size * 2, 0.0);
        self.mono.resize(max_buffer_size, 0.0);
        self.side.resize(max_buffer_size, 0.0);
        self.left_buf.resize(max_buffer_size, 0.0);
        self.right_buf.resize(max_buffer_size, 0.0);
        true
    }

    pub fn reset(&mut self) {
        self.lufs.reset();
        self.peak.reset();
        self.spectrum.reset();
        self.spectrum_side.reset();
        self.stereo.reset();
        self.dynamics.reset();
        self.publish_counter = 0;
    }

    /// Returns a cheap clone of the frame reader. Safe to call any number of
    /// times — the editor can be opened and closed repeatedly without issue.
    pub fn reader(&self) -> FrameReader {
        self.reader.clone()
    }

    pub fn process(&mut self, buffer: &mut Buffer, _playing: bool) {
        let num_samples = buffer.samples().min(self.mono.len());
        if num_samples == 0 {
            return;
        }

        {
            let slice = buffer.as_slice();
            let src_l: &[f32] = slice.first().map(|s| s.as_ref()).unwrap_or(&[]);
            let src_r: &[f32] = slice.get(1).map(|s| s.as_ref()).unwrap_or(src_l);
            let n = num_samples.min(src_l.len());

            self.left_buf[..n].copy_from_slice(&src_l[..n]);
            self.right_buf[..n].copy_from_slice(&src_r[..n]);

            for i in 0..n {
                self.interleaved[i * 2]     = src_l[i];
                self.interleaved[i * 2 + 1] = src_r[i];
                self.mono[i]                = (src_l[i] + src_r[i]) * 0.5;
                self.side[i]                = (src_l[i] - src_r[i]) * 0.5;
            }
        }

        self.lufs.process_interleaved(&self.interleaved[..num_samples * 2]);
        self.lufs.process_mono(&self.mono[..num_samples]);
        self.peak.process(&[
            &self.left_buf[..num_samples],
            &self.right_buf[..num_samples],
        ]);
        self.spectrum.process_mono(&self.mono[..num_samples]);
        self.spectrum_side.process_mono(&self.side[..num_samples]);
        self.stereo.process(
            &self.left_buf[..num_samples],
            &self.right_buf[..num_samples],
        );

        self.publish_counter += num_samples;
        if self.publish_counter >= self.publish_interval {
            self.publish_counter = 0;

            let true_peak  = self.peak.true_peak_dbtp();
            let short_term = self.lufs.short_term_lufs();
            self.dynamics.update(true_peak, short_term);

            let integrated = self.lufs.integrated_lufs();
            let plr        = dynamics::compute_plr(true_peak, integrated);

            // Per-band correlation from mid/side band powers:
            //   corr = (P_mid - P_side) / (P_mid + P_side)
            // Silent bands (both powers ~0) collapse to 1.0 (mono).
            let mut bands_corr = [1.0_f32; NUM_BANDS];
            for i in 0..NUM_BANDS {
                let m = self.spectrum.bands_power_linear[i];
                let s = self.spectrum_side.bands_power_linear[i];
                let total = m + s;
                if total > 1e-10 {
                    bands_corr[i] = ((m - s) / total).clamp(-1.0, 1.0);
                }
            }

            let spectral_tilt_db_per_oct =
                spectrum::band_slope_db_per_oct(&self.spectrum.bands_dbfs, -90.0);

            let frame = TrackFrame {
                lufs_momentary:        self.lufs.momentary_lufs(),
                lufs_short_term:       short_term,
                lufs_integrated:       integrated,
                lufs_integrated_mono:  self.lufs.integrated_lufs_mono(),
                true_peak_dbtp:        true_peak,
                sample_peak_dbfs:      self.peak.sample_peak_dbfs(),
                rms_dbfs:              self.peak.rms_dbfs(),
                plr,
                psr_min:               self.dynamics.psr_min,
                macrodynamics_lu:      self.dynamics.macrodynamics_lu,
                correlation:           self.stereo.correlation,
                stereo_width:          self.stereo.stereo_width,
                bands_dbfs:            self.spectrum.bands_dbfs,
                bands_corr,
                spectral_tilt_db_per_oct,
                dc_offset:             self.peak.dc_offset(),
                timestamp_ms:          timestamp_ms(),
            };

            self.writer.update(frame);
        }
    }
}

fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
