pub mod dynamics;
pub mod frame;
pub mod lufs;
pub mod peak;
pub mod spectrum;
pub mod stereo;

use frame::{FrameReader, FrameWriter, TrackFrame, frame_channel};
use nih_plug::prelude::Buffer;

pub struct AnalysisState {
    lufs: lufs::LufsAnalyzer,
    peak: peak::PeakAnalyzer,
    spectrum: spectrum::SpectrumAnalyzer,
    stereo: stereo::StereoAnalyzer,
    dynamics: dynamics::DynamicsAnalyzer,
    writer: FrameWriter,
    /// Cloneable — the editor can call reader() multiple times safely.
    reader: FrameReader,
    /// Pre-allocated scratch buffers. Grow on demand, never shrink.
    /// No heap allocation occurs on the audio thread after warm-up.
    interleaved: Vec<f32>,
    mono: Vec<f32>,
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
            stereo: stereo::StereoAnalyzer::new(),
            dynamics: dynamics::DynamicsAnalyzer::new(),
            writer,
            reader,
            interleaved: Vec::new(),
            mono: Vec::new(),
            left_buf: Vec::new(),
            right_buf: Vec::new(),
            sample_rate: 44100.0,
            publish_counter: 0,
            publish_interval: 4410,
        }
    }

    pub fn initialize(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.publish_interval = (sample_rate * 0.1) as usize;
        self.lufs.initialize(sample_rate);
        self.peak.initialize(sample_rate);
        self.spectrum.initialize(sample_rate);
        self.stereo.initialize(sample_rate);
    }

    pub fn reset(&mut self) {
        self.lufs.reset();
        self.peak.reset();
        self.spectrum.reset();
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
        let num_samples = buffer.samples();
        if num_samples == 0 {
            return;
        }

        // Grow scratch buffers if the block size increased. After the first few
        // blocks these are stable — no allocation on the steady-state audio path.
        if self.interleaved.len() < num_samples * 2 {
            self.interleaved.resize(num_samples * 2, 0.0);
        }
        if self.mono.len() < num_samples {
            self.mono.resize(num_samples, 0.0);
        }
        if self.left_buf.len() < num_samples {
            self.left_buf.resize(num_samples, 0.0);
        }
        if self.right_buf.len() < num_samples {
            self.right_buf.resize(num_samples, 0.0);
        }

        // Copy channel data from the buffer into pre-allocated scratch vecs.
        // buffer.as_slice() → &mut [&mut [f32]], outer index = channel.
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
            }
        }

        self.lufs.process_interleaved(&self.interleaved[..num_samples * 2]);
        self.peak.process(&[
            &self.left_buf[..num_samples],
            &self.right_buf[..num_samples],
        ]);
        self.spectrum.process_mono(&self.mono[..num_samples]);
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

            let frame = TrackFrame {
                lufs_momentary:   self.lufs.momentary_lufs(),
                lufs_short_term:  short_term,
                lufs_integrated:  integrated,
                true_peak_dbtp:   true_peak,
                sample_peak_dbfs: self.peak.sample_peak_dbfs(),
                rms_dbfs:         self.peak.rms_dbfs(),
                plr,
                psr_min:          self.dynamics.psr_min,
                correlation:      self.stereo.correlation,
                stereo_width:     self.stereo.stereo_width,
                bands_dbfs:       self.spectrum.bands_dbfs,
                dc_offset:        self.peak.dc_offset(),
                timestamp_ms:     timestamp_ms(),
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
