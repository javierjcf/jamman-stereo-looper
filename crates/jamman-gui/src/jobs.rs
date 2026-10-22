//! Trabajos en segundo plano: importar (y convertir) audio y calcular formas
//! de onda, sin bloquear la interfaz. Cada trabajo corre en su propio hilo y
//! comunica su avance por un canal; tras cada mensaje se pide repintar.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use jamman_audio::{ConvertOptions, convert_to_pedal_wav, needs_conversion};
use jamman_core::card::Card;
use jamman_core::device::Slot;

/// Avance de una importación.
#[derive(Debug)]
pub enum ImportEvent {
    /// Empieza un fichero (`index` desde 0).
    Started {
        /// Posición en la lista.
        index: usize,
        /// Ficheros en total.
        total: usize,
        /// Nombre del fichero.
        file: String,
    },
    /// Progreso de la conversión del fichero actual (0 a 1).
    Progress(f32),
    /// Fichero importado en esa posición.
    Imported(Slot),
    /// Fichero que no se pudo importar.
    Failed {
        /// Nombre del fichero.
        file: String,
        /// Motivo.
        error: String,
    },
    /// Fin de la importación.
    Done,
}

/// Envía un evento y pide repintar. Si la ventana se cerró, se ignora.
fn emit<T>(tx: &Sender<T>, ctx: &egui::Context, event: T) {
    let _ = tx.send(event);
    ctx.request_repaint();
}

/// Importa `files` en las primeras posiciones libres de `card`, convirtiendo
/// los que no estén ya en el formato del pedal.
pub fn spawn_import(
    card: Card,
    files: Vec<PathBuf>,
    normalize: bool,
    ctx: egui::Context,
) -> Receiver<ImportEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let total = files.len();
        for (index, path) in files.into_iter().enumerate() {
            let file = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            emit(
                &tx,
                &ctx,
                ImportEvent::Started {
                    index,
                    total,
                    file: file.clone(),
                },
            );
            match import_one(&card, &path, normalize, &tx, &ctx) {
                Ok(slot) => emit(&tx, &ctx, ImportEvent::Imported(slot)),
                Err(error) => emit(&tx, &ctx, ImportEvent::Failed { file, error }),
            }
        }
        emit(&tx, &ctx, ImportEvent::Done);
    });
    rx
}

fn import_one(
    card: &Card,
    path: &std::path::Path,
    normalize: bool,
    tx: &Sender<ImportEvent>,
    ctx: &egui::Context,
) -> Result<Slot, String> {
    let slot = card
        .first_free_slot()
        .ok_or("la tarjeta está llena (99 bucles)")?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let wav = if normalize || needs_conversion(path) {
        let dest = workdir.path().join("convertido.wav");
        let mut last = -1.0f32;
        convert_to_pedal_wav(path, &dest, &ConvertOptions { normalize }, |p| {
            // Un mensaje por cada 2 % es suficiente para la barra.
            if p - last >= 0.02 || p >= 1.0 {
                last = p;
                emit(tx, ctx, ImportEvent::Progress(p));
            }
        })
        .map_err(|e| e.to_string())?;
        dest
    } else {
        path.to_path_buf()
    };
    card.import_wav_from(slot, &wav, &name, path)
        .map(|e| e.slot)
        .map_err(|e| e.to_string())
}

/// Resultado del cálculo de una forma de onda.
pub struct WaveformResult {
    /// Bucle al que pertenece.
    pub slot: Slot,
    /// Picos (0 a 1) o el error.
    pub peaks: Result<Vec<f32>, String>,
}

/// Calcula en segundo plano la forma de onda del audio de un bucle (o la
/// toma de la caché en disco).
pub fn spawn_waveform(
    slot: Slot,
    path: PathBuf,
    buckets: usize,
    ctx: egui::Context,
) -> Receiver<WaveformResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let peaks = match crate::waveform_cache::load(&path, buckets) {
            Some(peaks) => Ok(peaks),
            None => jamman_audio::waveform(&path, buckets)
                .inspect(|p| crate::waveform_cache::store(&path, p))
                .map_err(|e| e.to_string()),
        };
        emit(&tx, &ctx, WaveformResult { slot, peaks });
    });
    rx
}
