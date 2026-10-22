//! Cliente síncrono del protocolo.
//!
//! Es genérico sobre cualquier transporte que implemente [`Read`] y [`Write`]:
//! en producción será el puerto serie del pedal y en los tests un transporte
//! en memoria. Así `jamman-core` no depende de ninguna librería de puertos.

use std::io::{Read, Write};

use super::header::{HEADER_LEN, Header};
use super::request::{OpenMode, Request};
use super::response::{EntryInfo, Response, WhoAmI, message_type};
use crate::{Error, Result};

/// Límite de seguridad para la carga declarada por una respuesta (1 MiB).
const MAX_RESPONSE_PAYLOAD: u32 = 1 << 20;

/// Cliente del pedal sobre un transporte `T`.
#[derive(Debug)]
pub struct Client<T> {
    io: T,
    next_serial: u32,
}

impl<T: Read + Write> Client<T> {
    /// Crea un cliente. Los números de serie empiezan en 0.
    pub fn new(io: T) -> Self {
        Self { io, next_serial: 0 }
    }

    /// Devuelve el transporte.
    pub fn into_inner(self) -> T {
        self.io
    }

    fn read_frame(&mut self) -> Result<(Header, Vec<u8>)> {
        let mut head = [0u8; HEADER_LEN];
        self.io.read_exact(&mut head)?;
        let header = Header::parse(&head)?;
        if header.payload_len > MAX_RESPONSE_PAYLOAD {
            return Err(Error::PayloadTooLarge(header.payload_len));
        }
        let mut payload = vec![0u8; header.payload_len as usize];
        self.io.read_exact(&mut payload)?;
        Ok((header, payload))
    }

    /// Envía una petición y espera su respuesta.
    ///
    /// Igual que el software original, descarta las respuestas cuyo número de
    /// serie no coincide y los mensajes asíncronos del pedal. Una
    /// `FailureResponse` se convierte en [`Error::Device`].
    pub fn call(&mut self, request: &Request) -> Result<Response> {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);

        self.io.write_all(&request.encode(serial)?)?;
        self.io.flush()?;

        loop {
            let (header, payload) = self.read_frame()?;
            if header.message_type >= message_type::FIRST_ASYNC || header.serial != serial {
                continue;
            }
            return match Response::parse(&header, &payload)? {
                Response::Failure { message } => Err(Error::Device(message)),
                response => Ok(response),
            };
        }
    }

    /// Identifica el dispositivo.
    pub fn who_am_i(&mut self) -> Result<WhoAmI> {
        match self.call(&Request::WhoAmI)? {
            Response::WhoAmI(info) => Ok(info),
            other => Err(unexpected("WhoAmI", &other)),
        }
    }

    /// Información de una entrada del sistema de ficheros del pedal.
    pub fn entry_info(&mut self, path: &str) -> Result<EntryInfo> {
        let request = Request::EntryInfo {
            path: path.into(),
            flag: 0,
        };
        match self.call(&request)? {
            Response::EntryInfo(info) => Ok(info),
            other => Err(unexpected("EntryInfo", &other)),
        }
    }

    /// Lista una carpeta. Devuelve las dos listas tal como las envía el pedal.
    pub fn folder_contents(&mut self, path: &str) -> Result<(Vec<String>, Vec<String>)> {
        let request = Request::FolderContents { path: path.into() };
        match self.call(&request)? {
            Response::FolderContents { first, second } => Ok((first, second)),
            other => Err(unexpected("FolderContents", &other)),
        }
    }

    /// Descarga un fichero completo: abre, lee por bloques y cierra.
    pub fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        let request = Request::FileOpen {
            path: path.into(),
            mode: OpenMode::Read,
        };
        let (handle, size) = match self.call(&request)? {
            Response::FileOpen { handle, size } => (handle, size.max(0) as usize),
            other => return Err(unexpected("FileOpen", &other)),
        };

        let data = self.read_all(handle, size);
        // Se intenta cerrar siempre; el error de lectura, si lo hay, prevalece.
        let closed = self.call(&Request::FileClose { handle });
        let data = data?;
        closed?;
        Ok(data)
    }

    fn read_all(&mut self, handle: i64, size: usize) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(size);
        while data.len() < size {
            let wanted = (size - data.len()).min(super::request::MAX_CHUNK) as u32;
            match self.call(&Request::FileRead {
                handle,
                len: wanted,
            })? {
                Response::FileRead { data: chunk, .. } if !chunk.is_empty() => {
                    data.extend_from_slice(&chunk);
                }
                Response::FileRead { .. } => {
                    return Err(Error::Truncated {
                        needed: size,
                        available: data.len(),
                    });
                }
                other => return Err(unexpected("FileRead", &other)),
            }
        }
        Ok(data)
    }
}

fn unexpected(expected: &'static str, got: &Response) -> Error {
    Error::UnexpectedResponse {
        expected,
        got: got.message_type(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::protocol::wire::Writer;

    /// Transporte en memoria: lee respuestas preparadas y guarda lo escrito.
    struct Mock {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Mock {
        fn with_replies(replies: Vec<Vec<u8>>) -> Self {
            Self {
                input: Cursor::new(replies.concat()),
                output: Vec::new(),
            }
        }
    }

    impl Read for Mock {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for Mock {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn reply(message_type: u16, serial: u32, payload: &[u8]) -> Vec<u8> {
        let header = Header {
            message_type,
            payload_len: payload.len() as u32,
            serial,
        };
        [header.to_bytes().as_slice(), payload].concat()
    }

    /// Tipos de mensaje de las peticiones que contiene un buffer de salida.
    fn sent_types(output: &[u8]) -> Vec<u16> {
        let mut types = Vec::new();
        let mut rest = output;
        while !rest.is_empty() {
            let h = Header::parse(rest).unwrap();
            types.push(h.message_type);
            rest = &rest[HEADER_LEN + h.payload_len as usize..];
        }
        types
    }

    #[test]
    fn call_sends_the_encoded_request_and_parses_the_reply() {
        let mock = Mock::with_replies(vec![reply(2, 0, &[])]);
        let mut client = Client::new(mock);
        assert_eq!(client.call(&Request::WhoAmI).unwrap(), Response::Success);
        assert_eq!(
            client.into_inner().output,
            Request::WhoAmI.encode(0).unwrap()
        );
    }

    #[test]
    fn serial_numbers_increase_per_call() {
        let mock = Mock::with_replies(vec![reply(2, 0, &[]), reply(2, 1, &[])]);
        let mut client = Client::new(mock);
        client.call(&Request::WhoAmI).unwrap();
        client.call(&Request::WhoAmI).unwrap();
        let out = client.into_inner().output;
        assert_eq!(Header::parse(&out[HEADER_LEN..]).unwrap().serial, 1);
    }

    #[test]
    fn replies_with_another_serial_and_async_messages_are_discarded() {
        let mock = Mock::with_replies(vec![
            reply(2, 99, &[]),
            reply(0x8001, 0, &[1, 2]),
            reply(2, 0, &[]),
        ]);
        let mut client = Client::new(mock);
        assert_eq!(client.call(&Request::WhoAmI).unwrap(), Response::Success);
    }

    #[test]
    fn failure_becomes_a_device_error() {
        let mut w = Writer::new();
        w.string("no se puede");
        let mock = Mock::with_replies(vec![reply(3, 0, &w.into_vec())]);
        let mut client = Client::new(mock);
        match client.call(&Request::WhoAmI) {
            Err(Error::Device(msg)) => assert_eq!(msg, "no se puede"),
            other => panic!("resultado inesperado: {other:?}"),
        }
    }

    #[test]
    fn oversized_declared_payload_is_rejected() {
        let head = Header {
            message_type: 2,
            payload_len: MAX_RESPONSE_PAYLOAD + 1,
            serial: 0,
        }
        .to_bytes()
        .to_vec();
        let mut client = Client::new(Mock::with_replies(vec![head]));
        assert!(matches!(
            client.call(&Request::WhoAmI),
            Err(Error::PayloadTooLarge(_))
        ));
    }

    #[test]
    fn missing_reply_is_an_io_error() {
        let mut client = Client::new(Mock::with_replies(vec![]));
        assert!(matches!(client.call(&Request::WhoAmI), Err(Error::Io(_))));
    }

    #[test]
    fn who_am_i_returns_the_identification() {
        let mut w = Writer::new();
        for v in [1u16, 2, 3, 4] {
            w.bytes(&v.to_le_bytes());
        }
        w.string("a").string("b");
        let mock = Mock::with_replies(vec![reply(1, 0, &w.into_vec())]);
        let info = Client::new(mock).who_am_i().unwrap();
        assert_eq!(info.ids, [1, 2, 3, 4]);
    }

    #[test]
    fn wrong_response_type_is_reported() {
        let mock = Mock::with_replies(vec![reply(2, 0, &[])]);
        assert!(matches!(
            Client::new(mock).who_am_i(),
            Err(Error::UnexpectedResponse {
                expected: "WhoAmI",
                got: 2
            })
        ));
    }

    #[test]
    fn download_opens_reads_in_chunks_and_closes() {
        let size = super::super::request::MAX_CHUNK + 10;
        let file: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();

        let mut open = Writer::new();
        open.i64(5).i64(size as i64);
        let chunk = |range: std::ops::Range<usize>| {
            let mut w = Writer::new();
            w.i64(range.start as i64)
                .u32(range.len() as u32)
                .bytes(&file[range]);
            w.into_vec()
        };
        let mock = Mock::with_replies(vec![
            reply(0x0b, 0, &open.into_vec()),
            reply(0x0d, 1, &chunk(0..super::super::request::MAX_CHUNK)),
            reply(0x0d, 2, &chunk(super::super::request::MAX_CHUNK..size)),
            reply(2, 3, &[]),
        ]);

        let mut client = Client::new(mock);
        assert_eq!(client.download("/x/phrase.wav").unwrap(), file);
        assert_eq!(sent_types(&client.into_inner().output), [10, 12, 12, 16]);
    }

    #[test]
    fn download_closes_the_file_even_when_a_read_fails() {
        let mut open = Writer::new();
        open.i64(5).i64(100);
        let mut failure = Writer::new();
        failure.string("lectura fallida");
        let mock = Mock::with_replies(vec![
            reply(0x0b, 0, &open.into_vec()),
            reply(3, 1, &failure.into_vec()),
            reply(2, 2, &[]),
        ]);
        let mut client = Client::new(mock);
        assert!(matches!(client.download("/x"), Err(Error::Device(_))));
        assert_eq!(sent_types(&client.into_inner().output), [10, 12, 16]);
    }

    #[test]
    fn download_fails_if_the_device_returns_no_data() {
        let mut open = Writer::new();
        open.i64(5).i64(10);
        let mut empty = Writer::new();
        empty.i64(0).u32(0);
        let mock = Mock::with_replies(vec![
            reply(0x0b, 0, &open.into_vec()),
            reply(0x0d, 1, &empty.into_vec()),
            reply(2, 2, &[]),
        ]);
        assert!(matches!(
            Client::new(mock).download("/x"),
            Err(Error::Truncated {
                needed: 10,
                available: 0
            })
        ));
    }
}
