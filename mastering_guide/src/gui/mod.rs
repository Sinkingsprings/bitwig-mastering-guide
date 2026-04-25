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

pub fn create_editor(
    params: Arc<MasteringGuideParams>,
    frame_reader: FrameReader,
    registry: Option<Arc<Registry>>,
    track_name: String,
) -> Option<Box<dyn Editor>> {
    let egui_state = EguiState::from_size(400, 500);
    let advice: Arc<Mutex<Vec<Advice>>> = Arc::new(Mutex::new(Vec::new()));
    let last_tracks: Arc<Mutex<Vec<(String, TrackFrame)>>> = Arc::new(Mutex::new(Vec::new()));

    create_egui_editor(
        egui_state,
        (frame_reader, registry, advice, last_tracks, track_name),
        |ctx, _| {
            ctx.style_mut(|s| {
                s.visuals.panel_fill = egui::Color32::from_rgb(28, 28, 32);
                s.visuals.override_text_color = Some(egui::Color32::from_rgb(220, 220, 220));
            });
        },
        move |ctx, setter, (frame_reader, registry, advice_store, last_tracks, track_name)| {
            let frame = frame_reader.read();
            let is_master = params.mode.value() == ModeParam::Master;

            // ── Write our frame to the registry every repaint ──────────
            if let Some(ref reg) = registry {
                reg.write(track_name, &frame);
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                // ── Header ─────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("MASTERING GUIDE")
                            .size(13.0)
                            .color(egui::Color32::from_rgb(160, 200, 255))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(ref reg) = registry {
                            ui.label(
                                egui::RichText::new(format!("Slot {}", reg.slot_index() + 1))
                                    .size(10.0)
                                    .color(egui::Color32::GRAY),
                            );
                        }
                        ui.add(widgets::ParamSlider::for_param(&params.mode, setter).without_value());
                    });
                });

                ui.separator();

                if is_master {
                    master_view(ui, setter, &params, &frame, registry, advice_store, last_tracks);
                } else {
                    track_view(ui, setter, &params, &frame);
                }
            });

            // Keep repainting so meters stay live
            ctx.request_repaint();
        },
    )
}

// ─── Master view ─────────────────────────────────────────────────────────────

fn master_view(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &Arc<MasteringGuideParams>,
    master_frame: &TrackFrame,
    registry: &Option<Arc<Registry>>,
    advice_store: &Arc<Mutex<Vec<Advice>>>,
    last_tracks: &Arc<Mutex<Vec<(String, TrackFrame)>>>,
) {
    // ── Master bus meters ───────────────────────────────────────────────
    ui.label(egui::RichText::new("MASTER BUS").size(10.0).color(egui::Color32::GRAY));
    egui::Grid::new("master_meters")
        .num_columns(4)
        .spacing([6.0, 2.0])
        .show(ui, |ui| {
            small_meter(ui, "Int LUFS", master_frame.lufs_integrated, -24.0, -6.0);
            small_meter(ui, "True Peak", master_frame.true_peak_dbtp, -6.0, 0.0);
            small_meter(ui, "PLR", master_frame.plr, 6.0, 20.0);
            small_meter(ui, "PSR min", master_frame.psr_min, 6.0, 20.0);
            ui.end_row();
        });

    ui.add_space(4.0);

    // ── Correlation bar ─────────────────────────────────────────────────
    correlation_bar(ui, master_frame.correlation);

    ui.add_space(4.0);
    ui.separator();

    // ── Track list ──────────────────────────────────────────────────────
    let tracks = last_tracks.lock().unwrap().clone();
    if !tracks.is_empty() {
        ui.label(egui::RichText::new("TRACKS").size(10.0).color(egui::Color32::GRAY));
        egui::Grid::new("track_table")
            .num_columns(5)
            .spacing([6.0, 1.0])
            .striped(true)
            .show(ui, |ui| {
                // Header
                for h in &["Track", "LUFS", "Peak", "PLR", "Corr"] {
                    ui.label(egui::RichText::new(*h).size(9.0).color(egui::Color32::DARK_GRAY));
                }
                ui.end_row();

                for (name, tf) in &tracks {
                    ui.label(egui::RichText::new(name).size(10.0));
                    ui.label(styled_val(tf.lufs_integrated, "LUFS", -24.0, -6.0));
                    ui.label(styled_val(tf.true_peak_dbtp, "dBTP", -6.0, 0.0));
                    ui.label(styled_val(tf.plr, "LU", 6.0, 20.0));
                    let corr = tf.correlation;
                    let (r, g, b) = correlation_rgb(corr);
                    ui.label(
                        egui::RichText::new(format!("{:+.2}", corr))
                            .size(10.0)
                            .color(egui::Color32::from_rgb(r, g, b)),
                    );
                    ui.end_row();
                }
            });
        ui.add_space(4.0);
    } else {
        ui.label(
            egui::RichText::new("No track instances detected yet.\nAdd Mastering Guide (Track mode) to each track.")
                .size(10.0)
                .color(egui::Color32::GRAY),
        );
        ui.add_space(4.0);
    }

    ui.separator();

    // ── Genre / Platform / Analyze ──────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Genre").size(10.0).color(egui::Color32::GRAY));
        ui.add(widgets::ParamSlider::for_param(&params.genre, setter).without_value());
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Platform").size(10.0).color(egui::Color32::GRAY));
        ui.add(widgets::ParamSlider::for_param(&params.platform, setter).without_value());
    });

    if ui.button("  Analyze Now  ").clicked() {
        // Snapshot current track list
        let tracks_now = registry
            .as_ref()
            .map(|r| r.read_tracks())
            .unwrap_or_default();
        *last_tracks.lock().unwrap() = tracks_now.clone();

        let genre = genre_for(&params.genre.value());
        let platform = platform_for(&params.platform.value());
        let eval_ctx = EvalContext {
            master: master_frame,
            tracks: &tracks_now,
            genre,
            platform,
        };
        *advice_store.lock().unwrap() = evaluate(&eval_ctx);
    }

    ui.separator();

    // ── Advice panel ────────────────────────────────────────────────────
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            let store = advice_store.lock().unwrap();
            if store.is_empty() {
                ui.label(
                    egui::RichText::new("Press \"Analyze Now\" to generate mastering advice.")
                        .size(11.0)
                        .color(egui::Color32::GRAY),
                );
            } else {
                for adv in store.iter() {
                    let (r, g, b) = adv.severity_rgb();
                    let sev_color = egui::Color32::from_rgb(r, g, b);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(adv.severity_label())
                                .size(10.0)
                                .color(sev_color)
                                .strong(),
                        );
                        ui.label(egui::RichText::new(&adv.title).size(11.0).strong());
                    });
                    ui.label(
                        egui::RichText::new(&adv.detail)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(200, 200, 200)),
                    );
                    if !adv.fix.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("Fix: {}", adv.fix))
                                .size(10.0)
                                .color(egui::Color32::from_rgb(120, 200, 120)),
                        );
                    }
                    ui.add_space(3.0);
                    ui.separator();
                }
            }
        });
}

// ─── Track view ──────────────────────────────────────────────────────────────

fn track_view(
    ui: &mut egui::Ui,
    _setter: &ParamSetter,
    _params: &Arc<MasteringGuideParams>,
    frame: &TrackFrame,
) {
    egui::Grid::new("track_meters")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            meter_row(ui, "LUFS Int",  frame.lufs_integrated, "LUFS", -24.0, -6.0);
            meter_row(ui, "LUFS S/T",  frame.lufs_short_term,  "LUFS", -24.0, -6.0);
            meter_row(ui, "LUFS Mom",  frame.lufs_momentary,   "LUFS", -24.0, -6.0);
            meter_row(ui, "True Peak", frame.true_peak_dbtp,   "dBTP", -6.0,   0.0);
            meter_row(ui, "RMS",       frame.rms_dbfs,         "dBFS", -24.0, -6.0);
            meter_row(ui, "PLR",       frame.plr,              "LU",    6.0,  20.0);
            meter_row(ui, "PSR min",   frame.psr_min,          "LU",    6.0,  20.0);
        });

    ui.add_space(4.0);
    correlation_bar(ui, frame.correlation);
    ui.add_space(6.0);

    ui.label(
        egui::RichText::new(
            "Track mode — analyzing this track's audio.\n\
             Add a Master mode instance on the master bus\n\
             to see full mastering advice.",
        )
        .size(10.0)
        .color(egui::Color32::GRAY),
    );
}

// ─── Shared widgets ───────────────────────────────────────────────────────────

fn correlation_bar(ui: &mut egui::Ui, correlation: f32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Correlation")
                .size(10.0)
                .color(egui::Color32::GRAY),
        );
        let corr = correlation.clamp(-1.0, 1.0);
        let bar_w = ui.available_width() - 48.0;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(bar_w, 10.0), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(45, 45, 50));
            let fill_w = ((corr + 1.0) / 2.0 * rect.width()).max(0.0);
            let (r, g, b) = correlation_rgb(corr);
            painter.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height())),
                2.0,
                egui::Color32::from_rgb(r, g, b),
            );
        }
        ui.label(egui::RichText::new(format!("{:+.2}", corr)).size(10.0));
    });
}

fn meter_row(ui: &mut egui::Ui, label: &str, value: f32, unit: &str, lo: f32, hi: f32) {
    ui.label(egui::RichText::new(label).size(10.0).color(egui::Color32::GRAY));
    ui.label(styled_val(value, unit, lo, hi));
    ui.end_row();
}

fn small_meter(ui: &mut egui::Ui, label: &str, value: f32, lo: f32, hi: f32) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label).size(9.0).color(egui::Color32::DARK_GRAY));
        let text = if value.is_finite() {
            format!("{:+.1}", value)
        } else {
            "---".into()
        };
        ui.label(egui::RichText::new(text).size(11.0).color(meter_color(value, lo, hi)));
    });
}

fn styled_val(value: f32, unit: &str, lo: f32, hi: f32) -> egui::RichText {
    let text = if value.is_finite() {
        format!("{:+.1} {}", value, unit)
    } else {
        format!("--- {}", unit)
    };
    egui::RichText::new(text).size(10.0).color(meter_color(value, lo, hi))
}

fn meter_color(value: f32, lo: f32, hi: f32) -> egui::Color32 {
    if !value.is_finite() {
        return egui::Color32::GRAY;
    }
    let t = (value - lo) / (hi - lo);
    if t < 0.4 {
        egui::Color32::from_rgb(80, 180, 80)
    } else if t < 0.75 {
        egui::Color32::from_rgb(200, 180, 50)
    } else {
        egui::Color32::from_rgb(220, 60, 60)
    }
}

fn correlation_rgb(corr: f32) -> (u8, u8, u8) {
    if corr < 0.0 {
        (220, 50, 50)
    } else if corr < 0.2 {
        (220, 130, 30)
    } else if corr < 0.3 {
        (200, 200, 50)
    } else {
        (60, 180, 60)
    }
}
