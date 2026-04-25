use crate::analysis::frame::FrameReader;
use crate::engine::advice::Advice;
use crate::engine::evaluator::{EvalContext, evaluate};
use crate::engine::genres::genre_for;
use crate::engine::platforms::platform_for;
use crate::params::{MasteringGuideParams, ModeParam};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};
use std::sync::{Arc, Mutex};

pub fn create_editor(
    params: Arc<MasteringGuideParams>,
    frame_reader: FrameReader,
) -> Option<Box<dyn Editor>> {
    let egui_state = EguiState::from_size(380, 440);
    let advice: Arc<Mutex<Vec<Advice>>> = Arc::new(Mutex::new(Vec::new()));

    create_egui_editor(
        egui_state,
        (frame_reader, advice),
        |ctx, _| {
            ctx.style_mut(|s| {
                s.visuals.panel_fill = egui::Color32::from_rgb(28, 28, 32);
                s.visuals.override_text_color = Some(egui::Color32::from_rgb(220, 220, 220));
            });
        },
        move |ctx, setter, (frame_reader, advice_store)| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let frame = frame_reader.read();

                // ── Header ──────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("MASTERING GUIDE")
                            .size(13.0)
                            .color(egui::Color32::from_rgb(160, 200, 255))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(widgets::ParamSlider::for_param(&params.mode, setter).without_value());
                    });
                });

                ui.separator();

                // ── Meters ──────────────────────────────────────────────
                egui::Grid::new("meters")
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

                // ── Stereo correlation bar ───────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Correlation")
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                    let corr = frame.correlation.clamp(-1.0, 1.0);
                    let bar_w = ui.available_width() - 48.0;
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(bar_w, 12.0), egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        let painter = ui.painter();
                        painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(50, 50, 55));
                        let fill_w = (corr + 1.0) / 2.0 * rect.width();
                        let color = correlation_color(corr);
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                rect.min,
                                egui::vec2(fill_w, rect.height()),
                            ),
                            2.0,
                            color,
                        );
                    }
                    ui.label(egui::RichText::new(format!("{:+.2}", corr)).size(11.0));
                });

                ui.add_space(4.0);
                ui.separator();

                // ── Master-mode controls and advice ─────────────────────
                let is_master = params.mode.value() == ModeParam::Master;
                if is_master {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Genre").size(10.0).color(egui::Color32::GRAY));
                        ui.add(widgets::ParamSlider::for_param(&params.genre, setter).without_value());
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Platform").size(10.0).color(egui::Color32::GRAY));
                        ui.add(widgets::ParamSlider::for_param(&params.platform, setter).without_value());
                    });

                    if ui.button("  Analyze Now  ").clicked() {
                        let genre = genre_for(&params.genre.value());
                        let platform = platform_for(&params.platform.value());
                        let eval_ctx = EvalContext {
                            master: &frame,
                            tracks: &[],
                            genre,
                            platform,
                        };
                        let result = evaluate(&eval_ctx);
                        if let Ok(mut store) = advice_store.lock() {
                            *store = result;
                        }
                    }

                    ui.separator();

                    // ── Advice panel ─────────────────────────────────────
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            let store = advice_store.lock().unwrap();
                            if store.is_empty() {
                                ui.label(
                                    egui::RichText::new(
                                        "Press \"Analyze Now\" to generate mastering advice.",
                                    )
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
                                        ui.label(
                                            egui::RichText::new(&adv.title).size(11.0).strong(),
                                        );
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
                                    ui.add_space(4.0);
                                    ui.separator();
                                }
                            }
                        });
                } else {
                    ui.label(
                        egui::RichText::new(
                            "Track mode: analyzing this track's audio.\n\
                             Add a Master mode instance on the master bus\n\
                             to see full mastering advice.",
                        )
                        .size(10.0)
                        .color(egui::Color32::GRAY),
                    );
                }
            });
        },
    )
}

fn meter_row(ui: &mut egui::Ui, label: &str, value: f32, unit: &str, lo: f32, hi: f32) {
    ui.label(egui::RichText::new(label).size(10.0).color(egui::Color32::GRAY));
    let text = if value.is_finite() {
        format!("{:+.1} {}", value, unit)
    } else {
        format!("--- {}", unit)
    };
    let color = meter_color(value, lo, hi);
    ui.label(egui::RichText::new(text).size(11.0).color(color));
    ui.end_row();
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

fn correlation_color(corr: f32) -> egui::Color32 {
    if corr < 0.0 {
        egui::Color32::from_rgb(220, 50, 50)
    } else if corr < 0.2 {
        egui::Color32::from_rgb(220, 130, 30)
    } else if corr < 0.3 {
        egui::Color32::from_rgb(200, 200, 50)
    } else {
        egui::Color32::from_rgb(60, 180, 60)
    }
}
