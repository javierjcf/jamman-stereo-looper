//! Tipo de error común de la librería.

use std::fmt;

/// Errores que puede devolver `jamman-core`.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Número de bucle fuera del rango admitido por el pedal.
    InvalidSlot(u8),
}

/// Alias de resultado de la librería.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSlot(n) => write!(f, "posición de bucle inválida: {n} (válido: 1..=99)"),
        }
    }
}

impl std::error::Error for Error {}
