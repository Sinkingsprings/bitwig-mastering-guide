mod spectrum;

use crate::analysis::frame::{FrameReader, TrackFrame};
use crate::engine::advice::Advice;
use crate::engine::evaluator::{EvalContext, evaluate};
use crate::engine::genres::genre_for;
use crate::engine::platforms::platform_for;
use crate::ipc::Registry;
use crate::ipc::registry::TrackEntry;
use crate::ipc::{GilliganState, spawn_gilligan};
use crate::params::{MasteringGuideParams, ModeParam};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Logical (unscaled) base size of the editor window. Kept here so the
/// initial `from_size` and the on-open resize request use the same numbers.
pub const BASE_WIDTH: u32 = 430;
pub const BASE_HEIGHT: u32 = 590;

// ─── Editor state shared across repaints ─────────────────────────────────────

struct MasterState {
    advice: Vec<Advice>,
    last_tracks: Vec<TrackEntry>,
    last_analysis: Option<Instant>,
    auto_analyze: bool,
    show_track_spectrum: bool,
    show_help: bool,
    reference: Option<TrackFrame>,
    last_applied_scale: f32,
    /// Shared handle to the Gilligan IPC client state.
    gilligan: Arc<Mutex<GilliganState>>,
}

impl MasterState {
    fn new(gilligan: Arc<Mutex<GilliganState>>) -> Self {
        Self {
            advice: Vec::new(),
            last_tracks: Vec::new(),
            last_analysis: None,
            auto_analyze: false,
            show_track_spectrum: true,
            show_help: false,
            reference: None,
            last_applied_scale: 0.0,
            gilligan,
        }
    }
}

fn scaled_size(scale: f32) -> (u32, u32) {
    (
        (BASE_WIDTH as f32 * scale).round() as u32,
        (BASE_HEIGHT as f32 * scale).round() as u32,
    )
}

// ─── Entry point ─────────────────────────────────────────────────────────────

pub fn create_editor(
    params: Arc<MasteringGuideParams>,
    frame_reader: FrameReader,
    registry: Option<Arc<Registry>>,
    track_name: String,
) -> Option<Box<dyn Editor>> {
    let egui_state = params.editor_state.clone();
    let gilligan: Arc<Mutex<GilliganState>> = Arc::new(Mutex::new(GilliganState::default()));
    spawn_gilligan(gilligan.clone());
    let state: Arc<Mutex<MasterState>> = Arc::new(Mutex::new(MasterState::new(gilligan)));
    let scale_egui_state = egui_state.clone();

    create_egui_editor(
        egui_state,
        (frame_reader, registry, state, track_name),
        move |ctx, _| {
            apply_theme(ctx);
        },
        move |ctx, setter, (frame_reader, registry, state, track_name)| {
            // ── UI scale ────────────────────────────────────────────────
            // Apply zoom + matching window size whenever the user changes
            // the UI Scale param (or on the first frame after each open).
            // `last_applied_scale` is reset to 0.0 in MasterState::default,
            // so the very first frame after spawn always re-applies — this
            // also handles hosts (Bitwig) that cache plugin window sizes.
            {
                let target_scale = params.ui_scale.value().factor();
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if (s.last_applied_scale - target_scale).abs() > f32::EPSILON {
                    ctx.set_zoom_factor(target_scale);
                    let (w, h) = scaled_size(target_scale);
                    scale_egui_state.set_requested_size(w, h);
                    s.last_applied_scale = target_scale;
                }
            }

            let frame = frame_reader.read();
            let is_master = params.mode.value() == ModeParam::Master;

            // Write our frame into the process-global registry every repaint.
            if let Some(ref reg) = registry {
                reg.write(track_name, params.track_role.value(), &frame);
            }

            // Auto-analyze trigger
            {
                if let Ok(mut s) = state.lock() {
                    if s.auto_analyze {
                        let stale = s
                            .last_analysis
                            .map(|t| t.elapsed() >= Duration::from_secs(5))
                            .unwrap_or(true);
                        if stale {
                            run_analysis(&mut s, &frame, registry, &params);
                        }
                    }
                }
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                render_header(ui, setter, &params, registry, state);
                ui.add_space(2.0);
                ui.separator();

                if is_master {
                    render_master(ui, setter, &params, &frame, registry, state);
                } else {
                    render_track(ui, setter, &params, &frame);
                }
            });

            // Floating help window — rendered outside CentralPanel so it overlaps.
            let mut show_help = state.lock().unwrap_or_else(|p| p.into_inner()).show_help;
            if show_help {
                egui::Window::new("Mastering Guide — Help")
                    .open(&mut show_help)
                    .collapsible(false)
                    .resizable(true)
                    .default_size([400.0, 480.0])
                    .show(ctx, |ui| {
                        render_help(ui);
                    });
                state.lock().unwrap_or_else(|p| p.into_inner()).show_help = show_help;
            }

        },
    )
}

// ─── Header ──────────────────────────────────────────────────────────────────

fn render_header(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &Arc<MasteringGuideParams>,
    registry: &Option<Arc<Registry>>,
    state: &Arc<Mutex<MasterState>>,
) {
    ui.horizontal(|ui| {
        // Mode value label on the left — mirrors where MASTERING GUIDE used to live,
        // giving the header a left anchor so it isn't right-biased.
        ui.label(
            egui::RichText::new(format!("{}", params.mode.value()))
                .size(11.0)
                .color(egui::Color32::from_rgb(140, 185, 255))
                .strong(),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Help button
            let help_clicked = ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(" ? ")
                            .size(10.0)
                            .color(egui::Color32::from_rgb(140, 185, 255)),
                    )
                    .min_size(egui::vec2(18.0, 16.0)),
                )
                .on_hover_text("Open user guide")
                .clicked();
            if help_clicked {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.show_help = !s.show_help;
            }

            if let Some(ref reg) = registry {
                ui.label(
                    egui::RichText::new(format!("#{}", reg.slot_index() + 1))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(80, 80, 90)),
                )
                .on_hover_text("Registry slot index for this instance");
            }
            ui.add(
                widgets::ParamSlider::for_param(&params.mode, setter)
                    .without_value()
                    .with_width(70.0),
            )
            .on_hover_text(
                "Track: sends analysis data to any Master instance in the session.\n\
                 Master: collects track data and generates advice.",
            );
        });
    });
}

// ─── Master view ─────────────────────────────────────────────────────────────

fn render_master(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &Arc<MasteringGuideParams>,
    master: &TrackFrame,
    registry: &Option<Arc<Registry>>,
    state: &Arc<Mutex<MasterState>>,
) {
    let genre = genre_for(&params.genre.value());

    // ── Master bus summary row ──────────────────────────────────────────
    ui.add_space(3.0);
    section_label(ui, "MASTER BUS");
    egui::Grid::new("master_meters")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            compact_meter(
                ui,
                "Int LUFS",
                master.lufs_integrated,
                -24.0,
                -8.0,
                "LUFS",
                "Integrated (long-term) loudness of the master bus.\n\
                 Streaming targets: Spotify/YouTube −14, Apple Music −16,\n\
                 Amazon −14, Tidal −14, Broadcast (EBU R128) −23 LUFS.",
            );
            compact_meter(
                ui,
                "True Peak",
                master.true_peak_dbtp,
                -6.0,
                0.0,
                "dBTP",
                "Maximum inter-sample peak level.\n\
                 Keep below −1.0 dBTP for streaming to prevent clipping\n\
                 after codec encode/decode. Broadcast limit is −3.0 dBTP.",
            );
            compact_meter(
                ui,
                "PLR",
                master.plr,
                4.0,
                20.0,
                "LU",
                "Peak-to-Loudness Ratio — dynamic headroom between peaks\n\
                 and integrated loudness. Higher = more dynamic.\n\
                 Typical range: 8–14 LU for pop/rock, 14+ LU for classical.",
            );
            compact_meter(
                ui,
                "PSR min",
                master.psr_min,
                4.0,
                20.0,
                "LU",
                "Minimum short-term loudness range over the track.\n\
                 Very low values (< 4 LU) indicate heavy limiting or\n\
                 excessive compression across the entire program.",
            );
            ui.end_row();
        });
    ui.add_space(2.0);
    correlation_bar(ui, master.correlation);
    ui.add_space(4.0);

    // ── Spectrum ────────────────────────────────────────────────────────
    let ref_bands = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .reference
        .as_ref()
        .map(|r| r.bands_dbfs);
    let header = if ref_bands.is_some() {
        "SPECTRUM  (bars · genre · cyan dashed = captured ref)"
    } else {
        "SPECTRUM  (bars = master bus · line = genre ref)"
    };
    section_label(ui, header);
    spectrum::spectrum_chart(
        ui,
        &master.bands_dbfs,
        &genre.bands_rel,
        ref_bands.as_ref(),
    );

    // Optional track overlay
    let (show_track_spec, track_bands) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let bands: Vec<(String, [f32; 10])> = s
            .last_tracks
            .iter()
            .map(|e| (e.name.clone(), e.frame.bands_dbfs))
            .collect();
        (s.show_track_spectrum, bands)
    };

    if show_track_spec && !track_bands.is_empty() {
        ui.add_space(2.0);
        section_label(ui, "TRACKS");
        spectrum::track_overlay_lines(ui, &track_bands);
        let names: Vec<String> = track_bands.iter().map(|(n, _)| n.clone()).collect();
        spectrum::track_legend(ui, &names);
    }

    ui.add_space(4.0);
    ui.separator();

    // ── Track table ─────────────────────────────────────────────────────
    {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let tracks = &s.last_tracks;
        if !tracks.is_empty() {
            section_label(ui, "TRACK READINGS");
            egui::ScrollArea::vertical()
                .id_salt("track_table_scroll")
                .max_height(100.0)
                .show(ui, |ui| {
                    egui::Grid::new("track_table")
                        .num_columns(6)
                        .spacing([6.0, 1.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (h, tip) in &[
                                ("Track", "Track instance name"),
                                ("Role",  "Mix role (drives role-aware advice rules)"),
                                ("LUFS",  "Integrated loudness"),
                                ("Peak",  "True peak level (dBTP)"),
                                ("PLR",   "Peak-to-Loudness Ratio"),
                                ("Corr",  "Stereo phase correlation"),
                            ] {
                                ui.label(
                                    egui::RichText::new(*h)
                                        .size(9.0)
                                        .color(egui::Color32::from_rgb(90, 90, 100)),
                                )
                                .on_hover_text(*tip);
                            }
                            ui.end_row();
                            for entry in tracks {
                                let tf = &entry.frame;
                                ui.label(egui::RichText::new(&entry.name).size(10.0));
                                ui.label(
                                    egui::RichText::new(format!("{}", entry.role))
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(160, 160, 175)),
                                );
                                ui.label(meter_text(tf.lufs_integrated, -24.0, -8.0));
                                ui.label(meter_text(tf.true_peak_dbtp, -6.0, 0.0));
                                ui.label(meter_text(tf.plr, 4.0, 20.0));
                                let (r, g, b) = corr_rgb(tf.correlation);
                                ui.label(
                                    egui::RichText::new(fmt_val(tf.correlation))
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(r, g, b)),
                                );
                                ui.end_row();
                            }
                        });
                });
        } else {
            ui.label(
                egui::RichText::new(
                    "No track instances found.\n\
                     Add Mastering Guide (Track mode) to each track, then press Analyze.",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(90, 90, 100)),
            );
        }
    }

    // ── Gilligan track list ──────────────────────────────────────────────
    {
        let g = state.lock().unwrap_or_else(|p| p.into_inner());
        let gs = g.gilligan.lock().unwrap_or_else(|p| p.into_inner());
        let (dot, dot_color) = if gs.connected {
            ("●", egui::Color32::from_rgb(80, 200, 100))
        } else {
            ("○", egui::Color32::from_rgb(90, 90, 100))
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(dot).size(9.0).color(dot_color))
                .on_hover_text(if gs.connected {
                    "Gilligan is connected and sending track data every 100 ms."
                } else {
                    "Gilligan is not connected.\n\
                     See the ? help guide for setup instructions."
                });
            ui.label(
                egui::RichText::new("GILLIGAN")
                    .size(9.0)
                    .color(egui::Color32::from_rgb(90, 90, 100)),
            )
            .on_hover_text(
                "Gilligan is a companion Bitwig controller extension that reads every \
                 track's name, type, color, and post-fader VU levels directly from \
                 the DAW — no per-track plugin insertion required.\n\n\
                 When connected, the track list below updates automatically every \
                 100 ms. Future versions will use this data to auto-fill track roles \
                 and apply fix actions (e.g. adjusting fader levels) from the advice \
                 panel.\n\n\
                 See the ? help guide for setup instructions.",
            );
        });

        if !gs.tracks.is_empty() {
            egui::ScrollArea::vertical()
                .id_salt("gilligan_scroll")
                .max_height(80.0)
                .show(ui, |ui| {
                    egui::Grid::new("gilligan_table")
                        .num_columns(2)
                        .spacing([6.0, 1.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (h, tip) in &[
                                ("Track",
                                 "Track name as reported by Bitwig. \
                                  Colour matches the track colour in the arrange view."),
                                ("Type",
                                 "Track type: Instrument, Audio, Effect, Group, or Master. \
                                  Used by the rule engine to apply type-aware advice."),
                            ] {
                                ui.label(
                                    egui::RichText::new(*h)
                                        .size(9.0)
                                        .color(egui::Color32::from_rgb(90, 90, 100)),
                                )
                                .on_hover_text(*tip);
                            }
                            ui.end_row();
                            for t in gs.tracks.iter() {
                                let [r, g, b] = t.color;
                                let row_tip = format!(
                                    "Name: {}\nType: {}{}\nBitwig position: {}",
                                    t.name,
                                    t.track_type,
                                    if t.is_group { " (Group)" } else { "" },
                                    if t.position >= 0 {
                                        t.position.to_string()
                                    } else {
                                        "master".to_string()
                                    },
                                );
                                ui.label(
                                    egui::RichText::new(&t.name)
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(r, g, b)),
                                )
                                .on_hover_text(&row_tip);
                                ui.label(
                                    egui::RichText::new(&t.track_type)
                                        .size(9.0)
                                        .color(egui::Color32::from_rgb(120, 120, 130)),
                                )
                                .on_hover_text(&row_tip);
                                ui.end_row();
                            }
                        });
                });
        } else if !gs.connected {
            ui.label(
                egui::RichText::new(
                    "Not connected. Open Bitwig Preferences → Controllers, \
                     add Gilligan, and enable it.",
                )
                .size(9.0)
                .color(egui::Color32::from_rgb(80, 80, 90)),
            )
            .on_hover_text(
                "Gilligan is a companion controller extension (not a MIDI device).\n\
                 In Bitwig: Preferences → Controllers → Add controller manually\n\
                 → Manufacturer: Sinkingsprings → Gilligan.\n\
                 No MIDI ports are needed — just enable it and click the tick mark.",
            );
        }
    }

    ui.add_space(4.0);
    ui.separator();

    // ── Controls ────────────────────────────────────────────────────────
    let value_color = egui::Color32::from_rgb(180, 200, 230);
    ui.horizontal(|ui| {
        ui.label(dim("Genre"));
        ui.add(
            widgets::ParamSlider::for_param(&params.genre, setter)
                .without_value()
                .with_width(80.0),
        )
        .on_hover_text(
            "Target genre for spectral reference curve and advice thresholds.\n\
             Affects the white line shown on the spectrum chart.",
        );
        ui.label(
            egui::RichText::new(format!("{}", params.genre.value()))
                .size(10.0)
                .color(value_color),
        );
    });
    ui.horizontal(|ui| {
        ui.label(dim("Platform"));
        ui.add(
            widgets::ParamSlider::for_param(&params.platform, setter)
                .without_value()
                .with_width(80.0),
        )
        .on_hover_text(
            "Target delivery platform. Sets the loudness normalization\n\
             target and true peak ceiling used in advice generation.",
        );
        ui.label(
            egui::RichText::new(format!("{}", params.platform.value()))
                .size(10.0)
                .color(value_color),
        );
    });
    ui.horizontal(|ui| {
        ui.label(dim("UI Scale"));
        ui.add(
            widgets::ParamSlider::for_param(&params.ui_scale, setter)
                .without_value()
                .with_width(80.0),
        )
        .on_hover_text(
            "Resize the whole plugin window proportionally.\n\
             Useful on HiDPI displays where 100% looks small.",
        );
        ui.label(
            egui::RichText::new(format!("{}", params.ui_scale.value()))
                .size(10.0)
                .color(value_color),
        );
    });

    ui.add_space(3.0);

    ui.horizontal(|ui| {
        let analyze_clicked = ui
            .add(egui::Button::new(
                egui::RichText::new("  Analyze Now  ")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(200, 220, 255)),
            ))
            .on_hover_text(
                "Capture current readings from all Track instances and\n\
                 generate mastering advice. Play from the start first\n\
                 for accurate integrated loudness.",
            )
            .clicked();

        if analyze_clicked {
            if let Ok(mut s) = state.lock() {
                run_analysis(&mut s, master, registry, params);
            }
        }

        ui.add_space(8.0);

        let mut auto = state.lock().unwrap_or_else(|p| p.into_inner()).auto_analyze;
        if ui
            .checkbox(&mut auto, egui::RichText::new("Auto (5 s)").size(10.0))
            .on_hover_text("Re-run analysis automatically every 5 seconds while the GUI is open.")
            .changed()
        {
            state.lock().unwrap_or_else(|p| p.into_inner()).auto_analyze = auto;
        }

        ui.add_space(8.0);

        let mut show = state.lock().unwrap_or_else(|p| p.into_inner()).show_track_spectrum;
        if ui
            .checkbox(&mut show, egui::RichText::new("Track spectrum").size(10.0))
            .on_hover_text("Show or hide individual track spectrum lines overlaid on the master chart.")
            .changed()
        {
            state.lock().unwrap_or_else(|p| p.into_inner()).show_track_spectrum = show;
        }

        ui.add_space(8.0);

        let has_ref = state.lock().unwrap_or_else(|p| p.into_inner()).reference.is_some();
        let ref_label = if has_ref { "  Update Ref  " } else { "  Capture Ref  " };
        if ui
            .add(egui::Button::new(
                egui::RichText::new(ref_label)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(180, 230, 235)),
            ))
            .on_hover_text(
                "Capture the current master spectrum and key metrics as a\n\
                 reference overlay (cyan dashed line on the chart). Useful\n\
                 for A/B'ing tweaks against an earlier state of your master.",
            )
            .clicked()
        {
            state.lock().unwrap_or_else(|p| p.into_inner()).reference = Some(master.clone());
        }
        if has_ref
            && ui
                .add(egui::Button::new(
                    egui::RichText::new("  Clear  ")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(180, 180, 180)),
                ))
                .on_hover_text("Discard the captured reference snapshot.")
                .clicked()
        {
            state.lock().unwrap_or_else(|p| p.into_inner()).reference = None;
        }
    });

    // Last analysis timestamp
    if let Some(t) = state.lock().unwrap_or_else(|p| p.into_inner()).last_analysis {
        let secs = t.elapsed().as_secs();
        ui.label(
            egui::RichText::new(format!("Last analyzed {} s ago", secs))
                .size(9.0)
                .color(egui::Color32::from_rgb(70, 70, 80)),
        );
    }

    ui.add_space(3.0);
    ui.separator();

    // ── Advice panel ────────────────────────────────────────────────────
    let advice = state.lock().unwrap_or_else(|p| p.into_inner()).advice.clone();
    let gilligan_connected = state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .gilligan
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .connected;

    if advice.is_empty() {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Press \"Analyze Now\" to generate mastering advice.")
                .size(11.0)
                .color(egui::Color32::from_rgb(80, 80, 90)),
        );
    } else {
        egui::ScrollArea::vertical()
            .id_salt("advice_scroll")
            .max_height(160.0)
            .show(ui, |ui| {
                for adv in &advice {
                    let (r, g, b) = adv.severity_rgb();
                    let sev_color = egui::Color32::from_rgb(r, g, b);

                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let badge_text = adv.severity_label();
                        ui.label(
                            egui::RichText::new(badge_text)
                                .size(11.0)
                                .color(sev_color),
                        )
                        .on_hover_text(severity_tip(badge_text));
                        ui.label(egui::RichText::new(&adv.title).size(11.0).strong());
                    });

                    let scope_str = match &adv.scope {
                        crate::engine::advice::Scope::MasterBus => "Master Bus".to_string(),
                        crate::engine::advice::Scope::Track(n)  => n.clone(),
                        crate::engine::advice::Scope::AllTracks => "All Tracks".to_string(),
                    };
                    ui.label(
                        egui::RichText::new(scope_str)
                            .size(9.0)
                            .color(egui::Color32::from_rgb(100, 100, 120)),
                    );

                    ui.label(
                        egui::RichText::new(&adv.detail)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(190, 190, 200)),
                    );
                    if !adv.fix.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Fix: {}", adv.fix))
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(100, 190, 110)),
                            );

                            // [Apply] button — only shown when Gilligan is connected
                            // and this advice has a concrete action.
                            if let Some(ref action) = adv.action {
                                if gilligan_connected {
                                    let desc = action.description();
                                    if ui
                                        .add(egui::Button::new(
                                            egui::RichText::new("Apply")
                                                .size(9.0)
                                                .color(egui::Color32::from_rgb(80, 200, 140)),
                                        ))
                                        .on_hover_text(format!(
                                            "Ask Gilligan to execute this fix in Bitwig:\n\
                                             {desc}\n\n\
                                             The change will be wrapped in an undo block \
                                             so you can Ctrl+Z to revert it."
                                        ))
                                        .clicked()
                                    {
                                        let json = action.to_json_msg();
                                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                                        let mut gs = s.gilligan.lock().unwrap_or_else(|p| p.into_inner());
                                        gs.outbound.push_back(json);
                                    }
                                } else {
                                    ui.label(
                                        egui::RichText::new("Apply")
                                            .size(9.0)
                                            .color(egui::Color32::from_rgb(60, 60, 70)),
                                    )
                                    .on_hover_text(
                                        "Connect Gilligan to enable one-click fixes.\n\
                                         See the ? help guide for setup instructions.",
                                    );
                                }
                            }
                        });
                    }
                    ui.add_space(2.0);
                    ui.separator();
                }
            });
    }
}

// ─── Track view ──────────────────────────────────────────────────────────────

fn render_track(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &Arc<MasteringGuideParams>,
    frame: &TrackFrame,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(dim("Role"));
        ui.add(
            widgets::ParamSlider::for_param(&params.track_role, setter)
                .without_value()
                .with_width(80.0),
        )
        .on_hover_text(
            "What this track is in the mix. Drives role-aware advice on the\n\
             master view (e.g. bass-vs-bass-drum collisions, vocal vs harmony\n\
             masking). Leave on Auto to be excluded from those rules.",
        );
        ui.label(
            egui::RichText::new(format!("{}", params.track_role.value()))
                .size(10.0)
                .color(egui::Color32::from_rgb(180, 200, 230)),
        );
    });
    ui.horizontal(|ui| {
        ui.label(dim("UI Scale"));
        ui.add(
            widgets::ParamSlider::for_param(&params.ui_scale, setter)
                .without_value()
                .with_width(80.0),
        )
        .on_hover_text(
            "Resize the whole plugin window proportionally.\n\
             Useful on HiDPI displays where 100% looks small.",
        );
        ui.label(
            egui::RichText::new(format!("{}", params.ui_scale.value()))
                .size(10.0)
                .color(egui::Color32::from_rgb(180, 200, 230)),
        );
    });
    ui.add_space(2.0);
    ui.separator();
    section_label(ui, "ANALYSIS");
    egui::Grid::new("track_meters")
        .num_columns(2)
        .spacing([10.0, 2.0])
        .show(ui, |ui| {
            meter_row(
                ui,
                "LUFS Integrated",
                frame.lufs_integrated,
                -24.0,
                -8.0,
                "LUFS",
                "Long-term average loudness measured over the full playback.\n\
                 Play from the start for an accurate reading.",
            );
            meter_row(
                ui,
                "LUFS Short-term",
                frame.lufs_short_term,
                -24.0,
                -8.0,
                "LUFS",
                "Average loudness over the last 3 seconds.\n\
                 Useful for checking loud/quiet section balance.",
            );
            meter_row(
                ui,
                "LUFS Momentary",
                frame.lufs_momentary,
                -24.0,
                -8.0,
                "LUFS",
                "Average loudness over the last 400 ms.\n\
                 Tracks transient loudness events closely.",
            );
            meter_row(
                ui,
                "True Peak",
                frame.true_peak_dbtp,
                -6.0,
                0.0,
                "dBTP",
                "Maximum inter-sample peak level.\n\
                 Keep below −1.0 dBTP for streaming to prevent clipping\n\
                 after codec encode/decode. Broadcast limit is −3.0 dBTP.",
            );
            meter_row(
                ui,
                "RMS",
                frame.rms_dbfs,
                -24.0,
                -6.0,
                "dBFS",
                "Root Mean Square average power level.\n\
                 Indicates perceived density and energy of the signal.",
            );
            meter_row(
                ui,
                "PLR",
                frame.plr,
                4.0,
                20.0,
                "LU",
                "Peak-to-Loudness Ratio — dynamic headroom between peaks\n\
                 and integrated loudness. Higher = more dynamic.\n\
                 Typical range: 8–14 LU for pop/rock, 14+ LU for classical.",
            );
            meter_row(
                ui,
                "PSR min",
                frame.psr_min,
                4.0,
                20.0,
                "LU",
                "Minimum short-term loudness range over the track.\n\
                 Very low values (< 4 LU) indicate heavy limiting or\n\
                 excessive compression across the entire program.",
            );
        });

    ui.add_space(4.0);
    correlation_bar(ui, frame.correlation);
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Track mode — this instance is contributing its analysis\n\
             to any Master mode instance in the session.",
        )
        .size(10.0)
        .color(egui::Color32::from_rgb(80, 80, 90)),
    );
}

// ─── Help window ─────────────────────────────────────────────────────────────

fn render_help(ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        help_section(ui, "SETUP");
        ui.label(dim_help(
            "1. Add this plugin in Track mode on every track you want to analyze.\n\
             2. Add one instance in Master mode on the master bus.\n\
             3. Play your mix from start to finish.\n\
             4. Press Analyze Now on the master instance.",
        ));

        ui.add_space(6.0);
        help_section(ui, "METRICS");

        help_entry(
            ui,
            "LUFS Integrated",
            "Long-term average loudness measured over the full playback. \
             Streaming platforms normalize to a target, so louder masters \
             are turned down — aim for the platform target, not maximum.",
        );
        help_entry(
            ui,
            "LUFS Short-term",
            "Average loudness over the last 3 seconds. \
             Compare loud and quiet sections to check balance.",
        );
        help_entry(
            ui,
            "LUFS Momentary",
            "Average loudness over the last 400 ms. \
             Tracks transient events closely.",
        );
        help_entry(
            ui,
            "True Peak (dBTP)",
            "Maximum inter-sample peak level. \
             Codec encode/decode can push peaks higher than the digital ceiling. \
             Keep below −1.0 dBTP for streaming, −3.0 dBTP for broadcast.",
        );
        help_entry(
            ui,
            "RMS (dBFS)",
            "Root Mean Square average power. \
             Indicates perceived density and energy.",
        );
        help_entry(
            ui,
            "PLR — Peak-to-Loudness Ratio",
            "Dynamic headroom between peaks and integrated loudness. \
             Higher values = more dynamic. Typical: 8–14 LU for pop/rock, \
             14+ LU for classical or jazz.",
        );
        help_entry(
            ui,
            "PSR min",
            "Minimum short-term loudness range across the program. \
             Values below 4 LU indicate the master is heavily limited \
             throughout with little dynamic variation.",
        );
        help_entry(
            ui,
            "Correlation",
            "Stereo phase relationship. \
             +1 = mono (fully correlated), 0 = uncorrelated stereo, \
             −1 = out of phase (will cancel in mono). \
             Aim for > 0.3 to stay mono-compatible.",
        );

        ui.add_space(6.0);
        help_section(ui, "PLATFORM LOUDNESS TARGETS");
        ui.label(dim_help(
            "Spotify          −14 LUFS  /  −1.0 dBTP\n\
             Apple Music      −16 LUFS  /  −1.0 dBTP\n\
             YouTube          −14 LUFS  /  −1.0 dBTP\n\
             Amazon Music     −14 LUFS  /  −2.0 dBTP\n\
             Tidal            −14 LUFS  /  −1.0 dBTP\n\
             SoundCloud       −14 LUFS  /  −1.0 dBTP\n\
             Broadcast EBU R128  −23 LUFS  /  −3.0 dBTP",
        ));

        ui.add_space(6.0);
        help_section(ui, "ADVICE SEVERITY");
        ui.label(dim_help(
            "INFO (green)  — Suggestions worth considering.\n\
             WARN (yellow) — Notable issues that should be addressed.\n\
             CRIT (red)    — Problems that will cause audible issues or \
             rejection on the target platform.",
        ));

        ui.add_space(6.0);
        help_section(ui, "GILLIGAN — COMPANION CONTROLLER EXTENSION");
        ui.label(dim_help(
            "Gilligan is an optional Bitwig controller extension that works \
             alongside this plugin. It reads every track's name, type, colour, \
             and post-fader VU levels directly from the DAW — no per-track \
             plugin insertion needed.",
        ));

        ui.add_space(4.0);
        help_entry(
            ui,
            "Why use Gilligan?",
            "Without it, the master view only knows about tracks that have a \
             Mastering Guide instance inserted. With Gilligan connected, every \
             track in your project appears automatically in the Gilligan list, \
             giving you a live VU overview of the whole session.",
        );
        help_entry(
            ui,
            "Future capabilities",
            "Upcoming phases will use Gilligan to auto-fill track roles \
             (Vocal/Drums/Bass…) from track names and colours, and to execute \
             \"Apply Fix\" actions — for example, reducing a track's fader by \
             4 dB with a single click, wrapped in Bitwig's undo history.",
        );

        ui.add_space(4.0);
        help_section(ui, "GILLIGAN SETUP");
        ui.label(dim_help(
            "1. In Bitwig: Preferences → Controllers.\n\
             2. Click \"Add controller manually\".\n\
             3. Manufacturer: Sinkingsprings  →  Controller: Gilligan.\n\
             4. No MIDI ports are required — leave both set to None.\n\
             5. Click the tick mark (✓) to activate.\n\
             6. The ● dot in the Mastering Guide master view turns green \
                when the connection is established (usually within 2 seconds).\n\n\
             Gilligan only needs to be added once per Bitwig installation. \
             It starts automatically every time Bitwig opens.",
        ));

        ui.add_space(4.0);
        help_entry(
            ui,
            "● / ○ indicator",
            "Green ● = Gilligan is connected and sending data. \
             Grey ○ = not connected. Check that the extension is enabled \
             in Preferences → Controllers.",
        );
        help_entry(
            ui,
            "Track type",
            "Instrument, Audio, Effect, Group, or Master — the Bitwig track type. \
             Future phases will use this alongside track name to auto-fill the \
             Track Role selector (Vocal/Drums/Bass…) without manual configuration.",
        );
        help_entry(
            ui,
            "Track colours",
            "Each row's text is drawn in the track's own colour from the \
             arrange view, making it easy to identify tracks at a glance.",
        );
        help_entry(
            ui,
            "Troubleshooting",
            "If Gilligan stays grey: confirm it is ticked in Bitwig \
             Preferences → Controllers (the extension must show a green LED, \
             not a grey one). If the LED is red, check the Bitwig script \
             console (Preferences → Controllers → Show script console) \
             for Java errors.",
        );

        ui.add_space(6.0);
        help_section(ui, "WORKFLOW TIPS");
        ui.label(dim_help(
            "• Play from the very start for accurate integrated LUFS.\n\
             • Use Auto (5 s) for a hands-free live update during playback.\n\
             • Switch Genre/Platform to compare different release contexts.\n\
             • Track spectrum overlay helps spot frequency clashes between stems.\n\
             • Install Gilligan for a whole-session VU overview without inserting \
               a plugin on every track.",
        ));
    });
}

fn help_section(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(10.0)
            .color(egui::Color32::from_rgb(140, 185, 255))
            .strong(),
    );
}

fn help_entry(ui: &mut egui::Ui, label: &str, body: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(label).size(10.0).strong());
        ui.label(dim_help(&format!("— {}", body)));
    });
}

fn dim_help(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(10.0)
        .color(egui::Color32::from_rgb(170, 170, 180))
}

// ─── Analysis runner ─────────────────────────────────────────────────────────

fn run_analysis(
    s: &mut MasterState,
    master: &TrackFrame,
    registry: &Option<Arc<Registry>>,
    params: &Arc<MasteringGuideParams>,
) {
    let tracks = registry
        .as_ref()
        .map(|r| r.read_tracks())
        .unwrap_or_default();

    s.last_tracks = tracks.clone();

    let genre = genre_for(&params.genre.value());
    let platform = platform_for(&params.platform.value());
    let ctx = EvalContext {
        master,
        tracks: &tracks,
        genre,
        platform,
    };
    s.advice = evaluate(&ctx);
    s.last_analysis = Some(Instant::now());
}

// ─── Reusable widgets ─────────────────────────────────────────────────────────

fn correlation_bar(ui: &mut egui::Ui, correlation: f32) {
    ui.horizontal(|ui| {
        ui.label(dim("Corr"))
            .on_hover_text(
                "Stereo phase correlation. +1 = mono-compatible, 0 = uncorrelated,\n\
                 −1 = out of phase (will cancel in mono). Aim for > 0.3.",
            );
        let corr = correlation.clamp(-1.0, 1.0);
        let avail = ui.available_width() - 42.0;
        let (rect, bar_resp) =
            ui.allocate_exact_size(egui::vec2(avail, 10.0), egui::Sense::hover());
        bar_resp.on_hover_text(
            "Stereo phase correlation. +1 = mono-compatible, 0 = uncorrelated,\n\
             −1 = out of phase (will cancel in mono). Aim for > 0.3.",
        );
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            p.rect_filled(rect, 2.0, egui::Color32::from_rgb(38, 38, 44));
            let fill_w = ((corr + 1.0) / 2.0 * rect.width()).max(0.0);
            let (r, g, b) = corr_rgb(corr);
            p.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height())),
                2.0,
                egui::Color32::from_rgb(r, g, b),
            );
        }
        ui.label(
            egui::RichText::new(fmt_val(corr))
                .size(10.0)
                .color(egui::Color32::from_rgb(180, 180, 180)),
        );
    });
}

fn compact_meter(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    lo: f32,
    hi: f32,
    unit: &str,
    tooltip: &str,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(9.0)
                .color(egui::Color32::from_rgb(90, 90, 100)),
        );
        let text = if value.is_finite() {
            format!("{:+.1}", value)
        } else {
            "---".into()
        };
        ui.label(
            egui::RichText::new(text)
                .size(12.0)
                .color(meter_color(value, lo, hi))
                .strong(),
        );
        ui.label(
            egui::RichText::new(unit)
                .size(8.0)
                .color(egui::Color32::from_rgb(70, 70, 80)),
        );
    })
    .response
    .on_hover_text(tooltip);
}

fn meter_row(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    lo: f32,
    hi: f32,
    unit: &str,
    tooltip: &str,
) {
    ui.label(dim(label)).on_hover_text(tooltip);
    ui.label(meter_text(value, lo, hi)).on_hover_text(tooltip);
    let _ = unit;
    ui.end_row();
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(9.0)
            .color(egui::Color32::from_rgb(90, 90, 100)),
    );
}

fn dim(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(10.0)
        .color(egui::Color32::from_rgb(110, 110, 120))
}

fn meter_text(value: f32, lo: f32, hi: f32) -> egui::RichText {
    egui::RichText::new(fmt_val(value))
        .size(10.0)
        .color(meter_color(value, lo, hi))
}

fn fmt_val(value: f32) -> String {
    if value.is_finite() {
        format!("{:+.1}", value)
    } else {
        "---".into()
    }
}

fn meter_color(value: f32, lo: f32, hi: f32) -> egui::Color32 {
    if !value.is_finite() {
        return egui::Color32::from_rgb(70, 70, 80);
    }
    let t = (value - lo) / (hi - lo);
    if t < 0.4 {
        egui::Color32::from_rgb(70, 175, 85)
    } else if t < 0.75 {
        egui::Color32::from_rgb(200, 165, 45)
    } else {
        egui::Color32::from_rgb(210, 65, 55)
    }
}

fn corr_rgb(corr: f32) -> (u8, u8, u8) {
    if corr < 0.0 {
        (210, 55, 55)
    } else if corr < 0.2 {
        (215, 130, 35)
    } else if corr < 0.3 {
        (195, 195, 50)
    } else {
        (60, 175, 65)
    }
}

fn severity_tip(label: &str) -> &'static str {
    match label {
        "INFO" => "Suggestions worth considering for a better result.",
        "WARN" => "Notable issues that should be addressed before release.",
        _      => "Critical problems that will cause audible issues or platform rejection.",
    }
}

fn apply_theme(ctx: &egui::Context) {
    ctx.style_mut(|s| {
        s.visuals.panel_fill = egui::Color32::from_rgb(24, 24, 28);
        s.visuals.window_fill = egui::Color32::from_rgb(24, 24, 28);
        s.visuals.override_text_color = Some(egui::Color32::from_rgb(210, 210, 215));
        s.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(40, 40, 46);
        s.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(55, 55, 62);
        s.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(70, 100, 160);
        s.visuals.selection.bg_fill = egui::Color32::from_rgb(60, 90, 150);
        s.spacing.item_spacing = egui::vec2(4.0, 3.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
    });
}
