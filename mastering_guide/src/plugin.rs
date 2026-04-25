use crate::analysis::AnalysisState;
use crate::ipc::Registry;
use crate::params::MasteringGuideParams;
use std::sync::Arc;

pub struct MasteringGuide {
    pub params: Arc<MasteringGuideParams>,
    pub analysis: AnalysisState,
    /// Claimed after the first `initialize()` call. `None` before activation.
    pub registry: Option<Arc<Registry>>,
    pub sample_rate: f32,
    /// Track name shown in the master instance's track list.
    /// Defaults to "Track <slot>" until a better source is available.
    pub track_name: String,
}

impl Default for MasteringGuide {
    fn default() -> Self {
        Self {
            params: MasteringGuideParams::new(),
            analysis: AnalysisState::new(),
            registry: None,
            sample_rate: 44100.0,
            track_name: String::new(),
        }
    }
}
