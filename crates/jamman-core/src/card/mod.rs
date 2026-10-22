//! Tarjeta SD/SDHC del JamMan Stereo.
//!
//! Se puede leer y escribir **sin tener el pedal conectado**: basta con la
//! tarjeta montada. Ver `docs/04-tarjeta-sd.md` para el formato.

pub mod wav;
#[allow(dead_code)]
pub(crate) mod xml;
