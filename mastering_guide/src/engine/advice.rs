#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical = 0,
    Warning = 1,
    Suggestion = 2,
    Good = 3,
}

#[derive(Debug, Clone)]
pub enum Category {
    Technical,
    Loudness,
    Dynamics,
    FrequencyBalance,
    Stereo,
    MixBalance,
}

#[derive(Debug, Clone)]
pub enum Scope {
    MasterBus,
    Track(String),
    AllTracks,
}

#[derive(Debug, Clone)]
pub struct Advice {
    pub severity: Severity,
    pub category: Category,
    pub scope: Scope,
    pub title: String,
    pub detail: String,
    pub fix: String,
}

impl Advice {
    pub fn severity_label(&self) -> &'static str {
        match self.severity {
            Severity::Critical   => "⛔",
            Severity::Warning    => "⚠",
            Severity::Suggestion => "→",
            Severity::Good       => "✓",
        }
    }

    /// Returns an RGB tuple used by the GUI layer to color-code advice.
    pub fn severity_rgb(&self) -> (u8, u8, u8) {
        match self.severity {
            Severity::Critical   => (220, 50, 50),
            Severity::Warning    => (220, 150, 30),
            Severity::Suggestion => (200, 200, 50),
            Severity::Good       => (50, 180, 50),
        }
    }
}
