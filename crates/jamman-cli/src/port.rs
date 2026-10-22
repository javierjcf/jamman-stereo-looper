//! Localización y apertura del puerto serie del pedal.

use std::error::Error;
use std::time::Duration;

use jamman_core::device::{USB_PRODUCT_ID, USB_VENDOR_ID};
use serialport::{DataBits, Parity, SerialPort, SerialPortType, StopBits};

/// Velocidad que usa el software original. En CDC-ACM no afecta al rendimiento.
const BAUD_RATE: u32 = 115_200;

/// Tiempo máximo de espera de lectura (el original usa 10 s).
const READ_TIMEOUT: Duration = Duration::from_secs(10);

fn is_jamman(port_type: &SerialPortType) -> bool {
    matches!(
        port_type,
        SerialPortType::UsbPort(usb) if usb.vid == USB_VENDOR_ID && usb.pid == USB_PRODUCT_ID
    )
}

/// Muestra los puertos serie USB y marca los que son un JamMan.
pub fn print_ports() -> Result<(), Box<dyn Error>> {
    let mut found = false;
    for p in serialport::available_ports()? {
        if let SerialPortType::UsbPort(usb) = &p.port_type {
            let mark = if is_jamman(&p.port_type) {
                "  <- JamMan"
            } else {
                ""
            };
            println!("{}  {:04x}:{:04x}{mark}", p.port_name, usb.vid, usb.pid);
            found = true;
        }
    }
    if !found {
        println!("No se ha encontrado ningún puerto serie USB.");
    }
    Ok(())
}

/// Abre el puerto indicado o, si no se indica, el primer JamMan encontrado.
pub fn open(name: Option<String>) -> Result<Box<dyn SerialPort>, Box<dyn Error>> {
    let name = match name {
        Some(n) => n,
        None => serialport::available_ports()?
            .into_iter()
            .find(|p| is_jamman(&p.port_type))
            .map(|p| p.port_name)
            .ok_or("no se encuentra ningún JamMan (USB 1210:0020); usa --port")?,
    };
    let mut port = serialport::new(&name, BAUD_RATE)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .timeout(READ_TIMEOUT)
        .open()
        .map_err(|e| format!("no se puede abrir {name}: {e}"))?;
    // El original deja DTR activo (handshake por hardware en Windows).
    port.write_data_terminal_ready(true)?;
    Ok(port)
}
