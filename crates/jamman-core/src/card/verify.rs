//! Comprobar que lo escrito está de verdad en la tarjeta.
//!
//! Algunas tarjetas averiadas aceptan las escrituras sin error pero no las
//! guardan: el sistema da la copia por buena y, mientras los datos sigan en
//! su caché, también la relectura. Para saberlo hay que leer **del
//! dispositivo**, sin la caché:
//!
//! - Linux: `O_DIRECT`.
//! - Windows: `FILE_FLAG_NO_BUFFERING`.
//! - Resto (macOS): sin equivalente seguro sin `unsafe`; se sincroniza el
//!   fichero y se relee, lo que puede no detectar la avería.
//!
//! Las lecturas sin caché exigen búferes y tamaños alineados al sector; se
//! usan bloques alineados a [`ALIGN`] bytes, que cubre sectores de 512 y 4096.
//! Si el sistema de ficheros no admite la lectura directa (p. ej. `tmpfs`),
//! también se recurre a sincronizar y releer.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::{AUDIO_FILE, Card, PATCH_FILE, PHRASE_DIR, PHRASE_FILE};
use crate::device::Slot;
use crate::{Error, Result};

/// Alineación de las lecturas directas.
pub const ALIGN: usize = 4096;

/// Tamaño de cada lectura (múltiplo de [`ALIGN`]).
const CHUNK: usize = 1 << 20;

/// Lector de un fichero que evita la caché del sistema cuando puede.
pub struct UncachedReader {
    file: File,
    direct: bool,
    /// Búfer con margen para tomar un tramo alineado de `CHUNK` bytes.
    raw: Vec<u8>,
    /// Inicio del tramo alineado dentro de `raw`.
    start: usize,
    /// Bytes válidos del tramo y posición de lectura en él.
    filled: usize,
    pos: usize,
}

impl UncachedReader {
    /// Abre `path` para leerlo del dispositivo. Si no se puede sin caché,
    /// sincroniza el fichero y lo abre normalmente (ver [`Self::is_direct`]).
    pub fn open(path: &Path) -> io::Result<Self> {
        let (file, direct) = match open_direct(path) {
            Ok(f) => (f, true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(e),
            Err(_) => {
                let file = File::open(path)?;
                // Al menos que lo pendiente llegue al dispositivo antes de releer.
                let _ = file.sync_all();
                (file, false)
            }
        };
        let raw = if direct {
            vec![0u8; CHUNK + ALIGN]
        } else {
            Vec::new()
        };
        let start = raw.as_ptr().align_offset(ALIGN);
        Ok(Self {
            file,
            direct,
            raw,
            start,
            filled: 0,
            pos: 0,
        })
    }

    /// `true` si se lee del dispositivo sin pasar por la caché.
    pub fn is_direct(&self) -> bool {
        self.direct
    }
}

impl Read for UncachedReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !self.direct {
            return self.file.read(out);
        }
        if self.pos == self.filled {
            // Cada lectura llena un tramo alineado entero; solo la última
            // (fin del fichero) se queda corta.
            let block = &mut self.raw[self.start..self.start + CHUNK];
            self.filled = self.file.read(block)?;
            self.pos = 0;
        }
        let n = out.len().min(self.filled - self.pos);
        let from = self.start + self.pos;
        out[..n].copy_from_slice(&self.raw[from..from + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(target_os = "linux")]
fn open_direct(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
}

#[cfg(windows)]
fn open_direct(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_NO_BUFFERING)
        .open(path)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn open_direct(_path: &Path) -> io::Result<File> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

/// `true` si los dos lectores dan exactamente los mismos bytes.
pub fn same_contents(a: &mut dyn Read, b: &mut dyn Read) -> io::Result<bool> {
    let mut buf_a = vec![0u8; 64 * 1024];
    let mut buf_b = vec![0u8; 64 * 1024];
    loop {
        let n = read_full(a, &mut buf_a)?;
        let m = read_full(b, &mut buf_b)?;
        if n != m || buf_a[..n] != buf_b[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

/// Lee hasta llenar `buf` o llegar al final.
fn read_full(r: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Comprueba que `written`, leído del dispositivo, contiene `expected`.
pub fn check_file(written: &Path, expected: &mut dyn Read) -> Result<()> {
    let mut on_disk = UncachedReader::open(written)?;
    if same_contents(&mut on_disk, expected)? {
        Ok(())
    } else {
        Err(Error::WriteNotPersisted(written.to_path_buf()))
    }
}

impl Card {
    /// Comprueba que un bucle recién escrito está de verdad en la tarjeta:
    /// el audio, contra `audio_source` (el WAV que se copió), y los XML,
    /// contra lo que el sistema tiene en caché (lo que se acaba de escribir).
    pub fn verify_loop(&self, slot: Slot, audio_source: &Path) -> Result<()> {
        let dir = self.patch_dir(slot);
        let phrase_dir = dir.join(PHRASE_DIR);
        check_file(&phrase_dir.join(AUDIO_FILE), &mut File::open(audio_source)?)?;
        for xml in [dir.join(PATCH_FILE), phrase_dir.join(PHRASE_FILE)] {
            check_file(&xml, &mut File::open(&xml)?)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::testutil::pedal_wav;

    fn contents(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i * 7 % 251) as u8).collect()
    }

    #[test]
    fn reads_every_byte_whatever_the_size() {
        let dir = tempfile::tempdir().unwrap();
        for size in [0, 1, ALIGN - 1, ALIGN, ALIGN + 1, CHUNK + 3] {
            let path = dir.path().join(format!("f{size}"));
            std::fs::write(&path, contents(size)).unwrap();
            let mut read = Vec::new();
            UncachedReader::open(&path)
                .unwrap()
                .read_to_end(&mut read)
                .unwrap();
            assert_eq!(read, contents(size), "tamaño {size}");
        }
    }

    #[test]
    fn same_contents_detects_any_difference() {
        let a = contents(100_000);
        let mut b = a.clone();
        assert!(same_contents(&mut a.as_slice(), &mut b.as_slice()).unwrap());
        b[99_999] ^= 1;
        assert!(!same_contents(&mut a.as_slice(), &mut b.as_slice()).unwrap());
        assert!(!same_contents(&mut a.as_slice(), &mut &a[..10]).unwrap());
        assert!(same_contents(&mut &b""[..], &mut &b""[..]).unwrap());
    }

    #[test]
    fn check_file_reports_a_file_without_what_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("phrase.wav");
        std::fs::write(&path, b"lo que hay en la tarjeta").unwrap();
        check_file(&path, &mut &b"lo que hay en la tarjeta"[..]).unwrap();
        let err = check_file(&path, &mut &b"lo que se escribio"[..]).unwrap_err();
        assert!(matches!(err, Error::WriteNotPersisted(ref p) if *p == path));
        assert!(err.to_string().contains("averiada"));
    }

    #[test]
    fn a_fresh_import_verifies_and_a_lost_audio_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let card = Card::create(dir.path()).unwrap();
        let source = dir.path().join("fuente.wav");
        std::fs::write(&source, pedal_wav(10_000)).unwrap();
        let slot = Slot::new(3).unwrap();
        card.import_wav(slot, &source, "Tres").unwrap();
        card.verify_loop(slot, &source).unwrap();

        // Como si la tarjeta hubiera guardado otra cosa en lugar del audio.
        std::fs::write(card.audio_path(slot), pedal_wav(9_000)).unwrap();
        assert!(matches!(
            card.verify_loop(slot, &source),
            Err(Error::WriteNotPersisted(p)) if p == card.audio_path(slot)
        ));
    }
}
