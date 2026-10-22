//! Cabecera de 10 bytes común a peticiones y respuestas.
//!
//! Orden en el cable (todo *little-endian*):
//!
//! | Bytes | Campo |
//! |---|---|
//! | 0..2 | tipo de mensaje (`u16`) |
//! | 2..6 | longitud de la carga que sigue, sin contar la cabecera (`u32`) |
//! | 6..10 | número de serie (`u32`), lo copia la respuesta de la petición |

use super::wire::Reader;
use crate::Result;

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
        out[2..6].copy_from_slice(&self.payload_len.to_le_bytes());
        out[6..10].copy_from_slice(&self.serial.to_le_bytes());
        out
    }

    /// Lee una cabecera de los primeros [`HEADER_LEN`] bytes de `buf`.
    pub fn parse(buf: &[u8]) -> Result<Self> {
        let mut r = Reader::new(buf);
        Ok(Self {
            message_type: r.u16()?,
            payload_len: r.u32()?,
            serial: r.u32()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn encodes_in_documented_order() {
        let h = Header {
            message_type: 0x0010,
            payload_len: 8,
            serial: 1,
        };
        assert_eq!(
            h.to_bytes(),
            [0x10, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]
        );
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
