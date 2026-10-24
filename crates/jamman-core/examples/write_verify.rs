//! Sube ficheros con contenido conocido a una carpeta aislada del pedal, los
//! vuelve a leer y compara. Borra la carpeta al terminar.
//!
//! ```bash
//! cargo run --release -p jamman-core --features serial --example write_verify -- /mnt/sdfs 100 8169 176444
//! ```

use jamman_core::protocol::request::EntryKind;

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761).to_le_bytes()[0] ^ seed)
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let base = args.next().ok_or("uso: write_verify <base> <tamaño>...")?;
    let sizes: Vec<usize> = args.map(|a| a.parse()).collect::<Result<_, _>>()?;
    let dir = format!("{base}/jamman-test");
    let mut client = jamman_core::serial::connect(None)?;
    if client.exists(&dir, EntryKind::Folder)? {
        return Err(format!("{dir} ya existe; bórrala antes").into());
    }
    client.create_folder(&dir)?;
    for (i, &size) in sizes.iter().enumerate() {
        let path = format!("{dir}/f{i}.bin");
        let data = pattern(size, i as u8 + 1);
        client.upload(&path, &data)?;
        let back = client.download(&path)?;
        match back.iter().zip(&data).position(|(a, b)| a != b) {
            None if back.len() == data.len() => println!("{size:>9} B: OK"),
            None => println!("{size:>9} B: tamaño {} ≠ {size}", back.len()),
            Some(at) => {
                let bad = back.iter().zip(&data).filter(|(a, b)| a != b).count();
                println!("{size:>9} B: MAL desde el byte {at} ({bad} bytes distintos)");
            }
        }
    }
    client.delete(&dir, EntryKind::Folder)?;
    println!("{dir} borrada");
    Ok(())
}
