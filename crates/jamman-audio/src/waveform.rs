//! Resumen de la forma de onda de un WAV para dibujarla.
//!
//! En ficheros grandes no se lee todo el audio: de cada tramo se lee una
//! ventana de [`WINDOW_BYTES`]. En una tarjeta SD lenta pasa de leer ~50 MB a
//! leer unos pocos, y para dibujar el resultado es indistinguible.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use jamman_core::card::wav;

use crate::AudioError;

/// Por encima de este tamaño se muestrea en lugar de leerlo todo.
const FULL_READ_LIMIT: u64 = 8 * 1024 * 1024;

/// Bytes que se leen de cada tramo al muestrear.
const WINDOW_BYTES: u64 = 32 * 1024;

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
    if info.data_len > FULL_READ_LIMIT {
        sampled(&mut file, &info, channels, &mut peaks)?;
    } else {
        full(file, &info, channels, frames, &mut peaks)?;
    }
    Ok(peaks)
}

/// Mayor valor absoluto (0 a 1) de las muestras de 16 bit de `bytes`.
fn peak_of(bytes: &[u8]) -> f32 {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|s| f32::from(i16::from_le_bytes(*s).unsigned_abs()) / 32768.0)
        .fold(0.0, f32::max)
}

/// Lee todo el audio y reparte los picos entre los tramos.
fn full(
    file: File,
    info: &wav::WavInfo,
    channels: usize,
    frames: usize,
    peaks: &mut [f32],
) -> Result<(), AudioError> {
    let buckets = peaks.len();
    let frames_per_bucket = frames.div_ceil(buckets);
    let mut file = file;
    file.seek(SeekFrom::Start(info.data_offset))?;
    let mut reader = BufReader::with_capacity(1 << 16, file.take(info.data_len));
    let frame_bytes = 2 * channels;
    let mut chunk = vec![0u8; frame_bytes * 4096];
    let mut frame_index = 0usize;
    loop {
        let read = read_full(&mut reader, &mut chunk)?;
        let usable = read - read % frame_bytes;
        if usable == 0 {
            break;
        }
        for frame in chunk[..usable].chunks_exact(frame_bytes) {
            let bucket = (frame_index / frames_per_bucket).min(buckets - 1);
            peaks[bucket] = peaks[bucket].max(peak_of(frame));
            frame_index += 1;
        }
    }
    Ok(())
}

/// Lee una ventana al principio de cada tramo.
fn sampled(
    file: &mut File,
    info: &wav::WavInfo,
    channels: usize,
    peaks: &mut [f32],
) -> Result<(), AudioError> {
    let frame_bytes = 2 * channels as u64;
    let buckets = peaks.len() as u64;
    let window = (WINDOW_BYTES / frame_bytes).max(1) * frame_bytes;
    let mut buf = vec![0u8; window as usize];
    for (i, peak) in peaks.iter_mut().enumerate() {
        let start = info.data_len * i as u64 / buckets;
        let start = start - start % frame_bytes;
        let len = window.min(info.data_len - start) as usize;
        file.seek(SeekFrom::Start(info.data_offset + start))?;
        let read = read_full(file, &mut buf[..len])?;
        *peak = peak_of(&buf[..read - read % 2]);
    }
    Ok(())
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
