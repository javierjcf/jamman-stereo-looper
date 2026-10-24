//! Conexión con el pedal por su puerto serie USB (feature `serial`).
//!
//! El pedal es un dispositivo CDC-ACM (`1210:0020`): en Linux aparece como
//! `/dev/ttyACM*` y en Windows como un puerto `COM`. Se abre con los mismos
//! parámetros que el software original.

use std::time::Duration;

use serialport::{DataBits, Parity, SerialPort, SerialPortType, StopBits};

use crate::device::{USB_PRODUCT_ID, USB_VENDOR_ID};
use crate::pedal::{PedalStore, Storage};
use crate::protocol::Client;
use crate::{Error, Result};

/// Puerto serie abierto.
pub type Port = Box<dyn SerialPort>;

/// Velocidad que usa el software original. En CDC-ACM no afecta al rendimiento.
const BAUD_RATE: u32 = 115_200;

/// Tiempo máximo de espera de lectura (el original usa 10 s).
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Puerto serie USB del sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbPort {
    /// Nombre del puerto (`/dev/ttyACM0`, `COM3`…).
    pub name: String,
    /// Fabricante USB.
    pub vid: u16,
    /// Producto USB.
    pub pid: u16,
}

impl UsbPort {
    /// `true` si es un JamMan Stereo.
    pub fn is_jamman(&self) -> bool {
        self.vid == USB_VENDOR_ID && self.pid == USB_PRODUCT_ID
    }
}

fn serial_error(e: serialport::Error) -> Error {
    Error::Serial(e.to_string())
}

/// Puertos serie USB conectados.
pub fn usb_ports() -> Result<Vec<UsbPort>> {
    Ok(serialport::available_ports()
        .map_err(serial_error)?
        .into_iter()
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(usb) => Some(UsbPort {
                name: p.port_name,
                vid: usb.vid,
                pid: usb.pid,
            }),
            _ => None,
        })
        .collect())
}

/// Puerto del primer JamMan conectado, si lo hay.
pub fn find_pedal() -> Option<String> {
    usb_ports()
        .ok()?
        .into_iter()
        .find(UsbPort::is_jamman)
        .map(|p| p.name)
}

/// Abre el puerto indicado o, si no se indica, el del primer JamMan.
pub fn open(name: Option<&str>) -> Result<Port> {
    let name = match name {
        Some(n) => n.to_owned(),
        None => find_pedal().ok_or(Error::PedalNotFound)?,
    };
    let mut port = serialport::new(&name, BAUD_RATE)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .timeout(READ_TIMEOUT)
        .open()
        .map_err(|e| Error::Serial(format!("no se puede abrir {name}: {e}")))?;
    // El original deja DTR activo (handshake por hardware en Windows).
    port.write_data_terminal_ready(true).map_err(serial_error)?;
    Ok(port)
}

/// Abre el pedal y devuelve un cliente del protocolo.
pub fn connect(name: Option<&str>) -> Result<Client<Port>> {
    Ok(Client::new(open(name)?))
}

/// Abre el pedal y uno de sus almacenes, comprobando que esté disponible.
pub fn open_store(name: Option<&str>, storage: Storage) -> Result<PedalStore<Port>> {
    PedalStore::open(connect(name)?, storage)
}
