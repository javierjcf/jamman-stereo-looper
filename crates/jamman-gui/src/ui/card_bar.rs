//! Cabecera de la tarjeta: cuál está abierta, ocupación y acciones sobre la
//! selección.

use eframe::egui::{self, Align, Layout, RichText};
use jamman_core::device::SLOTS_PER_STORAGE;

use crate::app::JamApp;
use crate::{fmt, theme};

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("💾 Tarjeta").strong().size(15.0));
        card_selector(app, ui);
        if ui.button("🔍").on_hover_text("Detectar tarjetas").clicked() {
            app.detect_cards();
        }
        if ui
            .button("📂")
            .on_hover_text("Abrir una carpeta como tarjeta…")
            .clicked()
        {
            app.pick_card_folder();
        }
        if ui
            .add_enabled(app.lib.card().is_some(), egui::Button::new("⟳"))
            .on_hover_text("Volver a leer la tarjeta")
            .clicked()
        {
            app.reload();
        }

        if app.lib.card().is_some() {
            ui.separator();
            let used = app.lib.rows().len();
            ui.label(format!("{used}/{SLOTS_PER_STORAGE}"))
                .on_hover_text("Posiciones ocupadas");
            ui.add(
                egui::ProgressBar::new(used as f32 / f32::from(SLOTS_PER_STORAGE))
                    .desired_width(80.0)
                    .fill(theme::ACCENT),
            );
            if let Some((free, total)) = app.space {
                // 10,6 MB por minuto de audio del pedal.
                let minutes = free as f64 / 10_584_000.0;
                ui.label(format!("{} libres", fmt::bytes(free)))
                    .on_hover_text(format!(
                        "{} libres de {} · caben ≈ {} de audio",
                        fmt::bytes(free),
                        fmt::bytes(total),
                        fmt::duration(minutes * 60.0)
                    ));
            }
        }
    });

    if app.lib.card().is_none() {
        return;
    }
    let selected = app.lib.selection().len();
    ui.horizontal(|ui| {
        let export = if app.show_browser {
            format!("⬅ Exportar a la carpeta ({selected})")
        } else {
            format!("⬅ Exportar ({selected})")
        };
        if ui
            .add_enabled(
                selected > 0 && app.can_transfer(),
                egui::Button::new(export),
            )
            .on_hover_text(format!(
                "Copia el audio de los bucles seleccionados a {}",
                app.browser.dir().display()
            ))
            .clicked()
        {
            app.export_selection(ui.ctx());
        }
        let delete =
            egui::Button::new(RichText::new(format!("🗑 Borrar ({selected})")).color(theme::DANGER));
        if ui.add_enabled(selected > 0, delete).clicked() {
            app.ask_delete_selection();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.weak("Ctrl+A: todo · Supr: borrar · Espacio: escuchar");
        });
    });
}

fn card_selector(app: &mut JamApp, ui: &mut egui::Ui) {
    let current = app.lib.mount().map(|p| p.to_path_buf());
    let label = |p: &std::path::Path| {
        p.file_name().map_or_else(
            || p.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    };
    let selected_text = current
        .as_deref()
        .map_or_else(|| "Ninguna abierta".to_owned(), label);
    let mut open = None;
    egui::ComboBox::from_id_salt("card_selector")
        .selected_text(selected_text)
        .width(170.0)
        .show_ui(ui, |ui| {
            if app.detected.is_empty() {
                ui.weak("No se ha detectado ninguna tarjeta");
            }
            for path in &app.detected {
                if ui
                    .selectable_label(current.as_deref() == Some(path.as_path()), label(path))
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    open = Some(path.clone());
                }
            }
        })
        .response
        .on_hover_text(
            current
                .as_deref()
                .map_or_else(String::new, |p| p.display().to_string()),
        );
    if let Some(path) = open {
        app.open_card(&path);
    }
}
