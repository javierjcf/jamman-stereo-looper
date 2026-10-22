//! Resumen de la forma de onda de un WAV para dibujarla.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use jamman_core::card::wav;

use crate::AudioError;

/// Divide el audio en `buckets` tramos y devuelve el pico (0 a 1) de cada uno,
/// tomando el mayor valor absoluto de todos los canales.
///
/// Admite WAV PCM de 16 bit, que es el formato del pedal.
pub fn waveform(path: &Path, buckets: usize) -> Result<Vec<f32>, AudioError> {
    let mut file = File::open(path)?;
    let info = wav::read_info(&mut file).map_err(|e| AudioError::Decode(e.to_string()))?;
    if info.format_tag != 1 || info.bits_per_sample != 16 || info.channels == 0 {
        return Err(AudioError::Unsupported(
            "la forma de onda requiere WAV PCM de 16 bit".into(),
        ));
    }
    let buckets = buckets.max(1);
    let channels = usize::from(info.channels);
    let frames = (info.data_len as usize) / (2 * channels);
    let mut peaks = vec![0.0f32; buckets];
    if frames == 0 {
        return Ok(peaks);
    }
    let frames_per_bucket = frames.div_ceil(buckets);

    file.seek(SeekFrom::Start(info.data_offset))?;
    let mut reader = BufReader::with_capacity(1 << 16, file.take(info.data_len));
    let mut chunk = vec![0u8; 2 * channels * 4096];
    let mut frame_index = 0usize;
    loop {
        let read = read_full(&mut reader, &mut chunk)?;
        let usable = read - read % (2 * channels);
        if usable == 0 {
            break;
        }
        for frame in chunk[..usable].chunks_exact(2 * channels) {
            let bucket = (frame_index / frames_per_bucket).min(buckets - 1);
            for sample in frame.as_chunks::<2>().0 {
                let v = f32::from(i16::from_le_bytes(*sample).unsigned_abs()) / 32768.0;
                if v > peaks[bucket] {
                    peaks[bucket] = v;
                }
            }
            frame_index += 1;
        }
    }
    Ok(peaks)
}

/// Lee hasta llenar `buf` o llegar al final.
fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match r.read(&mut buf[total..])? {
            0 => break,
            n => total += n,
        }
    }
    Ok(total)
}
