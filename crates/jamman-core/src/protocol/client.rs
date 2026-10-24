//! Cliente síncrono del protocolo.
//!
//! Es genérico sobre cualquier transporte que implemente [`Read`] y [`Write`]:
//! en producción será el puerto serie del pedal y en los tests un transporte
//! en memoria. Así `jamman-core` no depende de ninguna librería de puertos.

use std::io::{Read, Write};

use super::header::{HEADER_LEN, Header};
use super::request::{EntryKind, OpenMode, Request};
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
    pub fn entry_info(&mut self, path: &str, kind: EntryKind) -> Result<EntryInfo> {
        let request = Request::EntryInfo {
            path: path.into(),
            kind,
        };
        match self.call(&request)? {
            Response::EntryInfo(info) => Ok(info),
            other => Err(unexpected("EntryInfo", &other)),
        }
    }

    /// `true` si existe una entrada de ese tipo en `path`.
    pub fn exists(&mut self, path: &str, kind: EntryKind) -> Result<bool> {
        Ok(self.entry_info(path, kind)?.exists)
    }

    /// Lista una carpeta: `(ficheros, subcarpetas)`.
    pub fn folder_contents(&mut self, path: &str) -> Result<(Vec<String>, Vec<String>)> {
        let request = Request::FolderContents { path: path.into() };
        match self.call(&request)? {
            Response::FolderContents { files, folders } => Ok((files, folders)),
            other => Err(unexpected("FolderContents", &other)),
        }
    }

    fn expect_success(&mut self, request: &Request, what: &'static str) -> Result<()> {
        match self.call(request)? {
            Response::Success => Ok(()),
            other => Err(unexpected(what, &other)),
        }
    }

    /// Crea una carpeta.
    pub fn create_folder(&mut self, path: &str) -> Result<()> {
        let request = Request::EntryCreate {
            path: path.into(),
            kind: EntryKind::Folder,
        };
        self.expect_success(&request, "Success (crear carpeta)")
    }

    /// Borra un fichero o una carpeta con todo su contenido.
    pub fn delete(&mut self, path: &str, kind: EntryKind) -> Result<()> {
        let request = Request::EntryDelete {
            path: path.into(),
            kind,
        };
        self.expect_success(&request, "Success (borrar)")
    }

    fn open(&mut self, path: &str, mode: OpenMode) -> Result<(i64, u64)> {
        let request = Request::FileOpen {
            path: path.into(),
            mode,
        };
        match self.call(&request)? {
            Response::FileOpen { handle, size } => Ok((handle, size.max(0) as u64)),
            other => Err(unexpected("FileOpen", &other)),
        }
    }

    /// Ejecuta `body` con un fichero abierto y lo cierra siempre; el error de
    /// `body`, si lo hay, prevalece sobre el del cierre.
    fn with_file<R>(
        &mut self,
        path: &str,
        mode: OpenMode,
        body: impl FnOnce(&mut Self, i64, u64) -> Result<R>,
    ) -> Result<R> {
        let (handle, size) = self.open(path, mode)?;
        let result = body(self, handle, size);
        let closed = self.expect_success(&Request::FileClose { handle }, "Success (cerrar)");
        let value = result?;
        closed?;
        Ok(value)
    }

    fn read_chunk(&mut self, handle: i64, len: usize) -> Result<Vec<u8>> {
        let len = len.min(super::request::MAX_CHUNK) as u32;
        match self.call(&Request::FileRead { handle, len })? {
            Response::FileRead { data, .. } => Ok(data),
            other => Err(unexpected("FileRead", &other)),
        }
    }

    /// Descarga un fichero completo a memoria.
    pub fn download(&mut self, path: &str) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        self.download_to(path, &mut data, &mut |_, _| {})?;
        Ok(data)
    }

    /// Descarga un fichero a `out` por bloques. `progress(hechos, total)` se
    /// llama tras cada bloque. Devuelve los bytes descargados.
    pub fn download_to(
        &mut self,
        path: &str,
        out: &mut dyn Write,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<u64> {
        self.with_file(path, OpenMode::Read, |client, handle, size| {
            let mut done = 0u64;
            while done < size {
                let chunk = client.read_chunk(handle, (size - done) as usize)?;
                if chunk.is_empty() {
                    return Err(Error::Truncated {
                        needed: size as usize,
                        available: done as usize,
                    });
                }
                out.write_all(&chunk)?;
                done += chunk.len() as u64;
                progress(done, size);
            }
            Ok(done)
        })
    }

    /// Lee como mucho los primeros `max` bytes de un fichero (por ejemplo la
    /// cabecera de un WAV). Devuelve esos bytes y el tamaño total del fichero.
    pub fn read_head(&mut self, path: &str, max: usize) -> Result<(Vec<u8>, u64)> {
        self.with_file(path, OpenMode::Read, |client, handle, size| {
            let wanted = (max as u64).min(size) as usize;
            let mut data = Vec::with_capacity(wanted);
            while data.len() < wanted {
                let chunk = client.read_chunk(handle, wanted - data.len())?;
                if chunk.is_empty() {
                    break;
                }
                data.extend_from_slice(&chunk);
            }
            Ok((data, size))
        })
    }

    /// Sube `total` bytes de `input` a `path` (lo crea o lo sustituye).
    /// `progress(hechos, total)` se llama tras cada bloque.
    pub fn upload_from(
        &mut self,
        path: &str,
        input: &mut dyn Read,
        total: u64,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<()> {
        self.with_file(path, OpenMode::Write, |client, handle, _| {
            let mut buf = vec![0u8; super::request::MAX_CHUNK];
            let mut done = 0u64;
            loop {
                let n = read_up_to(input, &mut buf)?;
                if n == 0 {
                    break;
                }
                let request = Request::FileWrite {
                    handle,
                    data: buf[..n].to_vec(),
                };
                client.expect_success(&request, "Success (escribir)")?;
                done += n as u64;
                progress(done, total);
            }
            Ok(())
        })
    }

    /// Sube un fichero pequeño desde memoria.
    pub fn upload(&mut self, path: &str, data: &[u8]) -> Result<()> {
        self.upload_from(path, &mut &data[..], data.len() as u64, &mut |_, _| {})
    }
}

/// Lee hasta llenar `buf` o llegar al final; devuelve los bytes leídos.
fn read_up_to(r: &mut dyn Read, buf: &mut [u8]) -> Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match r.read(&mut buf[total..])? {
            0 => break,
            n => total += n,
        }
    }
    Ok(total)
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

    /// Frames enviados: `(tipo, carga)`.
    fn sent_frames(output: &[u8]) -> Vec<(u16, Vec<u8>)> {
        let mut frames = Vec::new();
        let mut rest = output;
        while !rest.is_empty() {
            let h = Header::parse(rest).unwrap();
            let end = HEADER_LEN + h.payload_len as usize;
            frames.push((h.message_type, rest[HEADER_LEN..end].to_vec()));
            rest = &rest[end..];
        }
        frames
    }

    fn open_reply(serial: u32, handle: i64, size: i64) -> Vec<u8> {
        let mut w = Writer::new();
        w.i64(handle).i64(size);
        reply(0x0b, serial, &w.into_vec())
    }

    #[test]
    fn upload_opens_for_writing_sends_chunks_and_closes() {
        let size = super::super::request::MAX_CHUNK + 5;
        let data: Vec<u8> = (0..size).map(|i| (i % 7) as u8).collect();
        let mock = Mock::with_replies(vec![
            open_reply(0, 3, 0),
            reply(2, 1, &[]),
            reply(2, 2, &[]),
            reply(2, 3, &[]),
        ]);
        let mut client = Client::new(mock);
        let mut seen = Vec::new();
        client
            .upload_from(
                "/a/b.wav",
                &mut &data[..],
                size as u64,
                &mut |done, total| {
                    seen.push((done, total));
                },
            )
            .unwrap();

        let frames = sent_frames(&client.into_inner().output);
        let types: Vec<u16> = frames.iter().map(|f| f.0).collect();
        assert_eq!(types, [10, 14, 14, 16]);
        assert!(frames[0].1.ends_with(&[2, 0]), "modo escritura validado");
        let sent: Vec<u8> = frames[1..3]
            .iter()
            .flat_map(|f| f.1[12..].to_vec())
            .collect();
        assert_eq!(sent, data, "los bloques reconstruyen el fichero");
        assert_eq!(seen.last(), Some(&(size as u64, size as u64)));
    }

    #[test]
    fn read_head_reads_only_what_is_needed_and_closes() {
        let mut chunk = Writer::new();
        chunk.i64(0).u32(4).bytes(b"RIFF");
        let mock = Mock::with_replies(vec![
            open_reply(0, 3, 1_000_000),
            reply(0x0d, 1, &chunk.into_vec()),
            reply(2, 2, &[]),
        ]);
        let mut client = Client::new(mock);
        let (head, total) = client.read_head("/x.wav", 4).unwrap();
        assert_eq!((head.as_slice(), total), (&b"RIFF"[..], 1_000_000));
        let types: Vec<u16> = sent_frames(&client.into_inner().output)
            .iter()
            .map(|f| f.0)
            .collect();
        assert_eq!(types, [10, 12, 16]);
    }

    #[test]
    fn folder_operations_send_the_validated_bytes() {
        let mut info = Writer::new();
        info.u8(1).string("/d").u8(1).i64(0).u8(0).u8(0);
        for _ in 0..3 {
            info.bytes(&[0; 6]);
        }
        let mock = Mock::with_replies(vec![
            reply(2, 0, &[]),
            reply(5, 1, &info.into_vec()),
            reply(2, 2, &[]),
        ]);
        let mut client = Client::new(mock);
        client.create_folder("/d").unwrap();
        assert!(client.exists("/d", EntryKind::Folder).unwrap());
        client.delete("/d/f.wav", EntryKind::File).unwrap();

        let frames = sent_frames(&client.into_inner().output);
        let path = [2, 0, 0, 0, b'/', b'd'];
        assert_eq!(frames[0], (7, [&[1u8][..], &path, &[1]].concat()));
        assert_eq!(frames[1], (4, [&[1u8][..], &path].concat()));
        assert_eq!(frames[2].0, 9);
        assert_eq!(frames[2].1[0], 0, "fichero");
        assert!(frames[2].1.ends_with(&[1, 0]));
    }

    #[test]
    fn a_failed_operation_reports_the_pedal_message() {
        let mut failure = Writer::new();
        failure.string("The folder does not exist.");
        let mut client = Client::new(Mock::with_replies(vec![reply(3, 0, &failure.into_vec())]));
        match client.folder_contents("") {
            Err(Error::Device(msg)) => assert_eq!(msg, "The folder does not exist."),
            other => panic!("resultado inesperado: {other:?}"),
        }
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
