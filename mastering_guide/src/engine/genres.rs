use crate::params::GenreParam;

/// 10-band relative spectral targets (dB, relative to the 1kHz band).
/// Bands: 31.5, 63, 125, 250, 500, 1k, 2k, 4k, 8k, 16kHz
pub struct GenreCurve {
    pub name: &'static str,
    pub bands_rel: [f32; 10],
    #[allow(dead_code)]
    pub lufs_target: f32,
    #[allow(dead_code)]
    pub lufs_range: f32,
    pub plr_min: f32,
    pub psr_min: f32,
}

pub const GENRES: &[GenreCurve] = &[
    GenreCurve {
        name: "Pop / R&B",
        //           31   63   125  250  500  1k   2k   4k   8k   16k
        bands_rel: [ 2.0, 4.0, 2.0, 0.0,-1.0, 0.0, 0.5, 0.0, 0.5,-1.0],
        lufs_target: -14.0, lufs_range: 2.0, plr_min: 9.0, psr_min: 8.0,
    },
    GenreCurve {
        name: "Rock",
        bands_rel: [ 1.0, 3.0, 2.0, 1.0,-1.0, 0.0, 1.0, 0.5, 0.5,-1.5],
        lufs_target: -12.0, lufs_range: 2.0, plr_min: 8.0, psr_min: 7.0,
    },
    GenreCurve {
        name: "EDM / Dance",
        bands_rel: [ 4.0, 5.0, 2.0,-1.0,-3.0, 0.0, 0.5, 0.0, 1.0, 0.5],
        lufs_target: -10.0, lufs_range: 2.0, plr_min: 7.0, psr_min: 6.0,
    },
    GenreCurve {
        name: "Hip-Hop",
        bands_rel: [ 3.0, 5.0, 3.0, 0.0,-2.0, 0.0, 0.5, 0.0, 0.5,-1.0],
        lufs_target: -12.0, lufs_range: 2.0, plr_min: 8.0, psr_min: 7.0,
    },
    GenreCurve {
        name: "Jazz / Acoustic",
        bands_rel: [ 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 0.5],
        lufs_target: -18.0, lufs_range: 3.0, plr_min: 14.0, psr_min: 12.0,
    },
    GenreCurve {
        name: "Classical",
        bands_rel: [-1.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 0.5],
        lufs_target: -23.0, lufs_range: 3.0, plr_min: 18.0, psr_min: 16.0,
    },
    GenreCurve {
        name: "Folk",
        bands_rel: [ 0.0, 1.5, 1.5, 0.5, 0.0, 0.0, 0.5, 0.5, 1.0, 0.5],
        lufs_target: -16.0, lufs_range: 3.0, plr_min: 12.0, psr_min: 10.0,
    },
];

pub fn genre_for(param: &GenreParam) -> &'static GenreCurve {
    match param {
        GenreParam::PopRnB        => &GENRES[0],
        GenreParam::Rock          => &GENRES[1],
        GenreParam::EdmDance      => &GENRES[2],
        GenreParam::HipHop        => &GENRES[3],
        GenreParam::JazzAcoustic  => &GENRES[4],
        GenreParam::Classical     => &GENRES[5],
        GenreParam::Folk          => &GENRES[6],
    }
}
