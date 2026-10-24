//! Subcomandos `jamman pedal …`: bucles del pedal conectado por USB.

use std::error::Error;
use std::path::Path;

use jamman_core::pedal::{PedalStore, Storage};
use jamman_core::serial::Port;

use crate::store_cmd::{self, CmdResult, Options};

pub const HELP: &str = "\
COMANDOS DEL PEDAL POR USB (<almacén>: interna | tarjeta):
    pedal ls <almacén>                         Lista los bucles y el espacio libre
    pedal add <almacén> <audio> [--slot N] [--name TEXTO] [--normalize]
                                               Convierte si hace falta y sube al pedal
    pedal export <almacén> <N> <destino.wav>   Descarga el audio de un bucle
    pedal rm <almacén> <N> [--yes]             Borra un bucle (pide confirmación)
    pedal backup <almacén> <carpeta>           Copia todos los bucles a <carpeta>/JamManStereo
    pedal restore <carpeta> <almacén>          Sube los bucles de una copia (solo a posiciones libres)";

fn storage(text: &str) -> Result<Storage, Box<dyn Error>> {
    match text.to_lowercase().as_str() {
        "interna" | "internal" | "int" => Ok(Storage::Internal),
        "tarjeta" | "card" | "sd" => Ok(Storage::Card),
        _ => Err(format!("almacén no válido: {text:?} (usa «interna» o «tarjeta»)").into()),
    }
}

pub fn run(mut args: Vec<String>, port: Option<String>) -> CmdResult {
    let opts = Options::take(&mut args)?;
    let open = |text: &str| -> Result<PedalStore<Port>, Box<dyn Error>> {
        let storage = storage(text)?;
        let store = jamman_core::serial::open_store(port.as_deref(), storage)?;
        println!("{}:", storage.label());
        Ok(store)
    };
    let Some((command, rest)) = args.split_first() else {
        return Err(format!("falta el subcomando de pedal\n\n{HELP}").into());
    };
    match (command.as_str(), rest) {
        ("ls", [s]) => store_cmd::list(&mut open(s)?),
        ("add", [s, audio]) => store_cmd::add(&mut open(s)?, Path::new(audio), &opts),
        ("export", [s, n, dest]) => store_cmd::export(&mut open(s)?, n, dest),
        ("rm", [s, n]) => store_cmd::remove(&mut open(s)?, n, &opts).map(|_| ()),
        ("backup", [s, dir]) => store_cmd::backup(&mut open(s)?, Path::new(dir)),
        ("restore", [dir, s]) => store_cmd::restore(&mut open(s)?, Path::new(dir)),
        _ => Err(format!("subcomando o argumentos no válidos\n\n{HELP}").into()),
    }
}
