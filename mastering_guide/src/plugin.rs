use crate::analysis::AnalysisState;
use crate::ipc::gilligan::GilliganState;
use crate::ipc::Registry;
use crate::params::MasteringGuideParams;
use std::sync::{Arc, Mutex};

pub struct MasteringGuide {
    pub params: Arc<MasteringGuideParams>,
    pub analysis: AnalysisState,
    /// Claimed after the first `initialize()` call. `None` before activation.
    pub registry: Option<Arc<Registry>>,
    pub sample_rate: f32,
    /// Track name shown in the master instance's track list.
    pub track_name: String,
    /// Gilligan IPC state — shared with the GUI editor. Spawned once on first
    /// `initialize()` so the connection attempt starts as soon as the plugin
    /// loads, not just when the GUI window is opened.
    pub gilligan: Arc<Mutex<GilliganState>>,
}

impl Default for MasteringGuide {
    fn default() -> Self {
        Self {
            params: MasteringGuideParams::new(),
            analysis: AnalysisState::new(),
            registry: None,
            sample_rate: 44100.0,
            track_name: String::new(),
            gilligan: Arc::new(Mutex::new(GilliganState::default())),
        }
    }
}
