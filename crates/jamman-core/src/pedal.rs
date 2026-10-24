//! Bucles guardados en el pedal, a través del USB.
//!
//! El pedal expone un sistema de ficheros de tipo Unix con la misma estructura
//! de bucles que la tarjeta leída desde el PC (validado con el pedal):
//!
//! ```text
//! /mnt/nandfs/JamManStereo/PatchNN/…   memoria interna
//! /mnt/sdfs/JamManStereo/PatchNN/…     tarjeta SD insertada en el pedal
//! ```
//!
//! [`PedalStore`] ofrece sobre él las mismas operaciones que
//! [`crate::card::Card`] y reutiliza su lógica pura (nombres de carpeta,
//! metadatos, ajustes). Cada operación avisa del progreso con
//! `progress(bytes hechos, bytes totales)`.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use crate::card::{
    AUDIO_FILE, Card, Entry, Listing, LoopSettings, PATCH_FILE, PHRASE_DIR, PHRASE_FILE, PatchMeta,
    PhraseMeta, apply_settings, imported_meta, patch_dir_name, slot_from_dir_name, wav,
};
use crate::device::{SLOTS_PER_STORAGE, Slot};
use crate::protocol::Client;
use crate::protocol::request::EntryKind;
use crate::{Error, Result};

/// Bytes que se leen del principio de cada WAV para conocer su formato.
const WAV_HEAD_BYTES: usize = 4096;

/// Almacén de bucles del pedal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Storage {
    /// Memoria interna (NAND).
    Internal,
    /// Tarjeta SD insertada en el pedal.
    Card,
}

impl Storage {
    /// Ambos almacenes.
    pub const ALL: [Self; 2] = [Self::Internal, Self::Card];

    /// Carpeta `JamManStereo` del almacén en el pedal.
    pub fn root(self) -> &'static str {
        match self {
            Self::Internal => "/mnt/nandfs/JamManStereo",
            Self::Card => "/mnt/sdfs/JamManStereo",
        }
    }

    /// Nombre para mostrar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Internal => "Memoria interna",
            Self::Card => "Tarjeta del pedal",
        }
    }
}

/// Bucles de un almacén del pedal sobre un cliente del protocolo.
#[derive(Debug)]
pub struct PedalStore<T> {
    client: Client<T>,
    storage: Storage,
}

fn join(dir: &str, name: &str) -> String {
    format!("{dir}/{name}")
}

impl<T: Read + Write> PedalStore<T> {
    /// Opera sobre `storage` usando `client`.
    pub fn new(client: Client<T>, storage: Storage) -> Self {
        Self { client, storage }
    }

    /// Almacén actual.
    pub fn storage(&self) -> Storage {
        self.storage
    }

    /// Cambia de almacén conservando la conexión.
    pub fn set_storage(&mut self, storage: Storage) {
        self.storage = storage;
    }

    /// Cliente del protocolo (para operaciones de bajo nivel).
    pub fn client(&mut self) -> &mut Client<T> {
        &mut self.client
    }

    /// Devuelve el cliente.
    pub fn into_client(self) -> Client<T> {
        self.client
    }

    fn patch_dir(&self, slot: Slot) -> String {
        join(self.storage.root(), &patch_dir_name(slot))
    }

    fn phrase_dir(&self, slot: Slot) -> String {
        join(&self.patch_dir(slot), PHRASE_DIR)
    }

    /// `true` si el almacén está disponible (la tarjeta puede no estar puesta).
    pub fn available(&mut self) -> Result<bool> {
        self.client.exists(self.storage.root(), EntryKind::Folder)
    }

    /// Posiciones con carpeta de bucle, ordenadas.
    pub fn slots(&mut self) -> Result<Vec<Slot>> {
        let (_, folders) = self.client.folder_contents(self.storage.root())?;
        let mut slots: Vec<Slot> = folders
            .iter()
            .filter_map(|f| slot_from_dir_name(f))
            .collect();
        slots.sort();
        slots.dedup();
        Ok(slots)
    }

    fn read_text(&mut self, path: &str) -> Result<String> {
        String::from_utf8(self.client.download(path)?).map_err(|_| Error::InvalidUtf8)
    }

    /// Lee un bucle: sus dos XML y la cabecera de su audio.
    pub fn read(&mut self, slot: Slot) -> Result<Entry> {
        if !self
            .client
            .exists(&self.patch_dir(slot), EntryKind::Folder)?
        {
            return Err(Error::SlotNotFound(slot.get()));
        }
        let patch = PatchMeta::parse(&self.read_text(&join(&self.patch_dir(slot), PATCH_FILE))?)?;
        let phrase =
            PhraseMeta::parse(&self.read_text(&join(&self.phrase_dir(slot), PHRASE_FILE))?)?;
        let audio_path = join(&self.phrase_dir(slot), AUDIO_FILE);
        if !self.client.exists(&audio_path, EntryKind::File)? {
            return Err(Error::MissingAudio(slot.get()));
        }
        let (head, size) = self.client.read_head(&audio_path, WAV_HEAD_BYTES)?;
        let audio = wav::parse_header(&head, size)?;
        Ok(Entry {
            slot,
            patch,
            phrase,
            audio,
        })
    }

    /// Lista los bucles. Uno dañado o sin audio no impide ver el resto.
    pub fn list(&mut self) -> Result<Vec<Listing>> {
        let slots = self.slots()?;
        Ok(slots
            .into_iter()
            .map(|slot| Listing {
                slot,
                entry: self.read(slot),
            })
            .collect())
    }

    /// Primera posición libre, si la hay.
    pub fn first_free_slot(&mut self) -> Result<Option<Slot>> {
        let used = self.slots()?;
        Ok((1..=SLOTS_PER_STORAGE)
            .filter_map(|n| Slot::new(n).ok())
            .find(|s| !used.contains(s)))
    }

    /// Sube los tres ficheros de un bucle. Si algo falla, borra lo subido.
    fn upload_loop(
        &mut self,
        slot: Slot,
        wav_path: &Path,
        patch_xml: &[u8],
        phrase_xml: &[u8],
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<()> {
        let dir = self.patch_dir(slot);
        if self.client.exists(&dir, EntryKind::Folder)? {
            return Err(Error::SlotOccupied(slot.get()));
        }
        let mut input = File::open(wav_path)?;
        let total = input.metadata()?.len();
        let result = (|| -> Result<()> {
            self.client.create_folder(&dir)?;
            self.client.create_folder(&self.phrase_dir(slot))?;
            let audio = join(&self.phrase_dir(slot), AUDIO_FILE);
            self.client
                .upload_from(&audio, &mut input, total, progress)?;
            self.client
                .upload(&join(&self.phrase_dir(slot), PHRASE_FILE), phrase_xml)?;
            // `patch.xml` el último: el bucle queda completo antes de aparecer.
            self.client.upload(&join(&dir, PATCH_FILE), patch_xml)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.client.delete(&dir, EntryKind::Folder);
        }
        result
    }

    /// Importa un WAV ya en el formato del pedal en una posición libre.
    /// `original` es el fichero que eligió el usuario (para la descripción).
    pub fn import_wav_from(
        &mut self,
        slot: Slot,
        wav_path: &Path,
        name: &str,
        original: &Path,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Entry> {
        wav::read_info(&mut File::open(wav_path)?)?.check_pedal_format()?;
        let (patch, phrase) = imported_meta(name, original);
        self.upload_loop(
            slot,
            wav_path,
            patch.render().as_bytes(),
            phrase.render().as_bytes(),
            progress,
        )?;
        self.read(slot)
    }

    /// Descarga el audio de un bucle a `dest`. No sobrescribe.
    pub fn export_wav(
        &mut self,
        slot: Slot,
        dest: &Path,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<u64> {
        let audio = join(&self.phrase_dir(slot), AUDIO_FILE);
        if !self.client.exists(&audio, EntryKind::File)? {
            return Err(Error::SlotNotFound(slot.get()));
        }
        let mut out = OpenOptions::new().write(true).create_new(true).open(dest)?;
        let result = self.client.download_to(&audio, &mut out, progress);
        if result.is_err() {
            drop(out);
            let _ = fs::remove_file(dest);
        }
        result
    }

    /// Borra un bucle entero.
    pub fn remove(&mut self, slot: Slot) -> Result<()> {
        let dir = self.patch_dir(slot);
        if !self.client.exists(&dir, EntryKind::Folder)? {
            return Err(Error::SlotNotFound(slot.get()));
        }
        self.client.delete(&dir, EntryKind::Folder)
    }

    /// Guarda los ajustes de un bucle (solo se suben los ficheros que cambian).
    pub fn update(&mut self, slot: Slot, settings: &LoopSettings) -> Result<Entry> {
        let patch_path = join(&self.patch_dir(slot), PATCH_FILE);
        let phrase_path = join(&self.phrase_dir(slot), PHRASE_FILE);
        if !self
            .client
            .exists(&self.patch_dir(slot), EntryKind::Folder)?
        {
            return Err(Error::SlotNotFound(slot.get()));
        }
        let patch_doc = self.read_text(&patch_path)?;
        let phrase_doc = self.read_text(&phrase_path)?;
        let (new_patch, new_phrase) = apply_settings(&patch_doc, &phrase_doc, settings)?;
        if new_phrase != phrase_doc {
            self.client.upload(&phrase_path, new_phrase.as_bytes())?;
        }
        if new_patch != patch_doc {
            self.client.upload(&patch_path, new_patch.as_bytes())?;
        }
        self.read(slot)
    }

    /// Copia un bucle completo del pedal a una tarjeta o copia local, tal
    /// cual (con sus ajustes e identificadores).
    pub fn backup_loop(
        &mut self,
        slot: Slot,
        dest: &Card,
        dest_slot: Slot,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<()> {
        let local = dest.patch_dir(dest_slot);
        if local.exists() {
            return Err(Error::SlotOccupied(dest_slot.get()));
        }
        let audio = join(&self.phrase_dir(slot), AUDIO_FILE);
        if !self.client.exists(&audio, EntryKind::File)? {
            return Err(Error::MissingAudio(slot.get()));
        }
        let tmp = dest
            .root()
            .join(format!(".tmp-{}", patch_dir_name(dest_slot)));
        let _ = fs::remove_dir_all(&tmp);
        let result = (|| -> Result<()> {
            fs::create_dir_all(tmp.join(PHRASE_DIR))?;
            let patch = self
                .client
                .download(&join(&self.patch_dir(slot), PATCH_FILE))?;
            let phrase = self
                .client
                .download(&join(&self.phrase_dir(slot), PHRASE_FILE))?;
            let mut wav_out = File::create(tmp.join(PHRASE_DIR).join(AUDIO_FILE))?;
            self.client.download_to(&audio, &mut wav_out, progress)?;
            wav_out.sync_all()?;
            fs::write(tmp.join(PATCH_FILE), patch)?;
            fs::write(tmp.join(PHRASE_DIR).join(PHRASE_FILE), phrase)?;
            fs::rename(&tmp, &local)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&tmp);
        }
        result
    }

    /// Copia un bucle completo de una tarjeta o copia local al pedal, tal cual.
    pub fn restore_loop(
        &mut self,
        source: &Card,
        source_slot: Slot,
        slot: Slot,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<()> {
        let local = source.patch_dir(source_slot);
        let patch = fs::read(local.join(PATCH_FILE))?;
        let phrase = fs::read(local.join(PHRASE_DIR).join(PHRASE_FILE))?;
        let wav_path = local.join(PHRASE_DIR).join(AUDIO_FILE);
        wav::read_info(&mut File::open(&wav_path)?)?.check_pedal_format()?;
        self.upload_loop(slot, &wav_path, &patch, &phrase, progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::RhythmType;
    use crate::card::testutil::pedal_wav;
    use crate::protocol::fake::FakePedal;

    const SD: &str = "/mnt/sdfs/JamManStereo";

    fn slot(n: u8) -> Slot {
        Slot::new(n).unwrap()
    }

    fn store(pedal: FakePedal, storage: Storage) -> PedalStore<FakePedal> {
        PedalStore::new(Client::new(pedal), storage)
    }

    /// Estado final del pedal simulado.
    fn pedal(store: PedalStore<FakePedal>) -> FakePedal {
        store.into_client().into_inner()
    }

    /// Pedal con un bucle completo en la tarjeta, como los que escribe la app.
    fn pedal_with_loop(n: u8, name: &str, audio_bytes: usize) -> FakePedal {
        let mut p = FakePedal::new();
        let (patch, phrase) = imported_meta(name, Path::new("x.mp3"));
        let dir = format!("{SD}/Patch{n:02}");
        p.add_file(&format!("{dir}/patch.xml"), patch.render().as_bytes());
        p.add_file(
            &format!("{dir}/PhraseA/phrase.xml"),
            phrase.render().as_bytes(),
        );
        p.add_file(
            &format!("{dir}/PhraseA/phrase.wav"),
            &pedal_wav(audio_bytes),
        );
        p
    }

    #[test]
    fn lists_and_reads_loops_like_a_card() {
        let mut p = pedal_with_loop(21, "Dumpweed", 10_000);
        // Una posición con ajustes guardados pero sin grabar (como en la
        // memoria interna real) y una carpeta ajena.
        p.add_file(&format!("{SD}/Patch03/PhraseA/phrase.xml"), b"<x/>");
        p.add_file(&format!("{SD}/Patch03/patch.xml"), b"<x/>");
        p.dirs.insert(format!("{SD}/Otra"));
        let mut s = store(p, Storage::Card);

        let listing = s.list().unwrap();
        let slots: Vec<u8> = listing.iter().map(|l| l.slot.get()).collect();
        assert_eq!(slots, [3, 21]);
        let entry = listing[1].entry.as_ref().unwrap();
        assert_eq!(entry.patch.name, "Dumpweed");
        assert_eq!(
            entry.audio.data_len, 10_000,
            "tamaño real aunque solo se lea la cabecera"
        );
        assert!(listing[0].entry.is_err());
    }

    #[test]
    fn internal_memory_is_a_different_root() {
        let p = pedal_with_loop(5, "en la tarjeta", 8);
        let mut s = store(p, Storage::Internal);
        assert!(s.list().unwrap().is_empty());
        s.set_storage(Storage::Card);
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn import_uploads_the_same_layout_as_a_card_and_reads_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("t.wav");
        let audio = pedal_wav(20_000);
        fs::write(&src, &audio).unwrap();
        let mut s = store(FakePedal::new(), Storage::Card);
        let mut last = (0, 0);
        let e = s
            .import_wav_from(
                slot(7),
                &src,
                "Nueva",
                Path::new("Nueva.mp3"),
                &mut |d, t| last = (d, t),
            )
            .unwrap();
        assert_eq!(e.patch.name, "Nueva");
        assert_eq!(e.patch.description, "Imported from Nueva.mp3");
        assert_eq!(last, (audio.len() as u64, audio.len() as u64));

        let p = pedal(s);
        assert_eq!(p.files[&format!("{SD}/Patch07/PhraseA/phrase.wav")], audio);
        assert!(p.files.contains_key(&format!("{SD}/Patch07/patch.xml")));
        assert!(
            p.files
                .contains_key(&format!("{SD}/Patch07/PhraseA/phrase.xml"))
        );
    }

    #[test]
    fn import_refuses_occupied_slots_and_wrong_audio() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("t.wav");
        fs::write(&src, pedal_wav(8)).unwrap();
        let mut s = store(pedal_with_loop(1, "x", 8), Storage::Card);
        assert!(matches!(
            s.import_wav_from(slot(1), &src, "y", &src, &mut |_, _| {}),
            Err(Error::SlotOccupied(1))
        ));
        let bad = dir.path().join("mono.wav");
        fs::write(
            &bad,
            crate::card::testutil::wav_bytes(1, 48_000, 16, &[0; 8], false),
        )
        .unwrap();
        assert!(matches!(
            s.import_wav_from(slot(2), &bad, "y", &bad, &mut |_, _| {}),
            Err(Error::UnsupportedAudio(_))
        ));
        assert_eq!(s.slots().unwrap(), [slot(1)], "no queda nada a medias");
    }

    #[test]
    fn a_failed_upload_leaves_no_half_written_loop() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("t.wav");
        fs::write(&src, pedal_wav(8)).unwrap();
        // Sin la carpeta JamManStereo de la tarjeta, crear el bucle falla.
        let mut p = FakePedal::new();
        p.dirs.remove(SD);
        let mut s = store(p, Storage::Card);
        assert!(
            s.import_wav_from(slot(1), &src, "y", &src, &mut |_, _| {})
                .is_err()
        );
        assert!(!pedal(s).dirs.iter().any(|d| d.contains("Patch01")));
    }

    #[test]
    fn export_downloads_the_audio_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(pedal_with_loop(4, "x", 30_000), Storage::Card);
        let dest = dir.path().join("out.wav");
        let n = s.export_wav(slot(4), &dest, &mut |_, _| {}).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), pedal_wav(30_000));
        assert_eq!(n, pedal_wav(30_000).len() as u64);
        assert!(s.export_wav(slot(4), &dest, &mut |_, _| {}).is_err());
        assert!(matches!(
            s.export_wav(slot(9), &dir.path().join("b.wav"), &mut |_, _| {}),
            Err(Error::SlotNotFound(9))
        ));
    }

    #[test]
    fn update_and_remove() {
        let mut s = store(pedal_with_loop(2, "viejo", 8), Storage::Card);
        let mut settings = LoopSettings::from_entry(&s.read(slot(2)).unwrap());
        settings.name = "nuevo".into();
        settings.rhythm_type = RhythmType::Conga.xml().into();
        settings.is_loop = true;
        let e = s.update(slot(2), &settings).unwrap();
        assert_eq!(LoopSettings::from_entry(&e), settings);
        s.remove(slot(2)).unwrap();
        assert!(s.slots().unwrap().is_empty());
        assert!(matches!(s.remove(slot(2)), Err(Error::SlotNotFound(2))));
    }

    #[test]
    fn backup_and_restore_copy_loops_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let backup = Card::create(dir.path()).unwrap();
        let mut s = store(pedal_with_loop(21, "Dumpweed", 5_000), Storage::Card);

        s.backup_loop(slot(21), &backup, slot(21), &mut |_, _| {})
            .unwrap();
        let local = backup.read(slot(21)).unwrap();
        assert_eq!(local.patch.name, "Dumpweed");
        let original_id = local.patch.id.clone();

        s.set_storage(Storage::Internal);
        s.restore_loop(&backup, slot(21), slot(1), &mut |_, _| {})
            .unwrap();
        let restored = s.read(slot(1)).unwrap();
        assert_eq!(
            restored.patch.id, original_id,
            "se conservan los identificadores"
        );
        assert!(matches!(
            s.backup_loop(slot(1), &backup, slot(21), &mut |_, _| {}),
            Err(Error::SlotOccupied(21))
        ));
    }

    #[test]
    fn first_free_slot_and_availability() {
        let mut s = store(pedal_with_loop(1, "x", 8), Storage::Card);
        assert!(s.available().unwrap());
        assert_eq!(s.first_free_slot().unwrap(), Some(slot(2)));
        let mut p = FakePedal::new();
        p.dirs.remove(SD);
        let mut s = store(p, Storage::Card);
        assert!(!s.available().unwrap(), "sin tarjeta en el pedal");
    }
}
