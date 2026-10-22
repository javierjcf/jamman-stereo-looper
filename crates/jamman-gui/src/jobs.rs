//! Trabajos en segundo plano: importar (y convertir) audio, exportar bucles y
//! calcular formas de onda, sin bloquear la interfaz. Cada trabajo corre en su propio hilo y
//! comunica su avance por un canal; tras cada mensaje se pide repintar.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use jamman_audio::{ConvertOptions, convert_to_pedal_wav, needs_conversion};
use jamman_core::card::Card;
use jamman_core::device::Slot;

/// Avance de una transferencia (importación o exportación).
#[derive(Debug)]
pub enum TransferEvent {
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
    /// Bucle exportado a esa ruta.
    Exported(PathBuf),
    /// Fichero que no se pudo importar.
    Failed {
        /// Nombre del fichero.
        file: String,
        /// Motivo.
        error: String,
    },
    /// Fin de la transferencia.
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
) -> Receiver<TransferEvent> {
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
                TransferEvent::Started {
                    index,
                    total,
                    file: file.clone(),
                },
            );
            match import_one(&card, &path, normalize, &tx, &ctx) {
                Ok(slot) => emit(&tx, &ctx, TransferEvent::Imported(slot)),
                Err(error) => emit(&tx, &ctx, TransferEvent::Failed { file, error }),
            }
        }
        emit(&tx, &ctx, TransferEvent::Done);
    });
    rx
}

fn import_one(
    card: &Card,
    path: &std::path::Path,
    normalize: bool,
    tx: &Sender<TransferEvent>,
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
                emit(tx, ctx, TransferEvent::Progress(p));
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

/// Nombre de fichero para exportar un bucle: `NN Nombre.wav`, sin los
/// caracteres que no admiten Windows o Linux.
pub fn export_file_name(slot: Slot, name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.').trim();
    if clean.is_empty() {
        format!("{:02} Bucle.wav", slot.get())
    } else {
        format!("{:02} {clean}.wav", slot.get())
    }
}

/// `dir/file`, o `dir/stem (2).ext`, `(3)`… si ya existe: nunca se pisa nada.
pub fn unique_path(dir: &Path, file: &str) -> PathBuf {
    let first = dir.join(file);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    (2..)
        .map(|n| {
            if ext.is_empty() {
                dir.join(format!("{stem} ({n})"))
            } else {
                dir.join(format!("{stem} ({n}).{ext}"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Exporta el audio de varios bucles `(posición, nombre)` a `dir`.
pub fn spawn_export(
    card: Card,
    loops: Vec<(Slot, String)>,
    dir: PathBuf,
    ctx: egui::Context,
) -> Receiver<TransferEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let total = loops.len();
        for (index, (slot, name)) in loops.into_iter().enumerate() {
            let file = export_file_name(slot, &name);
            emit(
                &tx,
                &ctx,
                TransferEvent::Started {
                    index,
                    total,
                    file: file.clone(),
                },
            );
            let dest = unique_path(&dir, &file);
            match card.export_wav(slot, &dest) {
                Ok(_) => {
                    emit(&tx, &ctx, TransferEvent::Progress(1.0));
                    emit(&tx, &ctx, TransferEvent::Exported(dest));
                }
                Err(e) => emit(
                    &tx,
                    &ctx,
                    TransferEvent::Failed {
                        file,
                        error: e.to_string(),
                    },
                ),
            }
        }
        emit(&tx, &ctx, TransferEvent::Done);
    });
    rx
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn export_names_are_safe_on_every_system() {
        let s = Slot::new(7).unwrap();
        assert_eq!(
            export_file_name(s, "AC/DC: Back in Black?"),
            "07 AC_DC_ Back in Black_.wav"
        );
        assert_eq!(
            export_file_name(s, "  fin con puntos... "),
            "07 fin con puntos.wav"
        );
        assert_eq!(export_file_name(s, ""), "07 Bucle.wav");
    }

    #[test]
    fn unique_path_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "a.wav"), dir.path().join("a.wav"));
        std::fs::write(dir.path().join("a.wav"), b"").unwrap();
        std::fs::write(dir.path().join("a (2).wav"), b"").unwrap();
        assert_eq!(
            unique_path(dir.path(), "a.wav"),
            dir.path().join("a (3).wav")
        );
    }

    #[test]
    fn export_copies_each_loop_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let card = Card::create(&dir.path().join("sd")).unwrap();
        let mp3 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../jamman-audio/tests/fixtures/tono-mono-22k.mp3");
        collect(&spawn_import(
            card.clone(),
            vec![mp3.clone(), mp3],
            false,
            egui::Context::default(),
        ));
        let out = dir.path().join("salida");
        std::fs::create_dir(&out).unwrap();
        std::fs::write(out.join("01 Uno.wav"), b"ya estaba").unwrap();

        let loops = vec![
            (Slot::new(1).unwrap(), "Uno".to_owned()),
            (Slot::new(2).unwrap(), "Dos".to_owned()),
            (Slot::new(9).unwrap(), "No existe".to_owned()),
        ];
        let events = collect(&spawn_export(
            card.clone(),
            loops,
            out.clone(),
            egui::Context::default(),
        ));

        let exported: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                TransferEvent::Exported(p) => {
                    Some(p.file_name().unwrap().to_string_lossy().into_owned())
                }
                _ => None,
            })
            .collect();
        assert_eq!(exported, ["01 Uno (2).wav", "02 Dos.wav"]);
        assert_eq!(std::fs::read(out.join("01 Uno.wav")).unwrap(), b"ya estaba");
        assert_eq!(
            std::fs::read(out.join("02 Dos.wav")).unwrap(),
            std::fs::read(card.audio_path(Slot::new(2).unwrap())).unwrap()
        );
        assert!(events.iter().any(
            |e| matches!(e, TransferEvent::Failed { file, .. } if file == "09 No existe.wav")
        ));
    }

    fn collect(rx: &Receiver<TransferEvent>) -> Vec<TransferEvent> {
        let mut events = Vec::new();
        loop {
            let event = rx
                .recv_timeout(Duration::from_secs(30))
                .expect("la importación no terminó");
            let done = matches!(event, TransferEvent::Done);
            events.push(event);
            if done {
                return events;
            }
        }
    }

    #[test]
    fn import_converts_valid_files_and_reports_invalid_ones() {
        let dir = tempfile::tempdir().unwrap();
        let card = Card::create(dir.path()).unwrap();
        let mp3 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../jamman-audio/tests/fixtures/tono-mono-22k.mp3");
        let junk = dir.path().join("roto.mp3");
        std::fs::write(&junk, b"no es audio").unwrap();

        let rx = spawn_import(
            card.clone(),
            vec![mp3, junk],
            false,
            egui::Context::default(),
        );
        let events = collect(&rx);

        let imported: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                TransferEvent::Imported(slot) => Some(slot.get()),
                _ => None,
            })
            .collect();
        assert_eq!(imported, [1], "el MP3 va a la primera posición libre");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TransferEvent::Failed { file, .. } if file == "roto.mp3"))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TransferEvent::Progress(p) if *p >= 1.0))
        );

        let entry = card.read(Slot::new(1).unwrap()).unwrap();
        assert_eq!(entry.patch.name, "tono-mono-22k");
        assert_eq!(entry.patch.description, "Imported from tono-mono-22k.mp3");
        entry.audio.check_pedal_format().unwrap();
    }

    #[test]
    fn import_into_a_full_card_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let card = Card::create(dir.path()).unwrap();
        for n in 1..=99 {
            std::fs::create_dir(card.root().join(format!("Patch{n:02}"))).unwrap();
        }
        let mp3 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../jamman-audio/tests/fixtures/tono-mono-22k.mp3");
        let events = collect(&spawn_import(
            card,
            vec![mp3],
            false,
            egui::Context::default(),
        ));
        assert!(
            events.iter().any(
                |e| matches!(e, TransferEvent::Failed { error, .. } if error.contains("llena"))
            )
        );
    }
}
