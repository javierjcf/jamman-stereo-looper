//! Diálogos modales: borrar, cambios sin guardar e informe de transferencia.

use eframe::egui::{self, Color32, Id, RichText};

use crate::app::JamApp;
use crate::theme;

pub fn show(app: &mut JamApp, ctx: &egui::Context) {
    confirm_delete(app, ctx);
    unsaved_changes(app, ctx);
    transfer_report(app, ctx);
}

fn confirm_delete(app: &mut JamApp, ctx: &egui::Context) {
    let Some(slots) = app.confirm_delete.clone() else {
        return;
    };
    let names: Vec<String> = slots
        .iter()
        .map(|s| {
            let name = app
                .lib
                .row(*s)
                .map(|r| r.name().to_owned())
                .unwrap_or_default();
            format!("{:02} · {name}", s.get())
        })
        .collect();
    let mut decision = None;
    let response = egui::Modal::new(Id::unique("confirm_delete")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(match slots.len() {
            1 => "¿Borrar el bucle?".to_owned(),
            n => format!("¿Borrar {n} bucles?"),
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for n in &names {
                    ui.label(n);
                }
            });
        ui.add_space(4.0);
        ui.label("Se borrarán de la tarjeta. No se puede deshacer.");
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
            app.delete(&slots, ctx);
        }
        Some(false) => app.confirm_delete = None,
        None => {}
    }
}

fn unsaved_changes(app: &mut JamApp, ctx: &egui::Context) {
    let Some(pending) = app.pending_select else {
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
        ui.label("El bucle que estás editando tiene cambios sin guardar.");
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
            app.save(ctx);
            if !app.lib.has_unsaved_changes() {
                app.apply_pending(pending);
            }
        }
        Choice::Discard => {
            app.lib.discard_draft();
            app.apply_pending(pending);
        }
        Choice::Cancel => {}
    }
}

fn transfer_report(app: &mut JamApp, ctx: &egui::Context) {
    let Some(report) = &app.transfer_report else {
        return;
    };
    let mut close = false;
    let response = egui::Modal::new(Id::unique("transfer_report")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading(&report.title);
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (file, error) in &report.failures {
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
        app.transfer_report = None;
    }
}
