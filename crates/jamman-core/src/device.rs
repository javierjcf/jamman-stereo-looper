//! Identificación y límites del dispositivo.

use crate::{Error, Result};

/// Identificador de fabricante USB (DigiTech), según `JamMan.inf`.
pub const USB_VENDOR_ID: u16 = 0x1210;
/// Identificador de producto USB del JamMan, según `JamMan.inf`.
pub const USB_PRODUCT_ID: u16 = 0x0020;
/// Número de posiciones de bucle por almacén (según el manual).
pub const SLOTS_PER_STORAGE: u8 = 99;

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

    /// Todas las posiciones, de la 1 a la 99.
    pub fn all() -> impl Iterator<Item = Slot> {
        (1..=SLOTS_PER_STORAGE).map(Slot)
    }
}

/// Primera posición libre desde `start` (incluida), saltando las ocupadas.
pub fn first_free_from(occupied: &[Slot], start: Slot) -> Option<Slot> {
    Slot::all()
        .skip_while(|s| *s < start)
        .find(|s| !occupied.contains(s))
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
        assert_eq!(Slot::all().count(), 99);
    }

    #[test]
    fn free_slots_are_searched_from_the_start_skipping_occupied_ones() {
        let s = |n| Slot::new(n).unwrap();
        let occupied = [s(1), s(5), s(6)];
        assert_eq!(first_free_from(&occupied, s(1)), Some(s(2)));
        assert_eq!(first_free_from(&occupied, s(5)), Some(s(7)));
        assert_eq!(first_free_from(&occupied, s(3)), Some(s(3)));
        assert_eq!(first_free_from(&[s(99)], s(99)), None);
    }
}
