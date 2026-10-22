//! Audio para el JamMan Stereo: conversión al formato del pedal y forma de onda.
//!
//! - [`convert_to_pedal_wav`] decodifica MP3, FLAC, OGG/Vorbis, WAV, AIFF y
//!   M4A/AAC/ALAC (con `symphonia`, Rust puro), lo pasa a estéreo, lo remuestrea
//!   a 44,1 kHz (con `rubato`) y lo guarda como WAV PCM de 16 bit.
//! - [`waveform`] resume un WAV de 16 bit en picos para dibujar su forma de onda.

mod convert;
mod error;
mod waveform;

pub use convert::{ConversionReport, ConvertOptions, convert_to_pedal_wav, needs_conversion};
pub use error::AudioError;
pub use waveform::waveform;

/// Extensiones de audio que se pueden importar.
pub const SUPPORTED_EXTENSIONS: [&str; 9] = [
    "wav", "mp3", "flac", "ogg", "oga", "aif", "aiff", "m4a", "mp4",
];
