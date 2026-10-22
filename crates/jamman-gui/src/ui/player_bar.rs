//! Reproductor del bucle seleccionado con su forma de onda.

use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, pos2, vec2};

use crate::app::JamApp;
use crate::{fmt, theme};

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    let Some(slot) = app.lib.selected() else {
        return;
    };
    let Some(row) = app.lib.row(slot).cloned() else {
        return;
    };
    let is_current = app.player.current() == Some(slot) && !app.player.finished();
    let playing = is_current && app.player.is_playing();
    let position = if is_current {
        app.player.position_secs()
    } else {
        0.0
    };

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let big = |icon: &str| {
            egui::Button::new(RichText::new(icon).size(20.0)).min_size(vec2(42.0, 34.0))
        };
        if ui
            .add(big(if playing { "⏸" } else { "▶" }))
            .on_hover_text("Reproducir / pausar (barra espaciadora)")
            .clicked()
        {
            app.toggle_play();
        }
        if ui
            .add_enabled(is_current, big("⏹"))
            .on_hover_text("Detener")
            .clicked()
        {
            app.player.stop();
        }
        ui.vertical(|ui| {
            ui.label(
                RichText::new(format!("{:02} · {}", slot.get(), row.name()))
                    .strong()
                    .size(15.0),
            );
            ui.weak(format!(
                "{} / {}",
                fmt::duration(position),
                fmt::duration(row.duration_secs)
            ));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut volume = app.player.volume();
            if ui
                .add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false))
                .changed()
            {
                app.player.set_volume(volume);
            }
            ui.label("🔊");
        });
    });

    // Forma de onda: clic para saltar a un punto.
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let bg = ui.visuals().extreme_bg_color;
    let weak = ui.visuals().weak_text_color();
    painter.rect_filled(rect, 6.0, bg);
    let fraction = if row.duration_secs > 0.0 {
        (position / row.duration_secs) as f32
    } else {
        0.0
    };
    let ctx = ui.ctx().clone();
    match app.waveform(slot, &ctx) {
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Calculando forma de onda…",
                FontId::proportional(13.0),
                weak,
            );
        }
        Some(Err(_)) => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Forma de onda no disponible",
                FontId::proportional(13.0),
                weak,
            );
        }
        Some(Ok(peaks)) if !peaks.is_empty() => {
            let width = rect.width().max(1.0) as usize;
            let mid = rect.center().y;
            for x in 0..width {
                let from = x * peaks.len() / width;
                let to = ((x + 1) * peaks.len() / width)
                    .max(from + 1)
                    .min(peaks.len());
                let amp = peaks[from..to].iter().copied().fold(0.0, f32::max);
                let h = (amp * rect.height() * 0.46).max(0.5);
                let cx = rect.left() + x as f32 + 0.5;
                let played = is_current && (x as f32 / width as f32) < fraction;
                let color = if played { theme::ACCENT } else { weak };
                painter.line_segment(
                    [pos2(cx, mid - h), pos2(cx, mid + h)],
                    Stroke::new(1.0, color),
                );
            }
        }
        Some(Ok(_)) => {}
    }
    if is_current {
        let x = rect.left() + fraction * rect.width();
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(2.0, theme::ACCENT),
        );
    }
    if let Some(p) = response.hover_pos() {
        painter.line_segment(
            [pos2(p.x, rect.top()), pos2(p.x, rect.bottom())],
            Stroke::new(1.0, weak),
        );
    }
    if (response.clicked() || response.drag_stopped())
        && let Some(p) = response.interact_pointer_pos()
    {
        let secs =
            f64::from(((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0)) * row.duration_secs;
        if is_current {
            app.player.seek(secs);
        } else {
            app.play(slot, secs);
        }
    }
    response.on_hover_text("Clic para escuchar desde ese punto");
    ui.add_space(6.0);
}
