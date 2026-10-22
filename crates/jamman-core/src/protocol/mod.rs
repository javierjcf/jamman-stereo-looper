//! Protocolo "Greenglass" que habla el JamMan Stereo por su puerto serie USB.
//!
//! Ver `docs/03-protocolo.md` para el detalle y el grado de certeza de cada
//! dato. Esta capa no hace E/S: solo construye y descifra mensajes.

pub mod client;
pub mod header;
pub mod request;
pub mod response;
pub(crate) mod wire;

pub use client::Client;
pub use header::{HEADER_LEN, Header};
