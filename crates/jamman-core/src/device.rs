//! Identificación y límites del dispositivo.

use crate::{Error, Result};

/// Identificador de fabricante USB (DigiTech), según `JamMan.inf`.
pub const USB_VENDOR_ID: u16 = 0x1210;
/// Identificador de producto USB del JamMan, según `JamMan.inf`.
pub const USB_PRODUCT_ID: u16 = 0x0020;
/// Número de posiciones de bucle por almacén (según el manual).
pub const SLOTS_PER_STORAGE: u8 = 99;

/// Almacén de bucles del pedal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    /// Memoria interna (unos 35 minutos en total).
    Internal,
    /// Tarjeta SD/SDHC opcional.
    External,
}

/// Posición de bucle validada (1 a 99).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(u8);

impl Slot {
    /// Crea una posición comprobando que está en `1..=99`.
    pub fn new(n: u8) -> Result<Self> {
        if (1..=SLOTS_PER_STORAGE).contains(&n) {
            Ok(Self(n))
        } else {
            Err(Error::InvalidSlot(n))
        }
    }

    /// Devuelve el número de posición.
    pub fn get(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_limits() {
        assert!(Slot::new(0).is_err());
        assert!(Slot::new(1).is_ok());
        assert!(Slot::new(99).is_ok());
        assert!(Slot::new(100).is_err());
    }
}
