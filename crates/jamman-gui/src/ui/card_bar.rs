//! Cabecera del destino (tarjeta del PC o pedal por USB): cuál está abierto,
//! ocupación y acciones sobre la selección.

use eframe::egui::{self, Align, Layout, RichText};
use jamman_core::device::SLOTS_PER_STORAGE;
use jamman_core::pedal::Storage;

use crate::app::JamApp;
use crate::{fmt, theme};

/// Destino elegido en el selector.
enum Target {
    Card(std::path::PathBuf),
    Pedal(Storage),
}

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    let on_pedal = app.lib.pedal().is_some();
    let busy = app.transfer.is_some();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let title = if on_pedal {
            "🎸 Pedal"
        } else {
            "💾 Tarjeta"
        };
        ui.label(RichText::new(title).strong().size(15.0));
        target_selector(app, ui);
        if ui
            .button("🔍")
            .on_hover_text("Detectar tarjetas y el pedal")
            .clicked()
        {
            app.detect_cards();
        }
        if ui
            .button("📂")
            .on_hover_text("Abrir una carpeta como tarjeta (también una copia de seguridad)…")
            .clicked()
        {
            app.pick_card_folder();
        }
        if ui
            .add_enabled(app.lib.is_open() && !busy, egui::Button::new("⟳"))
            .on_hover_text("Volver a leer")
            .clicked()
        {
            app.refresh(ui.ctx());
        }

        if app.lib.is_open() && !app.lib.loading {
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

    if !app.lib.is_open() {
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
        if ui.add_enabled(selected > 0 && !busy, delete).clicked() {
            app.ask_delete_selection();
        }
        if on_pedal {
            ui.separator();
            if ui
                .add_enabled(!busy, egui::Button::new("💾 Copia de seguridad…"))
                .on_hover_text(
                    "Copia todos los bucles del pedal, con sus ajustes, a una carpeta. \
                     La copia se puede abrir con 📂 como si fuera una tarjeta.",
                )
                .clicked()
            {
                app.backup(ui.ctx());
            }
            if ui
                .add_enabled(!busy, egui::Button::new("⤴ Restaurar…"))
                .on_hover_text(
                    "Sube al pedal los bucles de una copia de seguridad, \
                     en sus mismas posiciones si están libres",
                )
                .clicked()
            {
                app.restore(ui.ctx());
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.weak(if on_pedal {
                "Ctrl+A: todo · Supr: borrar"
            } else {
                "Ctrl+A: todo · Supr: borrar · Espacio: escuchar"
            });
        });
    });
}

fn target_selector(app: &mut JamApp, ui: &mut egui::Ui) {
    let current_card = app.lib.mount().map(|p| p.to_path_buf());
    let current_pedal = app.lib.pedal();
    let label = |p: &std::path::Path| {
        p.file_name().map_or_else(
            || p.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    };
    let selected_text = match (&current_card, current_pedal) {
        (_, Some(storage)) => storage.label().to_owned(),
        (Some(path), None) => label(path),
        (None, None) => "Ninguno abierto".to_owned(),
    };
    let mut choice = None;
    egui::ComboBox::from_id_salt("target_selector")
        .selected_text(selected_text)
        .width(190.0)
        .show_ui(ui, |ui| {
            ui.label(RichText::new("Tarjetas en el equipo").small().weak());
            if app.detected.is_empty() {
                ui.weak("Ninguna detectada");
            }
            for path in &app.detected {
                if ui
                    .selectable_label(
                        current_card.as_deref() == Some(path.as_path()),
                        format!("💾 {}", label(path)),
                    )
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    choice = Some(Target::Card(path.clone()));
                }
            }
            ui.separator();
            ui.label(RichText::new("Pedal por USB").small().weak());
            if app.pedal_port.is_none() {
                ui.weak("No conectado");
            } else {
                for storage in Storage::ALL {
                    if ui
                        .selectable_label(
                            current_pedal == Some(storage),
                            format!("🎸 {}", storage.label()),
                        )
                        .clicked()
                    {
                        choice = Some(Target::Pedal(storage));
                    }
                }
            }
        });
    match choice {
        Some(Target::Card(path)) => app.open_card(&path),
        Some(Target::Pedal(storage)) => app.open_pedal(storage, ui.ctx()),
        None => {}
    }
}
