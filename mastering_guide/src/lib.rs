use nih_plug::prelude::*;
use std::sync::Arc;

mod analysis;
mod engine;
mod gui;
mod ipc;
mod params;
mod plugin;

use plugin::MasteringGuide;

impl Plugin for MasteringGuide {
    const NAME: &'static str = "Mastering Guide";
    const VENDOR: &'static str = "Local";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.analysis.initialize(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.analysis.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let transport = context.transport();
        self.analysis.process(buffer, transport.playing);
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let analysis_data = self.analysis.reader();
        gui::create_editor(params, analysis_data)
    }
}

impl ClapPlugin for MasteringGuide {
    const CLAP_ID: &'static str = "local.mastering-guide";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Multi-track mastering analysis and guidance");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Analyzer,
        ClapFeature::Stereo,
        ClapFeature::Mixing,
        ClapFeature::Mastering,
    ];
}

nih_export_clap!(MasteringGuide);
