//! Subcomandos comunes a la tarjeta y al pedal (`ls`, `add`, `export`, `rm`,
//! `backup`, `restore`), escritos una sola vez sobre [`LoopStore`].

use std::error::Error;
use std::io::{self, Write};
use std::path::Path;

use jamman_audio::{ConvertOptions, convert_to_pedal_wav, needs_conversion};
use jamman_core::card::Card;
use jamman_core::device::Slot;
use jamman_core::store::LoopStore;

use crate::args::{take_flag, take_option};

pub type CmdResult = Result<(), Box<dyn Error>>;

/// Opciones que admiten los subcomandos.
pub struct Options {
    slot: Option<String>,
    name: Option<String>,
    yes: bool,
    normalize: bool,
}

impl Options {
    /// Saca las opciones de `args` y deja solo los argumentos posicionales.
    pub fn take(args: &mut Vec<String>) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            slot: take_option(args, "--slot")?,
            name: take_option(args, "--name")?,
            yes: take_flag(args, "--yes"),
            normalize: take_flag(args, "--normalize"),
        })
    }
}

pub fn parse_slot(text: &str) -> Result<Slot, Box<dyn Error>> {
    let n: u8 = text
        .parse()
        .map_err(|_| format!("posición no válida: {text:?} (debe ser un número de 1 a 99)"))?;
    Ok(Slot::new(n)?)
}

fn duration(secs: f64) -> String {
    let total = secs.round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
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

/// Pregunta sí o no; solo «sí» confirma.
fn confirm(question: &str) -> io::Result<bool> {
    print!("{question} [s/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    ))
}

pub fn list(store: &mut dyn LoopStore) -> CmdResult {
    let listing = store.list()?;
    println!(
        "{:>3}  {:>6}  {:>7}  {:>8}  Nombre",
        "Nº", "Dur.", "BPM", "Tamaño"
    );
    let (mut total_secs, mut total_bytes, mut broken) = (0.0, 0u64, 0usize);
    for item in &listing {
        match &item.entry {
            Ok(e) => {
                total_secs += e.audio.duration_secs();
                total_bytes += e.audio.data_len;
                let name = if e.patch.name.is_empty() {
                    "(sin nombre)"
                } else {
                    &e.patch.name
                };
                println!(
                    "{:>3}  {:>6}  {:>7}  {:>5} MB  {name}",
                    item.slot.get(),
                    duration(e.audio.duration_secs()),
                    e.phrase.bpm.round(),
                    e.audio.data_len / 1_000_000,
                );
            }
            Err(err) => {
                broken += 1;
                println!("{:>3}  ERROR: {err}", item.slot.get());
            }
        }
    }
    println!(
        "\n{} bucles, {} en total, {} MB de audio{}",
        listing.len(),
        duration(total_secs),
        total_bytes / 1_000_000,
        if broken > 0 {
            format!(", {broken} con errores")
        } else {
            String::new()
        }
    );
    if let Some((free, total)) = store.space()? {
        let gb = |b: u64| b as f64 / 1e9;
        println!("Libre: {:.2} GB de {:.2} GB", gb(free), gb(total));
    }
    Ok(())
}

/// Añade `source` (convirtiéndolo si hace falta) en `--slot` o en la primera
/// posición libre.
pub fn add(store: &mut dyn LoopStore, source: &Path, opts: &Options) -> CmdResult {
    let slot = match &opts.slot {
        Some(text) => parse_slot(text)?,
        None => store
            .first_free_slot()?
            .ok_or("no queda ninguna posición libre (99 bucles)")?,
    };
    let name = opts
        .name
        .clone()
        .or_else(|| source.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();

    // El directorio temporal se borra solo al salir de la función.
    let workdir = tempfile::tempdir()?;
    let wav = if opts.normalize || needs_conversion(source) {
        let dest = workdir.path().join("convertido.wav");
        print!("Convirtiendo {}… ", source.display());
        io::stdout().flush()?;
        let normalize = opts.normalize;
        let report = convert_to_pedal_wav(source, &dest, &ConvertOptions { normalize }, |_| {})?;
        println!(
            "{} Hz, {} canal(es) → 44100 Hz estéreo 16 bit{}",
            report.source_rate,
            report.source_channels,
            report
                .gain_db
                .map(|g| format!(", normalizado {g:+.1} dB"))
                .unwrap_or_default()
        );
        dest
    } else {
        source.to_path_buf()
    };

    let entry = store.import_wav_from(slot, &wav, &name, source, &mut progress("Copiando"))?;
    println!(
        "Añadido en la posición {:02}: {:?} ({})",
        entry.slot.get(),
        entry.patch.name,
        duration(entry.audio.duration_secs())
    );
    Ok(())
}

pub fn export(store: &mut dyn LoopStore, slot: &str, dest: &str) -> CmdResult {
    let bytes = store.export_wav(
        parse_slot(slot)?,
        Path::new(dest),
        &mut progress("Copiando"),
    )?;
    println!("{bytes} bytes guardados en {dest}");
    Ok(())
}

/// Borra un bucle tras confirmarlo (salvo `--yes`). Devuelve `false` si se
/// canceló.
pub fn remove(
    store: &mut dyn LoopStore,
    slot: &str,
    opts: &Options,
) -> Result<bool, Box<dyn Error>> {
    let slot = parse_slot(slot)?;
    // Un bucle dañado (XML ilegible) también se tiene que poder borrar.
    let name = match store.read(slot) {
        Ok(e) => format!("{:?} ({})", e.patch.name, duration(e.audio.duration_secs())),
        Err(jamman_core::Error::SlotNotFound(n)) => {
            return Err(format!("la posición {n} está vacía").into());
        }
        Err(e) => format!("(dañado: {e})"),
    };
    let question = format!(
        "¿Borrar el bucle {:02} {name}? No se puede deshacer.",
        slot.get()
    );
    if !opts.yes && !confirm(&question)? {
        println!("Cancelado.");
        return Ok(false);
    }
    store.remove(slot)?;
    println!("Bucle {:02} borrado.", slot.get());
    Ok(true)
}

/// Copia todos los bucles, tal cual, a `dir/JamManStereo`.
pub fn backup(store: &mut dyn LoopStore, dir: &Path) -> CmdResult {
    let card = Card::create(dir)?;
    let (mut ok, mut skipped) = (0, 0);
    for slot in store.slots()? {
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

/// Trae los bucles de una copia a sus mismas posiciones, si están libres.
pub fn restore(store: &mut dyn LoopStore, dir: &Path) -> CmdResult {
    let card = Card::open(dir)?;
    let (mut ok, mut skipped) = (0, 0);
    for l in card.list()? {
        let label = format!("Bucle {:02}", l.slot.get());
        let result = l
            .entry
            .and_then(|_| store.restore_loop(&card, l.slot, l.slot, &mut progress(&label)));
        match result {
            Ok(()) => ok += 1,
            Err(e) => {
                skipped += 1;
                println!("{label}: omitido ({e})");
            }
        }
    }
    println!("Restauración terminada: {ok} bucles copiados, {skipped} omitidos.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_parsing_rejects_garbage_and_out_of_range() {
        assert_eq!(parse_slot("07").unwrap().get(), 7);
        assert!(parse_slot("0").is_err());
        assert!(parse_slot("100").is_err());
        assert!(parse_slot("abc").is_err());
        assert!(parse_slot("-1").is_err());
    }

    #[test]
    fn duration_is_minutes_and_seconds() {
        assert_eq!(duration(304.2), "5:04");
        assert_eq!(duration(59.6), "1:00");
        assert_eq!(duration(0.0), "0:00");
    }
}
