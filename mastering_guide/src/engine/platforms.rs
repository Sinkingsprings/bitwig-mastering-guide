use crate::params::PlatformParam;

pub struct PlatformTarget {
    pub name: &'static str,
    pub lufs_target: f32,
    pub true_peak_ceil: f32,
}

pub const PLATFORMS: &[PlatformTarget] = &[
    PlatformTarget { name: "Spotify",              lufs_target: -14.0, true_peak_ceil: -1.0 },
    PlatformTarget { name: "Apple Music",          lufs_target: -16.0, true_peak_ceil: -1.0 },
    PlatformTarget { name: "YouTube",              lufs_target: -13.0, true_peak_ceil: -1.0 },
    PlatformTarget { name: "Amazon Music",         lufs_target: -14.0, true_peak_ceil: -2.0 },
    PlatformTarget { name: "Tidal",                lufs_target: -14.0, true_peak_ceil: -1.0 },
    PlatformTarget { name: "Broadcast (EBU R128)", lufs_target: -23.0, true_peak_ceil: -1.0 },
    PlatformTarget { name: "SoundCloud",           lufs_target: f32::NEG_INFINITY, true_peak_ceil: -0.3 },
];

pub fn platform_for(param: &PlatformParam) -> &'static PlatformTarget {
    match param {
        PlatformParam::Spotify      => &PLATFORMS[0],
        PlatformParam::AppleMusic   => &PLATFORMS[1],
        PlatformParam::YouTube      => &PLATFORMS[2],
        PlatformParam::AmazonMusic  => &PLATFORMS[3],
        PlatformParam::Tidal        => &PLATFORMS[4],
        PlatformParam::Broadcast    => &PLATFORMS[5],
        PlatformParam::SoundCloud   => &PLATFORMS[6],
    }
}
