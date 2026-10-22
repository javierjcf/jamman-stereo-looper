//! Núcleo del proyecto: todo lo que sabemos del DigiTech JamMan Stereo.
//!
//! Esta librería no tiene interfaz gráfica ni depende de ella. La CLI y,
//! más adelante, la GUI son capas finas por encima. Los datos de este
//! módulo proceden del análisis del software original; ver `docs/`.

pub mod audio;
pub mod card;
pub mod device;
pub mod error;
pub mod protocol;

pub use error::{Error, Result};
