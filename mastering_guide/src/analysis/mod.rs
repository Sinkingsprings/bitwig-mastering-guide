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
    reader: Option<FrameReader>,
    /// Interleaved scratch buffer for ebur128 (avoids allocation on audio thread
    /// after first call — we resize once then reuse)
    interleaved: Vec<f32>,
    /// Mono mix scratch buffer for spectrum
    mono: Vec<f32>,
    sample_rate: f32,
    /// Counts samples since last frame was published
    publish_counter: usize,
    /// Publish a new frame every ~100ms
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
            reader: Some(reader),
            interleaved: Vec::new(),
            mono: Vec::new(),
            sample_rate: 44100.0,
            publish_counter: 0,
            publish_interval: 4410, // 100ms at 44.1kHz; updated on init
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

    /// Takes ownership of the reader so the GUI can hold it.
    pub fn reader(&mut self) -> FrameReader {
        self.reader.take().expect("reader already taken")
    }

    pub fn process(&mut self, buffer: &mut Buffer, _playing: bool) {
        let num_samples = buffer.samples();
        if num_samples == 0 {
            return;
        }

        // Resize scratch buffers without allocation after first call
        if self.interleaved.len() < num_samples * 2 {
            self.interleaved.resize(num_samples * 2, 0.0);
        }
        if self.mono.len() < num_samples {
            self.mono.resize(num_samples, 0.0);
        }

        // Gather channel data — nih-plug Buffer iterates by channel
        let mut left = vec![0.0f32; num_samples];
        let mut right = vec![0.0f32; num_samples];

        for (i, channel_samples) in buffer.iter_samples().enumerate() {
            let mut ch = 0;
            for sample in channel_samples {
                match ch {
                    0 => left[i] = *sample,
                    1 => right[i] = *sample,
                    _ => {}
                }
                ch += 1;
            }
        }

        // Build interleaved for ebur128
        for i in 0..num_samples {
            self.interleaved[i * 2] = left[i];
            self.interleaved[i * 2 + 1] = right[i];
        }

        // Build mono mix for spectrum
        for i in 0..num_samples {
            self.mono[i] = (left[i] + right[i]) * 0.5;
        }

        // Run analyzers
        self.lufs.process_interleaved(&self.interleaved[..num_samples * 2]);
        self.peak.process(&[&left, &right]);
        self.spectrum.process_mono(&self.mono[..num_samples]);
        self.stereo.process(&left, &right);

        self.publish_counter += num_samples;
        if self.publish_counter >= self.publish_interval {
            self.publish_counter = 0;

            let true_peak = self.peak.true_peak_dbtp();
            let short_term = self.lufs.short_term_lufs();
            self.dynamics.update(true_peak, short_term);

            let integrated = self.lufs.integrated_lufs();
            let plr = dynamics::compute_plr(true_peak, integrated);

            let frame = TrackFrame {
                lufs_momentary: self.lufs.momentary_lufs(),
                lufs_short_term: short_term,
                lufs_integrated: integrated,
                true_peak_dbtp: true_peak,
                sample_peak_dbfs: self.peak.sample_peak_dbfs(),
                rms_dbfs: self.peak.rms_dbfs(),
                plr,
                psr_min: self.dynamics.psr_min,
                correlation: self.stereo.correlation,
                stereo_width: self.stereo.stereo_width,
                bands_dbfs: self.spectrum.bands_dbfs,
                dc_offset: self.peak.dc_offset(),
                timestamp_ms: timestamp_ms(),
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
