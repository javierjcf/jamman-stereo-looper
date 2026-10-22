//! Panel izquierdo: tarjeta, ocupación e importación.

use eframe::egui::{self, RichText};
use jamman_core::device::SLOTS_PER_STORAGE;

use crate::app::JamApp;
use crate::fmt;

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.heading("Tarjeta SD");
    ui.add_space(4.0);

    let current = app.lib.mount().map(|p| p.to_path_buf());
    let mut open = None;
    if app.detected.is_empty() {
        ui.weak("No se ha detectado ninguna tarjeta.");
    }
    for path in &app.detected {
        let label = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        if ui
            .selectable_label(
                current.as_deref() == Some(path.as_path()),
                format!("💾 {label}"),
            )
            .on_hover_text(path.display().to_string())
            .clicked()
        {
            open = Some(path.clone());
        }
    }
    if let Some(path) = open {
        app.open_card(&path);
    }

    ui.horizontal(|ui| {
        if ui.button("🔍 Detectar").clicked() {
            app.detect_cards();
        }
        if ui.button("📂 Abrir carpeta…").clicked() {
            app.pick_card_folder();
        }
    });

    if let Some(mount) = &current {
        ui.add_space(12.0);
        ui.separator();
        ui.small(mount.display().to_string());
        ui.add_space(6.0);

        let used = app.lib.rows().len();
        ui.label(format!("Posiciones: {used} de {SLOTS_PER_STORAGE}"));
        ui.add(egui::ProgressBar::new(
            used as f32 / f32::from(SLOTS_PER_STORAGE),
        ));
        ui.add_space(6.0);

        if let Some((free, total)) = app.space {
            ui.label(format!(
                "Espacio: {} libres de {}",
                fmt::bytes(free),
                fmt::bytes(total)
            ));
            let used_fraction = if total > 0 {
                1.0 - free as f32 / total as f32
            } else {
                0.0
            };
            ui.add(egui::ProgressBar::new(used_fraction));
            // 10,6 MB por minuto de audio del pedal.
            let minutes = free as f64 / 10_584_000.0;
            ui.weak(format!("≈ {} de audio más", fmt::duration(minutes * 60.0)));
        }
    }

    ui.add_space(12.0);
    ui.separator();
    ui.label(RichText::new("Al añadir canciones").strong());
    ui.checkbox(&mut app.normalize, "Normalizar volumen")
        .on_hover_text("Sube o baja el volumen para que el pico quede justo por debajo de 0 dB");
    ui.weak(
        "MP3, FLAC, OGG, AIFF, M4A y WAV se convierten solos a WAV 44,1 kHz, \
         16 bit, estéreo. Máximo 10 minutos por bucle.",
    );

    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(6.0);
        ui.weak("Expulsa la tarjeta antes de sacarla.");
    });
}
