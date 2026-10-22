//! Tarjeta SD/SDHC del JamMan Stereo.
//!
//! Se puede leer y escribir **sin tener el pedal conectado**: basta con la
//! tarjeta montada. Ver `docs/04-tarjeta-sd.md` para el formato.
//!
//! ```text
//! <tarjeta>/JamManStereo/
//!     Patch01/            <- una carpeta por posición de bucle (01..99)
//!         patch.xml       <- nombre, ritmo, metadatos
//!         PhraseA/
//!             phrase.wav  <- audio PCM 44,1 kHz, 16 bit, estéreo
//!             phrase.xml  <- tempo, bucle, inversión
//! ```

pub mod meta;
pub mod options;
#[cfg(test)]
pub(crate) mod testutil;
pub mod wav;
pub(crate) mod xml;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::device::{SLOTS_PER_STORAGE, Slot};
use crate::{Error, Result};
pub use meta::{PatchMeta, PhraseMeta};
pub use options::{RhythmType, StopMode};
pub use wav::WavInfo;

/// Nombre de la carpeta raíz del JamMan en la tarjeta.
pub const CARD_DIR: &str = "JamManStereo";

const PHRASE_DIR: &str = "PhraseA";
const PATCH_FILE: &str = "patch.xml";
const PHRASE_FILE: &str = "phrase.xml";
const AUDIO_FILE: &str = "phrase.wav";
const TMP_PREFIX: &str = ".tmp-";

/// Un bucle de la tarjeta, ya leído.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Posición del bucle.
    pub slot: Slot,
    /// Contenido de `patch.xml`.
    pub patch: PatchMeta,
    /// Contenido de `phrase.xml`.
    pub phrase: PhraseMeta,
    /// Formato y tamaño del audio.
    pub audio: WavInfo,
}

/// Ajustes editables de un bucle.
///
/// Ritmo y modo de parada se guardan como el texto XML para no perder valores
/// desconocidos; [`RhythmType`] y [`StopMode`] sirven para elegirlos.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopSettings {
    /// Nombre.
    pub name: String,
    /// Ritmo (valor XML, ver [`RhythmType::xml`]).
    pub rhythm_type: String,
    /// Modo de parada (valor XML, ver [`StopMode::xml`]).
    pub stop_mode: String,
    /// Tempo en BPM.
    pub bpm: f64,
    /// Tiempos por compás.
    pub beats_per_measure: u32,
    /// `true`: bucle; `false`: una sola vez (Single).
    pub is_loop: bool,
    /// Reproducción invertida.
    pub is_reversed: bool,
    /// Descripción.
    pub description: String,
    /// Artista.
    pub artist: String,
    /// Género.
    pub genre: String,
    /// Valoración (0 a 5).
    pub rating: u8,
    /// Etiquetas.
    pub tags: String,
}

impl LoopSettings {
    /// Ajustes actuales de un bucle leído.
    pub fn from_entry(e: &Entry) -> Self {
        Self {
            name: e.patch.name.clone(),
            rhythm_type: e.patch.rhythm_type.clone(),
            stop_mode: e.patch.stop_mode.clone(),
            bpm: e.phrase.bpm,
            beats_per_measure: e.phrase.beats_per_measure,
            is_loop: e.phrase.is_loop,
            is_reversed: e.phrase.is_reversed,
            description: e.patch.description.clone(),
            artist: e.patch.artist.clone(),
            genre: e.patch.genre.clone(),
            rating: e.patch.rating,
            tags: e.patch.tags.clone(),
        }
    }

    /// Comprueba los rangos del pedal.
    pub fn validate(&self) -> Result<()> {
        if !options::BPM_RANGE.contains(&self.bpm) {
            return Err(Error::InvalidSetting(format!(
                "tempo {} fuera de 40..=240 BPM",
                self.bpm
            )));
        }
        if !options::BEATS_PER_MEASURE_RANGE.contains(&self.beats_per_measure) {
            return Err(Error::InvalidSetting(format!(
                "{} tiempos por compás fuera de 2..=15",
                self.beats_per_measure
            )));
        }
        if self.rating > options::MAX_RATING {
            return Err(Error::InvalidSetting(format!(
                "valoración {} mayor que {}",
                self.rating,
                options::MAX_RATING
            )));
        }
        Ok(())
    }
}

/// Fija varias etiquetas de un documento: `(etiqueta, valor, padre)`. Una
/// etiqueta que no existía no se crea si su valor es el que se asume cuando
/// falta (vacío, o `0` en `Rating`), para no añadir nada innecesario a los
/// ficheros que escribe el pedal.
fn set_all(mut doc: String, file: &str, fields: &[(&str, String, &str)]) -> Result<String> {
    for (tag, value, parent) in fields {
        let default = if *tag == "Rating" { "0" } else { "" };
        if value == default && xml::text(&doc, tag).is_none() {
            continue;
        }
        if xml::text(&doc, parent).is_none()
            && *parent != "JamManPatch"
            && *parent != "JamManPhrase"
        {
            let root = if file == PATCH_FILE {
                "JamManPatch"
            } else {
                "JamManPhrase"
            };
            doc = xml::set_text(&doc, parent, "", root).ok_or_else(|| {
                Error::InvalidMetadata(format!("no se puede crear <{parent}> en {file}"))
            })?;
        }
        doc = xml::set_text(&doc, tag, value, parent).ok_or_else(|| {
            Error::InvalidMetadata(format!("no se puede escribir <{tag}> en {file}"))
        })?;
    }
    Ok(doc)
}

/// Resultado de leer una posición al listar la tarjeta. Un bucle dañado no
/// impide ver el resto.
#[derive(Debug)]
pub struct Listing {
    /// Posición de la carpeta.
    pub slot: Slot,
    /// El bucle, o el motivo por el que no se pudo leer.
    pub entry: Result<Entry>,
}

/// Tarjeta del JamMan montada en el sistema de ficheros.
#[derive(Debug, Clone)]
pub struct Card {
    root: PathBuf,
}

fn patch_dir_name(slot: Slot) -> String {
    format!("Patch{:02}", slot.get())
}

/// Interpreta `PatchNN` como posición de bucle.
fn slot_from_dir_name(name: &str) -> Option<Slot> {
    let digits = name.strip_prefix("Patch")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Slot::new(digits.parse().ok()?).ok()
}

impl Card {
    /// Abre una tarjeta. `path` puede ser el punto de montaje (que contiene
    /// `JamManStereo/`) o la propia carpeta `JamManStereo`.
    pub fn open(path: &Path) -> Result<Self> {
        let is_card_dir = path.file_name().is_some_and(|n| n == CARD_DIR);
        let root = if is_card_dir {
            path.to_path_buf()
        } else {
            path.join(CARD_DIR)
        };
        if root.is_dir() {
            Ok(Self { root })
        } else {
            Err(Error::NotACard(path.to_path_buf()))
        }
    }

    /// Prepara una tarjeta vacía creando `JamManStereo/` si no existe.
    pub fn create(mount: &Path) -> Result<Self> {
        fs::create_dir_all(mount.join(CARD_DIR))?;
        Self::open(mount)
    }

    /// Carpeta `JamManStereo` de la tarjeta.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn patch_dir(&self, slot: Slot) -> PathBuf {
        self.root.join(patch_dir_name(slot))
    }

    fn audio_path(&self, slot: Slot) -> PathBuf {
        self.patch_dir(slot).join(PHRASE_DIR).join(AUDIO_FILE)
    }

    /// Lee el bucle de una posición.
    pub fn read(&self, slot: Slot) -> Result<Entry> {
        let dir = self.patch_dir(slot);
        if !dir.is_dir() {
            return Err(Error::SlotNotFound(slot.get()));
        }
        let phrase_dir = dir.join(PHRASE_DIR);
        let patch = PatchMeta::parse(&fs::read_to_string(dir.join(PATCH_FILE))?)?;
        let phrase = PhraseMeta::parse(&fs::read_to_string(phrase_dir.join(PHRASE_FILE))?)?;
        let audio = wav::read_info(&mut File::open(phrase_dir.join(AUDIO_FILE))?)?;
        Ok(Entry {
            slot,
            patch,
            phrase,
            audio,
        })
    }

    /// Lista los bucles de la tarjeta ordenados por posición. Se ignoran las
    /// carpetas y ficheros que no sean `PatchNN` con NN entre 01 y 99.
    pub fn list(&self) -> Result<Vec<Listing>> {
        let mut slots: Vec<Slot> = fs::read_dir(&self.root)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| slot_from_dir_name(&e.file_name().to_string_lossy()))
            .collect();
        slots.sort();
        slots.dedup();
        Ok(slots
            .into_iter()
            .map(|slot| Listing {
                slot,
                entry: self.read(slot),
            })
            .collect())
    }

    /// Primera posición libre, si la hay.
    pub fn first_free_slot(&self) -> Option<Slot> {
        (1..=SLOTS_PER_STORAGE)
            .filter_map(|n| Slot::new(n).ok())
            .find(|&slot| !self.patch_dir(slot).exists())
    }

    /// Importa un WAV en una posición libre.
    ///
    /// El audio debe estar ya en el formato del pedal (PCM, 44,1 kHz, 16 bit,
    /// estéreo, hasta 10 minutos); el fichero se copia sin modificar. Se
    /// construye en una carpeta temporal y solo al final se renombra a
    /// `PatchNN`, de modo que un fallo (por ejemplo, tarjeta llena) no deja un
    /// bucle a medias.
    pub fn import_wav(&self, slot: Slot, source: &Path, name: &str) -> Result<Entry> {
        let final_dir = self.patch_dir(slot);
        if final_dir.exists() {
            return Err(Error::SlotOccupied(slot.get()));
        }

        let mut input = File::open(source)?;
        wav::read_info(&mut input)?.check_pedal_format()?;

        let id = uuid::Uuid::new_v4().to_string();
        let file_name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let patch = PatchMeta::imported(name, &format!("Imported from {file_name}"), &id);
        let phrase = PhraseMeta::imported(&id);

        let tmp = self
            .root
            .join(format!("{TMP_PREFIX}{}", patch_dir_name(slot)));
        let _ = fs::remove_dir_all(&tmp);

        let built = (|| -> Result<()> {
            fs::create_dir_all(tmp.join(PHRASE_DIR))?;
            input.seek(SeekFrom::Start(0))?;
            let mut out = File::create(tmp.join(PHRASE_DIR).join(AUDIO_FILE))?;
            io::copy(&mut input, &mut out)?;
            out.sync_all()?;
            fs::write(tmp.join(PATCH_FILE), patch.render())?;
            fs::write(tmp.join(PHRASE_DIR).join(PHRASE_FILE), phrase.render())?;
            fs::rename(&tmp, &final_dir)?;
            Ok(())
        })();
        if let Err(e) = built {
            let _ = fs::remove_dir_all(&tmp);
            return Err(e);
        }
        self.read(slot)
    }

    /// Guarda los ajustes de un bucle. Solo cambian las etiquetas afectadas; el
    /// resto de `patch.xml` y `phrase.xml` queda tal cual. Un fichero que no
    /// cambia no se reescribe.
    pub fn update(&self, slot: Slot, settings: &LoopSettings) -> Result<Entry> {
        settings.validate()?;
        let dir = self.patch_dir(slot);
        if !dir.is_dir() {
            return Err(Error::SlotNotFound(slot.get()));
        }
        let patch_path = dir.join(PATCH_FILE);
        let phrase_path = dir.join(PHRASE_DIR).join(PHRASE_FILE);
        let patch_doc = fs::read_to_string(&patch_path)?;
        let phrase_doc = fs::read_to_string(&phrase_path)?;

        let s = settings;
        let new_patch = set_all(
            patch_doc.clone(),
            PATCH_FILE,
            &[
                ("PatchName", s.name.clone(), "JamManPatch"),
                ("RhythmType", s.rhythm_type.clone(), "JamManPatch"),
                ("StopMode", s.stop_mode.clone(), "JamManPatch"),
                ("Description", s.description.clone(), "Metadata"),
                ("Artist", s.artist.clone(), "Metadata"),
                ("Genre", s.genre.clone(), "Metadata"),
                ("Rating", s.rating.to_string(), "Metadata"),
                ("Tags", s.tags.clone(), "Metadata"),
            ],
        )?;
        let flag = |b: bool| if b { "1" } else { "0" }.to_owned();
        let new_phrase = set_all(
            phrase_doc.clone(),
            PHRASE_FILE,
            &[
                ("BeatsPerMinute", s.bpm.to_string(), "JamManPhrase"),
                (
                    "BeatsPerMeasure",
                    s.beats_per_measure.to_string(),
                    "JamManPhrase",
                ),
                ("IsLoop", flag(s.is_loop), "JamManPhrase"),
                ("IsReversed", flag(s.is_reversed), "JamManPhrase"),
            ],
        )?;

        if new_patch != patch_doc {
            fs::write(&patch_path, new_patch)?;
        }
        if new_phrase != phrase_doc {
            fs::write(&phrase_path, new_phrase)?;
        }
        self.read(slot)
    }

    /// Mueve un bucle a una posición libre.
    pub fn move_to(&self, from: Slot, to: Slot) -> Result<()> {
        if from == to {
            return Ok(());
        }
        let source = self.patch_dir(from);
        if !source.is_dir() {
            return Err(Error::SlotNotFound(from.get()));
        }
        let target = self.patch_dir(to);
        if target.exists() {
            return Err(Error::SlotOccupied(to.get()));
        }
        fs::rename(source, target)?;
        Ok(())
    }

    /// Intercambia dos posiciones. Si la segunda está libre, equivale a mover.
    pub fn swap(&self, a: Slot, b: Slot) -> Result<()> {
        if a == b {
            return Ok(());
        }
        let (dir_a, dir_b) = (self.patch_dir(a), self.patch_dir(b));
        if !dir_a.is_dir() {
            return Err(Error::SlotNotFound(a.get()));
        }
        if !dir_b.exists() {
            return self.move_to(a, b);
        }
        let tmp = self
            .root
            .join(format!("{TMP_PREFIX}swap-{}", patch_dir_name(a)));
        fs::rename(&dir_a, &tmp)?;
        if let Err(e) = fs::rename(&dir_b, &dir_a) {
            let _ = fs::rename(&tmp, &dir_a);
            return Err(e.into());
        }
        if let Err(e) = fs::rename(&tmp, &dir_b) {
            // Deshace para no dejar la tarjeta a medias.
            let _ = fs::rename(&dir_a, &dir_b);
            let _ = fs::rename(&tmp, &dir_a);
            return Err(e.into());
        }
        Ok(())
    }

    /// Cambia el nombre de un bucle. El resto de `patch.xml` no se toca.
    pub fn rename(&self, slot: Slot, new_name: &str) -> Result<()> {
        let path = self.patch_dir(slot).join(PATCH_FILE);
        if !path.is_file() {
            return Err(Error::SlotNotFound(slot.get()));
        }
        let doc = fs::read_to_string(&path)?;
        let updated = xml::replace_text(&doc, "PatchName", new_name)
            .ok_or_else(|| Error::InvalidMetadata("falta <PatchName> en patch.xml".into()))?;
        fs::write(path, updated)?;
        Ok(())
    }

    /// Borra un bucle (su carpeta completa). No se puede deshacer.
    pub fn remove(&self, slot: Slot) -> Result<()> {
        let dir = self.patch_dir(slot);
        if !dir.is_dir() {
            return Err(Error::SlotNotFound(slot.get()));
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Copia el audio de un bucle a `dest`. No sobrescribe ficheros existentes.
    /// Devuelve los bytes copiados.
    pub fn export_wav(&self, slot: Slot, dest: &Path) -> Result<u64> {
        let source = self.audio_path(slot);
        if !source.is_file() {
            return Err(Error::SlotNotFound(slot.get()));
        }
        let mut input = File::open(source)?;
        let mut out = OpenOptions::new().write(true).create_new(true).open(dest)?;
        Ok(io::copy(&mut input, &mut out)?)
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::{pedal_wav, wav_bytes};
    use super::*;
    use tempfile::TempDir;

    fn slot(n: u8) -> Slot {
        Slot::new(n).unwrap()
    }

    fn new_card() -> (TempDir, Card) {
        let dir = tempfile::tempdir().unwrap();
        let card = Card::create(dir.path()).unwrap();
        (dir, card)
    }

    fn write_source(dir: &TempDir, file: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join(file);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn open_requires_the_jamman_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(Card::open(dir.path()), Err(Error::NotACard(_))));
        let card = Card::create(dir.path()).unwrap();
        assert!(Card::open(dir.path()).is_ok());
        assert!(
            Card::open(card.root()).is_ok(),
            "también la carpeta JamManStereo"
        );
    }

    #[test]
    fn empty_card_lists_nothing() {
        let (_dir, card) = new_card();
        assert!(card.list().unwrap().is_empty());
    }

    #[test]
    fn import_creates_the_original_layout_and_copies_the_audio_untouched() {
        let (dir, card) = new_card();
        let audio = wav_bytes(2, 44_100, 16, &[1, 2, 3, 4, 5, 6, 7, 8], true);
        let src = write_source(&dir, "Mi canción.wav", &audio);

        let entry = card.import_wav(slot(5), &src, "Mi canción").unwrap();

        assert_eq!(entry.slot, slot(5));
        assert_eq!(entry.patch.name, "Mi canción");
        assert_eq!(entry.patch.description, "Imported from Mi canción.wav");
        assert_eq!(
            entry.patch.origin_id.as_deref(),
            Some(entry.patch.id.as_str())
        );
        assert_eq!(entry.phrase.id, entry.patch.id);
        assert_eq!(entry.phrase.bpm, 120.0);
        assert_eq!(entry.audio.data_len, 8);

        let base = card.root().join("Patch05");
        assert_eq!(fs::read(base.join("PhraseA/phrase.wav")).unwrap(), audio);
        assert!(base.join("patch.xml").is_file());
        assert!(base.join("PhraseA/phrase.xml").is_file());
    }

    #[test]
    fn every_import_gets_a_distinct_id() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        let a = card.import_wav(slot(1), &src, "a").unwrap();
        let b = card.import_wav(slot(2), &src, "b").unwrap();
        assert_ne!(a.patch.id, b.patch.id);
    }

    #[test]
    fn import_refuses_an_occupied_slot_without_touching_it() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        card.import_wav(slot(3), &src, "primera").unwrap();
        let other = write_source(&dir, "b.wav", &pedal_wav(8));
        assert!(matches!(
            card.import_wav(slot(3), &other, "segunda"),
            Err(Error::SlotOccupied(3))
        ));
        assert_eq!(card.read(slot(3)).unwrap().patch.name, "primera");
    }

    #[test]
    fn import_rejects_wrong_audio_and_leaves_no_trace() {
        let (dir, card) = new_card();
        let mono = write_source(&dir, "mono.wav", &wav_bytes(1, 48_000, 16, &[0; 8], false));
        let junk = write_source(&dir, "no.wav", b"esto no es un wav");
        assert!(matches!(
            card.import_wav(slot(1), &mono, "x"),
            Err(Error::UnsupportedAudio(_))
        ));
        assert!(matches!(
            card.import_wav(slot(1), &junk, "x"),
            Err(Error::InvalidWav(_))
        ));
        assert!(matches!(
            card.import_wav(slot(1), &dir.path().join("no-existe.wav"), "x"),
            Err(Error::Io(_))
        ));
        let leftovers: Vec<_> = fs::read_dir(card.root()).unwrap().collect();
        assert!(leftovers.is_empty(), "no debe quedar nada: {leftovers:?}");
    }

    #[test]
    fn list_is_sorted_and_ignores_unrelated_entries() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        for n in [20, 2, 11] {
            card.import_wav(slot(n), &src, &format!("c{n}")).unwrap();
        }
        for ignored in ["Patch100", "Patch00", "Patchx1", "Otra", ".tmp-Patch07"] {
            fs::create_dir(card.root().join(ignored)).unwrap();
        }
        fs::write(card.root().join("Patch50"), b"un fichero, no una carpeta").unwrap();

        let slots: Vec<u8> = card.list().unwrap().iter().map(|l| l.slot.get()).collect();
        assert_eq!(slots, [2, 11, 20]);
    }

    #[test]
    fn a_damaged_patch_is_reported_without_hiding_the_others() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        card.import_wav(slot(1), &src, "bien").unwrap();
        fs::create_dir(card.root().join("Patch02")).unwrap(); // vacía

        let listing = card.list().unwrap();
        assert!(listing[0].entry.is_ok());
        assert!(listing[1].entry.is_err());
    }

    #[test]
    fn reads_a_patch_written_by_the_pedal() {
        let (_dir, card) = new_card();
        let phrase = card.root().join("Patch24/PhraseA");
        fs::create_dir_all(&phrase).unwrap();
        fs::write(
            card.root().join("Patch24/patch.xml"),
            "<JamManPatch>\n    <PatchName></PatchName>\n    <ID>81017a6a</ID>\n    <Metadata />\n</JamManPatch>\n",
        )
        .unwrap();
        fs::write(
            phrase.join("phrase.xml"),
            "<JamManPhrase>\n    <BeatsPerMinute>51.06</BeatsPerMinute>\n    <BeatsPerMeasure>4</BeatsPerMeasure>\n    <IsLoop>1</IsLoop>\n    <ID>81091b76</ID>\n</JamManPhrase>\n",
        )
        .unwrap();
        fs::write(phrase.join("phrase.wav"), pedal_wav(1000)).unwrap();

        let entry = card.read(slot(24)).unwrap();
        assert_eq!(entry.patch.name, "");
        assert!(entry.phrase.is_loop);
        assert_eq!(entry.audio.data_len, 1000);
    }

    #[test]
    fn first_free_slot_skips_used_ones() {
        let (dir, card) = new_card();
        assert_eq!(card.first_free_slot(), Some(slot(1)));
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        for n in [1, 2, 4] {
            card.import_wav(slot(n), &src, "x").unwrap();
        }
        assert_eq!(card.first_free_slot(), Some(slot(3)));
        for n in 1..=99 {
            if card.read(slot(n)).is_err() {
                card.import_wav(slot(n), &src, "x").unwrap();
            }
        }
        assert_eq!(card.first_free_slot(), None);
    }

    #[test]
    fn rename_changes_only_the_name() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        let before = card.import_wav(slot(7), &src, "viejo").unwrap();
        let xml_path = card.root().join("Patch07/patch.xml");
        let original = fs::read_to_string(&xml_path).unwrap();

        card.rename(slot(7), "Nuevo & mejor").unwrap();

        let after = card.read(slot(7)).unwrap();
        assert_eq!(after.patch.name, "Nuevo & mejor");
        assert_eq!(after.patch.id, before.patch.id);
        let restored = fs::read_to_string(&xml_path)
            .unwrap()
            .replace("Nuevo &amp; mejor", "viejo");
        assert_eq!(restored, original, "solo debe cambiar PatchName");
        assert!(matches!(
            card.rename(slot(8), "x"),
            Err(Error::SlotNotFound(8))
        ));
    }

    #[test]
    fn update_changes_only_the_edited_values() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        let entry = card.import_wav(slot(1), &src, "viejo").unwrap();
        let mut s = LoopSettings::from_entry(&entry);
        s.name = "Nuevo".into();
        s.rhythm_type = RhythmType::Conga.xml().into();
        s.stop_mode = StopMode::Fade.xml().into();
        s.bpm = 96.5;
        s.beats_per_measure = 3;
        s.is_loop = true;
        s.is_reversed = true;
        s.artist = "Hendrix".into();
        s.genre = "Blues".into();
        s.rating = 4;

        let updated = card.update(slot(1), &s).unwrap();
        assert_eq!(LoopSettings::from_entry(&updated), s);
        assert_eq!(updated.patch.id, entry.patch.id, "el GUID no cambia");

        let patch = fs::read_to_string(card.root().join("Patch01/patch.xml")).unwrap();
        assert!(
            patch.contains("\r\n  <RhythmType>Conga</RhythmType>\r\n"),
            "{patch}"
        );
        assert!(
            !patch.contains("\n\n<J") || patch.starts_with("<?xml"),
            "formato intacto"
        );
    }

    #[test]
    fn update_without_changes_does_not_rewrite() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        let entry = card.import_wav(slot(1), &src, "x").unwrap();
        let before = fs::read(card.root().join("Patch01/patch.xml")).unwrap();
        card.update(slot(1), &LoopSettings::from_entry(&entry))
            .unwrap();
        assert_eq!(
            fs::read(card.root().join("Patch01/patch.xml")).unwrap(),
            before
        );
    }

    #[test]
    fn update_fills_metadata_of_a_pedal_recorded_loop() {
        let (_dir, card) = new_card();
        let phrase = card.root().join("Patch24/PhraseA");
        fs::create_dir_all(&phrase).unwrap();
        fs::write(
            card.root().join("Patch24/patch.xml"),
            "<JamManPatch>\n    <PatchName></PatchName>\n    <RhythmType>Silence</RhythmType>\n    <StopMode>StopInstantly</StopMode>\n    <ID>a</ID>\n    <Metadata />\n</JamManPatch>\n",
        )
        .unwrap();
        fs::write(
            phrase.join("phrase.xml"),
            "<JamManPhrase>\n    <BeatsPerMinute>51.06</BeatsPerMinute>\n    <BeatsPerMeasure>4</BeatsPerMeasure>\n    <BpmValidated>1</BpmValidated>\n    <IsLoop>1</IsLoop>\n    <IsReversed>0</IsReversed>\n    <ID>b</ID>\n</JamManPhrase>\n",
        )
        .unwrap();
        fs::write(phrase.join("phrase.wav"), pedal_wav(8)).unwrap();

        let mut s = LoopSettings::from_entry(&card.read(slot(24)).unwrap());
        s.name = "Grabado en el pedal".into();
        s.artist = "Yo".into();
        let e = card.update(slot(24), &s).unwrap();
        assert_eq!(e.patch.name, "Grabado en el pedal");
        assert_eq!(e.patch.artist, "Yo");
        let patch = fs::read_to_string(card.root().join("Patch24/patch.xml")).unwrap();
        assert!(
            patch.contains("    <Metadata>\n        <Artist>Yo</Artist>\n    </Metadata>"),
            "{patch}"
        );
        assert!(
            !patch.contains("<Description>"),
            "no se añaden etiquetas vacías"
        );
        let phrase = fs::read_to_string(phrase.join("phrase.xml")).unwrap();
        assert!(
            phrase.contains("<BpmValidated>1</BpmValidated>"),
            "se conservan campos desconocidos"
        );
    }

    #[test]
    fn update_rejects_out_of_range_settings() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        let entry = card.import_wav(slot(1), &src, "x").unwrap();
        let ok = LoopSettings::from_entry(&entry);
        for bad in [
            LoopSettings {
                bpm: 39.0,
                ..ok.clone()
            },
            LoopSettings {
                beats_per_measure: 16,
                ..ok.clone()
            },
            LoopSettings {
                rating: 6,
                ..ok.clone()
            },
        ] {
            assert!(matches!(
                card.update(slot(1), &bad),
                Err(Error::InvalidSetting(_))
            ));
        }
        assert!(matches!(
            card.update(slot(2), &ok),
            Err(Error::SlotNotFound(2))
        ));
    }

    #[test]
    fn move_and_swap_positions() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        card.import_wav(slot(1), &src, "uno").unwrap();
        card.import_wav(slot(2), &src, "dos").unwrap();

        card.move_to(slot(1), slot(10)).unwrap();
        assert_eq!(card.read(slot(10)).unwrap().patch.name, "uno");
        assert!(matches!(
            card.move_to(slot(10), slot(2)),
            Err(Error::SlotOccupied(2))
        ));
        assert!(matches!(
            card.move_to(slot(1), slot(3)),
            Err(Error::SlotNotFound(1))
        ));

        card.swap(slot(10), slot(2)).unwrap();
        assert_eq!(card.read(slot(2)).unwrap().patch.name, "uno");
        assert_eq!(card.read(slot(10)).unwrap().patch.name, "dos");

        card.swap(slot(2), slot(50)).unwrap(); // destino libre: mueve
        assert_eq!(card.read(slot(50)).unwrap().patch.name, "uno");
        let names: Vec<_> = fs::read_dir(card.root())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 2, "sin restos temporales: {names:?}");
    }

    #[test]
    fn remove_deletes_the_whole_patch() {
        let (dir, card) = new_card();
        let src = write_source(&dir, "a.wav", &pedal_wav(4));
        card.import_wav(slot(9), &src, "x").unwrap();
        card.remove(slot(9)).unwrap();
        assert!(!card.root().join("Patch09").exists());
        assert!(matches!(card.remove(slot(9)), Err(Error::SlotNotFound(9))));
        assert!(matches!(card.read(slot(9)), Err(Error::SlotNotFound(9))));
    }

    #[test]
    fn export_copies_the_audio_and_never_overwrites() {
        let (dir, card) = new_card();
        let audio = pedal_wav(64);
        let src = write_source(&dir, "a.wav", &audio);
        card.import_wav(slot(1), &src, "x").unwrap();

        let dest = dir.path().join("salida.wav");
        assert_eq!(card.export_wav(slot(1), &dest).unwrap(), audio.len() as u64);
        assert_eq!(fs::read(&dest).unwrap(), audio);
        assert!(matches!(card.export_wav(slot(1), &dest), Err(Error::Io(_))));
        assert!(matches!(
            card.export_wav(slot(2), &dir.path().join("otra.wav")),
            Err(Error::SlotNotFound(2))
        ));
    }
}
