//! Errores de la conversión y lectura de audio.

use std::fmt;

/// Error de audio.
#[derive(Debug)]
#[non_exhaustive]
pub enum AudioError {
    /// Error de entrada/salida.
    Io(std::io::Error),
    /// El fichero no se puede decodificar (formato desconocido o dañado).
    Decode(String),
    /// El fichero no contiene ninguna pista de audio.
    NoAudioTrack,
    /// El fichero no contiene muestras de audio.
    Empty,
    /// El audio supera la duración máxima del pedal.
    TooLong {
        /// Duración del audio, en segundos.
        seconds: f64,
    },
    /// Error interno del remuestreo.
    Resample(String),
    /// Formato no admitido por la operación (p. ej. forma de onda de 24 bit).
    Unsupported(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "error de E/S: {e}"),
            Self::Decode(why) => write!(f, "no se puede leer el audio: {why}"),
            Self::NoAudioTrack => write!(f, "el fichero no contiene audio"),
            Self::Empty => write!(f, "el fichero de audio está vacío"),
            Self::TooLong { seconds } => write!(
                f,
                "el audio dura {:.0} s y el pedal admite como máximo {} s (10 minutos)",
                seconds,
                jamman_core::audio::MAX_LOOP_SECONDS
            ),
            Self::Resample(why) => write!(f, "error al remuestrear: {why}"),
            Self::Unsupported(why) => write!(f, "formato no admitido: {why}"),
        }
    }
}

impl std::error::Error for AudioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AudioError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
