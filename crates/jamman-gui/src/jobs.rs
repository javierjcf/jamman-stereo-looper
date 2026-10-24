//! Trabajos en segundo plano sobre la tarjeta o el pedal (importar
//! convirtiendo, exportar, borrar, guardar, copias de seguridad) y cálculo de
//! formas de onda, sin bloquear la interfaz. Cada trabajo corre en su propio
//! hilo y comunica su avance por un canal; tras cada mensaje se pide repintar.
//!
//! Todo se escribe una vez sobre [`LoopStore`]: el trabajo no sabe si los
//! bucles están en una tarjeta del PC o en el pedal.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use jamman_audio::{ConvertOptions, convert_to_pedal_wav, needs_conversion};
use jamman_core::card::{Card, Listing, LoopSettings};
use jamman_core::device::{Slot, first_free_from};
use jamman_core::store::LoopStore;

/// Avance de un trabajo.
#[derive(Debug)]
pub enum TransferEvent {
    /// Empieza un elemento (`index` desde 0).
    Started {
        /// Posición en la lista.
        index: usize,
        /// Elementos en total.
        total: usize,
        /// Nombre del elemento.
        file: String,
    },
    /// Progreso del elemento actual (0 a 1).
    Progress(f32),
    /// Bucle añadido en esa posición.
    Imported(Slot),
    /// Bucle exportado (o copiado) a esa ruta.
    Exported(PathBuf),
    /// Bucle borrado.
    Removed(Slot),
    /// Ajustes de un bucle guardados.
    Saved(Slot),
    /// Lista actualizada del almacén, al terminar.
    Listed(Vec<Listing>),
    /// Espacio del almacén en bytes, si el almacén lo sabe (el pedal).
    Space {
        /// Libres.
        free: u64,
        /// Totales.
        total: u64,
    },
    /// Elemento que falló.
    Failed {
        /// Nombre del elemento.
        file: String,
        /// Motivo.
        error: String,
    },
    /// Fin del trabajo.
    Done,
}

/// Operación sobre un almacén.
#[derive(Debug, Clone)]
pub enum Op {
    /// Solo leer la lista de bucles.
    List,
    /// Convertir si hace falta y añadir en las posiciones libres a partir de
    /// `start`, saltando las ocupadas.
    Import {
        /// Ficheros de audio.
        files: Vec<PathBuf>,
        /// Normalizar el volumen.
        normalize: bool,
        /// Primera posición en la que se puede copiar.
        start: Slot,
    },
    /// Copiar el audio de bucles `(posición, nombre)` a una carpeta.
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
    /// Traer los bucles de una copia a sus mismas posiciones, si están libres.
    Restore {
        /// Carpeta de la copia.
        dir: PathBuf,
    },
}

impl Op {
    /// Número de elementos, para la barra de progreso.
    pub fn total(&self) -> usize {
        match self {
            Self::Import { files, .. } => files.len(),
            Self::Export { loops, .. } => loops.len(),
            Self::Remove { slots } => slots.len(),
            Self::List | Self::Update { .. } | Self::Backup { .. } | Self::Restore { .. } => 1,
        }
    }
}

/// Envía un evento y pide repintar. Si la ventana se cerró, se ignora.
fn emit<T>(tx: &Sender<T>, ctx: &egui::Context, event: T) {
    let _ = tx.send(event);
    ctx.request_repaint();
}

/// Ejecuta `op` en un hilo sobre el almacén que devuelve `open` (abrirlo
/// puede tardar, como conectar con el pedal, así que también se hace en el
/// hilo). Termina enviando la lista actualizada y el espacio libre. Si se
/// corta la comunicación, para ahí (ver [`run_steps`]). `label` nombra el
/// almacén en los errores de apertura y lectura.
pub fn spawn<S, O>(label: String, open: O, op: Op, ctx: egui::Context) -> Receiver<TransferEvent>
where
    S: LoopStore,
    O: FnOnce() -> jamman_core::Result<S> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let fail = |error: jamman_core::Error| {
            let file = label.clone();
            emit(
                &tx,
                &ctx,
                TransferEvent::Failed {
                    file,
                    error: error.to_string(),
                },
            );
        };
        match open() {
            Ok(mut store) => {
                if run_op(&mut store, op, &tx, &ctx).is_ok() {
                    match store.list() {
                        Ok(listing) => emit(&tx, &ctx, TransferEvent::Listed(listing)),
                        Err(e) => fail(e),
                    }
                    if let Ok(Some((free, total))) = store.space() {
                        emit(&tx, &ctx, TransferEvent::Space { free, total });
                    }
                }
            }
            Err(e) => fail(e),
        }
        emit(&tx, &ctx, TransferEvent::Done);
    });
    rx
}

/// Se cortó la comunicación con el almacén y no se sigue.
struct ConnectionLost;

/// Error de un paso: del almacén o del propio equipo (p. ej. al convertir).
enum StepError {
    Store(jamman_core::Error),
    Local(String),
}

impl From<jamman_core::Error> for StepError {
    fn from(e: jamman_core::Error) -> Self {
        Self::Store(e)
    }
}

/// Ejecuta los pasos de una operación por lotes. Cada error se anota; si
/// corta la comunicación (p. ej. el pedal deja de responder), los elementos
/// que faltan se anotan como no intentados y se para: seguir enviando
/// peticiones a un pedal colgado solo acumula esperas y errores.
fn run_steps<I>(
    items: Vec<(String, I)>,
    tx: &Sender<TransferEvent>,
    ctx: &egui::Context,
    mut step: impl FnMut(I) -> Result<TransferEvent, StepError>,
) -> Result<(), ConnectionLost> {
    let total = items.len();
    let mut items = items.into_iter().enumerate();
    while let Some((index, (file, item))) = items.next() {
        let started = TransferEvent::Started {
            index,
            total,
            file: file.clone(),
        };
        emit(tx, ctx, started);
        let (error, lost) = match step(item) {
            Ok(event) => {
                emit(tx, ctx, event);
                continue;
            }
            Err(StepError::Local(error)) => (error, false),
            Err(StepError::Store(e)) => (e.to_string(), e.breaks_connection()),
        };
        emit(tx, ctx, TransferEvent::Failed { file, error });
        if lost {
            for (_, (file, _)) in items {
                let error = "no se ha intentado: se cortó la comunicación".to_owned();
                emit(tx, ctx, TransferEvent::Failed { file, error });
            }
            return Err(ConnectionLost);
        }
    }
    Ok(())
}

/// Nombre de un bucle en la lista de progreso.
fn loop_label(slot: Slot) -> String {
    format!("Bucle {:02}", slot.get())
}

/// Hace la operación. Devuelve `Err` si se cortó la comunicación.
fn run_op(
    store: &mut dyn LoopStore,
    op: Op,
    tx: &Sender<TransferEvent>,
    ctx: &egui::Context,
) -> Result<(), ConnectionLost> {
    // Progreso en bytes llevado al tramo [from, to] de la barra.
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
    match op {
        Op::List => Ok(()),
        Op::Import {
            files,
            normalize,
            start,
        } => {
            let items = files.into_iter().map(|p| (display_name(&p), p)).collect();
            run_steps(items, tx, ctx, |path| {
                let slot = first_free_from(&store.slots()?, start).ok_or_else(|| {
                    StepError::Local(format!(
                        "no quedan posiciones libres desde la {:02}",
                        start.get()
                    ))
                })?;
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
        Op::Export { loops, dir } => {
            let items = loops
                .into_iter()
                .map(|(slot, name)| (export_file_name(slot, &name), slot))
                .map(|(file, slot)| (file.clone(), (file, slot)))
                .collect();
            run_steps(items, tx, ctx, |(file, slot)| {
                let dest = unique_path(&dir, &file);
                store.export_wav(slot, &dest, &mut bytes_progress(0.0, 1.0))?;
                Ok(TransferEvent::Exported(dest))
            })
        }
        Op::Remove { slots } => {
            let items = slots.into_iter().map(|s| (loop_label(s), s)).collect();
            run_steps(items, tx, ctx, |slot| {
                store.remove(slot)?;
                Ok(TransferEvent::Removed(slot))
            })
        }
        Op::Update { slot, settings } => {
            run_steps(vec![(loop_label(slot), slot)], tx, ctx, |slot| {
                store.update(slot, &settings)?;
                Ok(TransferEvent::Saved(slot))
            })
        }
        Op::Backup { dir } => {
            let prepared = Card::create(&dir).and_then(|card| Ok((store.slots()?, card)));
            let (slots, card) = match prepared {
                Ok(v) => v,
                Err(e) => return setup_failed(&dir, e, tx, ctx),
            };
            let items = slots.into_iter().map(|s| (loop_label(s), s)).collect();
            run_steps(items, tx, ctx, |slot| {
                store.backup_loop(slot, &card, slot, &mut bytes_progress(0.0, 1.0))?;
                Ok(TransferEvent::Exported(
                    card.root().join(format!("Patch{:02}", slot.get())),
                ))
            })
        }
        Op::Restore { dir } => {
            let opened = Card::open(&dir).and_then(|card| Ok((card.list()?, card)));
            let (listing, card) = match opened {
                Ok(v) => v,
                Err(e) => return setup_failed(&dir, e, tx, ctx),
            };
            let items = listing
                .into_iter()
                .map(|l| (loop_label(l.slot), l))
                .collect();
            run_steps(items, tx, ctx, |l| {
                l.entry.map_err(|e| StepError::Local(e.to_string()))?;
                store.restore_loop(&card, l.slot, l.slot, &mut bytes_progress(0.0, 1.0))?;
                Ok(TransferEvent::Imported(l.slot))
            })
        }
    }
}

/// Anota que no se pudo preparar una operación (abrir la carpeta de la copia
/// o leer las posiciones).
fn setup_failed(
    dir: &Path,
    error: jamman_core::Error,
    tx: &Sender<TransferEvent>,
    ctx: &egui::Context,
) -> Result<(), ConnectionLost> {
    let lost = error.breaks_connection();
    let failed = TransferEvent::Failed {
        file: dir.display().to_string(),
        error: error.to_string(),
    };
    emit(tx, ctx, failed);
    if lost { Err(ConnectionLost) } else { Ok(()) }
}

/// Nombre del fichero para mostrar.
fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
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
        run_card(
            &card,
            Op::Import {
                files: vec![mp3.clone(), mp3],
                normalize: false,
                start: Slot::new(1).unwrap(),
            },
        );
        let out = dir.path().join("salida");
        std::fs::create_dir(&out).unwrap();
        std::fs::write(out.join("01 Uno.wav"), b"ya estaba").unwrap();

        let loops = vec![
            (Slot::new(1).unwrap(), "Uno".to_owned()),
            (Slot::new(2).unwrap(), "Dos".to_owned()),
            (Slot::new(9).unwrap(), "No existe".to_owned()),
        ];
        let events = run_card(
            &card,
            Op::Export {
                loops,
                dir: out.clone(),
            },
        );

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
        use jamman_core::pedal::{PedalStore, Storage};
        use jamman_core::protocol::Client;
        use jamman_core::protocol::fake::FakePedal;

        fn mp3() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../jamman-audio/tests/fixtures/tono-mono-22k.mp3")
        }

        /// Ejecuta `op` contra `fake` y devuelve los eventos.
        fn run(fake: FakePedal, storage: Storage, op: Op) -> Vec<TransferEvent> {
            run_on(storage, fake, op)
        }

        /// Como [`run`], con cualquier transporte.
        fn run_on<T: std::io::Read + std::io::Write + Send + 'static>(
            storage: Storage,
            io: T,
            op: Op,
        ) -> Vec<TransferEvent> {
            collect(&spawn(
                storage.label().to_owned(),
                move || PedalStore::open(Client::new(io), storage),
                op,
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
            let events = run_on(
                Storage::Card,
                pedal,
                Op::Import {
                    files: vec![mp3(), mp3(), mp3()],
                    normalize: false,
                    start: Slot::new(1).unwrap(),
                },
            );
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
            let events = run(FakePedal::new(), Storage::Card, Op::List);
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
                Op::Import {
                    files: vec![mp3()],
                    normalize: false,
                    start: Slot::new(1).unwrap(),
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
                Op::Export {
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
                Op::Remove { slots: vec![four] },
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
                Op::Update {
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
                Op::Backup {
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
                Op::Restore {
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
            let events = run(no_card, Storage::Card, Op::List);
            assert!(events.iter().any(
                |e| matches!(e, TransferEvent::Failed { error, .. } if error.contains("tarjeta"))
            ));
            assert!(matches!(events.last(), Some(TransferEvent::Done)));

            let rx = spawn::<PedalStore<FakePedal>, _>(
                "Pedal".into(),
                || Err(jamman_core::Error::PedalNotFound),
                Op::List,
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

    /// Ejecuta `op` sobre una tarjeta y devuelve los eventos.
    fn run_card(card: &Card, op: Op) -> Vec<TransferEvent> {
        let card = card.clone();
        collect(&spawn(
            "Tarjeta".into(),
            move || Ok(card),
            op,
            egui::Context::default(),
        ))
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

        let events = run_card(
            &card,
            Op::Import {
                files: vec![mp3, junk],
                normalize: false,
                start: Slot::new(1).unwrap(),
            },
        );

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
    fn import_starts_at_the_chosen_slot_and_skips_occupied_ones() {
        let dir = tempfile::tempdir().unwrap();
        let card = Card::create(dir.path()).unwrap();
        std::fs::create_dir(card.root().join("Patch06")).unwrap();
        let mp3 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../jamman-audio/tests/fixtures/tono-mono-22k.mp3");
        let events = run_card(
            &card,
            Op::Import {
                files: vec![mp3.clone(), mp3],
                normalize: false,
                start: Slot::new(5).unwrap(),
            },
        );
        let imported: Vec<u8> = events
            .iter()
            .filter_map(|e| match e {
                TransferEvent::Imported(slot) => Some(slot.get()),
                _ => None,
            })
            .collect();
        assert_eq!(imported, [5, 7]);
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
        let events = run_card(
            &card,
            Op::Import {
                files: vec![mp3],
                normalize: false,
                start: Slot::new(1).unwrap(),
            },
        );
        assert!(events.iter().any(
            |e| matches!(e, TransferEvent::Failed { error, .. } if error.contains("posiciones libres"))
        ));
    }
}
