//! Captura de la ventana para desarrollo y documentación.
//!
//! Si la variable `JAMMAN_SCREENSHOT` contiene una ruta, tras unos fotogramas
//! la app guarda su ventana como imagen PPM y se cierra. Con
//! `JAMMAN_SCREENSHOT_SLOT=N` selecciona antes el bucle `N` y con
//! `JAMMAN_SCREENSHOT_THEME=light|dark` fuerza el tema. Sirve para
//! revisar el diseño y generar las capturas del README:
//!
//! ```text
//! JAMMAN_SCREENSHOT=captura.ppm cargo run -p jamman-gui
//! ffmpeg -i captura.ppm captura.png
//! ```

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui::{self, ColorImage, ScreenshotCallback, ViewportCommand};

/// Tiempo que se espera antes de capturar (forma de onda, etc.).
const WARMUP: Duration = Duration::from_secs(5);

/// Captura pendiente.
pub struct DevShot {
    path: PathBuf,
    /// Bucle que se selecciona antes de capturar.
    pub select_slot: Option<u8>,
    started: Instant,
    requested: bool,
    done: Arc<AtomicBool>,
}

impl DevShot {
    /// Lee la configuración de las variables de entorno.
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("JAMMAN_SCREENSHOT")?;
        Some(Self {
            path: path.into(),
            select_slot: std::env::var("JAMMAN_SCREENSHOT_SLOT")
                .ok()
                .and_then(|s| s.parse().ok()),
            started: Instant::now(),
            requested: false,
            done: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Avanza un fotograma; captura y cierra cuando toca.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if self.done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        ctx.request_repaint();
        if self.started.elapsed() < WARMUP {
            return;
        }
        if !self.requested {
            self.requested = true;
            let (path, done, repaint) = (self.path.clone(), self.done.clone(), ctx.clone());
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(ScreenshotCallback::new(
                move |image| {
                    if let Err(e) = write_ppm(&path, &image) {
                        eprintln!("no se pudo guardar la captura: {e}");
                    }
                    done.store(true, Ordering::SeqCst);
                    repaint.request_repaint();
                },
            )));
        }
    }
}

/// Tema pedido para la captura, si se pidió alguno.
pub fn forced_theme() -> Option<egui::Theme> {
    std::env::var_os("JAMMAN_SCREENSHOT")?;
    match std::env::var("JAMMAN_SCREENSHOT_THEME").ok()?.as_str() {
        "light" => Some(egui::Theme::Light),
        "dark" => Some(egui::Theme::Dark),
        _ => None,
    }
}

/// Guarda la imagen en formato PPM (P6), que cualquier conversor entiende.
fn write_ppm(path: &PathBuf, image: &ColorImage) -> std::io::Result<()> {
    let [w, h] = image.size;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(out, "P6\n{w} {h}\n255\n")?;
    for p in &image.pixels {
        out.write_all(&[p.r(), p.g(), p.b()])?;
    }
    out.flush()
}
