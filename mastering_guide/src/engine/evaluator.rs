use crate::analysis::frame::TrackFrame;
use crate::engine::advice::{Advice, Category, FixAction, Scope, Severity};
use crate::engine::genres::GenreCurve;
use crate::engine::platforms::PlatformTarget;
use crate::ipc::registry::TrackEntry;
use crate::params::TrackRole;

pub struct EvalContext<'a> {
    pub master: &'a TrackFrame,
    pub tracks: &'a [TrackEntry],
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
            action: Some(FixAction::AdjustVolume {
                track_name: None,
                delta_db: -(m.true_peak_dbtp - (ctx.platform.true_peak_ceil - 0.5)),
            }),
        });
    }

    // T2: Digital clipping
    for TrackEntry { name, frame: track, .. } in ctx.tracks {
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
                action: Some(FixAction::AdjustVolume {
                    track_name: Some(name.clone()),
                    delta_db: -1.0 - track.sample_peak_dbfs,
                }),
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
            action: None,
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
            action: None,
        });
    }

    // T4: DC offset
    for TrackEntry { name, frame: track, .. } in ctx.tracks {
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
                action: None,
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
            action: None,
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
                action: Some(FixAction::AdjustVolume {
                    track_name: None,
                    delta_db: -delta,
                }),
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
                action: Some(FixAction::AdjustVolume {
                    track_name: None,
                    delta_db: -delta - 0.5,
                }),
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
                action: None,
            });
        }
    }

    // L4: Individual track loudness outlier
    if m.lufs_integrated.is_finite() {
        for TrackEntry { name, frame: track, .. } in ctx.tracks {
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
                    action: Some(FixAction::AdjustVolume {
                        track_name: Some(name.clone()),
                        delta_db: -((track.lufs_integrated - m.lufs_integrated) * 0.5),
                    }),
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
                action: None,
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
                action: None,
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
                action: None,
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
            action: None,
        });
    }

    // D4/D5: Macrodynamics — p95-p5 spread of short-term LUFS over the last
    // ~60 s. Distinguishes section-to-section contrast from sample-level
    // microdynamics (PSR/PLR). Lapatas's "macrodynamics" stage of the
    // mastering process.
    let macro_lu = m.macrodynamics_lu;
    if macro_lu.is_finite() {
        if macro_lu < 2.0 {
            out.push(Advice {
                severity: Severity::Suggestion,
                category: Category::Dynamics,
                scope: Scope::MasterBus,
                title: "Flat section contrast (macrodynamics)".into(),
                detail: format!(
                    "Short-term LUFS only varies by {:.1} LU between the quietest \
                     and loudest sections of the last minute. The arrangement may \
                     be missing dynamic contrast — verses, choruses and bridges \
                     are sitting at almost the same level.",
                    macro_lu
                ),
                fix: "Use mix-bus volume automation to drop verses and pre-choruses \
                      by 1–3 dB rather than relying on bus compression to create \
                      the lift into the chorus."
                    .into(),
                action: None,
            });
        } else if macro_lu > 12.0 {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::Dynamics,
                scope: Scope::MasterBus,
                title: "Extreme section jumps (macrodynamics)".into(),
                detail: format!(
                    "Short-term LUFS varies by {:.1} LU across recent sections. \
                     Loudness deltas above ~12 LU usually mean automation or \
                     gain-staging mistakes rather than musical intent — quiet \
                     sections will be inaudible on streaming services that \
                     normalise to the loudest segment.",
                    macro_lu
                ),
                fix: "Check master fader automation, sidechain ducking depth and \
                      any envelopes pulling the mix down hard. Aim for 4–8 LU of \
                      verse-to-chorus contrast for most modern genres."
                    .into(),
                action: None,
            });
        } else if (4.0..=8.0).contains(&macro_lu) {
            out.push(Advice {
                severity: Severity::Good,
                category: Category::Dynamics,
                scope: Scope::MasterBus,
                title: "Healthy section contrast".into(),
                detail: format!(
                    "Short-term LUFS spread is {:.1} LU — sections have musical \
                     contrast without extreme jumps.",
                    macro_lu
                ),
                fix: String::new(),
                action: None,
            });
        }
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
            action: None,
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
            action: None,
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
            action: None,
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
            action: None,
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
            action: None,
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
            action: None,
        });
    }

    // F8: Spectral tilt out of range. A typical professional master sits
    // around -3 to -5 dB/oct (close to pink-noise slope). Flatter than -2 is
    // usually harsh; steeper than -7 is dull/muffled.
    let tilt = ctx.master.spectral_tilt_db_per_oct;
    if tilt.is_finite() && ctx.master.lufs_integrated.is_finite() {
        if tilt > -1.5 {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::FrequencyBalance,
                scope: Scope::MasterBus,
                title: "Spectral tilt too flat / bright".into(),
                detail: format!(
                    "Master spectrum slopes at {:+.1} dB/oct. A balanced master \
                     typically sits around -3 to -5 dB/oct (pink-noise slope). \
                     A flatter spectrum tends to sound harsh and fatiguing.",
                    tilt
                ),
                fix: "Apply a gentle high-shelf cut of 1–2 dB above 6 kHz, or \
                      a Baxandall-style tilt EQ pulling the top down by ~1 dB."
                    .into(),
                action: None,
            });
        } else if tilt < -7.0 {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::FrequencyBalance,
                scope: Scope::MasterBus,
                title: "Spectral tilt too dark".into(),
                detail: format!(
                    "Master spectrum slopes at {:+.1} dB/oct. A balanced master \
                     typically sits around -3 to -5 dB/oct. A steeper roll-off \
                     leaves the mix sounding muffled or veiled on consumer playback.",
                    tilt
                ),
                fix: "Apply a 1–2 dB high-shelf boost above 6–8 kHz, or pull \
                      down 200–400 Hz by 1–2 dB to reveal the top end."
                    .into(),
                action: None,
            });
        } else if (-5.0..=-2.5).contains(&tilt) {
            out.push(Advice {
                severity: Severity::Good,
                category: Category::FrequencyBalance,
                scope: Scope::MasterBus,
                title: "Healthy spectral tilt".into(),
                detail: format!(
                    "Master tilt is {:+.1} dB/oct — close to the pink-noise \
                     reference slope expected of professional masters.",
                    tilt
                ),
                fix: String::new(),
                action: None,
            });
        }
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
            action: None,
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
            action: None,
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
            action: None,
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
            action: None,
        });
    }

    // S5: True mono-compatibility loss in LU. Compares stereo-integrated
    // LUFS with the integrated LUFS of the (L+R)/2 summed signal — this is a
    // direct measure of how much loudness disappears when the mix is folded
    // to mono, which is what phone speakers and many club PAs actually do.
    if m.lufs_integrated.is_finite()
        && m.lufs_integrated_mono.is_finite()
        && m.lufs_integrated > -60.0
    {
        let mono_loss = m.lufs_integrated - m.lufs_integrated_mono;
        if mono_loss > 4.0 {
            out.push(Advice {
                severity: Severity::Critical,
                category: Category::Stereo,
                scope: Scope::MasterBus,
                title: "Severe loudness loss in mono".into(),
                detail: format!(
                    "Mono-summed loudness is {:.1} LU below the stereo loudness — \
                     significant content is cancelling when summed to mono. Phone \
                     speakers, many laptops, and most club PA systems will sound \
                     dramatically thinner than your stereo monitors.",
                    mono_loss
                ),
                fix: "Solo each track and listen in mono using a Utility plugin set \
                      to sum L+R. Look for stereo wideners, M/S processors, or \
                      out-of-phase reverbs on the most affected tracks and reduce \
                      their width."
                    .into(),
                action: None,
            });
        } else if mono_loss > 2.0 {
            out.push(Advice {
                severity: Severity::Warning,
                category: Category::Stereo,
                scope: Scope::MasterBus,
                title: "Mono compatibility loss".into(),
                detail: format!(
                    "Mono-summed loudness is {:.1} LU below the stereo integrated \
                     loudness. Listeners on phones and mono playback systems will \
                     hear a noticeably weaker mix.",
                    mono_loss
                ),
                fix: "Check the spread on stereo wideners and any reverbs that have \
                      a width control. Aim for mono loss under 2 LU."
                    .into(),
                action: None,
            });
        }
    }

    // S6: Bass too wide. Lapatas: keep bass near-mono — low-frequency stereo
    // content collapses badly on small speakers and wastes limiter headroom.
    // Flags low correlation in the 31 Hz or 63 Hz band when that band has
    // audible energy.
    let sub_corr = m.bands_corr[0];
    let bass_corr = m.bands_corr[1];
    let bass_db = m.bands_dbfs[1];
    let worst_corr = sub_corr.min(bass_corr);
    let bass_audible = bass_db.is_finite() && bass_db > -40.0;
    if bass_audible && worst_corr < 0.5 {
        let band_name = if sub_corr < bass_corr { "31 Hz" } else { "63 Hz" };
        out.push(Advice {
            severity: if worst_corr < 0.0 { Severity::Critical } else { Severity::Warning },
            category: Category::Stereo,
            scope: Scope::MasterBus,
            title: "Bass too wide for safe mono playback".into(),
            detail: format!(
                "Per-band correlation at {band_name} is {:.2} — the low end is \
                 substantially out of phase between channels. Low frequencies \
                 should sit close to mono; wide bass collapses on phone speakers \
                 and forces the limiter to chase phantom peaks.",
                worst_corr
            ),
            fix: "Apply a stereo-narrowing or 'mono below 120 Hz' filter on the \
                  master bus, or check individual bass / sub tracks for stereo \
                  wideners and out-of-phase doubling. Bitwig's Mid-Side Split \
                  device followed by a high-pass on the side channel is a clean \
                  way to do this."
                .into(),
            action: None,
        });
    } else if bass_audible && worst_corr >= 0.85 {
        out.push(Advice {
            severity: Severity::Good,
            category: Category::Stereo,
            scope: Scope::MasterBus,
            title: "Bass is well-centred".into(),
            detail: format!(
                "Low-band correlation is {:.2} — the bass is solidly mono-compatible.",
                worst_corr
            ),
            fix: String::new(),
            action: None,
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
        .filter(|e| e.frame.bands_dbfs[1] > -20.0)
        .map(|e| e.name.as_str())
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
            action: None,
        });
    }

    // M2: Frequency masking — two tracks dominant in the same band
    let band_names = ["31Hz", "63Hz", "125Hz", "250Hz", "500Hz", "1kHz", "2kHz", "4kHz", "8kHz", "16kHz"];
    for band_idx in 2..8usize {
        let dominant: Vec<(&str, f32)> = ctx
            .tracks
            .iter()
            .filter(|e| e.frame.bands_dbfs[band_idx] > -30.0)
            .map(|e| (e.name.as_str(), e.frame.bands_dbfs[band_idx]))
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
                    action: None,
                });
                break; // One masking suggestion per analysis is enough
            }
        }
    }

    // ── Role-aware rules (skip tracks tagged Auto — undecided role). ────
    let by_role = |role: TrackRole| -> Vec<&TrackEntry> {
        ctx.tracks.iter().filter(|e| e.role == role).collect()
    };

    let bass_tracks = by_role(TrackRole::Bass);
    let drum_tracks = by_role(TrackRole::Drums);
    let vocal_tracks = by_role(TrackRole::Vocal);
    let harm_tracks = by_role(TrackRole::Harm);

    // M3: Bass vs bass-drum collision in 63–125 Hz. Lapatas's canonical
    // example — bass drum lives at 80–120 Hz, bass guitar at 80–300 Hz, both
    // need careful EQ pocketing or one will hide the other. Bands 1 (63 Hz)
    // and 2 (125 Hz) cover this overlap.
    if !bass_tracks.is_empty() && !drum_tracks.is_empty() {
        for band_idx in 1..=2 {
            let any_bass_loud = bass_tracks.iter().any(|e| e.frame.bands_dbfs[band_idx] > -25.0);
            let any_drum_loud = drum_tracks.iter().any(|e| e.frame.bands_dbfs[band_idx] > -25.0);
            if any_bass_loud && any_drum_loud {
                out.push(Advice {
                    severity: Severity::Suggestion,
                    category: Category::MixBalance,
                    scope: Scope::AllTracks,
                    title: format!("Bass and drums overlap at {}", band_names[band_idx]),
                    detail: format!(
                        "Both the bass and the drums have strong energy in the {} \
                         band. This is the classic kick-vs-bass collision and is \
                         the single most common cause of a muddy low end.",
                        band_names[band_idx]
                    ),
                    fix: "Pick which instrument owns this band and pocket the other \
                          out of it: e.g. cut 100 Hz on the bass to let the kick \
                          through, then boost 60 Hz on the bass for fundamental. \
                          Sidechaining the bass to the kick can also work."
                        .into(),
                    action: None,
                });
                break;
            }
        }
    }

    // M4: Vocal vs harmony masking in the 500 Hz – 2 kHz body range. Lead
    // vocals need clarity in the 1–3 kHz presence range; harmonies sitting
    // on the same frequencies will rob the lead of definition.
    if !vocal_tracks.is_empty() && !harm_tracks.is_empty() {
        let vocal_loud_in = |idx: usize| vocal_tracks.iter().any(|e| e.frame.bands_dbfs[idx] > -25.0);
        let harm_loud_in = |idx: usize| harm_tracks.iter().any(|e| e.frame.bands_dbfs[idx] > -25.0);
        // band 5 = 1 kHz, band 6 = 2 kHz — the vocal presence region.
        if (vocal_loud_in(5) && harm_loud_in(5)) || (vocal_loud_in(6) && harm_loud_in(6)) {
            out.push(Advice {
                severity: Severity::Suggestion,
                category: Category::MixBalance,
                scope: Scope::AllTracks,
                title: "Lead vocal and harmonies share the presence band".into(),
                detail: "Lead vocal and harmony tracks are both energetic in the \
                         1–2 kHz range. Listeners place vocals at the front of the \
                         mix using this band — when harmonies share it, the lead \
                         loses definition.".into(),
                fix: "Apply a gentle 2–3 dB cut on harmonies around 2–3 kHz, or \
                      side-chain a dynamic EQ on the harmonies that ducks when the \
                      lead vocal is present. Keep harmonies wider in the stereo \
                      field so they read as 'around' the lead, not 'on top of' it."
                    .into(),
                action: None,
            });
        }
    }

    // M5: Multiple Bass-role tracks. Phase / level competition is hard to
    // manage with two simultaneous bass instruments unless they're EQ-split
    // (e.g. sub-bass + mid-bass).
    if bass_tracks.len() >= 2 {
        let names: Vec<&str> = bass_tracks.iter().map(|e| e.name.as_str()).collect();
        out.push(Advice {
            severity: Severity::Suggestion,
            category: Category::MixBalance,
            scope: Scope::AllTracks,
            title: "Multiple bass tracks".into(),
            detail: format!(
                "{} tracks are tagged Bass: {}. Stacked bass instruments without \
                 frequency separation usually fight each other and produce phase \
                 cancellation in the low end.",
                bass_tracks.len(),
                names.join(", ")
            ),
            fix: "Split the spectrum: one bass owns sub (below ~80 Hz, near-mono), \
                  the other owns mid-bass (80–250 Hz). Use complementary high-pass \
                  / low-pass filters and check phase relationship between them."
                .into(),
            action: None,
        });
    }
}
