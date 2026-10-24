//! Propiedades del pedal que se leen con
//! [`Request::GetProperty`](super::request::Request::GetProperty).
//!
//! El original las lee para saber si cabe un fichero antes de copiarlo
//! (`FUN_0047b090`): con el pedal se ha comprobado que son kilobytes y que
//! cuadran con los bucles guardados.

/// KB libres en la memoria interna.
pub const INTERNAL_FREE_KB: u16 = 10;
/// KB totales de la memoria interna.
pub const INTERNAL_TOTAL_KB: u16 = 11;
/// KB libres en la tarjeta SD del pedal.
pub const CARD_FREE_KB: u16 = 12;
/// KB totales de la tarjeta SD del pedal.
pub const CARD_TOTAL_KB: u16 = 13;
