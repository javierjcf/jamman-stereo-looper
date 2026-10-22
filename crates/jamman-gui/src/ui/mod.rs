//! Vistas de `egui`. Solo dibujan y llaman a las acciones de [`JamApp`].
//!
//! Disposición, como en un cliente SFTP: explorador del equipo a la izquierda,
//! tarjeta en el centro y ajustes del bucle seleccionado a la derecha.

mod browser;
mod card_bar;
mod details;
mod dialogs;
mod player_bar;
mod status_bar;
mod table;
mod toolbar;

use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers};

use crate::app::JamApp;
use crate::selection::ClickMods;
use crate::theme;

/// Modificadores de selección del clic actual (Ctrl/Cmd y Mayús).
pub fn click_mods(ui: &egui::Ui) -> ClickMods {
    ui.input(|i| ClickMods {
        toggle: i.modifiers.command,
        range: i.modifiers.shift,
    })
}

/// Dibuja la ventana completa.
pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    handle_dropped_files(app, &ctx);
    handle_shortcuts(app, &ctx);

    egui::Panel::top("toolbar").show(ui, |ui| toolbar::show(app, ui));
    if app.lib.selected().is_some() {
        egui::Panel::top("player").show(ui, |ui| player_bar::show(app, ui));
    }
    egui::Panel::bottom("status").show(ui, |ui| status_bar::show(app, ui));
    if app.show_browser {
        egui::Panel::left("browser")
            .resizable(true)
            .default_size(330.0)
            .size_range(260.0..=640.0)
            .show(ui, |ui| browser::show(app, ui));
    }
    if app.lib.draft.is_some() {
        egui::Panel::right("details")
            .default_size(310.0)
            .size_range(280.0..=520.0)
            .show(ui, |ui| details::show(app, ui));
    }
    egui::CentralPanel::default().show(ui, |ui| {
        card_bar::show(app, ui);
        ui.separator();
        table::show(app, ui);
    });

    dialogs::show(app, &ctx);
    paint_drop_hint(app, &ctx);
}

fn handle_dropped_files(app: &mut JamApp, ctx: &egui::Context) {
    let dropped: Vec<_> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    });
    if !dropped.is_empty() {
        app.start_import(dropped, ctx);
    }
}

fn handle_shortcuts(app: &mut JamApp, ctx: &egui::Context) {
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
        app.save();
    }
    // Las teclas sueltas no deben robar el texto que se está escribiendo.
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
        app.toggle_play();
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
        app.ask_delete_selection();
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) && app.lib.card().is_some() {
        app.lib.select_all_visible();
    }
}

/// Aviso a pantalla completa mientras se arrastran ficheros sobre la ventana.
fn paint_drop_hint(app: &JamApp, ctx: &egui::Context) {
    if ctx.input(|i| i.raw.hovered_files.is_empty()) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("drop_hint"),
    ));
    let rect = ctx.content_rect();
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(190));
    let text = if app.lib.card().is_some() {
        "Suelta las canciones para añadirlas a la tarjeta"
    } else {
        "Abre primero una tarjeta"
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(26.0),
        theme::ACCENT,
    );
}
