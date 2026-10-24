//! Lectura sin caché sobre un disco de verdad: los tests unitarios usan
//! carpetas temporales que en muchos Linux están en `tmpfs`, que no admite
//! `O_DIRECT`. `CARGO_TARGET_TMPDIR` está dentro de `target/`.

use std::io::Read;

use jamman_core::card::verify::{ALIGN, UncachedReader, check_file};

#[test]
fn reads_from_the_device_on_a_real_disk() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    for size in [0, 10, ALIGN, 3 * ALIGN + 17, (1 << 20) + ALIGN + 1] {
        let path = dir.path().join(format!("f{size}"));
        let data: Vec<u8> = (0..size).map(|i| (i % 253) as u8).collect();
        std::fs::write(&path, &data).unwrap();

        let mut reader = UncachedReader::open(&path).unwrap();
        if cfg!(target_os = "linux") {
            assert!(reader.is_direct(), "O_DIRECT en {}", path.display());
        }
        let mut read = Vec::new();
        reader.read_to_end(&mut read).unwrap();
        assert_eq!(read, data, "tamaño {size}");
        check_file(&path, &mut data.as_slice()).unwrap();
    }
}
