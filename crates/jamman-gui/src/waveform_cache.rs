//! Caché en disco de las formas de onda.
//!
//! Leer el audio de una tarjeta SD lenta cuesta segundos, así que los picos se
//! guardan en la carpeta de caché del usuario. La clave combina ruta, tamaño,
//! fecha de modificación y número de tramos: si el fichero cambia, la entrada
//! deja de coincidir y se recalcula. Borrar la caché no tiene consecuencias.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Carpeta de caché de la aplicación según el sistema.
fn cache_dir() -> Option<PathBuf> {
    let base = if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(dir)
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".cache")
    };
    Some(base.join("jamman-stereo-looper").join("waveforms"))
}

fn entry(dir: &Path, audio: &Path, buckets: usize) -> Option<PathBuf> {
    let meta = fs::metadata(audio).ok()?;
    let mut h = DefaultHasher::new();
    audio.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok()?.hash(&mut h);
    buckets.hash(&mut h);
    Some(dir.join(format!("{:016x}.bin", h.finish())))
}

fn load_from(dir: &Path, audio: &Path, buckets: usize) -> Option<Vec<f32>> {
    let bytes = fs::read(entry(dir, audio, buckets)?).ok()?;
    let peaks: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    (peaks.len() == buckets).then_some(peaks)
}

fn store_in(dir: &Path, audio: &Path, peaks: &[f32]) {
    let Some(path) = entry(dir, audio, peaks.len()) else {
        return;
    };
    let bytes: Vec<u8> = peaks.iter().flat_map(|p| p.to_le_bytes()).collect();
    // Si no se puede escribir, simplemente no hay caché.
    let _ = fs::create_dir_all(dir).and_then(|()| fs::write(path, bytes));
}

/// Picos guardados para `audio`, si los hay y siguen siendo válidos.
pub fn load(audio: &Path, buckets: usize) -> Option<Vec<f32>> {
    load_from(&cache_dir()?, audio, buckets)
}

/// Guarda los picos de `audio`.
pub fn store(audio: &Path, peaks: &[f32]) {
    if let Some(dir) = cache_dir() {
        store_in(&dir, audio, peaks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_peaks_are_loaded_back() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("a.wav");
        fs::write(&audio, b"audio").unwrap();
        let cache = dir.path().join("cache");

        assert_eq!(load_from(&cache, &audio, 3), None);
        store_in(&cache, &audio, &[0.1, 0.5, 1.0]);
        assert_eq!(load_from(&cache, &audio, 3), Some(vec![0.1, 0.5, 1.0]));
        assert_eq!(load_from(&cache, &audio, 4), None, "otro número de tramos");
    }

    #[test]
    fn a_changed_file_is_not_served_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("a.wav");
        let cache = dir.path().join("cache");
        fs::write(&audio, b"audio").unwrap();
        store_in(&cache, &audio, &[0.3]);
        fs::write(&audio, b"otro audio distinto").unwrap();
        assert_eq!(load_from(&cache, &audio, 1), None);
    }
}
