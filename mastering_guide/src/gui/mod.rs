mod spectrum;

use crate::analysis::frame::{FrameReader, TrackFrame};
use crate::engine::advice::Advice;
use crate::engine::evaluator::{EvalContext, evaluate};
use crate::engine::genres::genre_for;
use crate::engine::platforms::platform_for;
use crate::ipc::Registry;
use crate::params::{MasteringGuideParams, ModeParam};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ─── Editor state shared across repaints ─────────────────────────────────────

struct MasterState {
    advice: Vec<Advice>,
    last_tracks: Vec<(String, TrackFrame)>,
    last_analysis: Option<Instant>,
    auto_analyze: bool,
    show_track_spectrum: bool,
}

impl Default for MasterState {
    fn default() -> Self {
        Self {
            advice: Vec::new(),
            last_tracks: Vec::new(),
            last_analysis: None,
            auto_analyze: false,
            show_track_spectrum: true,
        }
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

pub fn create_editor(
    params: Arc<MasteringGuideParams>,
    frame_reader: FrameReader,
    registry: Option<Arc<Registry>>,
    track_name: String,
) -> Option<Box<dyn Editor>> {
    let egui_state = EguiState::from_size(430, 590);
    let state: Arc<Mutex<MasterState>> = Arc::new(Mutex::new(MasterState::default()));

    create_egui_editor(
        egui_state,
        (frame_reader, registry, state, track_name),
        |ctx, _| {
            apply_theme(ctx);
        },
        move |ctx, setter, (frame_reader, registry, state, track_name)| {
            let frame = frame_reader.read();
            let is_master = params.mode.value() == ModeParam::Master;

            // Write our frame into the process-global registry every repaint.
            if let Some(ref reg) = registry {
                reg.write(track_name, &frame);
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
                render_header(ui, setter, &params, registry);
                ui.add_space(2.0);
                ui.separator();

                if is_master {
                    render_master(ui, setter, &params, &frame, registry, state);
                } else {
                    render_track(ui, &frame);
                }
            });

            ctx.request_repaint_after(Duration::from_millis(80));
        },
    )
}

// ─── Header ──────────────────────────────────────────────────────────────────

fn render_header(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &Arc<MasteringGuideParams>,
    registry: &Option<Arc<Registry>>,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("MASTERING GUIDE")
                .size(12.0)
                .color(egui::Color32::from_rgb(140, 185, 255))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(ref reg) = registry {
                ui.label(
                    egui::RichText::new(format!("#{}", reg.slot_index() + 1))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(80, 80, 90)),
                );
            }
            ui.add(
                widgets::ParamSlider::for_param(&params.mode, setter)
                    .without_value()
                    .with_width(90.0),
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
            compact_meter(ui, "Int LUFS",  master.lufs_integrated, -24.0, -8.0,  "LUFS");
            compact_meter(ui, "True Peak", master.true_peak_dbtp,  -6.0,   0.0,  "dBTP");
            compact_meter(ui, "PLR",       master.plr,              4.0,  20.0,  "LU");
            compact_meter(ui, "PSR min",   master.psr_min,          4.0,  20.0,  "LU");
            ui.end_row();
        });
    ui.add_space(2.0);
    correlation_bar(ui, master.correlation);
    ui.add_space(4.0);

    // ── Spectrum ────────────────────────────────────────────────────────
    section_label(ui, "SPECTRUM  (bars = master bus · line = genre ref)");
    spectrum::spectrum_chart(ui, &master.bands_dbfs, &genre.bands_rel);

    // Optional track overlay
    let (show_track_spec, track_bands) = {
        let s = state.lock().unwrap();
        let bands: Vec<(String, [f32; 10])> = s
            .last_tracks
            .iter()
            .map(|(n, f)| (n.clone(), f.bands_dbfs))
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
        let s = state.lock().unwrap();
        let tracks = &s.last_tracks;
        if !tracks.is_empty() {
            section_label(ui, "TRACK READINGS");
            egui::ScrollArea::vertical()
                .id_salt("track_table_scroll")
                .max_height(100.0)
                .show(ui, |ui| {
                    egui::Grid::new("track_table")
                        .num_columns(5)
                        .spacing([6.0, 1.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for h in &["Track", "LUFS", "Peak", "PLR", "Corr"] {
                                ui.label(
                                    egui::RichText::new(*h)
                                        .size(9.0)
                                        .color(egui::Color32::from_rgb(90, 90, 100)),
                                );
                            }
                            ui.end_row();
                            for (name, tf) in tracks {
                                ui.label(egui::RichText::new(name).size(10.0));
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

    ui.add_space(4.0);
    ui.separator();

    // ── Controls ────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(dim("Genre"));
        ui.add(
            widgets::ParamSlider::for_param(&params.genre, setter)
                .without_value()
                .with_width(100.0),
        );
        ui.add_space(4.0);
        ui.label(dim("Platform"));
        ui.add(
            widgets::ParamSlider::for_param(&params.platform, setter)
                .without_value()
                .with_width(110.0),
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
            .clicked();

        if analyze_clicked {
            if let Ok(mut s) = state.lock() {
                run_analysis(&mut s, master, registry, params);
            }
        }

        ui.add_space(8.0);

        let mut auto = state.lock().unwrap().auto_analyze;
        if ui.checkbox(&mut auto, egui::RichText::new("Auto (5 s)").size(10.0)).changed() {
            state.lock().unwrap().auto_analyze = auto;
        }

        ui.add_space(8.0);

        let mut show = state.lock().unwrap().show_track_spectrum;
        if ui.checkbox(&mut show, egui::RichText::new("Track spectrum").size(10.0)).changed() {
            state.lock().unwrap().show_track_spectrum = show;
        }
    });

    // Last analysis timestamp
    if let Some(t) = state.lock().unwrap().last_analysis {
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
    let advice = state.lock().unwrap().advice.clone();
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
                        // Severity badge
                        let badge_text = adv.severity_label();
                        ui.label(
                            egui::RichText::new(badge_text)
                                .size(11.0)
                                .color(sev_color),
                        );
                        ui.label(egui::RichText::new(&adv.title).size(11.0).strong());
                    });

                    // Scope tag
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
                        ui.label(
                            egui::RichText::new(format!("Fix: {}", adv.fix))
                                .size(10.0)
                                .color(egui::Color32::from_rgb(100, 190, 110)),
                        );
                    }
                    ui.add_space(2.0);
                    ui.separator();
                }
            });
    }
}

// ─── Track view ──────────────────────────────────────────────────────────────

fn render_track(ui: &mut egui::Ui, frame: &TrackFrame) {
    ui.add_space(4.0);
    section_label(ui, "ANALYSIS");
    egui::Grid::new("track_meters")
        .num_columns(2)
        .spacing([10.0, 2.0])
        .show(ui, |ui| {
            meter_row(ui, "LUFS Integrated", frame.lufs_integrated, -24.0, -8.0,  "LUFS");
            meter_row(ui, "LUFS Short-term", frame.lufs_short_term, -24.0, -8.0,  "LUFS");
            meter_row(ui, "LUFS Momentary",  frame.lufs_momentary,  -24.0, -8.0,  "LUFS");
            meter_row(ui, "True Peak",        frame.true_peak_dbtp,  -6.0,  0.0,  "dBTP");
            meter_row(ui, "RMS",              frame.rms_dbfs,       -24.0, -6.0,  "dBFS");
            meter_row(ui, "PLR",              frame.plr,             4.0,  20.0,  "LU");
            meter_row(ui, "PSR min",          frame.psr_min,         4.0,  20.0,  "LU");
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
        ui.label(dim("Corr"));
        let corr = correlation.clamp(-1.0, 1.0);
        let avail = ui.available_width() - 42.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 10.0), egui::Sense::hover());
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

fn compact_meter(ui: &mut egui::Ui, label: &str, value: f32, lo: f32, hi: f32, unit: &str) {
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
    });
}

fn meter_row(ui: &mut egui::Ui, label: &str, value: f32, lo: f32, hi: f32, unit: &str) {
    ui.label(dim(label));
    ui.label(meter_text(value, lo, hi));
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
