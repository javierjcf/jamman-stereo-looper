//! Pedal simulado para los tests (feature `testing`): habla el protocolo validado con el JamMan
//! real sobre un sistema de ficheros en memoria. Implementa [`Read`] y
//! [`Write`], así que se usa como transporte de un [`super::Client`].

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{self, Read, Write};

use super::header::{HEADER_LEN, Header};
use super::request::{MAX_CHUNK, message_type as req};
use super::response::message_type as resp;
use super::wire::{Reader, Writer};

/// Fichero abierto.
struct Open {
    path: String,
    pos: usize,
}

/// Pedal simulado.
pub struct FakePedal {
    /// Ficheros: ruta → contenido.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Carpetas existentes.
    pub dirs: BTreeSet<String>,
    input: Vec<u8>,
    output: VecDeque<u8>,
    open: HashMap<i64, Open>,
    next_handle: i64,
}

fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => "",
    }
}

impl FakePedal {
    /// Pedal con las carpetas de la memoria interna y de la tarjeta.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let dirs = [
            "/",
            "/mnt",
            "/mnt/nandfs",
            "/mnt/nandfs/JamManStereo",
            "/mnt/sdfs",
            "/mnt/sdfs/JamManStereo",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        Self {
            files: BTreeMap::new(),
            dirs,
            input: Vec::new(),
            output: VecDeque::new(),
            open: HashMap::new(),
            next_handle: 3,
        }
    }

    /// Añade un fichero creando sus carpetas.
    pub fn add_file(&mut self, path: &str, data: &[u8]) {
        let mut dir = parent(path);
        while !dir.is_empty() && self.dirs.insert(dir.to_owned()) {
            dir = parent(dir);
        }
        self.files.insert(path.to_owned(), data.to_vec());
    }

    fn reply(&mut self, message_type: u16, serial: u32, payload: &[u8]) {
        let header = Header {
            message_type,
            payload_len: payload.len() as u32,
            serial,
        };
        self.output.extend(header.to_bytes());
        self.output.extend(payload);
    }

    fn failure(&mut self, serial: u32, message: &str) {
        let mut w = Writer::new();
        w.string(message);
        self.reply(resp::FAILURE, serial, &w.into_vec());
    }

    /// Atiende todas las peticiones completas recibidas.
    fn process(&mut self) {
        while self.input.len() >= HEADER_LEN {
            let header = Header::parse(&self.input).expect("cabecera");
            let end = HEADER_LEN + header.payload_len as usize;
            if self.input.len() < end {
                return;
            }
            let payload = self.input[HEADER_LEN..end].to_vec();
            self.input.drain(..end);
            self.handle(header.message_type, header.serial, &payload);
        }
    }

    fn handle(&mut self, t: u16, serial: u32, payload: &[u8]) {
        let mut r = Reader::new(payload);
        match t {
            req::WHO_AM_I => {
                let mut w = Writer::new();
                w.u16(93).u16(3).u16(1).u16(4864);
                w.string("fake-guid").string("JamMan Stereo");
                self.reply(resp::WHO_AM_I, serial, &w.into_vec());
            }
            req::ENTRY_INFO => {
                let kind = r.u8().unwrap();
                let path = r.string().unwrap();
                let is_file = self.files.contains_key(&path);
                let is_dir = self.dirs.contains(&path);
                if (kind == 0 && is_dir) || (kind == 1 && is_file) {
                    return self.failure(serial, "Entry type does not match request type ");
                }
                let exists = if kind == 0 { is_file } else { is_dir };
                let size = self.files.get(&path).map_or(0, Vec::len) as i64;
                let mut w = Writer::new();
                w.u8(kind)
                    .string(&path)
                    .u8(u8::from(exists))
                    .i64(size)
                    .u8(0)
                    .u8(0);
                for _ in 0..3 {
                    w.bytes(&[0xb2, 0x07, 0, 1, 0, 0]);
                }
                self.reply(resp::ENTRY_INFO, serial, &w.into_vec());
            }
            req::ENTRY_CREATE => {
                let _kind = r.u8().unwrap();
                let path = r.string().unwrap();
                if !self.dirs.contains(parent(&path)) {
                    return self.failure(serial, "The parent folder does not exist.");
                }
                self.dirs.insert(path);
                self.reply(resp::SUCCESS, serial, &[]);
            }
            req::ENTRY_DELETE => {
                let kind = r.u8().unwrap();
                let path = r.string().unwrap();
                let removed = if kind == 0 {
                    self.files.remove(&path).is_some()
                } else if self.dirs.remove(&path) {
                    let prefix = format!("{path}/");
                    self.files.retain(|p, _| !p.starts_with(&prefix));
                    self.dirs.retain(|d| !d.starts_with(&prefix));
                    true
                } else {
                    false
                };
                if removed {
                    self.reply(resp::SUCCESS, serial, &[]);
                } else {
                    self.failure(serial, "The entry does not exist.");
                }
            }
            req::FILE_OPEN => {
                let path = r.string().unwrap();
                let mode = r.u8().unwrap();
                if mode == 2 {
                    if !self.dirs.contains(parent(&path)) {
                        return self.failure(serial, "The folder does not exist.");
                    }
                    self.files.insert(path.clone(), Vec::new());
                } else if !self.files.contains_key(&path) {
                    return self.failure(serial, "The file does not exist.");
                }
                let handle = self.next_handle;
                self.next_handle += 1;
                let size = self.files[&path].len() as i64;
                self.open.insert(handle, Open { path, pos: 0 });
                let mut w = Writer::new();
                w.i64(handle).i64(size);
                self.reply(resp::FILE_OPEN, serial, &w.into_vec());
            }
            req::FILE_READ => {
                let handle = r.i64().unwrap();
                let len = (r.u32().unwrap() as usize).min(MAX_CHUNK);
                let Some(f) = self.open.get_mut(&handle) else {
                    return self.failure(serial, "Invalid handle.");
                };
                let data = &self.files[&f.path];
                let end = (f.pos + len).min(data.len());
                let chunk = data[f.pos..end].to_vec();
                f.pos = end;
                let mut w = Writer::new();
                w.i64(handle).u32(chunk.len() as u32).bytes(&chunk);
                self.reply(resp::FILE_READ, serial, &w.into_vec());
            }
            req::FILE_WRITE => {
                let handle = r.i64().unwrap();
                let len = r.u32().unwrap() as usize;
                let data = r.take(len).unwrap().to_vec();
                let Some(path) = self.open.get(&handle).map(|f| f.path.clone()) else {
                    return self.failure(serial, "Invalid handle.");
                };
                self.files.get_mut(&path).unwrap().extend_from_slice(&data);
                self.reply(resp::SUCCESS, serial, &[]);
            }
            req::FILE_CLOSE => {
                let handle = r.i64().unwrap();
                self.open.remove(&handle);
                self.reply(resp::SUCCESS, serial, &[]);
            }
            req::FOLDER_CONTENTS => {
                let path = r.string().unwrap();
                if !self.dirs.contains(&path) {
                    return self.failure(serial, "The folder does not exist.");
                }
                let child =
                    |p: &String| (parent(p) == path).then(|| p[path.len() + 1..].to_owned());
                let files: Vec<String> = self.files.keys().filter_map(child).collect();
                let dirs: Vec<String> = self
                    .dirs
                    .iter()
                    .filter(|d| *d != "/")
                    .filter_map(child)
                    .collect();
                let mut w = Writer::new();
                w.u32(files.len() as u32);
                files.iter().for_each(|f| {
                    w.string(f);
                });
                w.u32(dirs.len() as u32);
                dirs.iter().for_each(|d| {
                    w.string(d);
                });
                self.reply(resp::FOLDER_CONTENTS, serial, &w.into_vec());
            }
            other => self.failure(serial, &format!("Unsupported request {other}")),
        }
    }
}

impl Read for FakePedal {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.output.len());
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "sin respuesta"));
        }
        for (b, v) in buf.iter_mut().zip(self.output.drain(..n)) {
            *b = v;
        }
        Ok(n)
    }
}

impl Write for FakePedal {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.input.extend_from_slice(buf);
        self.process();
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
