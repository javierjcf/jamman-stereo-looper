//! Respuestas del pedal.
//!
//! El orden de campos sale de las funciones de lectura del ejecutable original
//! (ver `docs/03-protocolo.md`). Los nombres de campos marcados como
//! "hipótesis" son suposiciones pendientes de validar con el pedal.

use super::header::Header;
use super::wire::Reader;
use crate::{Error, Result};

/// Identificadores de tipo de las respuestas.
pub mod message_type {
    /// `WhoAmIResponse`.
    pub const WHO_AM_I: u16 = 1;
    /// `SuccessResponse`.
    pub const SUCCESS: u16 = 2;
    /// `FailureResponse`.
    pub const FAILURE: u16 = 3;
    /// `FileSystemEntryInfoResponse`.
    pub const ENTRY_INFO: u16 = 5;
    /// `FileOpenResponse`.
    pub const FILE_OPEN: u16 = 0x0b;
    /// `FileReadResponse`.
    pub const FILE_READ: u16 = 0x0d;
    /// `FolderContentsResponse`.
    pub const FOLDER_CONTENTS: u16 = 0x12;
    /// Primer identificador de mensajes asíncronos (iniciados por el pedal).
    pub const FIRST_ASYNC: u16 = 0x8000;
}

/// Fecha y hora como las envía el pedal: año (`u16`) y cuatro bytes con signo.
///
/// Validado con un fichero creado el 22 de octubre: llega como mes 9, así que
/// **el mes empieza en 0** (como `struct tm` en C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    /// Año.
    pub year: u16,
    /// Mes, de 0 (enero) a 11.
    pub month: i8,
    /// Día del mes.
    pub day: i8,
    /// Hora.
    pub hour: i8,
    /// Minuto.
    pub minute: i8,
}

impl Timestamp {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            year: r.u16()?,
            month: r.i8()?,
            day: r.i8()?,
            hour: r.i8()?,
            minute: r.i8()?,
        })
    }
}

/// Identificación del dispositivo (`WhoAmIResponse`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoAmI {
    /// Cuatro valores de 16 bits de significado desconocido. Un JamMan Stereo
    /// real devuelve `[93, 3, 1, 4864]` (posiblemente versiones).
    pub ids: [u16; 4],
    /// Identificador único (GUID) del aparato y nombre del modelo
    /// («JamMan Stereo»). Validado con el pedal.
    pub texts: [String; 2],
}

/// Información de una entrada del sistema de ficheros del pedal.
///
/// Validados con el pedal: `kind`, `name`, `exists`, `size` y las fechas. El
/// resto de bytes tiene un significado desconocido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    /// Tipo por el que se preguntó (0 fichero, 1 carpeta).
    pub kind: u8,
    /// Ruta completa de la entrada.
    pub name: String,
    /// `true` si la entrada existe.
    pub exists: bool,
    /// Tamaño en bytes (0 si no existe o es una carpeta).
    pub size: i64,
    /// `true` si el byte original era `0` (hipótesis: no es solo lectura).
    pub zero_flag: bool,
    /// Último byte antes de las fechas, de significado desconocido.
    pub extra: u8,
    /// Tres fechas (probablemente creación, modificación y acceso; la tercera
    /// llega sin hora).
    pub times: [Timestamp; 3],
}

/// Respuesta del pedal ya interpretada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// La operación se realizó (`SuccessResponse`). Carga vacía.
    Success,
    /// La operación falló, con el texto que envía el pedal.
    Failure {
        /// Mensaje de error.
        message: String,
    },
    /// Identificación del dispositivo.
    WhoAmI(WhoAmI),
    /// Información de una entrada.
    EntryInfo(EntryInfo),
    /// Contenido de una carpeta (validado con el pedal).
    FolderContents {
        /// Ficheros.
        files: Vec<String>,
        /// Subcarpetas.
        folders: Vec<String>,
    },
    /// Fichero abierto.
    FileOpen {
        /// Manejador para las siguientes operaciones.
        handle: i64,
        /// Tamaño del fichero en bytes.
        size: i64,
    },
    /// Datos leídos: un `i64` (manejador o posición, sin confirmar), la
    /// longitud y los datos (validado con el pedal).
    FileRead {
        /// `i64` que precede a la longitud.
        position: i64,
        /// Datos leídos.
        data: Vec<u8>,
    },
    /// Tipo aún sin descifrar; se conserva la carga tal cual.
    Unparsed {
        /// Tipo de mensaje.
        message_type: u16,
        /// Carga sin interpretar.
        payload: Vec<u8>,
    },
}

impl Response {
    /// Identificador de tipo del mensaje del que procede esta respuesta.
    pub fn message_type(&self) -> u16 {
        match self {
            Self::Success => message_type::SUCCESS,
            Self::Failure { .. } => message_type::FAILURE,
            Self::WhoAmI(_) => message_type::WHO_AM_I,
            Self::EntryInfo(_) => message_type::ENTRY_INFO,
            Self::FolderContents { .. } => message_type::FOLDER_CONTENTS,
            Self::FileOpen { .. } => message_type::FILE_OPEN,
            Self::FileRead { .. } => message_type::FILE_READ,
            Self::Unparsed { message_type, .. } => *message_type,
        }
    }

    /// Interpreta la carga de un mensaje cuya cabecera ya se ha leído.
    pub fn parse(header: &Header, payload: &[u8]) -> Result<Self> {
        let mut r = Reader::new(payload);
        match header.message_type {
            message_type::SUCCESS => Ok(Self::Success),
            message_type::FAILURE => Ok(Self::Failure {
                message: r.string()?,
            }),
            message_type::WHO_AM_I => {
                let ids = [r.u16()?, r.u16()?, r.u16()?, r.u16()?];
                let texts = [r.string()?, r.string()?];
                Ok(Self::WhoAmI(WhoAmI { ids, texts }))
            }
            message_type::ENTRY_INFO => {
                let kind = r.u8()?;
                let name = r.string()?;
                let exists = r.u8()? != 0;
                let size = r.i64()?;
                let zero_flag = r.u8()? == 0;
                let extra = r.u8()?;
                let times = [
                    Timestamp::read(&mut r)?,
                    Timestamp::read(&mut r)?,
                    Timestamp::read(&mut r)?,
                ];
                Ok(Self::EntryInfo(EntryInfo {
                    kind,
                    name,
                    exists,
                    size,
                    zero_flag,
                    extra,
                    times,
                }))
            }
            message_type::FOLDER_CONTENTS => {
                let files = read_string_list(&mut r)?;
                let folders = read_string_list(&mut r)?;
                Ok(Self::FolderContents { files, folders })
            }
            message_type::FILE_OPEN => Ok(Self::FileOpen {
                handle: r.i64()?,
                size: r.i64()?,
            }),
            message_type::FILE_READ => {
                let position = r.i64()?;
                let len = r.u32()? as usize;
                let data = r.take(len)?.to_vec();
                Ok(Self::FileRead { position, data })
            }
            other if other >= message_type::FIRST_ASYNC => Err(Error::UnknownMessageType(other)),
            other => Ok(Self::Unparsed {
                message_type: other,
                payload: payload.to_vec(),
            }),
        }
    }
}

fn read_string_list(r: &mut Reader<'_>) -> Result<Vec<String>> {
    let count = r.u32()?;
    (0..count).map(|_| r.string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::wire::Writer;

    fn header(message_type: u16) -> Header {
        Header {
            message_type,
            payload_len: 0,
            serial: 0,
        }
    }

    #[test]
    fn success_has_no_payload() {
        assert_eq!(Response::parse(&header(2), &[]).unwrap(), Response::Success);
    }

    #[test]
    fn failure_carries_a_message() {
        let mut w = Writer::new();
        w.string("no existe");
        assert_eq!(
            Response::parse(&header(3), &w.into_vec()).unwrap(),
            Response::Failure {
                message: "no existe".into()
            }
        );
    }

    #[test]
    fn who_am_i_has_four_u16_and_two_strings() {
        let mut w = Writer::new();
        for v in [1u16, 2, 3, 4] {
            w.bytes(&v.to_le_bytes());
        }
        w.string("JamMan").string("1.2.3");
        let r = Response::parse(&header(1), &w.into_vec()).unwrap();
        assert_eq!(
            r,
            Response::WhoAmI(WhoAmI {
                ids: [1, 2, 3, 4],
                texts: ["JamMan".into(), "1.2.3".into()]
            })
        );
    }

    #[test]
    fn file_open_returns_handle_and_size() {
        let mut w = Writer::new();
        w.i64(42).i64(1_000_000);
        assert_eq!(
            Response::parse(&header(0x0b), &w.into_vec()).unwrap(),
            Response::FileOpen {
                handle: 42,
                size: 1_000_000
            }
        );
    }

    #[test]
    fn folder_contents_has_two_string_lists() {
        let mut w = Writer::new();
        w.u32(2)
            .string("a.wav")
            .string("b.wav")
            .u32(1)
            .string("PhraseA");
        assert_eq!(
            Response::parse(&header(0x12), &w.into_vec()).unwrap(),
            Response::FolderContents {
                files: vec!["a.wav".into(), "b.wav".into()],
                folders: vec!["PhraseA".into()],
            }
        );
    }

    #[test]
    fn entry_info_reads_three_timestamps() {
        let mut w = Writer::new();
        w.u8(1).string("phrase.wav").u8(2).i64(1234).u8(0).u8(9);
        for _ in 0..3 {
            w.bytes(&2024u16.to_le_bytes()).u8(5).u8(17).u8(13).u8(45);
        }
        let Response::EntryInfo(info) = Response::parse(&header(5), &w.into_vec()).unwrap() else {
            panic!("se esperaba EntryInfo");
        };
        assert_eq!(info.name, "phrase.wav");
        assert_eq!(info.size, 1234);
        assert!(info.zero_flag);
        assert_eq!(info.times[2].year, 2024);
        assert_eq!(info.times[0].minute, 45);
    }

    #[test]
    fn file_read_returns_declared_amount_of_data() {
        let mut w = Writer::new();
        w.i64(0).u32(3).bytes(&[7, 8, 9]);
        assert_eq!(
            Response::parse(&header(0x0d), &w.into_vec()).unwrap(),
            Response::FileRead {
                position: 0,
                data: vec![7, 8, 9]
            }
        );
    }

    #[test]
    fn file_read_with_missing_data_is_truncated() {
        let mut w = Writer::new();
        w.i64(0).u32(10).bytes(&[1, 2]);
        assert!(matches!(
            Response::parse(&header(0x0d), &w.into_vec()),
            Err(Error::Truncated { .. })
        ));
    }

    #[test]
    fn undeciphered_types_keep_their_payload() {
        assert_eq!(
            Response::parse(&header(0x14), &[1, 2, 3]).unwrap(),
            Response::Unparsed {
                message_type: 0x14,
                payload: vec![1, 2, 3]
            }
        );
    }

    #[test]
    fn message_type_matches_the_parsed_id() {
        for id in [2u16, 3, 0x0b, 0x14] {
            let mut w = Writer::new();
            w.string("x");
            let payload = if id == 3 { w.into_vec() } else { vec![0; 16] };
            if let Ok(r) = Response::parse(&header(id), &payload) {
                assert_eq!(r.message_type(), id);
            }
        }
    }

    #[test]
    fn async_ids_are_not_responses() {
        assert!(matches!(
            Response::parse(&header(0x8001), &[]),
            Err(Error::UnknownMessageType(0x8001))
        ));
    }
}
