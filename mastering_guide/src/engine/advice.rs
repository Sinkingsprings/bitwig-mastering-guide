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

/// A concrete action Gilligan can execute in Bitwig on the user's behalf.
#[derive(Debug, Clone)]
pub enum FixAction {
    /// Adjust a track fader (or the master fader when track_name is None)
    /// by delta_db decibels.  Positive = louder, negative = quieter.
    AdjustVolume {
        track_name: Option<String>,
        delta_db: f32,
    },
}

impl FixAction {
    /// Serialise to the JSON wire format sent to Gilligan.
    pub fn to_json_msg(&self) -> String {
        match self {
            FixAction::AdjustVolume { track_name: None, delta_db } => {
                format!(
                    r#"{{"type":"fix_action","action":"adjust_volume","delta_db":{:.2}}}"#,
                    delta_db
                )
            }
            FixAction::AdjustVolume { track_name: Some(name), delta_db } => {
                let escaped = name.replace('"', "\\\"");
                format!(
                    r#"{{"type":"fix_action","action":"adjust_volume","track_name":"{escaped}","delta_db":{:.2}}}"#,
                    delta_db
                )
            }
        }
    }

    /// Short description shown in the tooltip on the Apply button.
    pub fn description(&self) -> String {
        match self {
            FixAction::AdjustVolume { track_name: None, delta_db } => {
                if *delta_db < 0.0 {
                    format!("Reduce master fader by {:.1} dB", delta_db.abs())
                } else {
                    format!("Raise master fader by {:.1} dB", delta_db)
                }
            }
            FixAction::AdjustVolume { track_name: Some(name), delta_db } => {
                if *delta_db < 0.0 {
                    format!("Reduce {} fader by {:.1} dB", name, delta_db.abs())
                } else {
                    format!("Raise {} fader by {:.1} dB", name, delta_db)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Advice {
    pub severity: Severity,
    pub category: Category,
    pub scope: Scope,
    pub title: String,
    pub detail: String,
    pub fix: String,
    /// If present, Gilligan can execute this action with one click.
    pub action: Option<FixAction>,
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

    pub fn severity_rgb(&self) -> (u8, u8, u8) {
        match self.severity {
            Severity::Critical   => (220, 50, 50),
            Severity::Warning    => (220, 150, 30),
            Severity::Suggestion => (200, 200, 50),
            Severity::Good       => (50, 180, 50),
        }
    }
}
