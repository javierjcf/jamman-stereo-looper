//! Formato de audio que acepta el pedal.

/// Frecuencia de muestreo en Hz (manual, especificaciones técnicas).
pub const SAMPLE_RATE_HZ: u32 = 44_100;
/// Bits por muestra.
pub const BITS_PER_SAMPLE: u16 = 16;
/// Canales (estéreo).
pub const CHANNELS: u16 = 2;
/// Duración máxima de un bucle, en segundos (10 minutos).
pub const MAX_LOOP_SECONDS: u32 = 10 * 60;

/// Bytes por segundo de un WAV PCM en el formato del pedal.
pub const fn bytes_per_second() -> u32 {
    SAMPLE_RATE_HZ * (BITS_PER_SAMPLE as u32 / 8) * CHANNELS as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_ten_megabytes_per_minute() {
        // El manual indica ~10,6 MB/minuto en la tarjeta SD.
        let mb_per_min = bytes_per_second() as f64 * 60.0 / 1_000_000.0;
        assert!((mb_per_min - 10.6).abs() < 0.1);
    }
}
