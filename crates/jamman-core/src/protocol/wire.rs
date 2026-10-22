//! Primitivas de lectura y escritura del cable.
//!
//! El software original usa los flujos de JUCE (`writeShort`, `writeInt`,
//! `writeInt64`, `write`), que son siempre *little-endian*. Las cadenas se
//! envían como `u32` con la longitud en bytes seguido de los bytes, sin
//! terminador.

use crate::{Error, Result};

/// Escritor de bytes con las primitivas del protocolo.
#[derive(Debug, Default)]
pub(crate) struct Writer(Vec<u8>);

impl Writer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }

    pub(crate) fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub(crate) fn i64(&mut self, v: i64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub(crate) fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }

    /// Cadena: `u32` con la longitud en bytes y los bytes UTF-8.
    pub(crate) fn string(&mut self, s: &str) -> &mut Self {
        let len = u32::try_from(s.len()).unwrap_or(u32::MAX);
        self.u32(len).bytes(s.as_bytes())
    }

    pub(crate) fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

/// Lector de bytes que consume un trozo de memoria.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Toma `n` bytes o devuelve [`Error::Truncated`].
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.data.len() < n {
            return Err(Error::Truncated {
                needed: n,
                available: self.data.len(),
            });
        }
        let (head, tail) = self.data.split_at(n);
        self.data = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub(crate) fn i8(&mut self) -> Result<i8> {
        Ok(i8::from_le_bytes(self.array()?))
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(crate) fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    /// Cadena con prefijo de longitud `u32`.
    pub(crate) fn string(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8)
    }

    /// Bytes que quedan sin leer.
    pub(crate) fn remaining(&self) -> &'a [u8] {
        self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_little_endian() {
        let mut w = Writer::new();
        w.u8(0xAA).u32(0x0102_0304).i64(-2);
        assert_eq!(
            w.into_vec(),
            [
                0xAA, 0x04, 0x03, 0x02, 0x01, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF
            ]
        );
    }

    #[test]
    fn string_has_u32_length_prefix_and_no_terminator() {
        let mut w = Writer::new();
        w.string("hola");
        assert_eq!(w.into_vec(), [4, 0, 0, 0, b'h', b'o', b'l', b'a']);
    }

    #[test]
    fn reader_roundtrips_what_writer_wrote() {
        let mut w = Writer::new();
        w.u8(7).u32(70_000).i64(-9).string("áé");
        let bytes = w.into_vec();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u32().unwrap(), 70_000);
        assert_eq!(r.i64().unwrap(), -9);
        assert_eq!(r.string().unwrap(), "áé");
        assert!(r.remaining().is_empty());
    }

    #[test]
    fn reader_reports_truncation() {
        let mut r = Reader::new(&[1, 2]);
        match r.u32() {
            Err(Error::Truncated {
                needed: 4,
                available: 2,
            }) => {}
            other => panic!("resultado inesperado: {other:?}"),
        }
    }

    #[test]
    fn reader_rejects_invalid_utf8() {
        let mut r = Reader::new(&[1, 0, 0, 0, 0xFF]);
        assert!(matches!(r.string(), Err(Error::InvalidUtf8)));
    }
}
