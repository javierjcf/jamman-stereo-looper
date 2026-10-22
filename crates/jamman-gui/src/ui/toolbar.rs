//! Barra superior: título, añadir, búsqueda y tema.

use eframe::egui::{self, RichText};

use crate::app::JamApp;
use crate::theme;

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("◉ JamMan Stereo")
                .strong()
                .size(18.0)
                .color(theme::ACCENT),
        );
        ui.label(RichText::new("Looper").size(18.0));
        ui.separator();

        let has_card = app.lib.card().is_some();
        let add = egui::Button::new(RichText::new("＋ Añadir canciones…").strong());
        if ui
            .add_enabled(has_card && app.import.is_none(), add)
            .on_hover_text("También puedes arrastrar ficheros o carpetas a la ventana")
            .clicked()
        {
            app.pick_and_import(ui.ctx());
        }
        if ui
            .add_enabled(has_card, egui::Button::new("⟳ Recargar"))
            .on_hover_text("Vuelve a leer la tarjeta")
            .clicked()
        {
            app.reload();
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::widgets::global_theme_preference_buttons(ui);
            ui.add(
                egui::TextEdit::singleline(&mut app.lib.filter)
                    .hint_text("🔍 Buscar por nombre, artista, nº…")
                    .desired_width(260.0),
            );
        });
    });
    ui.add_space(4.0);
}
