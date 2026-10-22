//! Mide el tiempo de cálculo de la forma de onda: `waveform_time <wav> [tramos]`.
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("uso: waveform_time <wav> [tramos]");
    let buckets = std::env::args()
        .nth(2)
        .and_then(|b| b.parse().ok())
        .unwrap_or(1600);
    let t = std::time::Instant::now();
    let peaks = jamman_audio::waveform(path.as_ref(), buckets).unwrap();
    println!(
        "{} tramos en {:?}, pico máx {:.2}",
        peaks.len(),
        t.elapsed(),
        peaks.iter().copied().fold(0.0, f32::max)
    );
}
