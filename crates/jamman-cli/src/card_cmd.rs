//! Subcomandos `jamman card …`: gestión de la tarjeta SD sin necesitar el pedal.

use std::error::Error;
use std::io::{self, Write};
use std::path::Path;

use jamman_audio::{ConvertOptions, convert_to_pedal_wav, needs_conversion};
use jamman_core::card::Card;
use jamman_core::device::Slot;

use crate::args::{take_flag, take_option};

/// Aviso tras modificar la tarjeta: en FAT los cambios pueden quedar en caché.
const EJECT_HINT: &str = "Expulsa/desmonta la tarjeta antes de sacarla del ordenador.";

pub const HELP: &str = "\
COMANDOS DE TARJETA (no necesitan el pedal):
    card ls <tarjeta>                          Lista los bucles de la tarjeta
    card add <tarjeta> <audio> [--slot N] [--name TEXTO] [--normalize]
                                               Añade audio (WAV, MP3, FLAC, OGG, AIFF, M4A);
                                               lo convierte al formato del pedal si hace falta
    card rename <tarjeta> <N> <nombre>         Cambia el nombre de un bucle
    card export <tarjeta> <N> <destino.wav>    Copia el audio de un bucle
    card rm <tarjeta> <N> [--yes]              Borra un bucle (pide confirmación)
    card init <punto-de-montaje>               Prepara una tarjeta vacía

<tarjeta> es el punto de montaje (p. ej. /run/media/usuario/ETIQUETA o E:\\).";

pub fn run(mut args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let slot_opt = take_option(&mut args, "--slot")?;
    let name_opt = take_option(&mut args, "--name")?;
    let yes = take_flag(&mut args, "--yes");
    let normalize = take_flag(&mut args, "--normalize");

    let Some((command, rest)) = args.split_first() else {
        return Err(format!("falta el subcomando de card\n\n{HELP}").into());
    };
    match (command.as_str(), rest) {
        ("ls", [card]) => list(&Card::open(Path::new(card))?),
        ("add", [card, wav]) => add(
            &Card::open(Path::new(card))?,
            Path::new(wav),
            slot_opt.as_deref(),
            name_opt.as_deref(),
            normalize,
        ),
        ("rename", [card, slot, name]) => {
            let slot = parse_slot(slot)?;
            Card::open(Path::new(card))?.rename(slot, name)?;
            println!(
                "Bucle {:02} renombrado a {name:?}\n{EJECT_HINT}",
                slot.get()
            );
            Ok(())
        }
        ("export", [card, slot, dest]) => {
            let slot = parse_slot(slot)?;
            let bytes = Card::open(Path::new(card))?.export_wav(slot, Path::new(dest))?;
            println!("{bytes} bytes copiados a {dest}");
            Ok(())
        }
        ("rm", [card, slot]) => remove(&Card::open(Path::new(card))?, parse_slot(slot)?, yes),
        ("init", [mount]) => {
            let card = Card::create(Path::new(mount))?;
            println!("Tarjeta preparada en {}", card.root().display());
            Ok(())
        }
        _ => Err(format!("subcomando o argumentos no válidos\n\n{HELP}").into()),
    }
}

fn parse_slot(text: &str) -> Result<Slot, Box<dyn Error>> {
    let n: u8 = text
        .parse()
        .map_err(|_| format!("posición no válida: {text:?} (debe ser un número de 1 a 99)"))?;
    Ok(Slot::new(n)?)
}

fn duration(secs: f64) -> String {
    let total = secs.round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn list(card: &Card) -> Result<(), Box<dyn Error>> {
    let listing = card.list()?;
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
    Ok(())
}

fn add(
    card: &Card,
    source: &Path,
    slot: Option<&str>,
    name: Option<&str>,
    normalize: bool,
) -> Result<(), Box<dyn Error>> {
    let slot = match slot {
        Some(text) => parse_slot(text)?,
        None => card
            .first_free_slot()
            .ok_or("la tarjeta está llena (99 bucles); libera una posición")?,
    };
    let name = name
        .map(str::to_owned)
        .or_else(|| source.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();

    // El directorio temporal se borra solo al salir de la función.
    let workdir = tempfile::tempdir()?;
    let wav = if normalize || needs_conversion(source) {
        let dest = workdir.path().join("convertido.wav");
        print!("Convirtiendo {}… ", source.display());
        io::stdout().flush()?;
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

    let entry = card.import_wav_from(slot, &wav, &name, source)?;
    println!(
        "Añadido en la posición {:02}: {:?} ({})\n{EJECT_HINT}",
        entry.slot.get(),
        entry.patch.name,
        duration(entry.audio.duration_secs())
    );
    Ok(())
}

fn remove(card: &Card, slot: Slot, yes: bool) -> Result<(), Box<dyn Error>> {
    let entry = card.read(slot)?;
    if !yes {
        print!(
            "¿Borrar el bucle {:02} {:?} ({})? No se puede deshacer. [s/N] ",
            slot.get(),
            entry.patch.name,
            duration(entry.audio.duration_secs())
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
    card.remove(slot)?;
    println!("Bucle {:02} borrado.\n{EJECT_HINT}", slot.get());
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
