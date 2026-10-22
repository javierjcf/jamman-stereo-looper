//! Utilidades compartidas por los tests del módulo.

/// Construye un WAV con los parámetros dados y un bloque `LIST` opcional.
pub(crate) fn wav_bytes(channels: u16, rate: u32, bits: u16, data: &[u8], list: bool) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"WAVE");
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&16u32.to_le_bytes());
    body.extend_from_slice(&1u16.to_le_bytes());
    body.extend_from_slice(&channels.to_le_bytes());
    body.extend_from_slice(&rate.to_le_bytes());
    body.extend_from_slice(&(rate * u32::from(channels) * u32::from(bits / 8)).to_le_bytes());
    body.extend_from_slice(&(channels * (bits / 8)).to_le_bytes());
    body.extend_from_slice(&bits.to_le_bytes());
    if list {
        body.extend_from_slice(b"LIST");
        body.extend_from_slice(&5u32.to_le_bytes());
        body.extend_from_slice(b"INFO!\0"); // 5 bytes + relleno a par
    }
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(data);
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// WAV válido para el pedal con `n` bytes de audio.
pub(crate) fn pedal_wav(n: usize) -> Vec<u8> {
    wav_bytes(2, 44_100, 16, &vec![7u8; n], false)
}
