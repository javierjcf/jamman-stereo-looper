//! La aplicación: estado más acciones. Las vistas están en `ui/`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui;
use jamman_audio::SUPPORTED_EXTENSIONS;
use jamman_core::device::Slot;

use crate::jobs::{self, ImportEvent, WaveformResult};
use crate::player::Player;
use crate::state::Library;
use crate::{detect, fmt, theme, ui};

/// Tramos de la forma de onda. 400 bastan para dibujarla y, en una tarjeta
/// SD lenta, se leen en unos 2 s la primera vez (luego se usa la caché).
pub const WAVEFORM_BUCKETS: usize = 400;

/// Recordatorio tras escribir en la tarjeta.
const EJECT_HINT: &str = "Expulsa la tarjeta antes de sacarla.";

/// Tipo de mensaje de la barra de estado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Informativo.
    Info,
    /// Error.
    Error,
}

/// Mensaje de la barra de estado.
pub struct Status {
    /// Texto.
    pub text: String,
    /// Tipo.
    pub kind: StatusKind,
}

/// Importación en curso.
pub struct ImportState {
    rx: Receiver<ImportEvent>,
    /// Fichero actual.
    pub file: String,
    /// Índice del fichero actual (desde 0).
    pub index: usize,
    /// Ficheros en total.
    pub total: usize,
    /// Progreso del fichero actual (0 a 1).
    pub progress: f32,
    imported: usize,
    last_imported: Option<Slot>,
    failures: Vec<(String, String)>,
}

/// Estado completo de la aplicación.
pub struct JamApp {
    /// Biblioteca (tarjeta abierta y vista).
    pub lib: Library,
    /// Reproductor.
    pub player: Player,
    /// Tarjetas detectadas.
    pub detected: Vec<PathBuf>,
    /// Normalizar el volumen al importar.
    pub normalize: bool,
    /// Último mensaje.
    pub status: Option<Status>,
    /// Importación en curso.
    pub import: Option<ImportState>,
    /// Errores de la última importación, para mostrarlos.
    pub import_report: Option<Vec<(String, String)>>,
    /// Bucle pendiente de confirmar su borrado.
    pub confirm_delete: Option<Slot>,
    /// Selección pendiente mientras se decide qué hacer con cambios sin guardar.
    pub pending_select: Option<Option<Slot>>,
    /// Posición de destino del control "mover a".
    pub move_target: (Option<Slot>, u8),
    /// Espacio libre y total de la tarjeta.
    pub space: Option<(u64, u64)>,
    devshot: Option<crate::devshot::DevShot>,
    waveforms: HashMap<Slot, Result<Vec<f32>, String>>,
    waveform_jobs: Vec<Receiver<WaveformResult>>,
    waveform_pending: HashSet<Slot>,
}

impl JamApp {
    /// Crea la aplicación y abre la tarjeta si hay exactamente una conectada.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        if let Some(theme) = crate::devshot::forced_theme() {
            cc.egui_ctx.set_theme(theme);
        }
        let mut app = Self {
            lib: Library::default(),
            player: Player::default(),
            detected: detect::find_cards(),
            normalize: false,
            status: None,
            import: None,
            import_report: None,
            confirm_delete: None,
            pending_select: None,
            move_target: (None, 1),
            space: None,
            devshot: crate::devshot::DevShot::from_env(),
            waveforms: HashMap::new(),
            waveform_jobs: Vec::new(),
            waveform_pending: HashSet::new(),
        };
        // `jamman-gui <ruta>` abre esa tarjeta; si no, la única detectada.
        if let Some(path) = std::env::args_os().nth(1) {
            app.open_card(Path::new(&path));
        } else if let [only] = app.detected.as_slice() {
            let path = only.clone();
            app.open_card(&path);
        }
        if let Some(slot) = app
            .devshot
            .as_ref()
            .and_then(|d| d.select_slot)
            .and_then(|n| Slot::new(n).ok())
        {
            app.lib.select(Some(slot));
        }
        app
    }

    /// Mensaje informativo.
    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            kind: StatusKind::Info,
        });
    }

    /// Mensaje de error.
    pub fn error(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            kind: StatusKind::Error,
        });
    }

    /// Vuelve a buscar tarjetas conectadas.
    pub fn detect_cards(&mut self) {
        self.detected = detect::find_cards();
        self.info(match self.detected.len() {
            0 => "No se ha detectado ninguna tarjeta del JamMan.".to_owned(),
            1 => "Se ha detectado 1 tarjeta.".to_owned(),
            n => format!("Se han detectado {n} tarjetas."),
        });
    }

    /// Abre una tarjeta.
    pub fn open_card(&mut self, path: &Path) {
        self.player.stop();
        match self.lib.open(path) {
            Ok(()) => {
                self.after_change();
                let n = self.lib.rows().len();
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                self.info(format!("Tarjeta abierta: {name} ({n} bucles)"));
            }
            Err(e) => self.error(format!("No se puede abrir {}: {e}", path.display())),
        }
    }

    /// Pide una carpeta y la abre como tarjeta.
    pub fn pick_card_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Elige la tarjeta del JamMan (o su carpeta JamManStereo)")
            .pick_folder()
        {
            self.open_card(&dir);
        }
    }

    /// Recarga la tarjeta abierta.
    pub fn reload(&mut self) {
        if let Err(e) = self.lib.reload() {
            self.error(format!("No se puede leer la tarjeta: {e}"));
        }
        self.after_change();
    }

    /// Tras cualquier cambio en la tarjeta: olvida formas de onda y espacio.
    fn after_change(&mut self) {
        self.waveforms.clear();
        self.waveform_pending.clear();
        self.space = self
            .lib
            .mount()
            .and_then(|m| Some((fs4::available_space(m).ok()?, fs4::total_space(m).ok()?)));
    }

    /// Selecciona un bucle, preguntando antes si hay cambios sin guardar.
    pub fn request_select(&mut self, slot: Option<Slot>) {
        if slot == self.lib.selected() {
            return;
        }
        if self.lib.has_unsaved_changes() {
            self.pending_select = Some(slot);
        } else {
            self.lib.select(slot);
        }
    }

    /// Guarda los ajustes en edición.
    pub fn save(&mut self) {
        match self.lib.save_draft() {
            Ok(()) => self.info(format!("Cambios guardados. {EJECT_HINT}")),
            Err(e) => self.error(format!("No se pudieron guardar los cambios: {e}")),
        }
    }

    /// Pide ficheros de audio y los importa.
    pub fn pick_and_import(&mut self, ctx: &egui::Context) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Añadir canciones a la tarjeta")
            .add_filter("Audio", &SUPPORTED_EXTENSIONS)
            .pick_files()
        {
            self.start_import(files, ctx);
        }
    }

    /// Importa ficheros (o el audio de las carpetas indicadas).
    pub fn start_import(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let Some(card) = self.lib.card().cloned() else {
            self.error("Abre primero una tarjeta.");
            return;
        };
        if self.import.is_some() {
            self.error("Espera a que termine la importación en curso.");
            return;
        }
        let files = audio_files(paths);
        if files.is_empty() {
            self.error("No hay ficheros de audio compatibles (WAV, MP3, FLAC, OGG, AIFF, M4A).");
            return;
        }
        self.player.stop();
        self.import = Some(ImportState {
            total: files.len(),
            rx: jobs::spawn_import(card, files, self.normalize, ctx.clone()),
            file: String::new(),
            index: 0,
            progress: 0.0,
            imported: 0,
            last_imported: None,
            failures: Vec::new(),
        });
    }

    fn poll_import(&mut self) {
        let Some(state) = &mut self.import else {
            return;
        };
        let mut changed = false;
        let mut done = false;
        loop {
            match state.rx.try_recv() {
                Ok(ImportEvent::Started { index, total, file }) => {
                    state.index = index;
                    state.total = total;
                    state.file = file;
                    state.progress = 0.0;
                }
                Ok(ImportEvent::Progress(p)) => state.progress = p,
                Ok(ImportEvent::Imported(slot)) => {
                    state.imported += 1;
                    state.last_imported = Some(slot);
                    changed = true;
                }
                Ok(ImportEvent::Failed { file, error }) => state.failures.push((file, error)),
                Ok(ImportEvent::Done) | Err(TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if changed {
            self.reload();
        }
        if done && let Some(state) = self.import.take() {
            let msg = format!(
                "Importación terminada: {} añadidas, {} con error. {EJECT_HINT}",
                state.imported,
                state.failures.len()
            );
            if !self.lib.has_unsaved_changes() && state.last_imported.is_some() {
                self.lib.select(state.last_imported);
            }
            if state.failures.is_empty() {
                self.info(msg);
            } else {
                self.error(msg);
                self.import_report = Some(state.failures);
            }
        }
    }

    /// Forma de onda de un bucle; la calcula en segundo plano la primera vez.
    pub fn waveform(
        &mut self,
        slot: Slot,
        ctx: &egui::Context,
    ) -> Option<&Result<Vec<f32>, String>> {
        if !self.waveforms.contains_key(&slot)
            && self.waveform_pending.insert(slot)
            && let Some(card) = self.lib.card()
        {
            let path = card.audio_path(slot);
            self.waveform_jobs.push(jobs::spawn_waveform(
                slot,
                path,
                WAVEFORM_BUCKETS,
                ctx.clone(),
            ));
        }
        self.waveforms.get(&slot)
    }

    fn poll_waveforms(&mut self) {
        let mut results = Vec::new();
        self.waveform_jobs.retain(|rx| match rx.try_recv() {
            Ok(r) => {
                results.push(r);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        for r in results {
            // Si la tarjeta cambió mientras se calculaba, se descarta.
            if self.waveform_pending.remove(&r.slot) {
                self.waveforms.insert(r.slot, r.peaks);
            }
        }
    }

    /// Reproduce un bucle desde `start_secs`.
    pub fn play(&mut self, slot: Slot, start_secs: f64) {
        let (Some(card), Some(row)) = (self.lib.card(), self.lib.row(slot)) else {
            return;
        };
        let path = card.audio_path(slot);
        let duration = row.duration_secs;
        if let Err(e) = self.player.play(slot, &path, duration, start_secs) {
            self.error(e);
        }
    }

    /// Reproduce o pausa el bucle seleccionado.
    pub fn toggle_play(&mut self) {
        let Some(selected) = self.lib.selected() else {
            return;
        };
        if self.player.current() == Some(selected) && !self.player.finished() {
            self.player.toggle_pause();
        } else {
            self.play(selected, 0.0);
        }
    }

    /// Borra un bucle (tras confirmación de la interfaz).
    pub fn delete(&mut self, slot: Slot) {
        if self.player.current() == Some(slot) {
            self.player.stop();
        }
        let Some(card) = self.lib.card() else {
            return;
        };
        match card.remove(slot) {
            Ok(()) => {
                self.reload();
                self.info(format!("Bucle {:02} borrado. {EJECT_HINT}", slot.get()));
            }
            Err(e) => self.error(format!("No se pudo borrar: {e}")),
        }
    }

    /// Exporta el audio de un bucle a un WAV elegido por el usuario.
    pub fn export(&mut self, slot: Slot) {
        let Some(card) = self.lib.card().cloned() else {
            return;
        };
        let name = self
            .lib
            .row(slot)
            .map(|r| r.name().to_owned())
            .unwrap_or_default();
        let Some(dest) = rfd::FileDialog::new()
            .set_title("Exportar audio del bucle")
            .set_file_name(format!("{:02} {name}.wav", slot.get()))
            .add_filter("WAV", &["wav"])
            .save_file()
        else {
            return;
        };
        // El diálogo ya pidió confirmación si el fichero existía.
        if dest.exists()
            && let Err(e) = std::fs::remove_file(&dest)
        {
            self.error(format!("No se puede sobrescribir {}: {e}", dest.display()));
            return;
        }
        match card.export_wav(slot, &dest) {
            Ok(bytes) => self.info(format!(
                "Exportado {} ({})",
                dest.display(),
                fmt::bytes(bytes)
            )),
            Err(e) => self.error(format!("No se pudo exportar: {e}")),
        }
    }

    /// Mueve un bucle a otra posición; si está ocupada, las intercambia.
    pub fn move_or_swap(&mut self, from: Slot, to: Slot) {
        if from == to {
            return;
        }
        if matches!(self.player.current(), Some(s) if s == from || s == to) {
            self.player.stop();
        }
        let Some(card) = self.lib.card() else {
            return;
        };
        let swapped = self.lib.row(to).is_some();
        match card.swap(from, to) {
            Ok(()) => {
                self.reload();
                self.lib.select(Some(to));
                self.info(if swapped {
                    format!(
                        "Posiciones {:02} y {:02} intercambiadas. {EJECT_HINT}",
                        from.get(),
                        to.get()
                    )
                } else {
                    format!(
                        "Bucle movido de {:02} a {:02}. {EJECT_HINT}",
                        from.get(),
                        to.get()
                    )
                });
            }
            Err(e) => self.error(format!("No se pudo mover: {e}")),
        }
    }
}

/// Expande carpetas (un nivel) y se queda con las extensiones admitidas.
fn audio_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let is_audio = |p: &Path| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.to_lowercase().as_str()))
    };
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(&p)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|f| f.is_file() && is_audio(f))
                .collect();
            inside.sort();
            files.extend(inside);
        } else if is_audio(&p) {
            files.push(p);
        }
    }
    files
}

impl eframe::App for JamApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_import();
        self.poll_waveforms();
        if self.player.is_playing() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(shot) = &mut self.devshot {
            shot.tick(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::show(self, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::audio_files;

    #[test]
    fn folders_are_expanded_and_non_audio_ignored() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["b.MP3", "a.wav", "notas.txt"] {
            std::fs::write(dir.path().join(f), b"").unwrap();
        }
        let files = audio_files(vec![dir.path().to_path_buf()]);
        let names: Vec<_> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(names, ["a.wav", "b.MP3"]);
        assert!(audio_files(vec![dir.path().join("notas.txt")]).is_empty());
    }
}
