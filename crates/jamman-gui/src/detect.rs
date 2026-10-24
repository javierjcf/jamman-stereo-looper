//! Detección de tarjetas del JamMan montadas en el sistema y de unidades
//! vacías que se pueden preparar para el pedal.

use std::path::{Path, PathBuf};

use jamman_core::card::CARD_DIR;

use crate::volume;

/// Carpetas donde los sistemas montan las unidades extraíbles.
fn mount_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "windows") {
        // Las unidades son las propias raíces: se comprueban directamente.
        return roots;
    }
    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Volumes"));
        return roots;
    }
    if let Ok(user) = std::env::var("USER") {
        roots.push(Path::new("/run/media").join(&user));
        roots.push(Path::new("/media").join(&user));
    }
    roots.push(PathBuf::from("/media"));
    roots.push(PathBuf::from("/mnt"));
    roots
}

/// Puntos de montaje candidatos: los hijos de cada raíz (y, en Windows, las
/// unidades `A:\` a `Z:\`).
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(target_os = "windows") {
        out.extend((b'A'..=b'Z').map(|l| PathBuf::from(format!("{}:\\", l as char))));
    }
    out.extend(children(&mount_roots()));
    out
}

fn children(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .filter_map(|r| std::fs::read_dir(r).ok())
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect()
}

/// De una lista de puntos de montaje, los que contienen una tarjeta.
pub fn cards_among(candidates: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = candidates
        .iter()
        .filter(|p| p.join(CARD_DIR).is_dir())
        .cloned()
        .collect();
    found.sort();
    found.dedup();
    found
}

/// De una lista de puntos de montaje, los vacíos (ver [`volume::is_empty`]),
/// que se pueden preparar como tarjeta. `is_mount` descarta las carpetas de
/// las raíces que no son una unidad montada (en `/mnt` puede haber de todo).
pub fn empty_among(candidates: &[PathBuf], is_mount: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = candidates
        .iter()
        .filter(|p| !p.join(CARD_DIR).exists() && is_mount(p) && volume::is_empty(p))
        .cloned()
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Lo detectado: tarjetas del JamMan y unidades vacías.
#[derive(Debug, Default)]
pub struct Detected {
    /// Unidades con la carpeta `JamManStereo`.
    pub cards: Vec<PathBuf>,
    /// Unidades vacías que se pueden preparar.
    pub empty: Vec<PathBuf>,
}

/// Busca tarjetas del JamMan y unidades vacías montadas ahora mismo.
pub fn find() -> Detected {
    let candidates = candidates();
    // En Linux solo cuentan los puntos de montaje reales; en Windows y macOS
    // los candidatos ya son unidades.
    let mounts = volume::mount_points();
    let is_mount = |p: &Path| !cfg!(target_os = "linux") || mounts.iter().any(|m| m == p);
    Detected {
        cards: cards_among(&candidates),
        empty: empty_among(&candidates, is_mount),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_mounts_with_the_jamman_folder_are_cards() {
        let root = tempfile::tempdir().unwrap();
        let card = root.path().join("SD");
        let other = root.path().join("USB");
        std::fs::create_dir_all(card.join(CARD_DIR)).unwrap();
        std::fs::create_dir_all(&other).unwrap();

        let mounts = children(&[root.path().to_path_buf()]);
        assert_eq!(cards_among(&mounts), [card]);
    }

    #[test]
    fn empty_mounts_can_be_prepared_but_not_cards_or_folders() {
        let root = tempfile::tempdir().unwrap();
        let card = root.path().join("SD");
        let empty = root.path().join("NUEVA");
        let full = root.path().join("USB");
        let not_mounted = root.path().join("carpeta");
        std::fs::create_dir_all(card.join(CARD_DIR)).unwrap();
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&full).unwrap();
        std::fs::write(full.join("doc.txt"), b"").unwrap();
        std::fs::create_dir_all(&not_mounted).unwrap();

        let mounts = children(&[root.path().to_path_buf()]);
        let found = empty_among(&mounts, |p| p != not_mounted);
        assert_eq!(found, [empty]);
    }

    #[test]
    fn missing_roots_are_ignored() {
        assert!(children(&[PathBuf::from("/no/existe/seguro")]).is_empty());
    }
}
