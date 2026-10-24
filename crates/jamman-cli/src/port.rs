//! Listado de puertos serie USB (la apertura está en `jamman_core::serial`).

use std::error::Error;

use jamman_core::serial;

/// Muestra los puertos serie USB y marca los que son un JamMan.
pub fn print_ports() -> Result<(), Box<dyn Error>> {
    let ports = serial::usb_ports()?;
    if ports.is_empty() {
        println!("No se ha encontrado ningún puerto serie USB.");
    }
    for p in ports {
        let mark = if p.is_jamman() { "  <- JamMan" } else { "" };
        println!("{}  {:04x}:{:04x}{mark}", p.name, p.vid, p.pid);
    }
    Ok(())
}
