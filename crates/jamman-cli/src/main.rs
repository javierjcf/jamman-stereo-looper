//! CLI `jamman`: gestión del DigiTech JamMan Stereo desde la terminal.
//!
//! Uso: `jamman [--port <PUERTO>] <comando> [argumentos]`. Ver `jamman help`.

mod args;
mod card_cmd;
mod port;

use std::error::Error;
use std::fs;
use std::process::ExitCode;

use jamman_core::protocol::Client;
use jamman_core::protocol::request::EntryKind;
use jamman_core::{audio, device};

use args::take_option;

const HELP_PEDAL: &str = "\
jamman — gestor del DigiTech JamMan Stereo (EXPERIMENTAL: protocolo sin validar)

USO:
    jamman [--port <PUERTO>] <comando> [argumentos]
    jamman card <subcomando> ...        (solo la tarjeta, sin pedal)

COMANDOS DEL PEDAL (requieren conexión USB):
    about                 Datos conocidos del dispositivo
    ports                 Lista los puertos serie y marca los del JamMan
    who-am-i              Identificación que devuelve el pedal
    info <ruta>           Información de un fichero o carpeta del pedal
    ls <ruta>             Contenido de una carpeta del pedal
    get <ruta> <destino>  Descarga un fichero del pedal
    help                  Esta ayuda

Si no se indica --port se usa el primer puerto con VID:PID 1210:0020.

";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let port_name = take_option(&mut args, "--port")?;

    if args.first().map(String::as_str) == Some("card") {
        return card_cmd::run(args.split_off(1));
    }

    let Some((command, rest)) = args.split_first() else {
        println!("{}{}", HELP_PEDAL, card_cmd::HELP);
        return Ok(());
    };

    match (command.as_str(), rest) {
        ("help" | "--help" | "-h", _) => println!("{}{}", HELP_PEDAL, card_cmd::HELP),
        ("about", []) => about(),
        ("ports", []) => port::print_ports()?,
        ("who-am-i", []) => {
            let info = connect(port_name)?.who_am_i()?;
            println!("ids:   {:?}", info.ids);
            println!("texto: {:?}", info.texts);
        }
        ("info", [path]) => {
            let mut client = connect(port_name)?;
            // Si se pregunta por un fichero y es una carpeta, el pedal responde
            // con error («Entry type does not match»): se prueba como carpeta.
            let info = match client.entry_info(path, EntryKind::File) {
                Ok(info) if info.exists => info,
                _ => client.entry_info(path, EntryKind::Folder)?,
            };
            println!("{info:#?}");
        }
        ("ls", [path]) => {
            let (files, folders) = connect(port_name)?.folder_contents(path)?;
            folders.iter().for_each(|n| println!("📁 {n}/"));
            files.iter().for_each(|n| println!("   {n}"));
        }
        ("get", [remote, local]) => {
            let data = connect(port_name)?.download(remote)?;
            fs::write(local, &data)?;
            println!("{} bytes guardados en {local}", data.len());
        }
        _ => {
            return Err(format!(
                "comando o argumentos no válidos\n\n{HELP_PEDAL}{}",
                card_cmd::HELP
            )
            .into());
        }
    }
    Ok(())
}

fn connect(
    name: Option<String>,
) -> Result<Client<Box<dyn serialport::SerialPort>>, Box<dyn Error>> {
    Ok(Client::new(port::open(name)?))
}

fn about() {
    println!(
        "JamMan Stereo — USB {:04x}:{:04x}, {} bucles por almacén, WAV {} Hz/{} bit/{} canales",
        device::USB_VENDOR_ID,
        device::USB_PRODUCT_ID,
        device::SLOTS_PER_STORAGE,
        audio::SAMPLE_RATE_HZ,
        audio::BITS_PER_SAMPLE,
        audio::CHANNELS,
    );
}
