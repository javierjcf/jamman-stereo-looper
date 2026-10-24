//! Captura de la ventana para desarrollo y documentación.
//!
//! Si la variable `JAMMAN_SCREENSHOT` contiene una ruta, tras unos fotogramas
//! la app guarda su ventana como imagen PPM y se cierra. Con
//! `JAMMAN_SCREENSHOT_SLOT=N` selecciona antes el bucle `N`, con
//! `JAMMAN_SCREENSHOT_THEME=light|dark` fuerza el tema y con
//! `JAMMAN_SCREENSHOT_BROWSER=carpeta` abre esa carpeta en el explorador y con
//! `JAMMAN_SCREENSHOT_WAIT=segundos` cambia la espera (por defecto 5 s). Para
//! reproducir interacciones, `JAMMAN_SCREENSHOT_SCROLL=píxeles` desplaza la
//! tabla de bucles al segundo y `JAMMAN_SCREENSHOT_DELETE=N` pide borrar el bucle
//! `N` a los dos segundos y lo confirma al tercero. Sirve para revisar el diseño y generar las capturas del
//! README:
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
    /// Carpeta que se abre en el explorador.
    pub browser_dir: Option<PathBuf>,
    /// Píxeles que se desplaza la tabla (una vez).
    scroll: Option<f32>,
    /// Bucle que se borra (una vez).
    delete_slot: Option<u8>,
    started: Instant,
    wait: Duration,
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
            browser_dir: std::env::var_os("JAMMAN_SCREENSHOT_BROWSER").map(PathBuf::from),
            scroll: std::env::var("JAMMAN_SCREENSHOT_SCROLL")
                .ok()
                .and_then(|s| s.parse().ok()),
            delete_slot: std::env::var("JAMMAN_SCREENSHOT_DELETE")
                .ok()
                .and_then(|s| s.parse().ok()),
            started: Instant::now(),
            wait: std::env::var("JAMMAN_SCREENSHOT_WAIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .map_or(WARMUP, Duration::from_secs),
            requested: false,
            done: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Añade a la entrada la rueda del ratón pedida, sobre la tabla.
    pub fn inject_input(&mut self, raw: &mut egui::RawInput) {
        if self.started.elapsed() < Duration::from_secs(1) {
            return;
        }
        if let Some(pixels) = self.scroll.take() {
            let over_table = egui::pos2(700.0, 500.0);
            raw.events.push(egui::Event::PointerMoved(over_table));
            raw.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -pixels),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
                source: egui::MouseWheelSource::Wheel,
            });
        }
    }

    /// Bucle que toca borrar ahora, si se pidió.
    pub fn take_delete(&mut self) -> Option<u8> {
        if self.started.elapsed() < Duration::from_secs(2) {
            return None;
        }
        self.delete_slot.take()
    }

    /// `true` cuando toca confirmar el borrado pedido (un segundo después de
    /// abrir el diálogo).
    pub fn confirm_now(&self) -> bool {
        self.delete_slot.is_none() && self.started.elapsed() >= Duration::from_secs(3)
    }

    /// Avanza un fotograma; captura y cierra cuando toca.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if self.done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        ctx.request_repaint();
        if self.started.elapsed() < self.wait {
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
