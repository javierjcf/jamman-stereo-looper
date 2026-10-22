//! Detección de tarjetas del JamMan montadas en el sistema.

use std::path::{Path, PathBuf};

use jamman_core::card::CARD_DIR;

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

/// Busca tarjetas del JamMan montadas ahora mismo.
pub fn find_cards() -> Vec<PathBuf> {
    cards_among(&candidates())
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
    fn missing_roots_are_ignored() {
        assert!(children(&[PathBuf::from("/no/existe/seguro")]).is_empty());
    }
}
