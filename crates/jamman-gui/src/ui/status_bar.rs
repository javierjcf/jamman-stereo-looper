//! Barra inferior: mensajes y progreso de la importación.

use eframe::egui::{self, RichText};

use crate::app::{JamApp, StatusKind};
use crate::{fmt, theme};

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if let Some(import) = &app.import {
            ui.spinner();
            ui.label(format!(
                "Importando {}/{}: {}",
                import.index + 1,
                import.total,
                import.file
            ));
            ui.add(
                egui::ProgressBar::new(import.progress)
                    .desired_width(160.0)
                    .show_percentage(),
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
