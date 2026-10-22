//! Conversión de cualquier audio admitido al WAV que acepta el pedal.
//!
//! El audio completo se decodifica a memoria en `f32` (para 10 minutos a
//! 48 kHz son unos 230 MB como máximo), se remuestrea en un solo paso y se
//! escribe con una cabecera WAV canónica de 44 bytes.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use jamman_core::audio::{BITS_PER_SAMPLE, CHANNELS, MAX_LOOP_SECONDS, SAMPLE_RATE_HZ};
use jamman_core::card::wav;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::AudioError;

/// Pico objetivo al normalizar (≈ -0,2 dBFS), para dejar algo de margen.
const NORMALIZE_PEAK: f32 = 0.98;

/// Opciones de conversión.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConvertOptions {
    /// Ajusta el volumen para que el pico quede justo por debajo de 0 dBFS.
    pub normalize: bool,
}

/// Resumen de una conversión.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversionReport {
    /// Frecuencia de muestreo original.
    pub source_rate: u32,
    /// Canales originales.
    pub source_channels: usize,
    /// Duración del resultado en segundos.
    pub duration_secs: f64,
    /// Ganancia aplicada al normalizar, en dB (`None` si no se normalizó).
    pub gain_db: Option<f32>,
}

/// `true` si el fichero no es ya un WAV que el pedal acepte tal cual.
pub fn needs_conversion(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return true;
    };
    !matches!(
        wav::read_info(&mut file).map(|i| i.check_pedal_format()),
        Ok(Ok(()))
    )
}

/// Audio decodificado en estéreo intercalado (`L, R, L, R…`).
struct Decoded {
    rate: u32,
    channels: usize,
    samples: Vec<f32>,
}

impl Decoded {
    fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}

/// Decodifica el fichero entero a estéreo `f32`. Los ficheros mono se duplican
/// en ambos canales y de los multicanal se toman los dos primeros.
fn decode(src: &Path, progress: &mut dyn FnMut(f32)) -> Result<Decoded, AudioError> {
    let file = Box::new(File::open(src)?);
    let mss = MediaSourceStream::new(file, Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = src.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| AudioError::Decode(e.to_string()))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(AudioError::NoAudioTrack)?;
    let track_id = track.id;
    let expected_frames = track.num_frames;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(AudioError::NoAudioTrack)?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| AudioError::Decode(e.to_string()))?;

    let mut out = Decoded {
        rate: 0,
        channels: 0,
        samples: Vec::new(),
    };
    let mut scratch: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(AudioError::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(buf) => buf,
            // Un paquete dañado se salta, como hacen los reproductores.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(AudioError::Decode(e.to_string())),
        };
        let channels = buf.spec().channels().count();
        if channels == 0 {
            continue;
        }
        out.rate = buf.spec().rate();
        out.channels = channels;
        scratch.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut scratch);
        for frame in scratch.chunks_exact(channels) {
            let left = frame[0];
            let right = if channels > 1 { frame[1] } else { left };
            out.samples.push(left);
            out.samples.push(right);
        }

        let seconds = out.frames() as f64 / f64::from(out.rate.max(1));
        if seconds > f64::from(MAX_LOOP_SECONDS) {
            return Err(AudioError::TooLong { seconds });
        }
        if let Some(total) = expected_frames.filter(|&t| t > 0) {
            progress((out.frames() as f32 / total as f32).min(1.0) * 0.8);
        }
    }
    if out.samples.is_empty() {
        return Err(AudioError::Empty);
    }
    Ok(out)
}

/// Remuestrea audio estéreo intercalado a la frecuencia del pedal.
fn resample(samples: &[f32], rate_in: u32) -> Result<Vec<f32>, AudioError> {
    let frames = samples.len() / 2;
    let input = InterleavedSlice::new(samples, 2, frames)
        .map_err(|e| AudioError::Resample(e.to_string()))?;
    let mut resampler = Fft::<f32>::new(
        rate_in as usize,
        SAMPLE_RATE_HZ as usize,
        1024,
        2,
        FixedSync::Both,
    )
    .map_err(|e| AudioError::Resample(e.to_string()))?;
    let output = resampler
        .process_all(&input, frames, None)
        .map_err(|e| AudioError::Resample(e.to_string()))?;
    Ok(output.take_data())
}

/// Escribe estéreo 16 bit a 44,1 kHz con cabecera canónica.
fn write_wav(dest: &Path, samples: &[i16]) -> Result<(), AudioError> {
    let data_len = u32::try_from(samples.len() * 2)
        .map_err(|_| AudioError::Unsupported("audio demasiado grande para WAV".into()))?;
    let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
    let byte_rate = SAMPLE_RATE_HZ * u32::from(block_align);

    let mut w = BufWriter::new(File::create(dest)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&CHANNELS.to_le_bytes())?;
    w.write_all(&SAMPLE_RATE_HZ.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    w.write_all(&bytes)?;
    w.into_inner().map_err(|e| e.into_error())?.sync_all()?;
    Ok(())
}

/// Convierte `src` a un WAV que el pedal acepta y lo guarda en `dest`.
///
/// `progress` recibe valores de 0 a 1 (aproximados; algunos formatos no
/// declaran su duración y solo se informa al final).
pub fn convert_to_pedal_wav(
    src: &Path,
    dest: &Path,
    options: &ConvertOptions,
    mut progress: impl FnMut(f32),
) -> Result<ConversionReport, AudioError> {
    let decoded = decode(src, &mut progress)?;
    let mut samples = if decoded.rate == SAMPLE_RATE_HZ {
        decoded.samples
    } else {
        resample(&decoded.samples, decoded.rate)?
    };
    progress(0.9);

    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let gain_db = if options.normalize && peak > 0.0 {
        let gain = NORMALIZE_PEAK / peak;
        samples.iter_mut().for_each(|s| *s *= gain);
        Some(20.0 * gain.log10())
    } else {
        None
    };

    let pcm: Vec<i16> = samples
        .iter()
        .map(|s| (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16)
        .collect();
    let duration_secs = (pcm.len() / 2) as f64 / f64::from(SAMPLE_RATE_HZ);
    if duration_secs > f64::from(MAX_LOOP_SECONDS) {
        return Err(AudioError::TooLong {
            seconds: duration_secs,
        });
    }
    write_wav(dest, &pcm)?;
    progress(1.0);

    Ok(ConversionReport {
        source_rate: decoded.rate,
        source_channels: decoded.channels,
        duration_secs,
        gain_db,
    })
}
