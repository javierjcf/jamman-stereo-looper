//! Barra inferior: mensajes y progreso de las transferencias.

use eframe::egui::{self, RichText};

use crate::app::{JamApp, StatusKind, TransferKind};
use crate::{fmt, theme};

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if let Some(t) = &app.transfer {
            ui.spinner();
            let verb = match t.kind {
                TransferKind::Import => "Añadiendo",
                TransferKind::Export => "Exportando",
            };
            ui.label(format!("{verb} {}/{}: {}", t.index + 1, t.total, t.file));
            ui.add(
                egui::ProgressBar::new(t.progress)
                    .desired_width(160.0)
                    .show_percentage()
                    .fill(theme::ACCENT),
            );
        } else if let Some(status) = &app.status {
            let text = RichText::new(&status.text);
            ui.label(match status.kind {
                StatusKind::Info => text,
                StatusKind::Error => text.color(theme::DANGER),
            });
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.lib.card().is_some() {
                ui.weak(format!(
                    "{} bucles · {}",
                    app.lib.rows().len(),
                    fmt::duration(app.lib.total_duration_secs())
                ));
            }
        });
    });
}
