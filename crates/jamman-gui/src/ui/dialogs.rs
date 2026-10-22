//! Diálogos modales: borrar, cambios sin guardar e informe de importación.

use eframe::egui::{self, Color32, Id, RichText};

use crate::app::JamApp;
use crate::theme;

pub fn show(app: &mut JamApp, ctx: &egui::Context) {
    confirm_delete(app, ctx);
    unsaved_changes(app, ctx);
    import_report(app, ctx);
}

fn confirm_delete(app: &mut JamApp, ctx: &egui::Context) {
    let Some(slot) = app.confirm_delete else {
        return;
    };
    let name = app
        .lib
        .row(slot)
        .map(|r| r.name().to_owned())
        .unwrap_or_default();
    let mut decision = None;
    let response = egui::Modal::new(Id::unique("confirm_delete")).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.heading("¿Borrar el bucle?");
        ui.add_space(6.0);
        ui.label(format!(
            "Se borrará el bucle {:02} «{name}» de la tarjeta. No se puede deshacer.",
            slot.get()
        ));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let delete =
                egui::Button::new(RichText::new("🗑 Borrar").strong().color(Color32::WHITE))
                    .fill(theme::DANGER);
            if ui.add(delete).clicked() {
                decision = Some(true);
            }
            if ui.button("Cancelar").clicked() {
                decision = Some(false);
            }
        });
    });
    if decision.is_none() && response.should_close() {
        decision = Some(false);
    }
    match decision {
        Some(true) => {
            app.confirm_delete = None;
            app.delete(slot);
        }
        Some(false) => app.confirm_delete = None,
        None => {}
    }
}

fn unsaved_changes(app: &mut JamApp, ctx: &egui::Context) {
    let Some(next) = app.pending_select else {
        return;
    };
    #[derive(Clone, Copy)]
    enum Choice {
        Save,
        Discard,
        Cancel,
    }
    let mut choice = None;
    let response = egui::Modal::new(Id::unique("unsaved")).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.heading("Cambios sin guardar");
        ui.add_space(6.0);
        ui.label("El bucle seleccionado tiene cambios sin guardar.");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("💾 Guardar").strong()).clicked() {
                choice = Some(Choice::Save);
            }
            if ui.button("Descartar").clicked() {
                choice = Some(Choice::Discard);
            }
            if ui.button("Cancelar").clicked() {
                choice = Some(Choice::Cancel);
            }
        });
    });
    if choice.is_none() && response.should_close() {
        choice = Some(Choice::Cancel);
    }
    let Some(choice) = choice else {
        return;
    };
    app.pending_select = None;
    match choice {
        Choice::Save => {
            app.save();
            if !app.lib.has_unsaved_changes() {
                app.lib.select(next);
            }
        }
        Choice::Discard => app.lib.select(next),
        Choice::Cancel => {}
    }
}

fn import_report(app: &mut JamApp, ctx: &egui::Context) {
    let Some(failures) = &app.import_report else {
        return;
    };
    let mut close = false;
    let response = egui::Modal::new(Id::unique("import_report")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading("Algunas canciones no se pudieron añadir");
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (file, error) in failures {
                    ui.label(RichText::new(file).strong());
                    ui.label(RichText::new(error).color(theme::DANGER));
                    ui.add_space(4.0);
                }
            });
        ui.add_space(8.0);
        if ui.button("Cerrar").clicked() {
            close = true;
        }
    });
    if close || response.should_close() {
        app.import_report = None;
    }
}
