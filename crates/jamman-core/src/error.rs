//! Tipo de error común de la librería.

use std::fmt;

/// Errores que puede devolver `jamman-core`.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Número de bucle fuera del rango admitido por el pedal.
    InvalidSlot(u8),
    /// El mensaje recibido o por enviar está incompleto.
    Truncated {
        /// Bytes que se necesitaban.
        needed: usize,
        /// Bytes disponibles.
        available: usize,
    },
    /// Una cadena del protocolo no es UTF-8 válido.
    InvalidUtf8,
    /// El pedal respondió con un tipo de mensaje desconocido.
    UnknownMessageType(u16),
    /// La ruta supera el máximo del protocolo (255 bytes).
    PathTooLong(usize),
    /// El bloque de datos supera el máximo por mensaje.
    ChunkTooLarge(usize),
    /// El mensaje declara una carga mayor que el límite aceptado.
    PayloadTooLarge(u32),
    /// El pedal respondió con un error (`FailureResponse`).
    Device(String),
    /// La respuesta no es del tipo que correspondía a la petición.
    UnexpectedResponse {
        /// Tipo de respuesta esperado.
        expected: &'static str,
        /// Tipo de mensaje recibido.
        got: u16,
    },
    /// Error de entrada/salida del transporte.
    Io(std::io::Error),
}

/// Alias de resultado de la librería.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSlot(n) => write!(f, "posición de bucle inválida: {n} (válido: 1..=99)"),
            Self::Truncated { needed, available } => {
                write!(
                    f,
                    "mensaje incompleto: faltan bytes ({needed} necesarios, {available} disponibles)"
                )
            }
            Self::InvalidUtf8 => write!(f, "cadena del protocolo con UTF-8 inválido"),
            Self::UnknownMessageType(t) => write!(f, "tipo de mensaje desconocido: {t:#06x}"),
            Self::PathTooLong(n) => write!(f, "ruta demasiado larga: {n} bytes (máximo 255)"),
            Self::ChunkTooLarge(n) => write!(f, "bloque demasiado grande: {n} bytes"),
            Self::PayloadTooLarge(n) => write!(f, "carga declarada demasiado grande: {n} bytes"),
            Self::Device(msg) => write!(f, "error del dispositivo: {msg}"),
            Self::UnexpectedResponse { expected, got } => {
                write!(
                    f,
                    "respuesta inesperada: se esperaba {expected}, llegó el tipo {got:#06x}"
                )
            }
            Self::Io(e) => write!(f, "error de E/S: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
