//! Mide cuánto tarda el pedal en aceptar cada bloque al subir un fichero.
//!
//! Sube `<fichero>` a la carpeta aislada `<carpeta>/jamman-test` del pedal
//! (por defecto `/mnt/sdfs`), con un límite de espera de 2 minutos, separa el
//! tiempo de escritura del de espera de la respuesta y borra la carpeta al
//! terminar.
//!
//! ```bash
//! cargo run --release -p jamman-core --features serial --example upload_latency -- cancion.wav
//! ```

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use jamman_core::protocol::Client;
use jamman_core::protocol::request::EntryKind;

/// Transporte que acumula el tiempo bloqueado en escribir y en leer.
struct Timed<T> {
    inner: T,
    write: Duration,
    read: Duration,
}

impl<T: Write> Write for Timed<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let t = Instant::now();
        let r = self.inner.write(buf);
        self.write += t.elapsed();
        r
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let t = Instant::now();
        let r = self.inner.flush();
        self.write += t.elapsed();
        r
    }
}

impl<T: Read> Read for Timed<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let t = Instant::now();
        let r = self.inner.read(buf);
        self.read += t.elapsed();
        r
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let file = args
        .next()
        .ok_or("uso: upload_latency <fichero> [/mnt/sdfs]")?;
    let base = args.next().unwrap_or_else(|| "/mnt/sdfs".to_owned());
    let dir = format!("{base}/jamman-test");

    let mut port = jamman_core::serial::open(None)?;
    port.set_timeout(Duration::from_secs(120))?;
    let mut client = Client::new(Timed {
        inner: port,
        write: Duration::ZERO,
        read: Duration::ZERO,
    });
    println!("{:?}", client.who_am_i()?);
    if client.exists(&dir, EntryKind::Folder)? {
        return Err(format!("{dir} ya existe; bórrala antes").into());
    }
    client.create_folder(&dir)?;

    let mut input = std::fs::File::open(&file)?;
    let total = input.metadata()?.len();
    let start = Instant::now();
    let mut last = Instant::now();
    let mut worst = Duration::ZERO;
    let mut chunk = 0u32;
    let mut slow = 0u32;
    let result = client.upload_from(
        &format!("{dir}/phrase.wav"),
        &mut input,
        total,
        &mut |done, total| {
            let dt = last.elapsed();
            last = Instant::now();
            chunk += 1;
            worst = worst.max(dt);
            if dt > Duration::from_millis(500) {
                slow += 1;
                println!("bloque {chunk:5} ({done}/{total}): {dt:.2?}");
            }
        },
    );
    let elapsed = start.elapsed();
    let io = client.into_inner();
    println!(
        "{result:?}\n{chunk} bloques en {elapsed:.1?} ({:.2} MB/s); peor bloque {worst:.2?}; \
         {slow} de más de 0,5 s\nbloqueado escribiendo {:.1?}, esperando respuestas {:.1?}",
        total as f64 / elapsed.as_secs_f64() / 1e6,
        io.write,
        io.read,
    );
    let mut client = Client::new(io.inner);
    client.delete(&dir, EntryKind::Folder)?;
    println!("{dir} borrada");
    Ok(())
}
