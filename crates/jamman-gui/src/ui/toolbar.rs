//! Barra superior: título, explorador, añadir, búsqueda y tema.

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

        ui.toggle_value(&mut app.show_browser, "📁 Explorador")
            .on_hover_text("Muestra u oculta el explorador de ficheros del equipo");
        let add = egui::Button::new(RichText::new("＋ Añadir canciones…").strong());
        if ui
            .add_enabled(app.can_transfer(), add)
            .on_hover_text("También puedes usar el explorador o arrastrar ficheros a la ventana")
            .clicked()
        {
            app.pick_and_import(ui.ctx());
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::widgets::global_theme_preference_buttons(ui);
            ui.add(
                egui::TextEdit::singleline(&mut app.lib.filter)
                    .hint_text("🔍 Buscar bucles…")
                    .desired_width(240.0),
            );
        });
    });
    ui.add_space(4.0);
}
