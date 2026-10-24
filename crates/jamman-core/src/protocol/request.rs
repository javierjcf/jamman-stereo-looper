//! Peticiones que el PC envía al pedal.
//!
//! Cada petición se codifica como [`Header`] seguido de su carga. Los IDs y el
//! formato de cada carga salen de las funciones `*Request::vftable` del
//! ejecutable original (ver `docs/03-protocolo.md`).

use super::header::{HEADER_LEN, Header};
use super::wire::Writer;
use crate::{Error, Result};

/// Longitud máxima de una ruta, en bytes (`_path.size() <= 0xFF` en el original).
pub const MAX_PATH_LEN: usize = 255;

/// Máximo de bytes de fichero por mensaje de lectura o escritura (`0x1fe9`).
pub const MAX_CHUNK: usize = 0x1fe9;

/// Identificadores de tipo de las peticiones.
pub mod message_type {
    /// `WhoAmIRequest`.
    pub const WHO_AM_I: u16 = 0;
    /// `FileSystemEntryInfoRequest`.
    pub const ENTRY_INFO: u16 = 4;
    /// `FileSystemEntryCreateRequest`.
    pub const ENTRY_CREATE: u16 = 7;
    /// `FileSystemEntryDeleteRequest`.
    pub const ENTRY_DELETE: u16 = 9;
    /// `FileOpenRequest`.
    pub const FILE_OPEN: u16 = 10;
    /// `FileReadRequest`.
    pub const FILE_READ: u16 = 12;
    /// `FileWriteRequest`.
    pub const FILE_WRITE: u16 = 14;
    /// `FileSeekRequest`.
    pub const FILE_SEEK: u16 = 15;
    /// `FileCloseRequest`.
    pub const FILE_CLOSE: u16 = 16;
    /// `FolderContentsRequest`.
    pub const FOLDER_CONTENTS: u16 = 17;
    /// `GetPropertyRequest`.
    pub const GET_PROPERTY: u16 = 0x1a;
}

/// Modo de apertura de un fichero remoto (dos bytes en el cable).
///
/// Validado con el pedal: `[1, 1]` abre para leer y `[2, 0]` crea el fichero
/// si no existe, lo vacía y lo abre para escribir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Abrir para leer.
    Read,
    /// Crear o vaciar y abrir para escribir.
    Write,
}

/// Tipo de entrada del sistema de ficheros del pedal (primer byte de las
/// peticiones de información, creación y borrado; validado con el pedal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Fichero.
    File,
    /// Carpeta.
    Folder,
}

impl EntryKind {
    fn byte(self) -> u8 {
        match self {
            Self::File => 0,
            Self::Folder => 1,
        }
    }
}

impl OpenMode {
    fn bytes(self) -> [u8; 2] {
        match self {
            Self::Read => [1, 1],
            Self::Write => [2, 0],
        }
    }
}

/// Petición al pedal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Pide la identificación del dispositivo. Carga vacía.
    WhoAmI,
    /// Información de un fichero o carpeta. Si el tipo no coincide con el de
    /// la entrada, el pedal responde con error.
    EntryInfo {
        /// Ruta en el sistema de ficheros del pedal.
        path: String,
        /// Tipo de entrada por el que se pregunta.
        kind: EntryKind,
    },
    /// Crea una entrada (validado para carpetas; los ficheros se crean al
    /// abrirlos para escribir). Último byte fijo `1`, como el original.
    EntryCreate {
        /// Ruta a crear.
        path: String,
        /// Tipo de entrada.
        kind: EntryKind,
    },
    /// Borra una entrada. Una carpeta se borra con todo su contenido.
    /// Bytes finales fijos `1, 0`, como el original.
    EntryDelete {
        /// Ruta a borrar.
        path: String,
        /// Tipo de entrada.
        kind: EntryKind,
    },
    /// Abre un fichero y devuelve un manejador de 64 bits.
    FileOpen {
        /// Ruta del fichero.
        path: String,
        /// Modo de apertura.
        mode: OpenMode,
    },
    /// Lee hasta `len` bytes (se recorta a [`MAX_CHUNK`]).
    FileRead {
        /// Manejador devuelto por la apertura.
        handle: i64,
        /// Bytes pedidos.
        len: u32,
    },
    /// Escribe `data` (máximo [`MAX_CHUNK`] bytes) en la posición actual.
    FileWrite {
        /// Manejador devuelto por la apertura.
        handle: i64,
        /// Datos a escribir.
        data: Vec<u8>,
    },
    /// Cambia la posición del fichero.
    FileSeek {
        /// Manejador devuelto por la apertura.
        handle: i64,
        /// Nueva posición en bytes.
        position: i64,
        /// Byte final, de significado desconocido (el original envía `0`).
        flag: u8,
    },
    /// Cierra un fichero.
    FileClose {
        /// Manejador devuelto por la apertura.
        handle: i64,
    },
    /// Lista el contenido de una carpeta.
    FolderContents {
        /// Ruta de la carpeta.
        path: String,
    },
    /// Lee una propiedad del pedal (validado con el pedal). La carga es una
    /// lista de `u16` que el original envía vacía (un `u8` con la cuenta) y
    /// el identificador. Ver [`property`](super::property).
    GetProperty {
        /// Identificador de la propiedad.
        id: u16,
    },
}

fn check_path(path: &str) -> Result<()> {
    if path.len() > MAX_PATH_LEN {
        return Err(Error::PathTooLong(path.len()));
    }
    Ok(())
}

impl Request {
    /// Identificador de tipo de esta petición.
    pub fn message_type(&self) -> u16 {
        match self {
            Self::WhoAmI => message_type::WHO_AM_I,
            Self::EntryInfo { .. } => message_type::ENTRY_INFO,
            Self::EntryCreate { .. } => message_type::ENTRY_CREATE,
            Self::EntryDelete { .. } => message_type::ENTRY_DELETE,
            Self::FileOpen { .. } => message_type::FILE_OPEN,
            Self::FileRead { .. } => message_type::FILE_READ,
            Self::FileWrite { .. } => message_type::FILE_WRITE,
            Self::FileSeek { .. } => message_type::FILE_SEEK,
            Self::FileClose { .. } => message_type::FILE_CLOSE,
            Self::FolderContents { .. } => message_type::FOLDER_CONTENTS,
            Self::GetProperty { .. } => message_type::GET_PROPERTY,
        }
    }

    fn payload(&self) -> Result<Vec<u8>> {
        let mut w = Writer::new();
        match self {
            Self::WhoAmI => {}
            Self::EntryInfo { path, kind } => {
                check_path(path)?;
                w.u8(kind.byte()).string(path);
            }
            Self::EntryCreate { path, kind } => {
                check_path(path)?;
                w.u8(kind.byte()).string(path).u8(1);
            }
            Self::EntryDelete { path, kind } => {
                check_path(path)?;
                w.u8(kind.byte()).string(path).u8(1).u8(0);
            }
            Self::FileOpen { path, mode } => {
                check_path(path)?;
                w.string(path).bytes(&mode.bytes());
            }
            Self::FileRead { handle, len } => {
                let len = (*len).min(MAX_CHUNK as u32);
                w.i64(*handle).u32(len);
            }
            Self::FileWrite { handle, data } => {
                if data.len() > MAX_CHUNK {
                    return Err(Error::ChunkTooLarge(data.len()));
                }
                w.i64(*handle).u32(data.len() as u32).bytes(data);
            }
            Self::FileSeek {
                handle,
                position,
                flag,
            } => {
                w.i64(*handle).i64(*position).u8(*flag);
            }
            Self::FileClose { handle } => {
                w.i64(*handle);
            }
            Self::FolderContents { path } => {
                check_path(path)?;
                w.string(path);
            }
            Self::GetProperty { id } => {
                w.u8(0).u16(*id);
            }
        }
        Ok(w.into_vec())
    }

    /// Codifica la petición completa (cabecera y carga) con el número de serie dado.
    pub fn encode(&self, serial: u32) -> Result<Vec<u8>> {
        let payload = self.payload()?;
        let header = Header {
            message_type: self.message_type(),
            payload_len: payload.len() as u32,
            serial,
        };
        let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
        frame.extend_from_slice(&header.to_bytes());
        frame.extend_from_slice(&payload);
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(frame: &[u8]) -> (Header, &[u8]) {
        let h = Header::parse(frame).unwrap();
        (h, &frame[HEADER_LEN..])
    }

    #[test]
    fn get_property_is_an_empty_list_then_the_id() {
        // Validado con el pedal: así pide el original el espacio libre.
        let frame = Request::GetProperty { id: 12 }.encode(100).unwrap();
        assert_eq!(frame, [0x1a, 0, 7, 0, 0, 0, 100, 0, 0, 0, 0, 12, 0]);
    }

    #[test]
    fn who_am_i_has_empty_payload() {
        let frame = Request::WhoAmI.encode(7).unwrap();
        // La longitud (4) cuenta solo la serie: la carga está vacía.
        assert_eq!(frame, [0, 0, 4, 0, 0, 0, 7, 0, 0, 0]);
    }

    #[test]
    fn file_close_carries_the_handle() {
        let frame = Request::FileClose {
            handle: 0x0102_0304_0506_0708,
        }
        .encode(1)
        .unwrap();
        let (h, payload) = split(&frame);
        assert_eq!(h.message_type, 16);
        assert_eq!(h.payload_len, 8);
        assert_eq!(payload, [8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn entry_info_is_kind_then_string() {
        let frame = Request::EntryInfo {
            path: "/a".into(),
            kind: EntryKind::Folder,
        }
        .encode(0)
        .unwrap();
        let (h, payload) = split(&frame);
        assert_eq!(h.message_type, 4);
        assert_eq!(h.payload_len, 7); // longitud de la ruta + 5
        assert_eq!(payload, [1, 2, 0, 0, 0, b'/', b'a']);
    }

    #[test]
    fn file_open_is_string_then_mode_bytes() {
        let frame = Request::FileOpen {
            path: "x".into(),
            mode: OpenMode::Read,
        }
        .encode(0)
        .unwrap();
        let (h, payload) = split(&frame);
        assert_eq!(h.message_type, 10);
        assert_eq!(h.payload_len, 7); // longitud de la ruta + 6
        assert_eq!(payload, [1, 0, 0, 0, b'x', 1, 1]);
    }

    #[test]
    fn create_and_delete_payload_sizes_match_the_original() {
        let create = Request::EntryCreate {
            path: "x".into(),
            kind: EntryKind::Folder,
        }
        .encode(0)
        .unwrap();
        let delete = Request::EntryDelete {
            path: "x".into(),
            kind: EntryKind::Folder,
        }
        .encode(0)
        .unwrap();
        assert_eq!(split(&create).0.payload_len, 7); // ruta + 6
        assert_eq!(split(&delete).0.payload_len, 8); // ruta + 7
        assert_eq!(split(&create).0.message_type, 7);
        assert_eq!(split(&delete).0.message_type, 9);
    }

    #[test]
    fn folder_contents_is_just_the_string() {
        let frame = Request::FolderContents { path: "x".into() }
            .encode(0)
            .unwrap();
        let (h, payload) = split(&frame);
        assert_eq!(h.message_type, 17);
        assert_eq!(h.payload_len, 5); // ruta + 4
        assert_eq!(payload, [1, 0, 0, 0, b'x']);
    }

    #[test]
    fn file_seek_payload_is_17_bytes() {
        let frame = Request::FileSeek {
            handle: 1,
            position: 0x28,
            flag: 0,
        }
        .encode(0)
        .unwrap();
        let (h, _) = split(&frame);
        assert_eq!(h.message_type, 15);
        assert_eq!(h.payload_len, 17);
    }

    #[test]
    fn file_read_length_is_capped() {
        let frame = Request::FileRead {
            handle: 1,
            len: 100_000,
        }
        .encode(0)
        .unwrap();
        let (h, payload) = split(&frame);
        assert_eq!(h.message_type, 12);
        assert_eq!(h.payload_len, 12);
        assert_eq!(&payload[8..], &(MAX_CHUNK as u32).to_le_bytes());
    }

    #[test]
    fn file_write_declares_length_then_data() {
        let frame = Request::FileWrite {
            handle: 2,
            data: vec![9, 8, 7],
        }
        .encode(0)
        .unwrap();
        let (h, payload) = split(&frame);
        assert_eq!(h.message_type, 14);
        assert_eq!(h.payload_len, 15); // 8 + 4 + 3
        assert_eq!(&payload[8..], [3, 0, 0, 0, 9, 8, 7]);
    }

    #[test]
    fn rejects_oversized_inputs() {
        let long = "a".repeat(MAX_PATH_LEN + 1);
        assert!(matches!(
            Request::FolderContents { path: long }.encode(0),
            Err(Error::PathTooLong(256))
        ));
        assert!(matches!(
            Request::FileWrite {
                handle: 1,
                data: vec![0; MAX_CHUNK + 1]
            }
            .encode(0),
            Err(Error::ChunkTooLarge(_))
        ));
    }

    #[test]
    fn accepts_a_path_of_exactly_the_maximum() {
        let path = "a".repeat(MAX_PATH_LEN);
        assert!(Request::FolderContents { path }.encode(0).is_ok());
    }
}
