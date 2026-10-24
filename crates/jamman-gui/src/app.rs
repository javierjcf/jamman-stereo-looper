//! La aplicación: estado más acciones. Las vistas están en `ui/`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui;
use jamman_core::device::Slot;
use jamman_core::pedal::Storage;

use crate::browser::{self, LocalBrowser};
use crate::jobs::{self, Op, TransferEvent, WaveformResult};
use crate::player::Player;
use crate::selection::ClickMods;
use crate::state::Library;
use crate::{detect, theme, ui};

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

/// Tipo de trabajo en segundo plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    /// Del equipo a la tarjeta o al pedal.
    Import,
    /// De la tarjeta o el pedal al equipo.
    Export,
    /// Leer la lista de bucles.
    List,
    /// Borrar bucles.
    Delete,
    /// Guardar ajustes.
    Save,
    /// Copia de seguridad en una carpeta del equipo.
    Backup,
    /// Traer una copia de seguridad.
    Restore,
}

impl TransferKind {
    /// Verbo para la barra de estado.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Import => "Añadiendo",
            Self::Export => "Exportando",
            Self::List => "Leyendo",
            Self::Delete => "Borrando",
            Self::Save => "Guardando",
            Self::Backup => "Copiando",
            Self::Restore => "Restaurando",
        }
    }

    /// `true` si el trabajo escribe en el destino abierto.
    fn modifies(self) -> bool {
        matches!(
            self,
            Self::Import | Self::Delete | Self::Save | Self::Restore
        )
    }
}

/// Transferencia en curso.
pub struct TransferState {
    rx: Receiver<TransferEvent>,
    /// Sentido.
    pub kind: TransferKind,
    /// Fichero actual.
    pub file: String,
    /// Índice del fichero actual (desde 0).
    pub index: usize,
    /// Ficheros en total.
    pub total: usize,
    /// Progreso del fichero actual (0 a 1).
    pub progress: f32,
    done: usize,
    last_imported: Option<Slot>,
    exported: Vec<PathBuf>,
    removed: Vec<Slot>,
    failures: Vec<(String, String)>,
}

/// Informe de errores de la última transferencia.
pub struct TransferReport {
    /// Título del diálogo.
    pub title: String,
    /// `(fichero, motivo)` de cada error.
    pub failures: Vec<(String, String)>,
}

/// Cambio de selección pendiente mientras se decide qué hacer con los cambios
/// sin guardar del bucle que se está editando.
#[derive(Debug, Clone, Copy)]
pub enum PendingSelect {
    /// Seleccionar solo este bucle (o ninguno).
    Single(Option<Slot>),
    /// Aplicar un clic con modificadores.
    Click(Slot, ClickMods),
}

/// Estado completo de la aplicación.
pub struct JamApp {
    /// Biblioteca (tarjeta abierta y vista).
    pub lib: Library,
    /// Explorador de ficheros del equipo.
    pub browser: LocalBrowser,
    /// Mostrar el explorador.
    pub show_browser: bool,
    /// Reproductor.
    pub player: Player,
    /// Tarjetas detectadas.
    pub detected: Vec<PathBuf>,
    /// Puerto del pedal conectado por USB, si lo hay.
    pub pedal_port: Option<String>,
    /// Normalizar el volumen al importar.
    pub normalize: bool,
    /// Posición desde la que se copia al importar; `None`: el primer hueco.
    pub import_start: Option<Slot>,
    /// Último mensaje.
    pub status: Option<Status>,
    /// Transferencia en curso.
    pub transfer: Option<TransferState>,
    /// Errores de la última transferencia, para mostrarlos.
    pub transfer_report: Option<TransferReport>,
    /// Bucles pendientes de confirmar su borrado.
    pub confirm_delete: Option<Vec<Slot>>,
    /// Selección pendiente por cambios sin guardar.
    pub pending_select: Option<PendingSelect>,
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
            browser: LocalBrowser::new(browser::default_start()),
            show_browser: true,
            player: Player::default(),
            detected: detect::find_cards(),
            pedal_port: jamman_core::serial::find_pedal(),
            normalize: false,
            import_start: None,
            status: None,
            transfer: None,
            transfer_report: None,
            confirm_delete: None,
            pending_select: None,
            move_target: (None, 1),
            space: None,
            devshot: crate::devshot::DevShot::from_env(),
            waveforms: HashMap::new(),
            waveform_jobs: Vec::new(),
            waveform_pending: HashSet::new(),
        };
        // `jamman-gui <ruta>` abre esa tarjeta; si no, la única detectada o,
        // si no hay ninguna, la tarjeta del pedal conectado.
        if let Some(path) = std::env::args_os().nth(1) {
            app.open_card(Path::new(&path));
        } else if let [only] = app.detected.as_slice() {
            let path = only.clone();
            app.open_card(&path);
        } else if app.detected.is_empty() && app.pedal_port.is_some() {
            app.open_pedal(Storage::Card, &cc.egui_ctx);
        }
        if let Some(shot) = &app.devshot {
            if let Some(dir) = shot.browser_dir.clone() {
                app.browser.navigate(dir);
            }
            if let Some(slot) = shot.select_slot.and_then(|n| Slot::new(n).ok()) {
                app.lib.select(Some(slot));
            }
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

    /// Vuelve a buscar tarjetas y el pedal.
    pub fn detect_cards(&mut self) {
        self.detected = detect::find_cards();
        self.pedal_port = jamman_core::serial::find_pedal();
        let cards = match self.detected.len() {
            0 => "ninguna tarjeta".to_owned(),
            1 => "1 tarjeta".to_owned(),
            n => format!("{n} tarjetas"),
        };
        let pedal = if self.pedal_port.is_some() {
            "pedal conectado por USB"
        } else {
            "pedal no conectado"
        };
        self.info(format!("Detectado: {cards}, {pedal}."));
    }

    /// Muestra un almacén del pedal y pide su lista en segundo plano.
    pub fn open_pedal(&mut self, storage: Storage, ctx: &egui::Context) {
        if self.transfer.is_some() {
            self.error("Espera a que termine la operación en curso.");
            return;
        }
        self.player.stop();
        self.lib.open_pedal(storage);
        self.space = None;
        self.after_change();
        self.start(Op::List, TransferKind::List, ctx);
    }

    /// Lanza una operación en segundo plano sobre lo que esté abierto.
    fn start(&mut self, op: Op, kind: TransferKind, ctx: &egui::Context) {
        if self.transfer.is_some() {
            self.error("Espera a que termine la operación en curso.");
            return;
        }
        let total = op.total();
        let ctx = ctx.clone();
        let rx = if let Some(storage) = self.lib.pedal() {
            // El puerto se abre en el hilo del trabajo y se cierra al acabar.
            let port = self.pedal_port.clone();
            let open = move || jamman_core::serial::open_store(port.as_deref(), storage);
            jobs::spawn(storage.label().to_owned(), open, op, ctx)
        } else if let Some(card) = self.lib.card().cloned() {
            jobs::spawn("Tarjeta".to_owned(), move || Ok(card), op, ctx)
        } else {
            self.error("Abre primero una tarjeta o el pedal.");
            return;
        };
        self.transfer = Some(TransferState::new(rx, kind, total));
    }

    /// Botón de recargar: relee la tarjeta o pide de nuevo la lista al pedal.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.lib.pedal().is_some() {
            self.start(Op::List, TransferKind::List, ctx);
        } else {
            self.reload();
        }
    }

    /// Copia de seguridad de lo abierto en una carpeta elegida.
    pub fn backup(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Carpeta para la copia de seguridad")
            .set_directory(self.browser.dir())
            .pick_folder()
        {
            self.start(Op::Backup { dir }, TransferKind::Backup, ctx);
        }
    }

    /// Trae los bucles de una copia (solo a posiciones libres).
    pub fn restore(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Copia de seguridad que se va a traer")
            .set_directory(self.browser.dir())
            .pick_folder()
        {
            self.start(Op::Restore { dir }, TransferKind::Restore, ctx);
        }
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
    /// El espacio del pedal llega con cada operación (`TransferEvent::Space`).
    fn after_change(&mut self) {
        self.waveforms.clear();
        self.waveform_pending.clear();
        if self.lib.pedal().is_some() {
            return;
        }
        self.space = self
            .lib
            .mount()
            .and_then(|m| Some((fs4::available_space(m).ok()?, fs4::total_space(m).ok()?)));
    }

    /// Selecciona solo un bucle, preguntando antes si hay cambios sin guardar.
    pub fn request_select(&mut self, slot: Option<Slot>) {
        if slot == self.lib.selected() && self.lib.selection().len() <= 1 {
            return;
        }
        if self.lib.has_unsaved_changes() && slot != self.lib.selected() {
            self.pending_select = Some(PendingSelect::Single(slot));
        } else {
            self.lib.select(slot);
        }
    }

    /// Clic en una fila con modificadores. Pregunta antes si el clic cambia
    /// el bucle que se está editando y hay cambios sin guardar.
    pub fn request_click(&mut self, slot: Slot, mods: ClickMods) {
        let order: Vec<Slot> = self.lib.visible().iter().map(|r| r.slot).collect();
        let mut preview = self.lib.selection().clone();
        preview.click(slot, mods, &order);
        if self.lib.has_unsaved_changes() && preview.primary().copied() != self.lib.selected() {
            self.pending_select = Some(PendingSelect::Click(slot, mods));
        } else {
            self.lib.click(slot, mods);
        }
    }

    /// Aplica un cambio de selección pendiente.
    pub fn apply_pending(&mut self, pending: PendingSelect) {
        match pending {
            PendingSelect::Single(slot) => self.lib.select(slot),
            PendingSelect::Click(slot, mods) => self.lib.click(slot, mods),
        }
    }

    /// Guarda los ajustes en edición.
    pub fn save(&mut self, ctx: &egui::Context) {
        if let Some(d) = self.lib.draft.as_ref().filter(|d| d.is_dirty()) {
            let op = Op::Update {
                slot: d.slot,
                settings: d.edited.clone(),
            };
            self.start(op, TransferKind::Save, ctx);
        }
    }

    /// `true` si se puede empezar una transferencia.
    pub fn can_transfer(&self) -> bool {
        self.lib.is_open() && self.transfer.is_none()
    }

    /// Pide ficheros de audio y los importa.
    pub fn pick_and_import(&mut self, ctx: &egui::Context) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Añadir canciones a la tarjeta")
            .add_filter("Audio", &jamman_audio::SUPPORTED_EXTENSIONS)
            .pick_files()
        {
            self.start_import(files, ctx);
        }
    }

    /// Importa lo seleccionado en el explorador.
    pub fn import_from_browser(&mut self, ctx: &egui::Context) {
        let paths = self.browser.selected_paths();
        self.start_import(paths, ctx);
    }

    /// Posición desde la que se copiará al importar: la elegida o, si no, el
    /// primer hueco libre.
    pub fn import_slot(&self) -> Option<Slot> {
        self.import_start
            .or_else(|| self.lib.first_free_from(Slot::new(1).ok()?))
    }

    /// Motivo por el que no se puede importar en la posición elegida.
    pub fn import_blocked(&self) -> Option<String> {
        match self.import_slot() {
            None => Some("No quedan posiciones libres.".to_owned()),
            Some(s) if self.lib.row(s).is_some() => Some(format!(
                "La posición {:02} está ocupada: elige un hueco libre.",
                s.get()
            )),
            Some(_) => None,
        }
    }

    /// Importa ficheros (o el audio de las carpetas indicadas).
    pub fn start_import(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        if !self.lib.is_open() {
            self.error("Abre primero una tarjeta o el pedal.");
            return;
        }
        if self.transfer.is_some() {
            self.error("Espera a que termine la operación en curso.");
            return;
        }
        let files = audio_files(paths);
        if files.is_empty() {
            self.error("No hay ficheros de audio compatibles (WAV, MP3, FLAC, OGG, AIFF, M4A).");
            return;
        }
        if let Some(why) = self.import_blocked() {
            self.error(why);
            return;
        }
        let Some(start) = self.import_slot() else {
            return;
        };
        self.player.stop();
        let normalize = self.normalize;
        let op = Op::Import {
            files,
            normalize,
            start,
        };
        self.start(op, TransferKind::Import, ctx);
        // La próxima vez se vuelve a proponer el primer hueco libre.
        self.import_start = None;
    }

    /// Exporta los bucles seleccionados a la carpeta del explorador.
    pub fn export_selection(&mut self, ctx: &egui::Context) {
        let slots = self.lib.selected_slots();
        self.export_to_browser(slots, ctx);
    }

    /// Exporta bucles a la carpeta abierta en el explorador.
    pub fn export_to_browser(&mut self, slots: Vec<Slot>, ctx: &egui::Context) {
        let loops: Vec<(Slot, String)> = slots
            .into_iter()
            .filter_map(|s| self.lib.row(s).map(|r| (s, r.name().to_owned())))
            .collect();
        if loops.is_empty() {
            return;
        }
        let dir = self.browser.dir().to_path_buf();
        self.start(Op::Export { loops, dir }, TransferKind::Export, ctx);
    }

    fn poll_transfer(&mut self) {
        let Some(state) = &mut self.transfer else {
            return;
        };
        let mut listed = false;
        let mut finished = false;
        loop {
            match state.rx.try_recv() {
                Ok(TransferEvent::Started { index, total, file }) => {
                    state.index = index;
                    state.total = total;
                    state.file = file;
                    state.progress = 0.0;
                }
                Ok(TransferEvent::Progress(p)) => state.progress = p,
                Ok(TransferEvent::Imported(slot)) => {
                    state.done += 1;
                    state.last_imported = Some(slot);
                }
                Ok(TransferEvent::Exported(path)) => {
                    state.done += 1;
                    state.exported.push(path);
                }
                Ok(TransferEvent::Removed(slot)) => {
                    state.done += 1;
                    state.removed.push(slot);
                }
                Ok(TransferEvent::Saved(slot)) => {
                    state.done += 1;
                    if self.lib.draft.as_ref().is_some_and(|d| d.slot == slot) {
                        self.lib.mark_draft_saved();
                    }
                }
                Ok(TransferEvent::Listed(listing)) => {
                    self.lib.set_listing(listing);
                    listed = true;
                }
                Ok(TransferEvent::Space { free, total }) => self.space = Some((free, total)),
                Ok(TransferEvent::Failed { file, error }) => state.failures.push((file, error)),
                Ok(TransferEvent::Done) | Err(TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if listed {
            self.after_change();
        }
        if finished && let Some(state) = self.transfer.take() {
            self.finish_transfer(state);
        }
    }

    fn finish_transfer(&mut self, state: TransferState) {
        self.lib.loading = false;
        let errors = state.failures.len();
        // Solo hay que expulsar la tarjeta del PC, y solo si se escribió en ella.
        let hint = if self.lib.card().is_some() && state.kind.modifies() {
            EJECT_HINT
        } else {
            ""
        };
        let (msg, title) = match state.kind {
            TransferKind::Import => {
                if !self.lib.has_unsaved_changes() && state.last_imported.is_some() {
                    self.lib.select(state.last_imported);
                }
                (
                    format!(
                        "Importación terminada: {} añadidas, {errors} con error. {hint}",
                        state.done
                    ),
                    "Algunas canciones no se pudieron añadir",
                )
            }
            TransferKind::List => (
                format!("{} bucles.", self.lib.rows().len()),
                "No se pudieron leer los bucles",
            ),
            TransferKind::Delete => (
                match state.removed.as_slice() {
                    [one] if errors == 0 => format!("Bucle {:02} borrado. {hint}", one.get()),
                    _ => format!("{} bucles borrados, {errors} con error. {hint}", state.done),
                },
                "Algunos bucles no se pudieron borrar",
            ),
            TransferKind::Save => (
                format!("Cambios guardados. {hint}"),
                "No se pudieron guardar los cambios",
            ),
            TransferKind::Backup => {
                self.browser.refresh();
                (
                    format!(
                        "Copia de seguridad terminada: {} bucles, {errors} omitidos.",
                        state.done
                    ),
                    "Bucles no copiados",
                )
            }
            TransferKind::Restore => (
                format!(
                    "Restauración terminada: {} bucles copiados, {errors} omitidos. {hint}",
                    state.done
                ),
                "Bucles no restaurados",
            ),
            TransferKind::Export => {
                // Como en un cliente SFTP: se ven seleccionados los ficheros nuevos.
                self.browser.refresh();
                self.browser.selection.select_all(&state.exported);
                (
                    format!(
                        "Exportación terminada: {} copiadas a {}, {errors} con error.",
                        state.done,
                        self.browser.dir().display()
                    ),
                    "Algunos bucles no se pudieron exportar",
                )
            }
        };
        if errors == 0 {
            self.info(msg);
        } else {
            self.error(msg);
            self.transfer_report = Some(TransferReport {
                title: title.to_owned(),
                failures: state.failures,
            });
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

    /// Reproduce un bucle desde `start_secs` (solo tarjetas del PC: en el
    /// pedal habría que descargarlo antes).
    pub fn play(&mut self, slot: Slot, start_secs: f64) {
        if self.lib.pedal().is_some() {
            self.info("Para escuchar un bucle del pedal, expórtalo al equipo.");
            return;
        }
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

    /// Pide confirmación para borrar los bucles seleccionados.
    pub fn ask_delete_selection(&mut self) {
        let slots = self.lib.selected_slots();
        if !slots.is_empty() {
            self.confirm_delete = Some(slots);
        }
    }

    /// Borra bucles (tras confirmación de la interfaz).
    pub fn delete(&mut self, slots: &[Slot], ctx: &egui::Context) {
        if self.player.current().is_some_and(|s| slots.contains(&s)) {
            self.player.stop();
        }
        let op = Op::Remove {
            slots: slots.to_vec(),
        };
        self.start(op, TransferKind::Delete, ctx);
    }

    /// Mueve un bucle a otra posición; si está ocupada, las intercambia.
    pub fn move_or_swap(&mut self, from: Slot, to: Slot) {
        if from == to {
            return;
        }
        if self.lib.pedal().is_some() {
            self.error("Mover bucles no está disponible en el pedal (no admite renombrar).");
            return;
        }
        if self.lib.has_unsaved_changes() {
            self.error("Guarda o descarta los cambios antes de mover el bucle.");
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

impl TransferState {
    fn new(rx: Receiver<TransferEvent>, kind: TransferKind, total: usize) -> Self {
        Self {
            rx,
            kind,
            file: String::new(),
            index: 0,
            total,
            progress: 0.0,
            done: 0,
            last_imported: None,
            exported: Vec::new(),
            removed: Vec::new(),
            failures: Vec::new(),
        }
    }
}

/// Expande carpetas (un nivel) y se queda con las extensiones admitidas.
fn audio_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(&p)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|f| f.is_file() && browser::is_audio(f))
                .collect();
            inside.sort();
            files.extend(inside);
        } else if browser::is_audio(&p) {
            files.push(p);
        }
    }
    files
}

impl eframe::App for JamApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_transfer();
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
