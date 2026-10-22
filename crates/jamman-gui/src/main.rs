//! Interfaz gráfica de jamman-stereo-looper.
//!
//! Gestiona los bucles de la tarjeta SD del DigiTech JamMan Stereo sin
//! necesitar el pedal: escuchar, añadir (con conversión automática), editar
//! ajustes, mover, exportar y borrar.

mod app;
mod browser;
mod detect;
mod devshot;
mod fmt;
mod jobs;
mod player;
mod selection;
mod state;
mod theme;
mod ui;
mod waveform_cache;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("JamMan Stereo Looper")
            .with_app_id("jamman-stereo-looper")
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([980.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "JamMan Stereo Looper",
        options,
        Box::new(|cc| Ok(Box::new(app::JamApp::new(cc)))),
    )
}
