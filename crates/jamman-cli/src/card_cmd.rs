//! Subcomandos `jamman card …`: gestión de la tarjeta SD sin necesitar el pedal.

use std::error::Error;
use std::path::Path;

use jamman_core::card::Card;

use crate::store_cmd::{self, CmdResult, Options, parse_slot};

/// Aviso tras modificar la tarjeta: en FAT los cambios pueden quedar en caché.
const EJECT_HINT: &str = "Expulsa/desmonta la tarjeta antes de sacarla del ordenador.";

pub const HELP: &str = "\
COMANDOS DE TARJETA (no necesitan el pedal):
    card ls <tarjeta>                          Lista los bucles de la tarjeta
    card add <tarjeta> <audio> [--slot N] [--name TEXTO] [--normalize]
                                               Añade audio (WAV, MP3, FLAC, OGG, AIFF, M4A);
                                               lo convierte al formato del pedal si hace falta
    card rename <tarjeta> <N> <nombre>         Cambia el nombre de un bucle
    card mv <tarjeta> <N> <M>                  Mueve un bucle (si M está ocupada, los intercambia)
    card export <tarjeta> <N> <destino.wav>    Copia el audio de un bucle
    card rm <tarjeta> <N> [--yes]              Borra un bucle (pide confirmación)
    card backup <tarjeta> <carpeta>            Copia todos los bucles a <carpeta>/JamManStereo
    card restore <carpeta> <tarjeta>           Trae los bucles de una copia (solo a posiciones libres)
    card init <punto-de-montaje>               Prepara una tarjeta vacía

<tarjeta> es el punto de montaje (p. ej. /run/media/usuario/ETIQUETA o E:\\).";

fn open(path: &str) -> Result<Card, Box<dyn Error>> {
    Ok(Card::open(Path::new(path))?)
}

pub fn run(mut args: Vec<String>) -> CmdResult {
    let opts = Options::take(&mut args)?;
    let Some((command, rest)) = args.split_first() else {
        return Err(format!("falta el subcomando de card\n\n{HELP}").into());
    };
    // Lo que modifica la tarjeta termina recordando que hay que expulsarla.
    let modified = match (command.as_str(), rest) {
        ("ls", [card]) => store_cmd::list(&mut open(card)?).map(|()| false),
        ("export", [card, n, dest]) => store_cmd::export(&mut open(card)?, n, dest).map(|()| false),
        ("backup", [card, dir]) => {
            store_cmd::backup(&mut open(card)?, Path::new(dir)).map(|()| false)
        }
        ("add", [card, audio]) => {
            store_cmd::add(&mut open(card)?, Path::new(audio), &opts).map(|()| true)
        }
        ("rm", [card, n]) => store_cmd::remove(&mut open(card)?, n, &opts),
        ("restore", [dir, card]) => {
            store_cmd::restore(&mut open(card)?, Path::new(dir)).map(|()| true)
        }
        ("rename", [card, n, name]) => {
            let slot = parse_slot(n)?;
            open(card)?.rename(slot, name)?;
            println!("Bucle {:02} renombrado a {name:?}", slot.get());
            Ok(true)
        }
        ("mv", [card, from, to]) => {
            let (from, to) = (parse_slot(from)?, parse_slot(to)?);
            open(card)?.swap(from, to)?;
            println!("Bucle {:02} movido a {:02}", from.get(), to.get());
            Ok(true)
        }
        ("init", [mount]) => {
            let card = Card::create(Path::new(mount))?;
            println!("Tarjeta preparada en {}", card.root().display());
            Ok(true)
        }
        _ => Err(format!("subcomando o argumentos no válidos\n\n{HELP}").into()),
    }?;
    if modified {
        println!("{EJECT_HINT}");
    }
    Ok(())
}
