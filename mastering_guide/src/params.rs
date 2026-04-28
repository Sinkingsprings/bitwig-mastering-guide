use crate::gui::{BASE_HEIGHT, BASE_WIDTH};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Enum)]
pub enum GenreParam {
    PopRnB,
    Rock,
    EdmDance,
    HipHop,
    JazzAcoustic,
    Classical,
    Folk,
}

impl std::fmt::Display for GenreParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenreParam::PopRnB => write!(f, "Pop / R&B"),
            GenreParam::Rock => write!(f, "Rock"),
            GenreParam::EdmDance => write!(f, "EDM / Dance"),
            GenreParam::HipHop => write!(f, "Hip-Hop"),
            GenreParam::JazzAcoustic => write!(f, "Jazz / Acoustic"),
            GenreParam::Classical => write!(f, "Classical"),
            GenreParam::Folk => write!(f, "Folk"),
        }
    }
}

/// Role of the track this plugin instance is monitoring. Used by the rule
/// engine to apply role-aware mix-balance advice (bass-vs-bass-drum
/// collisions at 80–120 Hz, vocal-vs-harmony masking, etc.). `Auto` is a
/// placeholder that will be filled by the Gilligan extension in a later
/// phase by inspecting the Bitwig track name / colour / type; until then
/// it behaves as "no specific role" and tracks set to Auto are excluded
/// from role-specific rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum TrackRole {
    Auto,
    Vocal,
    Drums,
    Bass,
    Harm,
    Pad,
    Fx,
}

impl std::fmt::Display for TrackRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackRole::Auto  => write!(f, "Auto"),
            TrackRole::Vocal => write!(f, "Vocal"),
            TrackRole::Drums => write!(f, "Drums"),
            TrackRole::Bass  => write!(f, "Bass"),
            TrackRole::Harm  => write!(f, "Harmony"),
            TrackRole::Pad   => write!(f, "Pad"),
            TrackRole::Fx    => write!(f, "FX"),
        }
    }
}

/// User-facing UI scale. Multiplies both `ctx.set_zoom_factor` and the
/// requested window size, so the whole editor scales as one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum UiScale {
    Pct75,
    Pct100,
    Pct125,
    Pct150,
    Pct200,
}

impl UiScale {
    pub fn factor(self) -> f32 {
        match self {
            UiScale::Pct75  => 0.75,
            UiScale::Pct100 => 1.00,
            UiScale::Pct125 => 1.25,
            UiScale::Pct150 => 1.50,
            UiScale::Pct200 => 2.00,
        }
    }
}

impl std::fmt::Display for UiScale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiScale::Pct75  => write!(f, "75%"),
            UiScale::Pct100 => write!(f, "100%"),
            UiScale::Pct125 => write!(f, "125%"),
            UiScale::Pct150 => write!(f, "150%"),
            UiScale::Pct200 => write!(f, "200%"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Enum)]
pub enum PlatformParam {
    Spotify,
    AppleMusic,
    YouTube,
    AmazonMusic,
    Tidal,
    Broadcast,
    SoundCloud,
}

impl std::fmt::Display for PlatformParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformParam::Spotify => write!(f, "Spotify"),
            PlatformParam::AppleMusic => write!(f, "Apple Music"),
            PlatformParam::YouTube => write!(f, "YouTube"),
            PlatformParam::AmazonMusic => write!(f, "Amazon Music"),
            PlatformParam::Tidal => write!(f, "Tidal"),
            PlatformParam::Broadcast => write!(f, "Broadcast (EBU R128)"),
            PlatformParam::SoundCloud => write!(f, "SoundCloud"),
        }
    }
}

#[derive(Params)]
pub struct MasteringGuideParams {
    // Intentionally NOT persisted. Earlier UI-scale experiments left some
    // projects holding a ~230 px width in their saved editor-state blob,
    // which then overrode the from_size default and clipped the layout.
    // Re-creating instances does not always shake that loose. Until we
    // implement a proper user-driven resize that we want to persist, the
    // window opens fresh at 430 × 590 every time.
    pub editor_state: Arc<EguiState>,

    #[id = "mode"]
    pub mode: EnumParam<ModeParam>,

    #[id = "genre"]
    pub genre: EnumParam<GenreParam>,

    #[id = "platform"]
    pub platform: EnumParam<PlatformParam>,

    /// Slot index in shared memory (0–31). Auto-assigned but user can override.
    #[id = "slot"]
    pub slot_id: IntParam,

    /// What role this track plays in the mix. Drives role-aware advice in
    /// the rule engine.
    #[id = "track_role"]
    pub track_role: EnumParam<TrackRole>,

    /// User-facing UI scale. nih_plug_egui forces `pixels_per_point = 1.0`
    /// on Linux, so on HiDPI displays the editor renders smaller than the
    /// host UI. This param applies a matching `ctx.set_zoom_factor` and
    /// resizes the host window so the whole editor scales together.
    #[id = "ui_scale"]
    pub ui_scale: EnumParam<UiScale>,
}

#[derive(Debug, Clone, PartialEq, Enum)]
pub enum ModeParam {
    Track,
    Master,
}

impl std::fmt::Display for ModeParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModeParam::Track => write!(f, "Track"),
            ModeParam::Master => write!(f, "Master"),
        }
    }
}

impl MasteringGuideParams {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            editor_state: EguiState::from_size(BASE_WIDTH, BASE_HEIGHT),
            mode: EnumParam::new("Mode", ModeParam::Track),
            genre: EnumParam::new("Genre", GenreParam::PopRnB),
            platform: EnumParam::new("Platform", PlatformParam::Spotify),
            slot_id: IntParam::new("Slot", -1, IntRange::Linear { min: -1, max: 31 })
                .with_value_to_string(Arc::new(|v| {
                    if v < 0 {
                        "Auto".to_string()
                    } else {
                        v.to_string()
                    }
                })),
            track_role: EnumParam::new("Track Role", TrackRole::Auto),
            ui_scale: EnumParam::new("UI Scale", UiScale::Pct100),
        })
    }
}
