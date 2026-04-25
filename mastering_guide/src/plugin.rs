use crate::analysis::AnalysisState;
use crate::ipc::registry::Registry;
use crate::params::MasteringGuideParams;
use std::sync::Arc;

pub struct MasteringGuide {
    pub params: Arc<MasteringGuideParams>,
    pub analysis: AnalysisState,
    pub registry: Registry,
    pub sample_rate: f32,
}

impl Default for MasteringGuide {
    fn default() -> Self {
        Self {
            params: MasteringGuideParams::new(),
            analysis: AnalysisState::new(),
            registry: Registry::new(),
            sample_rate: 44100.0,
        }
    }
}
