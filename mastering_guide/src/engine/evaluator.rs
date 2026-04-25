use crate::analysis::frame::TrackFrame;
use crate::engine::advice::{Advice, Category, Scope, Severity};
use crate::engine::genres::GenreCurve;
use crate::engine::platforms::PlatformTarget;

pub struct EvalContext<'a> {
    pub master: &'a TrackFrame,
    pub tracks: &'a [(String, TrackFrame)],
    pub genre: &'a GenreCurve,
    pub platform: &'a PlatformTarget,
}

pub fn evaluate(ctx: &EvalContext) -> Vec<Advice> {
    let mut advice: Vec<Advice> = Vec::new();

    evaluate_technical(ctx, &mut advice);
    evaluate_loudness(ctx, &mut advice);
    evaluate_dynamics(ctx, &mut advice);
    evaluate_frequency(ctx, &mut advice);
    evaluate_stereo(ctx, &mut advice);
    evaluate_mix_balance(ctx, &mut advice);

    advice.sort_by_key(|a| (a.severity.clone(), format!("{:?}", a.category)));
    advice
}

// ─── Technical ───────────────────────────────────────────────────────────────

fn evaluate_technical(ctx: &EvalContext, out: &mut Vec<Advice>) {
    let m = ctx.master;

    // T1: True peak violation
    if m.true_peak_dbtp.is_finite() && m.true_peak_dbtp > ctx.platform.true_peak_ceil {
        out.push(Advice {
            severity: Severity::Critical,
            category: Category::Technical,
            scope: Scope::MasterBus,
            title: "True peak exceeds platform ceiling".into(),
            detail: format!(
                "Master bus true peak is {:.1} dBTP, but {} requires {:.1} dBTP. \
                 After AAC/MP3 encoding, inter-sample peaks will clip the playback output.",
                m.true_peak_dbtp, ctx.platform.name, ctx.platform.true_peak_ceil
            ),
            fix: format!(
                "Set your brickwall limiter's true peak ceiling to {:.1} dBTP (at least 0.5 dB \
                 below the target). Reduce input gain by {:.1} dB until the meter clears.",
                ctx.platform.true_peak_ceil - 0.5,
                m.true_peak_dbtp - (ctx.platform.true_peak_ceil - 0.5)
            ),
        });
    }

    // T2: Digital clipping
    for (name, track) in ctx.tracks {
        if track.sample_peak_dbfs >= -0.1 {
            out.push(Advice {
                severity: Severity::Critical,
                category: Category::Technical,
                scope: Scope::Track(name.clone()),
                title: format!("Digital clipping on {name}"),
                detail: format!(
                    "{name} is hitting {:.1} dBFS — hard clipping is occurring before the master bus.",
                    track.sample_peak_dbfs
                ),
                fix: "Reduce the fader or insert gain on this track. Add a limiter or clipper \
                      before the master bus send to prevent this from feeding the master chain."
                    .into(),
            });
        }
    }
    if ctx.master.sample_peak_dbfs >= -0.1 {
        out.push(Advice {
            severity: Severity::Critical,
            category: Category::Technical,
            scope: Scope::MasterBus,
            title: "Digital clipping on master bus".into(),
            detail: format!(
                "Master bus sample peak is {:.1} dBFS — clipping is occurring in the master chain.",
                ctx.master.sample_peak_dbfs
            ),
            fix: "Reduce master bus gain. Check that your limiter is the last plugin in the \
                  master chain and that its ceiling is at 0 dBFS or below."
                .into(),
        });
    }

    // T3: Large inter-sample gap (true peak >> sample peak)
    if m.true_peak_dbtp.is_finite()
        && m.sample_peak_dbfs.is_finite()
        && m.true_peak_dbtp > m.sample_peak_dbfs + 3.0
    {
        out.push(Advice {
            severity: Severity::Warning,
            category: Category::Technical,
            scope: Scope::MasterBus,
            title: "High inter-sample peak detected".into(),
            detail: format!(
                "True peak ({:.1} dBTP) is {:.1} dB above sample peak ({:.1} dBFS). \
                 Codec reconstruction will produce peaks higher than the PCM values show.",
                m.true_peak_dbtp,
                m.true_peak_dbtp - m.sample_peak_dbfs,
                m.sample_peak_dbfs
            ),
            fix: "Use a true peak limiter (not just a sample peak limiter) with a -1 dBTP \
                  ceiling. Most modern limiters have a 'true peak' mode — enable it."
                .into(),
        });
    }

    // T4: DC offset
    for (name, track) in ctx.tracks {
        if track.dc_offset.abs() > 0.001 {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::Technical,
                scope: Scope::Track(name.clone()),
                title: format!("DC offset on {name}"),
                detail: format!(
                    "{name} has a DC offset of {:.4}. This wastes headroom and can cause clicks \
                     when crossfading or bouncing.",
                    track.dc_offset
                ),
                fix: "Insert a DC filter (high-pass filter at 5–10 Hz) at the start of this \
                      track's FX chain to remove the offset."
                    .into(),
            });
        }
    }

    // T5: True peak headroom OK (positive feedback)
    if m.true_peak_dbtp.is_finite()
        && m.true_peak_dbtp <= ctx.platform.true_peak_ceil - 0.5
        && m.true_peak_dbtp > ctx.platform.true_peak_ceil - 3.0
    {
        out.push(Advice {
            severity: Severity::Good,
            category: Category::Technical,
            scope: Scope::MasterBus,
            title: "True peak within target".into(),
            detail: format!(
                "Master true peak is {:.1} dBTP — comfortably within the {} ceiling of {:.1} dBTP.",
                m.true_peak_dbtp, ctx.platform.name, ctx.platform.true_peak_ceil
            ),
            fix: String::new(),
        });
    }
}

// ─── Loudness ─────────────────────────────────────────────────────────────────

fn evaluate_loudness(ctx: &EvalContext, out: &mut Vec<Advice>) {
    let m = ctx.master;
    let target = ctx.platform.lufs_target;

    if !target.is_finite() {
        return; // SoundCloud — no normalization target
    }

    if m.lufs_integrated.is_finite() {
        let delta = m.lufs_integrated - target;

        if delta > 1.0 {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::Loudness,
                scope: Scope::MasterBus,
                title: format!("Master too loud for {}", ctx.platform.name),
                detail: format!(
                    "Integrated loudness is {:.1} LUFS, which is {:.1} LU above the {} target \
                     of {:.1} LUFS. The platform will turn your track down by this amount.",
                    m.lufs_integrated, delta, ctx.platform.name, target
                ),
                fix: format!(
                    "Reduce master bus output gain by {:.1} dB, then re-check true peak.",
                    delta
                ),
            });
        } else if delta < -3.0 {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::Loudness,
                scope: Scope::MasterBus,
                title: format!("Master significantly quiet for {}", ctx.platform.name),
                detail: format!(
                    "Integrated loudness is {:.1} LUFS, which is {:.1} LU below the {} target. \
                     The platform will turn your track up, reducing signal-to-noise ratio.",
                    m.lufs_integrated, -delta, ctx.platform.name
                ),
                fix: format!(
                    "Increase master bus input gain by approximately {:.1} dB and verify true \
                     peak stays within {:.1} dBTP.",
                    -delta - 0.5,
                    ctx.platform.true_peak_ceil
                ),
            });
        } else if delta.abs() <= 1.0 {
            out.push(Advice {
                severity: Severity::Good,
                category: Category::Loudness,
                scope: Scope::MasterBus,
                title: "Loudness on target".into(),
                detail: format!(
                    "Integrated loudness is {:.1} LUFS — within 1 LU of the {} target ({:.1} LUFS).",
                    m.lufs_integrated, ctx.platform.name, target
                ),
                fix: String::new(),
            });
        }
    }

    // L4: Individual track loudness outlier
    if m.lufs_integrated.is_finite() {
        for (name, track) in ctx.tracks {
            if track.lufs_integrated.is_finite()
                && track.lufs_integrated > m.lufs_integrated + 6.0
            {
                out.push(Advice {
                    severity: Severity::Warning,
                    category: Category::Loudness,
                    scope: Scope::Track(name.clone()),
                    title: format!("{name} is significantly louder than the mix"),
                    detail: format!(
                        "{name} is at {:.1} LUFS — {:.1} LU louder than the master bus average \
                         ({:.1} LUFS). It is likely dominating the mix.",
                        track.lufs_integrated,
                        track.lufs_integrated - m.lufs_integrated,
                        m.lufs_integrated
                    ),
                    fix: format!(
                        "Consider reducing {name}'s fader by {:.0}–{:.0} dB, or apply more \
                         dynamic control (compression/limiting) to this track.",
                        (track.lufs_integrated - m.lufs_integrated) * 0.5,
                        track.lufs_integrated - m.lufs_integrated
                    ),
                });
            }
        }
    }
}

// ─── Dynamics ─────────────────────────────────────────────────────────────────

fn evaluate_dynamics(ctx: &EvalContext, out: &mut Vec<Advice>) {
    let m = ctx.master;

    if m.psr_min.is_finite() {
        if m.psr_min < 6.0 {
            out.push(Advice {
                severity: Severity::Critical,
                category: Category::Dynamics,
                scope: Scope::MasterBus,
                title: "Severe over-compression".into(),
                detail: format!(
                    "Peak-to-Short-term loudness Ratio (PSR) drops to {:.1} LU during the loudest \
                     sections. This means the limiter is crushing transients flat — kick and snare \
                     attacks will sound weak and fatiguing.",
                    m.psr_min
                ),
                fix: "Reduce the limiter input gain until PSR stays above 8 LU in the loudest \
                      chorus. Consider a clipper before the limiter to handle peaks transparently, \
                      allowing the limiter to work less hard."
                    .into(),
            });
        } else if m.psr_min < ctx.genre.psr_min {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::Dynamics,
                scope: Scope::MasterBus,
                title: "Over-compression for genre".into(),
                detail: format!(
                    "PSR is {:.1} LU during loud sections — below the {:.1} LU typical for {}. \
                     The master sounds more compressed than expected for this genre.",
                    m.psr_min, ctx.genre.psr_min, ctx.genre.name
                ),
                fix: "Reduce limiter input gain by 1–2 dB and listen to the loudest section. \
                      The transients and punch should become more audible."
                    .into(),
            });
        } else if m.psr_min >= ctx.genre.psr_min + 2.0 {
            out.push(Advice {
                severity: Severity::Good,
                category: Category::Dynamics,
                scope: Scope::MasterBus,
                title: "Good dynamic range".into(),
                detail: format!(
                    "PSR is {:.1} LU — healthy dynamics for {}. Transients are preserved.",
                    m.psr_min, ctx.genre.name
                ),
                fix: String::new(),
            });
        }
    }

    if m.plr.is_finite() && m.plr < ctx.genre.plr_min {
        out.push(Advice {
            severity: Severity::Warning,
            category: Category::Dynamics,
            scope: Scope::MasterBus,
            title: "Low peak-to-loudness ratio (PLR)".into(),
            detail: format!(
                "PLR is {:.1} LU — below the {:.1} LU minimum for {}. The overall dynamic \
                 envelope is narrow.",
                m.plr, ctx.genre.plr_min, ctx.genre.name
            ),
            fix: "Check your master bus compression ratio. A PLR below the genre minimum usually \
                  means a compressor is working too hard before the limiter stage."
                .into(),
        });
    }
}

// ─── Frequency Balance ────────────────────────────────────────────────────────

fn evaluate_frequency(ctx: &EvalContext, out: &mut Vec<Advice>) {
    let b = &ctx.master.bands_dbfs;

    // Only run frequency rules if we have meaningful data
    if b.iter().all(|&v| v <= -100.0) {
        return;
    }

    // Normalize bands relative to the 1kHz (index 5) band for comparison
    let ref_band = b[5];
    if !ref_band.is_finite() || ref_band < -80.0 {
        return;
    }

    let b_rel: [f32; 10] = std::array::from_fn(|i| b[i] - ref_band);
    let g = &ctx.genre.bands_rel;

    // F1: Low-end mud — 125+250Hz significantly louder than 500Hz–2kHz
    let low_mid_avg = (b_rel[2] + b_rel[3]) / 2.0;
    let presence_avg = (b_rel[4] + b_rel[5]) / 2.0;
    let genre_low_mid = (g[2] + g[3]) / 2.0;
    let genre_presence = (g[4] + g[5]) / 2.0;
    let mud_excess = (low_mid_avg - presence_avg) - (genre_low_mid - genre_presence);
    if mud_excess > 4.0 {
        out.push(Advice {
            severity: Severity::Warning,
            category: Category::FrequencyBalance,
            scope: Scope::MasterBus,
            title: "Low-end mud detected".into(),
            detail: format!(
                "The 125–250 Hz range is {:.1} dB louder relative to the 500 Hz–1 kHz presence \
                 range than expected for {}. The mix may sound boomy or congested in the low-mids.",
                mud_excess, ctx.genre.name
            ),
            fix: "Apply a gentle cut of 2–4 dB centered at 200–300 Hz with a wide Q (0.7) on \
                  the master bus. Then check individual bass/kick tracks — one is likely boosted \
                  in this range."
                .into(),
        });
    }

    // F2: Sub-bass excess
    if b_rel[0] > b_rel[1] + 2.0 {
        out.push(Advice {
            severity: Severity::Warning,
            category: Category::FrequencyBalance,
            scope: Scope::MasterBus,
            title: "Excessive sub-bass energy".into(),
            detail: format!(
                "Energy at 31.5 Hz is {:.1} dB above the 63 Hz band. Content below 40 Hz is \
                 inaudible on most speakers but consumes loudness budget.",
                b_rel[0] - b_rel[1]
            ),
            fix: "Apply a high-pass filter at 30–35 Hz on the master bus (or on bass/kick tracks) \
                  to remove inaudible sub-rumble. This frees headroom and allows the limiter to \
                  work less hard."
                .into(),
        });
    }

    // F3: Harshness (4kHz band)
    let harshness_excess = b_rel[7] - g[7];
    if harshness_excess > 4.0 {
        out.push(Advice {
            severity: Severity::Warning,
            category: Category::FrequencyBalance,
            scope: Scope::MasterBus,
            title: "Harshness / listener fatigue risk".into(),
            detail: format!(
                "The 4 kHz octave band is {:.1} dB above the {} reference. Excess energy in \
                 3–6 kHz causes listener fatigue and can make the mix feel harsh.",
                harshness_excess, ctx.genre.name
            ),
            fix: "Apply a gentle dip of 1.5–3 dB at 3–5 kHz on the master bus using a wide \
                  bell EQ (Q ~1.0). Check which tracks are contributing — often overheads, \
                  guitars, or a poorly EQed vocal."
                .into(),
        });
    }

    // F5: Missing presence
    let presence_deficit = g[6] - b_rel[6];
    if presence_deficit > 4.0 {
        out.push(Advice {
            severity: Severity::Suggestion,
            category: Category::FrequencyBalance,
            scope: Scope::MasterBus,
            title: "Missing upper presence".into(),
            detail: format!(
                "The 2 kHz band is {:.1} dB below the {} reference. Vocals and lead instruments \
                 may sound distant or lacking definition.",
                presence_deficit, ctx.genre.name
            ),
            fix: "Boost 2–3 kHz by 1–2 dB on the master bus. Check whether the vocal track \
                  has sufficient presence boost in its own EQ."
                .into(),
        });
    }

    // F6: Missing air
    let air_deficit = g[8] - b_rel[8];
    if air_deficit > 4.0 {
        out.push(Advice {
            severity: Severity::Suggestion,
            category: Category::FrequencyBalance,
            scope: Scope::MasterBus,
            title: "Missing air / top-end sheen".into(),
            detail: format!(
                "The 8 kHz band is {:.1} dB below the {} reference. The master may sound \
                 dull or overly dark.",
                air_deficit, ctx.genre.name
            ),
            fix: "Apply a high-shelf boost of 1–2 dB at 10 kHz on the master bus. An air-band \
                  EQ (e.g. Neve 33609 style shelf) is ideal for this."
                .into(),
        });
    }

    // F7: Good frequency balance
    let max_deviation: f32 = (0..10)
        .map(|i| (b_rel[i] - g[i]).abs())
        .fold(0.0f32, f32::max);
    if max_deviation < 3.0 {
        out.push(Advice {
            severity: Severity::Good,
            category: Category::FrequencyBalance,
            scope: Scope::MasterBus,
            title: "Good frequency balance".into(),
            detail: format!(
                "All 10 octave bands are within {:.1} dB of the {} reference curve. \
                 The tonal balance is well-suited to the genre.",
                max_deviation, ctx.genre.name
            ),
            fix: String::new(),
        });
    }
}

// ─── Stereo ───────────────────────────────────────────────────────────────────

fn evaluate_stereo(ctx: &EvalContext, out: &mut Vec<Advice>) {
    let m = ctx.master;
    let corr = m.correlation;

    if corr < 0.0 {
        out.push(Advice {
            severity: Severity::Critical,
            category: Category::Stereo,
            scope: Scope::MasterBus,
            title: "Phase inversion — mix will cancel in mono".into(),
            detail: format!(
                "Stereo correlation is {:.2}. A negative correlation means the left and right \
                 channels are partially out of phase — the mix will thin out or disappear \
                 entirely when summed to mono.",
                corr
            ),
            fix: "Check for inverted phase on individual tracks. Use a phase correlation meter \
                  per track to identify the culprit. Common causes: a flipped polarity button, \
                  a stereo widener set too aggressively, or mid/side processing with incorrect \
                  M/S encoding."
                .into(),
        });
    } else if corr < 0.2 {
        out.push(Advice {
            severity: Severity::Warning,
            category: Category::Stereo,
            scope: Scope::MasterBus,
            title: "Mono compatibility risk".into(),
            detail: format!(
                "Stereo correlation is {:.2}. Significant content will cancel when played in \
                 mono (phone speakers, club PA systems, many streaming contexts).",
                corr
            ),
            fix: "Check your stereo wideners and M/S processors. Listen in mono using a \
                  Utility plugin (sum to mono). Identify which elements disappear and reduce \
                  widening on those tracks."
                .into(),
        });
    } else if corr > 0.95 {
        out.push(Advice {
            severity: Severity::Suggestion,
            category: Category::Stereo,
            scope: Scope::MasterBus,
            title: "Very narrow stereo field".into(),
            detail: format!(
                "Stereo correlation is {:.2} — the mix is nearly mono. While appropriate for \
                 some genres, this may feel narrow on headphones.",
                corr
            ),
            fix: "Consider adding stereo width to reverb returns, pads, or guitars using subtle \
                  panning or a stereo imager. Keep bass and kick near-mono."
                .into(),
        });
    } else if (0.3..=0.7).contains(&corr) {
        out.push(Advice {
            severity: Severity::Good,
            category: Category::Stereo,
            scope: Scope::MasterBus,
            title: "Healthy stereo field".into(),
            detail: format!(
                "Stereo correlation is {:.2} — a well-balanced stereo image that translates \
                 well to both stereo and mono playback.",
                corr
            ),
            fix: String::new(),
        });
    }
}

// ─── Mix Balance ──────────────────────────────────────────────────────────────

fn evaluate_mix_balance(ctx: &EvalContext, out: &mut Vec<Advice>) {
    if ctx.tracks.len() < 2 {
        return;
    }

    // M1: Low-end competition — multiple tracks with heavy bass content
    let bass_heavy: Vec<&str> = ctx
        .tracks
        .iter()
        .filter(|(_, t)| t.bands_dbfs[1] > -20.0)
        .map(|(n, _)| n.as_str())
        .collect();
    if bass_heavy.len() >= 3 {
        out.push(Advice {
            severity: Severity::Warning,
            category: Category::MixBalance,
            scope: Scope::AllTracks,
            title: "Low-end competition between tracks".into(),
            detail: format!(
                "{} tracks have significant 63 Hz energy: {}. Competing low-end content causes \
                 muddiness and makes limiting harder.",
                bass_heavy.len(),
                bass_heavy.join(", ")
            ),
            fix: "Use high-pass filters to clear unnecessary low-end from non-bass tracks. \
                  Only the kick, bass, and possibly a pad should have significant energy below \
                  100 Hz. Use sidechaining or dynamic EQ to duck bass tracks when kick hits."
                .into(),
        });
    }

    // M2: Frequency masking — two tracks dominant in the same band
    let band_names = ["31Hz", "63Hz", "125Hz", "250Hz", "500Hz", "1kHz", "2kHz", "4kHz", "8kHz", "16kHz"];
    for band_idx in 2..8usize {
        let dominant: Vec<(&str, f32)> = ctx
            .tracks
            .iter()
            .filter(|(_, t)| t.bands_dbfs[band_idx] > -30.0)
            .map(|(n, t)| (n.as_str(), t.bands_dbfs[band_idx]))
            .collect();
        if dominant.len() >= 2 {
            let max = dominant.iter().map(|&(_, v)| v).fold(f32::NEG_INFINITY, f32::max);
            let competing: Vec<&str> = dominant
                .iter()
                .filter(|&&(_, v)| v >= max - 3.0)
                .map(|&(n, _)| n)
                .collect();
            if competing.len() >= 2 {
                out.push(Advice {
                    severity: Severity::Suggestion,
                    category: Category::MixBalance,
                    scope: Scope::AllTracks,
                    title: format!("Frequency masking around {}", band_names[band_idx]),
                    detail: format!(
                        "{} are competing in the {} band. Tracks with similar energy in the \
                         same frequency range mask each other, reducing clarity.",
                        competing.join(" and "),
                        band_names[band_idx]
                    ),
                    fix: format!(
                        "Use EQ to carve space: boost one track slightly at {} and cut the \
                         other by the same amount. This creates separation without removing energy.",
                        band_names[band_idx]
                    ),
                });
                break; // One masking suggestion per analysis is enough
            }
        }
    }
}
