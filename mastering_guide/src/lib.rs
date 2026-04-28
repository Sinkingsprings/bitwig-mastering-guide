use nih_plug::prelude::*;
use std::sync::Arc;

/// Installed once (via std::sync::Once) so multiple plugin instances don't
/// fight over the hook.
fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            eprintln!("[MasteringGuide] PANIC: {}", info);
            eprintln!("[MasteringGuide] panic backtrace:\n{:?}", std::backtrace::Backtrace::capture());
            default_hook(info);
        }));
    });
}

mod analysis;
mod engine;
mod gui;
mod ipc;
mod params;
mod plugin;

use ipc::Registry;
use ipc::spawn_gilligan;
use params::ModeParam;
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
        context: &mut impl InitContext<Self>,
    ) -> bool {
        install_panic_hook();
        self.sample_rate = buffer_config.sample_rate;
        if !self.analysis.initialize(buffer_config.sample_rate, buffer_config.max_buffer_size as usize) {
            return false;
        }

        // Spawn the Gilligan IPC client once — on the first initialize() call
        // the Arc has only the plugin's reference (refcount = 1), so spawning
        // hasn't happened yet. On re-activation the GUI's Arc clone keeps the
        // thread alive across reinitializations, so we don't double-spawn.
        if Arc::strong_count(&self.gilligan) == 1 {
            spawn_gilligan(self.gilligan.clone());
        }

        // Use the CLAP track-info extension to get the real track name when
        // available (Bitwig supports clap.track-info). Falls back to slot index.
        let name_from_host = context.track_info()
            .and_then(|ti| ti.name.clone())
            .filter(|n| !n.is_empty());

        if self.registry.is_none() {
            let mode = if self.params.mode.value() == ModeParam::Master { 1 } else { 0 };
            if let Some(reg) = Registry::new(mode, String::from("Track ?")) {
                let slot = reg.slot_index();
                self.track_name = name_from_host
                    .unwrap_or_else(|| format!("Track {}", slot + 1));
                self.registry = Some(Arc::new(reg));
            }
        } else if let Some(real_name) = name_from_host {
            // Re-activation (e.g. sample-rate change): refresh the name.
            self.track_name = real_name;
        }

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
        let frame_reader = self.analysis.reader();
        let registry = self.registry.clone();
        let track_name = self.track_name.clone();
        let gilligan = self.gilligan.clone();
        gui::create_editor(params, frame_reader, registry, track_name, gilligan)
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
