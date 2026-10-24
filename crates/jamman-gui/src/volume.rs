//! Comprobaciones de la unidad donde está (o estará) la tarjeta: sistema de
//! ficheros, capacidad y si está vacía. **Nunca se formatea nada**: si la
//! tarjeta no sirve, se explica cómo formatearla en el propio pedal.

use std::path::{Path, PathBuf};

/// Capacidad máxima de una tarjeta SDHC, la mayor que admite el pedal.
pub const MAX_CARD_BYTES: u64 = 32 * 1024 * 1024 * 1024;

/// Cómo formatear la tarjeta en el pedal, según el manual.
pub const FORMAT_STEPS: &str = "Formatéala en el propio pedal (borra todo lo que tenga):\n\
     1. Mantén Shift y pulsa Delete durante 4 s: aparece «FO».\n\
     2. Pulsa Delete: aparece «F?».\n\
     3. Mantén Delete 2 s: aparece «Bu» mientras formatea.\n\
     Si el pedal muestra «nF» (tarjeta sin formato), sigue los mismos pasos con Store.";

/// Entradas que crean los sistemas en una unidad y no cuentan como contenido.
const SYSTEM_ENTRIES: &[&str] = &[
    "System Volume Information",
    "$RECYCLE.BIN",
    "lost+found",
    ".Spotlight-V100",
    ".fseventsd",
    ".Trashes",
    ".DS_Store",
];

/// Motivo por el que el pedal no podrá usar la tarjeta, si lo hay.
/// `fs_type` es el tipo de sistema de ficheros (como en `/proc/mounts`) y
/// `total` la capacidad en bytes; `None` si no se sabe (no se avisa).
pub fn format_problem(fs_type: Option<&str>, total: Option<u64>) -> Option<String> {
    let mut problems = Vec::new();
    if let Some(fs) = fs_type
        && !is_fat(fs)
    {
        problems.push(format!(
            "está formateada como {} y el pedal solo lee FAT32",
            fs_label(fs)
        ));
    }
    if let Some(total) = total
        && total > MAX_CARD_BYTES
    {
        problems.push(format!(
            "tiene {:.0} GB y el pedal admite tarjetas SD/SDHC de hasta 32 GB",
            total as f64 / 1e9
        ));
    }
    if problems.is_empty() {
        None
    } else {
        Some(format!("La tarjeta {}.", problems.join(" y ")))
    }
}

fn is_fat(fs: &str) -> bool {
    matches!(fs, "vfat" | "msdos" | "fat" | "fat32" | "FAT" | "FAT32")
}

fn fs_label(fs: &str) -> String {
    match fs {
        "exfat" => "exFAT".to_owned(),
        "ntfs" | "ntfs3" => "NTFS".to_owned(),
        // NTFS o exFAT montados con FUSE: no se sabe cuál.
        "fuseblk" => "un sistema que no es FAT (NTFS o exFAT)".to_owned(),
        other => other.to_owned(),
    }
}

/// Problema de formato de la unidad montada en `mount`, si lo hay. Una
/// carpeta que no es la raíz de una unidad (p. ej. una copia de seguridad en
/// el disco del equipo) no se comprueba.
pub fn check(mount: &Path) -> Option<String> {
    if !is_volume_root(mount) {
        return None;
    }
    let fs = fs_type(mount);
    format_problem(fs.as_deref(), fs4::total_space(mount).ok())
}

/// `true` si `path` es la raíz de una unidad montada.
fn is_volume_root(path: &Path) -> bool {
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    if cfg!(target_os = "linux") {
        mount_points().contains(&path)
    } else if cfg!(target_os = "macos") {
        path.parent() == Some(Path::new("/Volumes"))
    } else {
        path.parent().is_none()
    }
}

/// Tipo de sistema de ficheros de `mount`. Solo en Linux, leyendo
/// `/proc/mounts`. En Windows haría falta `GetVolumeInformationW` y en macOS
/// `statfs`, que necesitan `unsafe` (prohibido en el proyecto): allí solo se
/// avisa por la capacidad.
fn fs_type(mount: &Path) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let mount = mount.canonicalize().ok()?;
    fs_type_in(&mounts, &mount)
}

/// Puntos de montaje de `/proc/mounts`.
pub fn mount_points() -> Vec<PathBuf> {
    std::fs::read_to_string("/proc/mounts")
        .map(|m| parse_mounts(&m).into_iter().map(|(p, _)| p).collect())
        .unwrap_or_default()
}

/// Tipo del sistema de ficheros que contiene `path`: el del punto de montaje
/// más largo que lo incluye.
fn fs_type_in(mounts: &str, path: &Path) -> Option<String> {
    parse_mounts(mounts)
        .into_iter()
        .filter(|(mount, _)| path.starts_with(mount))
        .max_by_key(|(mount, _)| mount.components().count())
        .map(|(_, fs)| fs)
}

/// `(punto de montaje, tipo)` de cada línea de `/proc/mounts`.
fn parse_mounts(mounts: &str) -> Vec<(PathBuf, String)> {
    mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _device = fields.next()?;
            let mount = fields.next()?;
            let fs = fields.next()?;
            Some((PathBuf::from(unescape(mount)), fs.to_owned()))
        })
        .collect()
}

/// `/proc/mounts` escribe espacios y otros caracteres como `\040` (octal).
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes
            .get(i + 1..i + 4)
            .filter(|d| bytes[i] == b'\\' && d.iter().all(|c| (b'0'..=b'7').contains(c)));
        if let Some(d) = octal {
            out.push(d.iter().fold(0u8, |acc, c| acc * 8 + (c - b'0')));
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `true` si la unidad no tiene nada (salvo lo que crea el sistema).
pub fn is_empty(mount: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(mount) else {
        return false;
    };
    entries.filter_map(|e| e.ok()).all(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        SYSTEM_ENTRIES.contains(&name.as_str()) || name.starts_with(".Trash-")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    #[test]
    fn fat32_up_to_32_gb_is_fine() {
        assert_eq!(format_problem(Some("vfat"), Some(31 * GB)), None);
        assert_eq!(format_problem(Some("msdos"), Some(2 * GB)), None);
        // Sin datos no se avisa.
        assert_eq!(format_problem(None, None), None);
    }

    #[test]
    fn other_filesystems_are_reported() {
        let exfat = format_problem(Some("exfat"), Some(31 * GB)).unwrap();
        assert!(
            exfat.contains("exFAT") && exfat.contains("FAT32"),
            "{exfat}"
        );
        let ntfs = format_problem(Some("ntfs3"), None).unwrap();
        assert!(ntfs.contains("NTFS"), "{ntfs}");
        assert!(format_problem(Some("fuseblk"), None).is_some());
    }

    #[test]
    fn cards_over_32_gb_are_reported() {
        let big = format_problem(Some("vfat"), Some(64 * GB)).unwrap();
        assert!(big.contains("64 GB") && big.contains("32 GB"), "{big}");
        let both = format_problem(Some("exfat"), Some(128 * GB)).unwrap();
        assert!(both.contains("exFAT") && both.contains("128 GB"), "{both}");
    }

    const MOUNTS: &str = "\
/dev/nvme0n1p2 / btrfs rw,noatime 0 0
/dev/nvme0n1p1 /boot vfat rw,relatime 0 0
/dev/sdb1 /run/media/javier/JAMMAN vfat rw,nosuid,nodev 0 0
/dev/sdc1 /run/media/javier/Tarjeta\\040nueva exfat rw,nosuid,nodev 0 0
";

    #[test]
    fn the_longest_mount_point_wins() {
        let fs = |p: &str| fs_type_in(MOUNTS, Path::new(p));
        assert_eq!(fs("/run/media/javier/JAMMAN").as_deref(), Some("vfat"));
        assert_eq!(
            fs("/run/media/javier/JAMMAN/JamManStereo").as_deref(),
            Some("vfat")
        );
        assert_eq!(fs("/home/javier").as_deref(), Some("btrfs"));
        assert_eq!(
            fs("/run/media/javier/Tarjeta nueva").as_deref(),
            Some("exfat")
        );
        // Un nombre que solo empieza igual no es el mismo punto de montaje.
        assert_eq!(fs("/run/media/javier/JAMMAN2").as_deref(), Some("btrfs"));
    }

    #[test]
    fn mount_points_with_spaces_are_unescaped() {
        let points: Vec<PathBuf> = parse_mounts(MOUNTS).into_iter().map(|(p, _)| p).collect();
        assert!(points.contains(&PathBuf::from("/run/media/javier/Tarjeta nueva")));
        assert_eq!(unescape(r"a\040b\134c"), r"a b\c");
        assert_eq!(unescape(r"sin\escape"), r"sin\escape");
    }

    #[test]
    fn only_system_entries_leave_a_drive_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_empty(dir.path()));
        std::fs::create_dir(dir.path().join("System Volume Information")).unwrap();
        std::fs::create_dir(dir.path().join(".Trash-1000")).unwrap();
        assert!(is_empty(dir.path()));
        std::fs::write(dir.path().join("foto.jpg"), b"").unwrap();
        assert!(!is_empty(dir.path()));
        assert!(!is_empty(&dir.path().join("no-existe")));
    }
}
