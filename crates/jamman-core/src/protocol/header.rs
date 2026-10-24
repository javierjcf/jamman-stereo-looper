//! Cabecera de 10 bytes común a peticiones y respuestas.
//!
//! Orden en el cable (todo *little-endian*):
//!
//! | Bytes | Campo |
//! |---|---|
//! | 0..2 | tipo de mensaje (`u16`) |
//! | 2..6 | bytes que siguen a este campo: número de serie (4) + carga (`u32`) |
//! | 6..10 | número de serie (`u32`), lo copia la respuesta de la petición |
//!
//! El campo de longitud **incluye los 4 bytes del número de serie**; validado
//! con el pedal: un `WhoAmI` con longitud 0 se responde ignorando la serie y
//! con longitud 4 la devuelve. [`Header::payload_len`] guarda solo la carga.

use super::wire::Reader;
use crate::{Error, Result};

/// Bytes del número de serie, incluidos en el campo de longitud.
const SERIAL_LEN: u32 = 4;

/// Tamaño de la cabecera en bytes.
pub const HEADER_LEN: usize = 10;

/// Cabecera de un mensaje.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Identificador del tipo de mensaje.
    pub message_type: u16,
    /// Bytes de carga que siguen a la cabecera.
    pub payload_len: u32,
    /// Número de serie que empareja petición y respuesta.
    pub serial: u32,
}

impl Header {
    /// Serializa la cabecera.
    pub fn to_bytes(self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..2].copy_from_slice(&self.message_type.to_le_bytes());
        out[2..6].copy_from_slice(&(self.payload_len + SERIAL_LEN).to_le_bytes());
        out[6..10].copy_from_slice(&self.serial.to_le_bytes());
        out
    }

    /// Lee una cabecera de los primeros [`HEADER_LEN`] bytes de `buf`.
    pub fn parse(buf: &[u8]) -> Result<Self> {
        let mut r = Reader::new(buf);
        let message_type = r.u16()?;
        let length = r.u32()?;
        let payload_len = length.checked_sub(SERIAL_LEN).ok_or(Error::Truncated {
            needed: SERIAL_LEN as usize,
            available: length as usize,
        })?;
        Ok(Self {
            message_type,
            payload_len,
            serial: r.u32()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_in_documented_order() {
        let h = Header {
            message_type: 0x0010,
            payload_len: 8,
            serial: 1,
        };
        // La longitud (12) cuenta la serie (4) más la carga (8).
        assert_eq!(
            h.to_bytes(),
            [0x10, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn parses_a_real_who_am_i_reply_from_the_pedal() {
        // Primeros 10 bytes de la respuesta real del pedal (75 bytes en total).
        let h =
            Header::parse(&[0x01, 0x00, 0x45, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(h.message_type, 1);
        assert_eq!(h.serial, 8);
        assert_eq!(HEADER_LEN + h.payload_len as usize, 75);
    }

    #[test]
    fn a_length_shorter_than_the_serial_is_rejected() {
        assert!(matches!(
            Header::parse(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0]),
            Err(Error::Truncated { .. })
        ));
    }

    #[test]
    fn parse_is_the_inverse_of_to_bytes() {
        let h = Header {
            message_type: 0x8001,
            payload_len: 123_456,
            serial: 0xDEAD_BEEF,
        };
        assert_eq!(Header::parse(&h.to_bytes()).unwrap(), h);
    }

    #[test]
    fn parse_rejects_short_input() {
        assert!(matches!(
            Header::parse(&[0; 9]),
            Err(Error::Truncated { .. })
        ));
    }
}
