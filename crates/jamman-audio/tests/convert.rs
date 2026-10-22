//! Tests de conversión y forma de onda con ficheros reales.

use std::f32::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};

use jamman_audio::{AudioError, ConvertOptions, convert_to_pedal_wav, needs_conversion, waveform};
use jamman_core::card::wav::{WavInfo, read_info};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Escribe un WAV PCM con una señal generada por `sample(frame, canal)` en -1..1.
fn write_pcm_wav(
    path: &Path,
    rate: u32,
    channels: u16,
    bits: u16,
    frames: usize,
    sample: impl Fn(usize, u16) -> f32,
) {
    let bytes_per_sample = usize::from(bits / 8);
    let mut data = Vec::with_capacity(frames * usize::from(channels) * bytes_per_sample);
    for f in 0..frames {
        for c in 0..channels {
            let v = sample(f, c).clamp(-1.0, 1.0);
            match bits {
                16 => data.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes()),
                24 => data.extend_from_slice(&((v * 8_388_607.0) as i32).to_le_bytes()[..3]),
                _ => unreachable!(),
            }
        }
    }
    let block_align = channels * (bits / 8);
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    fs::write(path, out).unwrap();
}

fn sine(rate: u32, amplitude: f32) -> impl Fn(usize, u16) -> f32 {
    move |f, _| amplitude * (TAU * 440.0 * f as f32 / rate as f32).sin()
}

fn converted_info(dest: &Path) -> WavInfo {
    let info = read_info(&mut fs::File::open(dest).unwrap()).unwrap();
    info.check_pedal_format().unwrap();
    info
}

fn peak_of(path: &Path) -> f32 {
    waveform(path, 1).unwrap()[0]
}

#[test]
fn compressed_formats_are_converted_to_the_pedal_format() {
    let dir = tempfile::tempdir().unwrap();
    // Pico de referencia medido con `ffmpeg -af volumedetect` (decodificador
    // independiente): -18,5 dB, -20,7 dB y -21,1 dB.
    for (name, rate, channels, reference_peak) in [
        ("tono-mono-22k.mp3", 22_050, 1, 0.119),
        ("tono-estereo-48k.ogg", 48_000, 2, 0.092),
        ("tono-estereo-44k.flac", 44_100, 2, 0.088),
    ] {
        let src = fixture(name);
        assert!(needs_conversion(&src), "{name}");
        let dest = dir.path().join(format!("{name}.wav"));
        let report = convert_to_pedal_wav(&src, &dest, &ConvertOptions::default(), |_| {}).unwrap();
        assert_eq!(report.source_rate, rate, "{name}");
        assert_eq!(report.source_channels, channels, "{name}");
        assert!(
            (report.duration_secs - 1.0).abs() < 0.08,
            "{name}: {}",
            report.duration_secs
        );
        let info = converted_info(&dest);
        assert!((info.duration_secs() - report.duration_secs).abs() < 1e-6);
        assert!(
            !needs_conversion(&dest),
            "{name}: el resultado ya vale tal cual"
        );
        let peak = peak_of(&dest);
        assert!(
            (peak - reference_peak).abs() < 0.005,
            "{name}: pico {peak}, ffmpeg mide {reference_peak}"
        );
    }
}

#[test]
fn wav_with_other_rate_depth_and_channels_is_converted() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("in.wav");
    write_pcm_wav(&src, 48_000, 1, 24, 48_000 * 2, sine(48_000, 0.5));
    assert!(needs_conversion(&src));

    let dest = dir.path().join("out.wav");
    let report = convert_to_pedal_wav(&src, &dest, &ConvertOptions::default(), |_| {}).unwrap();
    assert_eq!((report.source_rate, report.source_channels), (48_000, 1));
    assert!(
        (report.duration_secs - 2.0).abs() < 0.01,
        "{}",
        report.duration_secs
    );
    converted_info(&dest);
    let peak = peak_of(&dest);
    assert!((peak - 0.5).abs() < 0.05, "el volumen se conserva: {peak}");
    assert_eq!(report.gain_db, None);
}

#[test]
fn normalization_raises_the_peak_to_just_below_full_scale() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bajo.wav");
    write_pcm_wav(&src, 44_100, 2, 16, 44_100, sine(44_100, 0.25));
    let dest = dir.path().join("out.wav");
    let report =
        convert_to_pedal_wav(&src, &dest, &ConvertOptions { normalize: true }, |_| {}).unwrap();
    let gain = report.gain_db.unwrap();
    assert!((gain - 11.86).abs() < 0.2, "≈ +11,9 dB: {gain}");
    let peak = peak_of(&dest);
    assert!((peak - 0.98).abs() < 0.01, "{peak}");
}

#[test]
fn progress_ends_at_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut last = 0.0;
    convert_to_pedal_wav(
        &fixture("tono-estereo-48k.ogg"),
        &dir.path().join("o.wav"),
        &ConvertOptions::default(),
        |p| {
            assert!(p >= last, "el progreso no retrocede");
            last = p;
        },
    )
    .unwrap();
    assert_eq!(last, 1.0);
}

#[test]
fn audio_longer_than_ten_minutes_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("largo.wav");
    write_pcm_wav(&src, 8_000, 1, 16, 8_000 * 601, |_, _| 0.1);
    let err = convert_to_pedal_wav(
        &src,
        &dir.path().join("o.wav"),
        &ConvertOptions::default(),
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(err, AudioError::TooLong { .. }), "{err}");
}

#[test]
fn garbage_and_missing_files_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("x.mp3");
    fs::write(&junk, b"esto no es audio en absoluto").unwrap();
    let out = dir.path().join("o.wav");
    assert!(matches!(
        convert_to_pedal_wav(&junk, &out, &ConvertOptions::default(), |_| {}),
        Err(AudioError::Decode(_))
    ));
    assert!(matches!(
        convert_to_pedal_wav(
            &dir.path().join("no.wav"),
            &out,
            &ConvertOptions::default(),
            |_| {}
        ),
        Err(AudioError::Io(_))
    ));
}

#[test]
fn waveform_follows_the_signal() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("mitad.wav");
    // Primera mitad en silencio y segunda a media escala.
    write_pcm_wav(&src, 44_100, 2, 16, 44_100, |f, _| {
        if f < 22_050 { 0.0 } else { 0.5 }
    });
    let peaks = waveform(&src, 4).unwrap();
    assert_eq!(peaks.len(), 4);
    assert_eq!(&peaks[..2], &[0.0, 0.0]);
    assert!(
        peaks[2..].iter().all(|p| (p - 0.5).abs() < 0.01),
        "{peaks:?}"
    );
}

#[test]
fn waveform_rejects_non_16_bit_files() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("24.wav");
    write_pcm_wav(&src, 44_100, 2, 24, 100, |_, _| 0.0);
    assert!(matches!(
        waveform(&src, 10),
        Err(AudioError::Unsupported(_))
    ));
}
