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
    #[persist = "editor-state"]
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
            editor_state: EguiState::from_size(430, 590),
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
        })
    }
}
