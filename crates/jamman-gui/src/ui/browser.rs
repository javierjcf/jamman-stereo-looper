//! Panel izquierdo: explorador de ficheros del equipo.

use eframe::egui::{self, Align, Layout, RichText, Sense};
use egui_extras::{Column, TableBuilder};

use jamman_core::device::{SLOTS_PER_STORAGE, Slot};

use crate::app::JamApp;
use crate::{fmt, theme};

/// Acción pedida desde una fila del explorador.
enum Action {
    Click(std::path::PathBuf),
    Open(std::path::PathBuf),
    Import(std::path::PathBuf),
}

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("📁 Equipo").strong().size(15.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("📂").on_hover_text("Elegir carpeta…").clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_directory(app.browser.dir())
                    .pick_folder()
            {
                app.browser.navigate(dir);
            }
            if ui.button("⟳").on_hover_text("Actualizar").clicked() {
                app.browser.refresh();
            }
            if ui.button("🏠").on_hover_text("Carpeta personal").clicked() {
                app.browser.go_home();
            }
            if ui.button("⬆").on_hover_text("Carpeta superior").clicked() {
                app.browser.up();
            }
        });
    });
    ui.add(egui::Label::new(RichText::new(app.browser.display_dir()).small()).truncate())
        .on_hover_text(app.browser.dir().display().to_string());
    if let Some(err) = &app.browser.error {
        ui.label(RichText::new(err).color(theme::DANGER).small());
    }

    // Pie fijo con las acciones; la lista ocupa el resto.
    egui::Panel::bottom("browser_footer")
        .show_separator_line(true)
        .show(ui, |ui| footer(app, ui));

    let entries = app.browser.entries().to_vec();
    if entries.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.weak("No hay carpetas ni audio compatible aquí."));
        return;
    }
    let mut action = None;
    TableBuilder::new(ui)
        .id_salt("browser_table")
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::remainder().clip(true))
        .column(Column::exact(64.0))
        .body(|body| {
            body.rows(26.0, entries.len(), |mut row| {
                let e = &entries[row.index()];
                row.set_selected(app.browser.selection.contains(&e.path));
                row.col(|ui| {
                    let icon = if e.is_dir { "📁" } else { "🎵" };
                    ui.add(
                        egui::Label::new(format!("{icon} {}", e.name))
                            .truncate()
                            .selectable(false),
                    );
                });
                row.col(|ui| {
                    if !e.is_dir {
                        ui.weak(fmt::bytes(e.size));
                    }
                });
                let response = row.response();
                if response.double_clicked() {
                    action = Some(if e.is_dir {
                        Action::Open(e.path.clone())
                    } else {
                        Action::Import(e.path.clone())
                    });
                } else if response.clicked() {
                    action = Some(Action::Click(e.path.clone()));
                }
                if !e.is_dir {
                    response.on_hover_text("Doble clic: añadir a la tarjeta");
                }
            });
        });

    match action {
        Some(Action::Click(path)) => {
            let mods = super::click_mods(ui);
            let order = app.browser.order();
            app.browser.selection.click(path, mods, &order);
        }
        Some(Action::Open(dir)) => app.browser.navigate(dir),
        Some(Action::Import(file)) => app.start_import(vec![file], ui.ctx()),
        None => {}
    }
}

fn footer(app: &mut JamApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let selected = app.browser.selection.len();
    let target = if app.lib.pedal().is_some() {
        "al pedal"
    } else {
        "a la tarjeta"
    };
    ui.horizontal(|ui| {
        ui.label("Desde la posición");
        let mut n = app.import_slot().map_or(1, Slot::get);
        if ui
            .add(egui::DragValue::new(&mut n).range(1..=SLOTS_PER_STORAGE))
            .on_hover_text("Las canciones van a los huecos libres a partir de aquí")
            .changed()
        {
            app.import_start = Slot::new(n).ok();
        }
        match app.import_blocked() {
            Some(why) => {
                let text = if app.import_slot().is_some() {
                    "ocupada"
                } else {
                    "sin huecos"
                };
                ui.colored_label(theme::DANGER, text).on_hover_text(why);
            }
            None => {
                ui.weak("libre");
            }
        }
    });
    ui.horizontal(|ui| {
        let label = if selected == 0 {
            format!("➡ Añadir {target}")
        } else {
            format!("➡ Añadir {target} ({selected})")
        };
        let button = egui::Button::new(RichText::new(label).strong());
        if ui
            .add_enabled(
                app.can_transfer()
                    && !app.browser.selection.is_empty()
                    && app.import_blocked().is_none(),
                button,
            )
            .on_hover_text(format!(
                "Convierte y copia {target} lo seleccionado (ficheros o carpetas)"
            ))
            .on_disabled_hover_text(
                app.import_blocked().unwrap_or_else(|| {
                    "Selecciona canciones y abre una tarjeta o el pedal".to_owned()
                }),
            )
            .clicked()
        {
            app.import_from_browser(ui.ctx());
        }
    });
    ui.checkbox(&mut app.normalize, "Normalizar volumen")
        .on_hover_text("Sube o baja el volumen para que el pico quede justo por debajo de 0 dB");
    ui.weak(
        "Ctrl+clic y Mayús+clic para seleccionar varias. Doble clic en una canción para añadirla.",
    );
    ui.add_space(2.0);
}
