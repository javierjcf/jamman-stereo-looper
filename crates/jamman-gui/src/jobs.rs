//! Trabajos en segundo plano: importar (y convertir) audio, exportar bucles y
//! calcular formas de onda, sin bloquear la interfaz. Cada trabajo corre en su propio hilo y
//! comunica su avance por un canal; tras cada mensaje se pide repintar.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use jamman_audio::{ConvertOptions, convert_to_pedal_wav, needs_conversion};
use jamman_core::card::{Card, Listing, LoopSettings};
use jamman_core::device::Slot;
use jamman_core::pedal::{PedalStore, Storage};
use jamman_core::protocol::Client;

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
    /// Bucle borrado.
    Removed(Slot),
    /// Ajustes de un bucle guardados.
    Saved(Slot),
    /// Lista actualizada del almacén del pedal.
    Listed(Vec<Listing>),
    /// Espacio del almacén del pedal, en bytes.
    Space {
        /// Libres.
        free: u64,
        /// Totales.
        total: u64,
    },
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

/// Nombre del bucle a partir del fichero elegido.
fn loop_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Deja `path` en el formato del pedal: si hace falta lo convierte dentro de
/// `workdir`. `progress` recibe valores de 0 a 1.
fn prepare_wav(
    path: &Path,
    normalize: bool,
    workdir: &Path,
    progress: &mut dyn FnMut(f32),
) -> Result<PathBuf, String> {
    if !normalize && !needs_conversion(path) {
        return Ok(path.to_path_buf());
    }
    let dest = workdir.join("convertido.wav");
    convert_to_pedal_wav(path, &dest, &ConvertOptions { normalize }, progress)
        .map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Devuelve un avisador de progreso que envía un mensaje cada 2 % como mucho,
/// llevando `0..1` al tramo `[from, to]` de la barra.
fn progress_sender<'a>(
    tx: &'a Sender<TransferEvent>,
    ctx: &'a egui::Context,
    from: f32,
    to: f32,
) -> impl FnMut(f32) + 'a {
    let mut last = -1.0f32;
    move |p| {
        let value = from + (to - from) * p.clamp(0.0, 1.0);
        if value - last >= 0.02 || p >= 1.0 {
            last = value;
            emit(tx, ctx, TransferEvent::Progress(value));
        }
    }
}

fn import_one(
    card: &Card,
    path: &Path,
    normalize: bool,
    tx: &Sender<TransferEvent>,
    ctx: &egui::Context,
) -> Result<Slot, String> {
    let slot = card
        .first_free_slot()
        .ok_or("la tarjeta está llena (99 bucles)")?;
    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let wav = prepare_wav(
        path,
        normalize,
        workdir.path(),
        &mut progress_sender(tx, ctx, 0.0, 1.0),
    )?;
    card.import_wav_from(slot, &wav, &loop_name(path), path)
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

/// Operación sobre un almacén del pedal.
#[derive(Debug, Clone)]
pub enum PedalOp {
    /// Solo leer la lista de bucles.
    List,
    /// Convertir si hace falta y subir a las primeras posiciones libres.
    Import {
        /// Ficheros de audio.
        files: Vec<PathBuf>,
        /// Normalizar el volumen.
        normalize: bool,
    },
    /// Descargar el audio de bucles `(posición, nombre)` a una carpeta.
    Export {
        /// Bucles.
        loops: Vec<(Slot, String)>,
        /// Carpeta de destino.
        dir: PathBuf,
    },
    /// Borrar bucles.
    Remove {
        /// Posiciones.
        slots: Vec<Slot>,
    },
    /// Guardar los ajustes de un bucle.
    Update {
        /// Posición.
        slot: Slot,
        /// Ajustes nuevos.
        settings: LoopSettings,
    },
    /// Copiar todos los bucles, tal cual, a `dir/JamManStereo`.
    Backup {
        /// Carpeta de destino.
        dir: PathBuf,
    },
    /// Subir los bucles de una copia a las mismas posiciones, si están libres.
    Restore {
        /// Carpeta de la copia.
        dir: PathBuf,
    },
}

/// Número de elementos de una operación, para la barra de progreso.
pub fn pedal_op_total(op: &PedalOp) -> usize {
    match op {
        PedalOp::List
        | PedalOp::Update { .. }
        | PedalOp::Backup { .. }
        | PedalOp::Restore { .. } => 1,
        PedalOp::Import { files, .. } => files.len(),
        PedalOp::Export { loops, .. } => loops.len(),
        PedalOp::Remove { slots } => slots.len(),
    }
}

/// Ejecuta `op` sobre `storage` en un hilo: conecta con `connect`, hace la
/// operación y termina enviando la lista actualizada del almacén y su espacio
/// libre. Si el pedal deja de responder, para en ese punto (ver
/// [`run_pedal_op`]).
pub fn spawn_pedal<T, C>(
    storage: Storage,
    op: PedalOp,
    connect: C,
    ctx: egui::Context,
) -> Receiver<TransferEvent>
where
    T: std::io::Read + std::io::Write + 'static,
    C: FnOnce() -> jamman_core::Result<Client<T>> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let fail = |file: &str, error: String| {
            emit(
                &tx,
                &ctx,
                TransferEvent::Failed {
                    file: file.to_owned(),
                    error,
                },
            );
        };
        let mut store = match connect() {
            Ok(client) => PedalStore::new(client, storage),
            Err(e) => {
                fail("Pedal", e.to_string());
                emit(&tx, &ctx, TransferEvent::Done);
                return;
            }
        };
        match store.available() {
            Ok(true) => {
                if run_pedal_op(&mut store, op, &tx, &ctx).is_ok() {
                    match store.list() {
                        Ok(listing) => emit(&tx, &ctx, TransferEvent::Listed(listing)),
                        Err(e) => fail(storage.label(), e.to_string()),
                    }
                    if let Ok((free, total)) = store.space() {
                        emit(&tx, &ctx, TransferEvent::Space { free, total });
                    }
                }
            }
            Ok(false) => fail(
                storage.label(),
                "no disponible: ¿está la tarjeta metida en el pedal?".to_owned(),
            ),
            Err(e) => fail(storage.label(), e.to_string()),
        }
        emit(&tx, &ctx, TransferEvent::Done);
    });
    rx
}

/// La conversación con el pedal se ha cortado y no se sigue.
struct ConnectionLost;

/// Error de un paso de una operación: del pedal o del propio equipo.
enum StepError {
    Pedal(jamman_core::Error),
    Local(String),
}

impl From<jamman_core::Error> for StepError {
    fn from(e: jamman_core::Error) -> Self {
        Self::Pedal(e)
    }
}

/// Ejecuta los pasos de una operación por lotes. Cada error se anota; si
/// rompe la conversación con el pedal (p. ej. deja de responder), los
/// elementos que faltan se anotan como no intentados y se para: seguir
/// enviando peticiones a un pedal colgado solo acumula esperas y errores.
fn run_steps<I>(
    items: Vec<(String, I)>,
    tx: &Sender<TransferEvent>,
    ctx: &egui::Context,
    mut step: impl FnMut(I) -> Result<TransferEvent, StepError>,
) -> Result<(), ConnectionLost> {
    let total = items.len();
    let mut items = items.into_iter().enumerate();
    while let Some((index, (file, item))) = items.next() {
        emit(
            tx,
            ctx,
            TransferEvent::Started {
                index,
                total,
                file: file.clone(),
            },
        );
        let error = match step(item) {
            Ok(event) => {
                emit(tx, ctx, event);
                continue;
            }
            Err(StepError::Local(error)) => error,
            Err(StepError::Pedal(e)) if e.breaks_connection() => {
                emit(
                    tx,
                    ctx,
                    TransferEvent::Failed {
                        file,
                        error: e.to_string(),
                    },
                );
                for (_, (file, _)) in items {
                    let error = "no se ha intentado: el pedal dejó de responder".to_owned();
                    emit(tx, ctx, TransferEvent::Failed { file, error });
                }
                return Err(ConnectionLost);
            }
            Err(StepError::Pedal(e)) => e.to_string(),
        };
        emit(tx, ctx, TransferEvent::Failed { file, error });
    }
    Ok(())
}

/// Hace la operación. Devuelve `Err` si el pedal dejó de responder.
fn run_pedal_op<T: std::io::Read + std::io::Write>(
    store: &mut PedalStore<T>,
    op: PedalOp,
    tx: &Sender<TransferEvent>,
    ctx: &egui::Context,
) -> Result<(), ConnectionLost> {
    // Progreso de bytes del USB llevado al tramo [from, to] de la barra.
    let bytes_progress = |from: f32, to: f32| {
        let mut send = progress_sender(tx, ctx, from, to);
        move |done: u64, total: u64| {
            send(if total == 0 {
                1.0
            } else {
                done as f32 / total as f32
            })
        }
    };
    let loop_label = |slot: Slot| format!("Bucle {:02}", slot.get());
    match op {
        PedalOp::List => Ok(()),
        PedalOp::Import { files, normalize } => {
            let items = files
                .into_iter()
                .map(|path| {
                    let file = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    );
                    (file, path)
                })
                .collect();
            run_steps(items, tx, ctx, |path| {
                let slot = store
                    .first_free_slot()?
                    .ok_or_else(|| StepError::Local("no queda ninguna posición libre".into()))?;
                let workdir = tempfile::tempdir().map_err(|e| StepError::Local(e.to_string()))?;
                let wav = prepare_wav(
                    &path,
                    normalize,
                    workdir.path(),
                    &mut progress_sender(tx, ctx, 0.0, 0.3),
                )
                .map_err(StepError::Local)?;
                let entry = store.import_wav_from(
                    slot,
                    &wav,
                    &loop_name(&path),
                    &path,
                    &mut bytes_progress(0.3, 1.0),
                )?;
                Ok(TransferEvent::Imported(entry.slot))
            })
        }
        PedalOp::Export { loops, dir } => {
            let items = loops
                .into_iter()
                .map(|(slot, name)| (export_file_name(slot, &name), slot))
                .collect::<Vec<_>>();
            let items = items
                .into_iter()
                .map(|(file, slot)| (file.clone(), (file, slot)))
                .collect();
            run_steps(items, tx, ctx, |(file, slot)| {
                let dest = unique_path(&dir, &file);
                store.export_wav(slot, &dest, &mut bytes_progress(0.0, 1.0))?;
                Ok(TransferEvent::Exported(dest))
            })
        }
        PedalOp::Remove { slots } => {
            let items = slots.into_iter().map(|s| (loop_label(s), s)).collect();
            run_steps(items, tx, ctx, |slot| {
                store.remove(slot)?;
                Ok(TransferEvent::Removed(slot))
            })
        }
        PedalOp::Update { slot, settings } => {
            run_steps(vec![(loop_label(slot), slot)], tx, ctx, |slot| {
                store.update(slot, &settings)?;
                Ok(TransferEvent::Saved(slot))
            })
        }
        PedalOp::Backup { dir } => {
            let card = Card::create(&dir);
            let slots = store.slots();
            let (card, slots) = match (card, slots) {
                (Ok(card), Ok(slots)) => (card, slots),
                (Err(e), _) => {
                    let error = e.to_string();
                    emit(
                        tx,
                        ctx,
                        TransferEvent::Failed {
                            file: dir.display().to_string(),
                            error,
                        },
                    );
                    return Ok(());
                }
                (_, Err(e)) => {
                    let lost = e.breaks_connection();
                    emit(
                        tx,
                        ctx,
                        TransferEvent::Failed {
                            file: "Pedal".into(),
                            error: e.to_string(),
                        },
                    );
                    return if lost { Err(ConnectionLost) } else { Ok(()) };
                }
            };
            let items = slots.into_iter().map(|s| (loop_label(s), s)).collect();
            run_steps(items, tx, ctx, |slot| {
                store.backup_loop(slot, &card, slot, &mut bytes_progress(0.0, 1.0))?;
                Ok(TransferEvent::Exported(
                    card.root().join(format!("Patch{:02}", slot.get())),
                ))
            })
        }
        PedalOp::Restore { dir } => {
            let opened = Card::open(&dir).and_then(|card| Ok((card.list()?, card)));
            let (listing, card) = match opened {
                Ok(v) => v,
                Err(e) => {
                    let error = e.to_string();
                    emit(
                        tx,
                        ctx,
                        TransferEvent::Failed {
                            file: dir.display().to_string(),
                            error,
                        },
                    );
                    return Ok(());
                }
            };
            let items = listing
                .into_iter()
                .map(|l| (loop_label(l.slot), l))
                .collect();
            run_steps(items, tx, ctx, |l| {
                if let Err(e) = l.entry {
                    return Err(StepError::Local(e.to_string()));
                }
                store.restore_loop(&card, l.slot, l.slot, &mut bytes_progress(0.0, 1.0))?;
                Ok(TransferEvent::Imported(l.slot))
            })
        }
    }
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

    mod pedal {
        use super::*;
        use jamman_core::protocol::fake::FakePedal;

        fn mp3() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../jamman-audio/tests/fixtures/tono-mono-22k.mp3")
        }

        /// Ejecuta `op` contra `fake` y devuelve los eventos.
        fn run(fake: FakePedal, storage: Storage, op: PedalOp) -> Vec<TransferEvent> {
            collect(&spawn_pedal(
                storage,
                op,
                move || Ok(Client::new(fake)),
                egui::Context::default(),
            ))
        }

        /// Pedal simulado con un bucle importado en la tarjeta.
        fn fake_with_loop() -> FakePedal {
            let mut store = PedalStore::new(Client::new(FakePedal::new()), Storage::Card);
            let dir = tempfile::tempdir().unwrap();
            let wav = dir.path().join("a.wav");
            jamman_audio::convert_to_pedal_wav(&mp3(), &wav, &Default::default(), |_| {}).unwrap();
            store
                .import_wav_from(Slot::new(4).unwrap(), &wav, "Cuatro", &wav, &mut |_, _| {})
                .unwrap();
            store.into_client().into_inner()
        }

        fn listed(events: &[TransferEvent]) -> Vec<u8> {
            events
                .iter()
                .find_map(|e| match e {
                    TransferEvent::Listed(l) => Some(l.iter().map(|l| l.slot.get()).collect()),
                    _ => None,
                })
                .expect("debe terminar con la lista")
        }

        /// Pedal que deja de contestar tras `requests` peticiones, como el
        /// real cuando se cuelga a mitad de una copia.
        struct Hangs {
            pedal: FakePedal,
            requests: usize,
        }

        impl std::io::Write for Hangs {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if self.requests == 0 {
                    return Ok(buf.len());
                }
                self.requests -= 1;
                self.pedal.write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl std::io::Read for Hangs {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.pedal.read(buf)
            }
        }

        #[test]
        fn import_stops_at_the_first_communication_failure() {
            let pedal = Hangs {
                pedal: FakePedal::new(),
                requests: 12,
            };
            let events = collect(&spawn_pedal(
                Storage::Card,
                PedalOp::Import {
                    files: vec![mp3(), mp3(), mp3()],
                    normalize: false,
                },
                move || Ok(Client::new(pedal)),
                egui::Context::default(),
            ));
            let failures: Vec<&str> = events
                .iter()
                .filter_map(|e| match e {
                    TransferEvent::Failed { error, .. } => Some(error.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(failures.len(), 3, "{failures:?}");
            assert!(failures[0].contains("no responde"), "{failures:?}");
            assert!(
                failures[1..]
                    .iter()
                    .all(|f| f.contains("no se ha intentado"))
            );
            let started = events
                .iter()
                .filter(|e| matches!(e, TransferEvent::Started { .. }))
                .count();
            assert_eq!(started, 1, "no se sigue con las demás");
            assert!(
                !events.iter().any(|e| matches!(e, TransferEvent::Listed(_))),
                "tampoco se intenta volver a listar"
            );
            assert!(matches!(events.last(), Some(TransferEvent::Done)));
        }

        #[test]
        fn listing_reports_the_free_space_of_the_storage() {
            let events = run(FakePedal::new(), Storage::Card, PedalOp::List);
            assert!(events.iter().any(|e| matches!(
                e,
                TransferEvent::Space { free, total }
                    if *free == 13_000_000 * 1024 && *total == 15_000_000 * 1024
            )));
        }

        #[test]
        fn import_converts_uploads_and_returns_the_new_listing() {
            let events = run(
                FakePedal::new(),
                Storage::Internal,
                PedalOp::Import {
                    files: vec![mp3()],
                    normalize: false,
                },
            );
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, TransferEvent::Imported(s) if s.get() == 1))
            );
            assert_eq!(listed(&events), [1]);
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, TransferEvent::Progress(p) if *p >= 1.0))
            );
        }

        #[test]
        fn export_remove_and_update() {
            let dir = tempfile::tempdir().unwrap();
            let four = Slot::new(4).unwrap();
            let events = run(
                fake_with_loop(),
                Storage::Card,
                PedalOp::Export {
                    loops: vec![(four, "Cuatro".into())],
                    dir: dir.path().to_path_buf(),
                },
            );
            assert!(
                events.iter().any(
                    |e| matches!(e, TransferEvent::Exported(p) if p.ends_with("04 Cuatro.wav"))
                )
            );
            assert!(dir.path().join("04 Cuatro.wav").is_file());

            let events = run(
                fake_with_loop(),
                Storage::Card,
                PedalOp::Remove { slots: vec![four] },
            );
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, TransferEvent::Removed(s) if *s == four))
            );
            assert!(listed(&events).is_empty());

            let mut settings = {
                let mut store = PedalStore::new(Client::new(fake_with_loop()), Storage::Card);
                jamman_core::card::LoopSettings::from_entry(&store.read(four).unwrap())
            };
            settings.name = "Renombrado".into();
            let events = run(
                fake_with_loop(),
                Storage::Card,
                PedalOp::Update {
                    slot: four,
                    settings,
                },
            );
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, TransferEvent::Saved(s) if *s == four))
            );
            let TransferEvent::Listed(l) = events
                .iter()
                .find(|e| matches!(e, TransferEvent::Listed(_)))
                .unwrap()
            else {
                unreachable!()
            };
            assert_eq!(l[0].entry.as_ref().unwrap().patch.name, "Renombrado");
        }

        #[test]
        fn backup_then_restore_into_another_storage() {
            let dir = tempfile::tempdir().unwrap();
            let events = run(
                fake_with_loop(),
                Storage::Card,
                PedalOp::Backup {
                    dir: dir.path().to_path_buf(),
                },
            );
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, TransferEvent::Exported(_)))
            );
            let copy = Card::open(dir.path()).unwrap();
            assert_eq!(
                copy.read(Slot::new(4).unwrap()).unwrap().patch.name,
                "Cuatro"
            );

            let events = run(
                FakePedal::new(),
                Storage::Internal,
                PedalOp::Restore {
                    dir: dir.path().to_path_buf(),
                },
            );
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, TransferEvent::Imported(s) if s.get() == 4))
            );
            assert_eq!(listed(&events), [4]);
        }

        #[test]
        fn a_missing_card_or_pedal_is_reported() {
            let mut no_card = FakePedal::new();
            no_card.dirs.remove(Storage::Card.root());
            let events = run(no_card, Storage::Card, PedalOp::List);
            assert!(events.iter().any(
                |e| matches!(e, TransferEvent::Failed { error, .. } if error.contains("tarjeta"))
            ));
            assert!(matches!(events.last(), Some(TransferEvent::Done)));

            let rx = spawn_pedal::<FakePedal, _>(
                Storage::Card,
                PedalOp::List,
                || Err(jamman_core::Error::PedalNotFound),
                egui::Context::default(),
            );
            let events = collect(&rx);
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, TransferEvent::Failed { file, .. } if file == "Pedal"))
            );
        }
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
