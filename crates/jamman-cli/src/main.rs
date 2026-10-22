//! CLI `jamman`: gestión del DigiTech JamMan Stereo desde la terminal.
//!
//! Uso: `jamman [--port <PUERTO>] <comando> [argumentos]`. Ver `jamman help`.

mod port;

use std::error::Error;
use std::fs;
use std::process::ExitCode;

use jamman_core::protocol::Client;
use jamman_core::{audio, device};

const HELP: &str = "\
jamman — gestor del DigiTech JamMan Stereo (EXPERIMENTAL: protocolo sin validar)

USO:
    jamman [--port <PUERTO>] <comando> [argumentos]

COMANDOS:
    about                 Datos conocidos del dispositivo
    ports                 Lista los puertos serie y marca los del JamMan
    who-am-i              Identificación que devuelve el pedal
    info <ruta>           Información de un fichero o carpeta del pedal
    ls <ruta>             Contenido de una carpeta del pedal
    get <ruta> <destino>  Descarga un fichero del pedal
    help                  Esta ayuda

Si no se indica --port se usa el primer puerto con VID:PID 1210:0020.";

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

    let Some((command, rest)) = args.split_first() else {
        println!("{HELP}");
        return Ok(());
    };

    match (command.as_str(), rest) {
        ("help" | "--help" | "-h", _) => println!("{HELP}"),
        ("about", []) => about(),
        ("ports", []) => port::print_ports()?,
        ("who-am-i", []) => {
            let info = connect(port_name)?.who_am_i()?;
            println!("ids:   {:?}", info.ids);
            println!("texto: {:?}", info.texts);
        }
        ("info", [path]) => {
            let info = connect(port_name)?.entry_info(path)?;
            println!("{info:#?}");
        }
        ("ls", [path]) => {
            let (first, second) = connect(port_name)?.folder_contents(path)?;
            println!("lista 1 ({}):", first.len());
            first.iter().for_each(|n| println!("  {n}"));
            println!("lista 2 ({}):", second.len());
            second.iter().for_each(|n| println!("  {n}"));
        }
        ("get", [remote, local]) => {
            let data = connect(port_name)?.download(remote)?;
            fs::write(local, &data)?;
            println!("{} bytes guardados en {local}", data.len());
        }
        _ => return Err(format!("comando o argumentos no válidos\n\n{HELP}").into()),
    }
    Ok(())
}

fn connect(
    name: Option<String>,
) -> Result<Client<Box<dyn serialport::SerialPort>>, Box<dyn Error>> {
    Ok(Client::new(port::open(name)?))
}

/// Extrae `--opción valor` de los argumentos si está presente.
fn take_option(args: &mut Vec<String>, name: &str) -> Result<Option<String>, String> {
    let Some(i) = args.iter().position(|a| a == name) else {
        return Ok(None);
    };
    if i + 1 >= args.len() {
        return Err(format!("falta el valor de {name}"));
    }
    let value = args.remove(i + 1);
    args.remove(i);
    Ok(Some(value))
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

#[cfg(test)]
mod tests {
    use super::take_option;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn extracts_the_option_and_its_value() {
        let mut a = args(&["ls", "--port", "/dev/ttyACM0", "/"]);
        assert_eq!(
            take_option(&mut a, "--port").unwrap().as_deref(),
            Some("/dev/ttyACM0")
        );
        assert_eq!(a, ["ls", "/"]);
    }

    #[test]
    fn missing_option_is_none() {
        let mut a = args(&["about"]);
        assert_eq!(take_option(&mut a, "--port").unwrap(), None);
    }

    #[test]
    fn option_without_value_is_an_error() {
        let mut a = args(&["ls", "--port"]);
        assert!(take_option(&mut a, "--port").is_err());
    }
}
