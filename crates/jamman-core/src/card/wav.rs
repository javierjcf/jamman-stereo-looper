//! Lectura de la cabecera de un fichero WAV.
//!
//! Solo se leen los metadatos (formato y tamaño de los datos), sin cargar el
//! audio. Se recorren los *chunks* RIFF, de modo que se admiten ficheros con
//! bloques intermedios como `LIST/INFO` (los que escribe `ffmpeg`), que
//! desplazan el inicio de los datos respecto a la cabecera canónica de 44 bytes.

use std::io::{Read, Seek, SeekFrom};

use crate::audio;
use crate::{Error, Result};

/// Etiqueta de formato PCM sin compresión.
const FORMAT_PCM: u16 = 1;

/// Metadatos de un WAV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    /// Etiqueta de formato (1 = PCM).
    pub format_tag: u16,
    /// Número de canales.
    pub channels: u16,
    /// Frecuencia de muestreo en Hz.
    pub sample_rate: u32,
    /// Bits por muestra.
    pub bits_per_sample: u16,
    /// Posición del primer byte de audio en el fichero.
    pub data_offset: u64,
    /// Bytes de audio. Se recorta al tamaño real si la cabecera miente.
    pub data_len: u64,
}

impl WavInfo {
    /// Bytes por segundo.
    pub fn byte_rate(&self) -> u64 {
        u64::from(self.sample_rate) * u64::from(self.channels) * u64::from(self.bits_per_sample / 8)
    }

    /// Duración en segundos.
    pub fn duration_secs(&self) -> f64 {
        match self.byte_rate() {
            0 => 0.0,
            rate => self.data_len as f64 / rate as f64,
        }
    }

    /// Comprueba que el audio es el que acepta el pedal: PCM, 44,1 kHz, 16 bit,
    /// estéreo y de 10 minutos como máximo. Devuelve el motivo si no lo es.
    pub fn check_pedal_format(&self) -> Result<()> {
        let mut problems = Vec::new();
        if self.format_tag != FORMAT_PCM {
            problems.push(format!("no es PCM (formato {})", self.format_tag));
        }
        if self.channels != audio::CHANNELS {
            problems.push(format!(
                "{} canales (hacen falta {})",
                self.channels,
                audio::CHANNELS
            ));
        }
        if self.sample_rate != audio::SAMPLE_RATE_HZ {
            problems.push(format!(
                "{} Hz (hacen falta {})",
                self.sample_rate,
                audio::SAMPLE_RATE_HZ
            ));
        }
        if self.bits_per_sample != audio::BITS_PER_SAMPLE {
            problems.push(format!(
                "{} bits (hacen falta {})",
                self.bits_per_sample,
                audio::BITS_PER_SAMPLE
            ));
        }
        if problems.is_empty() && self.duration_secs() > f64::from(audio::MAX_LOOP_SECONDS) {
            problems.push(format!(
                "dura {:.0} s (máximo {} s)",
                self.duration_secs(),
                audio::MAX_LOOP_SECONDS
            ));
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(Error::UnsupportedAudio(problems.join("; ")))
        }
    }
}

/// Lee los metadatos de un WAV.
pub fn read_info<R: Read + Seek>(r: &mut R) -> Result<WavInfo> {
    let total = r.seek(SeekFrom::End(0))?;
    r.seek(SeekFrom::Start(0))?;

    let mut head = [0u8; 12];
    read_exact(r, &mut head)?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err(Error::InvalidWav("no es un fichero RIFF/WAVE"));
    }

    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut pos = 12u64;
    while pos + 8 <= total {
        r.seek(SeekFrom::Start(pos))?;
        let mut chunk = [0u8; 8];
        read_exact(r, &mut chunk)?;
        let size = u64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]));
        let body = pos + 8;

        match &chunk[0..4] {
            b"fmt " => {
                if size < 16 {
                    return Err(Error::InvalidWav("bloque fmt demasiado corto"));
                }
                let mut f = [0u8; 16];
                read_exact(r, &mut f)?;
                fmt = Some((
                    u16::from_le_bytes([f[0], f[1]]),
                    u16::from_le_bytes([f[2], f[3]]),
                    u32::from_le_bytes([f[4], f[5], f[6], f[7]]),
                    u16::from_le_bytes([f[14], f[15]]),
                ));
            }
            b"data" => {
                let (format_tag, channels, sample_rate, bits_per_sample) =
                    fmt.ok_or(Error::InvalidWav("el bloque data aparece antes que fmt"))?;
                return Ok(WavInfo {
                    format_tag,
                    channels,
                    sample_rate,
                    bits_per_sample,
                    data_offset: body,
                    data_len: size.min(total - body),
                });
            }
            _ => {}
        }
        // Los bloques se alinean a un número par de bytes.
        pos = body + size + (size & 1);
    }
    Err(Error::InvalidWav("falta el bloque data"))
}

/// Interpreta la cabecera de un WAV a partir de sus primeros bytes y del
/// tamaño real del fichero. Sirve cuando el fichero está en el pedal y solo se
/// descarga el principio.
pub fn parse_header(head: &[u8], file_len: u64) -> Result<WavInfo> {
    let mut info = read_info(&mut std::io::Cursor::new(head))?;
    // `read_info` recorta los datos a lo leído: se corrige con el tamaño real.
    let declared = data_declared_len(head, info.data_offset)?;
    info.data_len = declared.min(file_len.saturating_sub(info.data_offset));
    Ok(info)
}

/// Longitud declarada en el bloque `data` que empieza en `data_offset`.
fn data_declared_len(head: &[u8], data_offset: u64) -> Result<u64> {
    let at = (data_offset as usize)
        .checked_sub(4)
        .ok_or(Error::InvalidWav("cabecera truncada"))?;
    let bytes: [u8; 4] = head
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .ok_or(Error::InvalidWav("cabecera truncada"))?;
    Ok(u64::from(u32::from_le_bytes(bytes)))
}

fn read_exact<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<()> {
    r.read_exact(buf).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => Error::InvalidWav("fichero truncado"),
        _ => Error::Io(e),
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::card::testutil::wav_bytes;

    fn info(bytes: Vec<u8>) -> Result<WavInfo> {
        read_info(&mut Cursor::new(bytes))
    }

    #[test]
    fn reads_a_canonical_header() {
        let i = info(wav_bytes(2, 44_100, 16, &[0; 400], false)).unwrap();
        assert_eq!(
            (i.channels, i.sample_rate, i.bits_per_sample),
            (2, 44_100, 16)
        );
        assert_eq!(i.data_offset, 44);
        assert_eq!(i.data_len, 400);
        i.check_pedal_format().unwrap();
    }

    #[test]
    fn skips_list_chunks_and_even_padding() {
        let i = info(wav_bytes(2, 44_100, 16, &[0; 8], true)).unwrap();
        assert_eq!(i.data_offset, 44 + 8 + 6);
        assert_eq!(i.data_len, 8);
    }

    #[test]
    fn duration_is_data_over_byte_rate() {
        let i = info(wav_bytes(2, 44_100, 16, &vec![0; 176_400], false)).unwrap();
        assert!((i.duration_secs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn declared_size_larger_than_the_file_is_clamped() {
        let mut bytes = wav_bytes(2, 44_100, 16, &[0; 100], false);
        bytes.truncate(44 + 40);
        assert_eq!(info(bytes).unwrap().data_len, 40);
    }

    #[test]
    fn header_from_the_first_bytes_uses_the_real_file_length() {
        let full = wav_bytes(2, 44_100, 16, &[0; 1000], true);
        let info = parse_header(&full[..80], full.len() as u64).unwrap();
        assert_eq!(info.data_len, 1000);
        assert_eq!(info.data_offset, 58);
        // Si el fichero real es más corto que lo declarado, se recorta.
        let info = parse_header(&full[..80], 58 + 10).unwrap();
        assert_eq!(info.data_len, 10);
    }

    #[test]
    fn rejects_non_wav_and_truncated_input() {
        assert!(matches!(
            info(b"hola mundo!!".to_vec()),
            Err(Error::InvalidWav(_))
        ));
        assert!(matches!(info(vec![1, 2, 3]), Err(Error::InvalidWav(_))));
        let no_data = wav_bytes(2, 44_100, 16, &[], false)[..36].to_vec();
        assert!(matches!(info(no_data), Err(Error::InvalidWav(_))));
    }

    #[test]
    fn pedal_format_check_reports_every_problem() {
        let i = info(wav_bytes(1, 48_000, 24, &[0; 12], false)).unwrap();
        let Err(Error::UnsupportedAudio(why)) = i.check_pedal_format() else {
            panic!("debía rechazarse");
        };
        assert!(
            why.contains("canales") && why.contains("Hz") && why.contains("bits"),
            "{why}"
        );
    }

    #[test]
    fn pedal_format_check_enforces_the_ten_minute_limit() {
        let ok = info(wav_bytes(2, 44_100, 16, &[0; 4], false)).unwrap();
        let too_long = WavInfo {
            data_len: ok.byte_rate() * 601,
            ..ok
        };
        assert!(matches!(
            too_long.check_pedal_format(),
            Err(Error::UnsupportedAudio(_))
        ));
        let exact = WavInfo {
            data_len: ok.byte_rate() * 600,
            ..ok
        };
        exact.check_pedal_format().unwrap();
    }
}
