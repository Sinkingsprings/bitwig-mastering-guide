use nih_plug_egui::egui;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};

pub const BAND_LABELS: &[&str] = &[
    "31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];

const DB_MIN: f32 = -70.0;
const DB_MAX: f32 = 0.0;

/// Draw a 10-band octave spectrum chart for one track (master bus or any track).
///
/// Bars are coloured by deviation from the genre reference curve:
///   green  = within ±3 dB of target
///   amber  = 3–6 dB off
///   red    = >6 dB off
///
/// A white line connects the genre-reference level for each band. If a
/// `captured_reference` is provided it is drawn as a dashed cyan overlay in
/// absolute dBFS — the user's own captured snapshot.
pub fn spectrum_chart(
    ui: &mut egui::Ui,
    bands_dbfs: &[f32; 10],
    genre_rel: &[f32; 10],
    captured_reference: Option<&[f32; 10]>,
) {
    let desired = egui::vec2(ui.available_width(), 96.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Color32::from_rgb(18, 18, 22));

    let y_label_w = 26.0;
    let x_label_h = 14.0;
    let chart_x0 = rect.left() + y_label_w;
    let chart_y0 = rect.top() + 4.0;
    let chart_w = rect.width() - y_label_w;
    let chart_h = rect.height() - x_label_h - 4.0;
    let n = 10usize;
    let bar_w = chart_w / n as f32;

    // Gridlines + y-axis labels
    for &db in &[0.0f32, -18.0, -36.0, -54.0] {
        let y = db_to_y(db, chart_y0, chart_h);
        painter.line_segment(
            [Pos2::new(chart_x0, y), Pos2::new(rect.right(), y)],
            Stroke::new(0.5, Color32::from_rgb(45, 45, 50)),
        );
        painter.text(
            Pos2::new(rect.left() + 1.0, y),
            egui::Align2::LEFT_CENTER,
            format!("{}", db as i32),
            egui::FontId::proportional(8.0),
            Color32::from_rgb(90, 90, 100),
        );
    }

    // Genre reference in absolute dBFS, anchored to the 1 kHz (index 5) measurement
    let ref_1k = bands_dbfs[5];
    let genre_abs: [f32; 10] =
        std::array::from_fn(|i| if ref_1k.is_finite() { ref_1k + genre_rel[i] } else { f32::NEG_INFINITY });

    // Bars
    for i in 0..n {
        let db = bands_dbfs[i];
        if !db.is_finite() || db < DB_MIN {
            continue;
        }
        let db_clamped = db.clamp(DB_MIN, DB_MAX);
        let top_y = db_to_y(db_clamped, chart_y0, chart_h);
        let bot_y = db_to_y(DB_MIN, chart_y0, chart_h);
        let x = chart_x0 + i as f32 * bar_w;
        let bar_rect = Rect::from_min_max(
            Pos2::new(x + 1.5, top_y),
            Pos2::new(x + bar_w - 1.5, bot_y),
        );

        let color = if genre_abs[i].is_finite() {
            let dev = (db - genre_abs[i]).abs();
            if dev < 3.0 {
                Color32::from_rgb(55, 160, 75)
            } else if dev < 6.0 {
                Color32::from_rgb(200, 155, 40)
            } else {
                Color32::from_rgb(200, 65, 50)
            }
        } else {
            Color32::from_rgb(70, 110, 180)
        };
        painter.rect_filled(bar_rect, 1.0, color);

        // X-axis labels
        painter.text(
            Pos2::new(x + bar_w * 0.5, rect.bottom() - 1.0),
            egui::Align2::CENTER_BOTTOM,
            BAND_LABELS[i],
            egui::FontId::proportional(8.0),
            Color32::from_rgb(110, 110, 120),
        );
    }

    // Genre reference line
    let ref_pts: Vec<Pos2> = (0..n)
        .filter_map(|i| {
            let db = genre_abs[i];
            if !db.is_finite() {
                return None;
            }
            let y = db_to_y(db.clamp(DB_MIN, DB_MAX), chart_y0, chart_h);
            let x = chart_x0 + (i as f32 + 0.5) * bar_w;
            Some(Pos2::new(x, y))
        })
        .collect();

    if ref_pts.len() >= 2 {
        for pair in ref_pts.windows(2) {
            painter.line_segment(
                [pair[0], pair[1]],
                Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 140)),
            );
        }
        for &p in &ref_pts {
            painter.circle_filled(p, 2.5, Color32::WHITE);
        }
    }

    // Captured-reference overlay (dashed cyan).
    if let Some(snap) = captured_reference {
        let snap_pts: Vec<Pos2> = (0..n)
            .filter_map(|i| {
                let db = snap[i];
                if !db.is_finite() || db < DB_MIN {
                    return None;
                }
                let y = db_to_y(db.clamp(DB_MIN, DB_MAX), chart_y0, chart_h);
                let x = chart_x0 + (i as f32 + 0.5) * bar_w;
                Some(Pos2::new(x, y))
            })
            .collect();
        let cyan = Color32::from_rgba_unmultiplied(80, 220, 230, 200);
        if snap_pts.len() >= 2 {
            // Dashed look: render every other unit segment along each pair.
            for pair in snap_pts.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let total_len = (b - a).length();
                let dash_len = 4.0_f32;
                let gap_len = 3.0_f32;
                let step = dash_len + gap_len;
                let mut t = 0.0_f32;
                while t < total_len {
                    let t0 = t / total_len;
                    let t1 = ((t + dash_len) / total_len).min(1.0);
                    let p0 = a + (b - a) * t0;
                    let p1 = a + (b - a) * t1;
                    painter.line_segment([p0, p1], Stroke::new(1.4, cyan));
                    t += step;
                }
            }
            for &p in &snap_pts {
                painter.circle_stroke(p, 2.0, Stroke::new(1.0, cyan));
            }
        }
    }
}

/// Draw per-track spectrum as overlaid coloured lines (no bars).
/// Used alongside the master spectrum to show individual track contributions.
pub fn track_overlay_lines(
    ui: &mut egui::Ui,
    tracks: &[(String, [f32; 10])],
) {
    if tracks.is_empty() {
        return;
    }

    let desired = egui::vec2(ui.available_width(), 60.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Color32::from_rgb(18, 18, 22));

    let y_label_w = 26.0;
    let x_label_h = 14.0;
    let chart_x0 = rect.left() + y_label_w;
    let chart_y0 = rect.top() + 4.0;
    let chart_w = rect.width() - y_label_w;
    let chart_h = rect.height() - x_label_h - 4.0;
    let n = 10usize;
    let bar_w = chart_w / n as f32;

    // Light gridlines
    for &db in &[0.0f32, -36.0] {
        let y = db_to_y(db, chart_y0, chart_h);
        painter.line_segment(
            [Pos2::new(chart_x0, y), Pos2::new(rect.right(), y)],
            Stroke::new(0.5, Color32::from_rgb(40, 40, 45)),
        );
    }

    let palette = TRACK_PALETTE;
    for (ti, (_, bands)) in tracks.iter().enumerate() {
        let (r, g, b) = palette[ti % palette.len()];
        let color = Color32::from_rgba_unmultiplied(r, g, b, 200);
        let pts: Vec<Pos2> = (0..n)
            .filter_map(|i| {
                let db = bands[i];
                if !db.is_finite() || db < DB_MIN {
                    return None;
                }
                let y = db_to_y(db.clamp(DB_MIN, DB_MAX), chart_y0, chart_h);
                let x = chart_x0 + (i as f32 + 0.5) * bar_w;
                Some(Pos2::new(x, y))
            })
            .collect();

        if pts.len() >= 2 {
            for pair in pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], Stroke::new(1.2, color));
            }
        }
    }

    // X-axis labels
    for i in 0..n {
        let x = chart_x0 + (i as f32 + 0.5) * bar_w;
        painter.text(
            Pos2::new(x, rect.bottom() - 1.0),
            egui::Align2::CENTER_BOTTOM,
            BAND_LABELS[i],
            egui::FontId::proportional(8.0),
            Color32::from_rgb(90, 90, 100),
        );
    }
}

/// Track legend: coloured dot + name in a horizontal flow.
pub fn track_legend(ui: &mut egui::Ui, names: &[String]) {
    ui.horizontal_wrapped(|ui| {
        let palette = TRACK_PALETTE;
        for (i, name) in names.iter().enumerate() {
            let (r, g, b) = palette[i % palette.len()];
            let dot_color = Color32::from_rgb(r, g, b);
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot_rect.center(), 4.0, dot_color);
            ui.label(egui::RichText::new(name).size(9.0));
            ui.add_space(4.0);
        }
    });
}

fn db_to_y(db: f32, chart_y0: f32, chart_h: f32) -> f32 {
    let t = (DB_MAX - db) / (DB_MAX - DB_MIN);
    chart_y0 + t * chart_h
}

const TRACK_PALETTE: &[(u8, u8, u8)] = &[
    (100, 160, 240),
    (240, 160, 50),
    (80, 200, 100),
    (220, 80,  80),
    (180, 100, 230),
    (60,  210, 210),
    (240, 200, 60),
    (240, 120, 180),
];
