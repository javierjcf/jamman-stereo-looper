//! Subcomandos `jamman pedal …`: bucles del pedal conectado por USB.

use std::error::Error;
use std::io::{self, Write};
use std::path::Path;

use jamman_audio::{ConvertOptions, convert_to_pedal_wav, needs_conversion};
use jamman_core::card::Card;
use jamman_core::device::Slot;
use jamman_core::pedal::{PedalStore, Storage};

use crate::args::{take_flag, take_option};

pub const HELP: &str = "\
COMANDOS DEL PEDAL POR USB (<almacén>: interna | tarjeta):
    pedal ls <almacén>                         Lista los bucles
    pedal add <almacén> <audio> [--slot N] [--name TEXTO] [--normalize]
                                               Convierte si hace falta y sube al pedal
    pedal export <almacén> <N> <destino.wav>   Descarga el audio de un bucle
    pedal rm <almacén> <N> [--yes]             Borra un bucle (pide confirmación)
    pedal backup <almacén> <carpeta>           Copia todos los bucles a <carpeta>/JamManStereo
    pedal restore <carpeta> <almacén>          Sube los bucles de una copia (solo a posiciones libres)";

type Store = PedalStore<jamman_core::serial::Port>;

fn storage(text: &str) -> Result<Storage, Box<dyn Error>> {
    match text.to_lowercase().as_str() {
        "interna" | "internal" | "int" => Ok(Storage::Internal),
        "tarjeta" | "card" | "sd" => Ok(Storage::Card),
        _ => Err(format!("almacén no válido: {text:?} (usa «interna» o «tarjeta»)").into()),
    }
}

fn parse_slot(text: &str) -> Result<Slot, Box<dyn Error>> {
    let n: u8 = text
        .parse()
        .map_err(|_| format!("posición no válida: {text:?} (1 a 99)"))?;
    Ok(Slot::new(n)?)
}

fn duration(secs: f64) -> String {
    let t = secs.round() as u64;
    format!("{}:{:02}", t / 60, t % 60)
}

/// Barra de progreso en una línea.
fn progress(label: &str) -> impl FnMut(u64, u64) + '_ {
    let mut last = u64::MAX;
    move |done, total| {
        let pct = (done * 100).checked_div(total).unwrap_or(100);
        if pct != last {
            last = pct;
            print!("\r{label} {pct:>3} %");
            let _ = io::stdout().flush();
            if done >= total {
                println!();
            }
        }
    }
}

fn open(port_name: Option<String>, where_: Storage) -> Result<Store, Box<dyn Error>> {
    let mut store = PedalStore::new(jamman_core::serial::connect(port_name.as_deref())?, where_);
    if !store.available()? {
        return Err(format!(
            "{} no disponible (¿falta la tarjeta en el pedal?)",
            where_.label()
        )
        .into());
    }
    Ok(store)
}

pub fn run(mut args: Vec<String>, port_name: Option<String>) -> Result<(), Box<dyn Error>> {
    let slot_opt = take_option(&mut args, "--slot")?;
    let name_opt = take_option(&mut args, "--name")?;
    let yes = take_flag(&mut args, "--yes");
    let normalize = take_flag(&mut args, "--normalize");
    let Some((command, rest)) = args.split_first() else {
        return Err(format!("falta el subcomando de pedal\n\n{HELP}").into());
    };
    match (command.as_str(), rest) {
        ("ls", [s]) => list(&mut open(port_name, storage(s)?)?),
        ("add", [s, audio]) => add(
            &mut open(port_name, storage(s)?)?,
            Path::new(audio),
            slot_opt.as_deref(),
            name_opt.as_deref(),
            normalize,
        ),
        ("export", [s, n, dest]) => {
            let mut store = open(port_name, storage(s)?)?;
            let bytes = store.export_wav(
                parse_slot(n)?,
                Path::new(dest),
                &mut progress("Descargando"),
            )?;
            println!("{bytes} bytes guardados en {dest}");
            Ok(())
        }
        ("rm", [s, n]) => remove(&mut open(port_name, storage(s)?)?, parse_slot(n)?, yes),
        ("backup", [s, dir]) => backup(&mut open(port_name, storage(s)?)?, Path::new(dir)),
        ("restore", [dir, s]) => restore(&mut open(port_name, storage(s)?)?, Path::new(dir)),
        _ => Err(format!("subcomando o argumentos no válidos\n\n{HELP}").into()),
    }
}

fn list(store: &mut Store) -> Result<(), Box<dyn Error>> {
    println!("{}:", store.storage().label());
    for l in store.list()? {
        match l.entry {
            Ok(e) => println!(
                "{:>3}  {:>6}  {:>5} BPM  {}",
                l.slot.get(),
                duration(e.audio.duration_secs()),
                e.phrase.bpm.round(),
                e.patch.name
            ),
            Err(err) => println!("{:>3}  ({err})", l.slot.get()),
        }
    }
    Ok(())
}

fn add(
    store: &mut Store,
    source: &Path,
    slot: Option<&str>,
    name: Option<&str>,
    normalize: bool,
) -> Result<(), Box<dyn Error>> {
    let slot = match slot {
        Some(t) => parse_slot(t)?,
        None => store
            .first_free_slot()?
            .ok_or("no queda ninguna posición libre")?,
    };
    let name = name
        .map(str::to_owned)
        .or_else(|| source.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let workdir = tempfile::tempdir()?;
    let wav = if normalize || needs_conversion(source) {
        let dest = workdir.path().join("convertido.wav");
        println!("Convirtiendo {}…", source.display());
        convert_to_pedal_wav(source, &dest, &ConvertOptions { normalize }, |_| {})?;
        dest
    } else {
        source.to_path_buf()
    };
    let e = store.import_wav_from(
        slot,
        &wav,
        &name,
        source,
        &mut progress("Subiendo al pedal"),
    )?;
    println!(
        "Añadido en {} {:02}: {:?} ({})",
        store.storage().label(),
        e.slot.get(),
        e.patch.name,
        duration(e.audio.duration_secs())
    );
    Ok(())
}

fn remove(store: &mut Store, slot: Slot, yes: bool) -> Result<(), Box<dyn Error>> {
    let e = store.read(slot)?;
    if !yes {
        print!(
            "¿Borrar del pedal el bucle {:02} {:?}? No se puede deshacer. [s/N] ",
            slot.get(),
            e.patch.name
        );
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(
            answer.trim().to_lowercase().as_str(),
            "s" | "si" | "sí" | "y" | "yes"
        ) {
            println!("Cancelado.");
            return Ok(());
        }
    }
    store.remove(slot)?;
    println!("Bucle {:02} borrado del pedal.", slot.get());
    Ok(())
}

fn backup(store: &mut Store, dir: &Path) -> Result<(), Box<dyn Error>> {
    let card = Card::create(dir)?;
    let slots = store.slots()?;
    let (mut ok, mut skipped) = (0, 0);
    for slot in slots {
        let label = format!("Bucle {:02}", slot.get());
        match store.backup_loop(slot, &card, slot, &mut progress(&label)) {
            Ok(()) => ok += 1,
            Err(e) => {
                skipped += 1;
                println!("{label}: omitido ({e})");
            }
        }
    }
    println!(
        "Copia terminada en {}: {ok} bucles, {skipped} omitidos.",
        card.root().display()
    );
    Ok(())
}

fn restore(store: &mut Store, dir: &Path) -> Result<(), Box<dyn Error>> {
    let card = Card::open(dir)?;
    let (mut ok, mut skipped) = (0, 0);
    for l in card.list()? {
        let label = format!("Bucle {:02}", l.slot.get());
        if let Err(e) = l.entry {
            skipped += 1;
            println!("{label}: omitido ({e})");
            continue;
        }
        match store.restore_loop(&card, l.slot, l.slot, &mut progress(&label)) {
            Ok(()) => ok += 1,
            Err(e) => {
                skipped += 1;
                println!("{label}: omitido ({e})");
            }
        }
    }
    println!("Restauración terminada: {ok} bucles subidos, {skipped} omitidos.");
    Ok(())
}
