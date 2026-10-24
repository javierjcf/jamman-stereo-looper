//! Operaciones comunes a cualquier sitio donde viven bucles: una tarjeta SD
//! abierta en el PC ([`Card`]) o un almacén del pedal por USB
//! ([`PedalStore`]).
//!
//! Las interfaces (CLI y GUI) trabajan con [`LoopStore`] y no necesitan saber
//! dónde están los bucles. Lo que solo tiene sentido en un sitio (mover o
//! renombrar en la tarjeta, comprobar si el pedal tiene la tarjeta puesta) se
//! queda en el tipo concreto.

use std::io::{Read, Write};
use std::path::Path;

use crate::Result;
use crate::card::{Card, Entry, Listing, LoopSettings};
use crate::device::Slot;
use crate::pedal::PedalStore;

/// Aviso de progreso: `(bytes hechos, bytes totales)`.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// Bucles de un almacén, sea cual sea.
pub trait LoopStore {
    /// Todas las posiciones ocupadas, con su bucle o el error al leerlo.
    fn list(&mut self) -> Result<Vec<Listing>>;

    /// Lee un bucle.
    fn read(&mut self, slot: Slot) -> Result<Entry>;

    /// Posiciones ocupadas, sin leer los bucles.
    fn slots(&mut self) -> Result<Vec<Slot>>;

    /// Primera posición libre, si queda alguna.
    fn first_free_slot(&mut self) -> Result<Option<Slot>>;

    /// Añade un WAV ya en el formato del pedal en una posición libre.
    /// `original` es el fichero que eligió el usuario (para la descripción).
    fn import_wav_from(
        &mut self,
        slot: Slot,
        wav: &Path,
        name: &str,
        original: &Path,
        progress: Progress,
    ) -> Result<Entry>;

    /// Copia el audio de un bucle a `dest` sin sobrescribir nada.
    fn export_wav(&mut self, slot: Slot, dest: &Path, progress: Progress) -> Result<u64>;

    /// Borra un bucle.
    fn remove(&mut self, slot: Slot) -> Result<()>;

    /// Guarda los ajustes de un bucle.
    fn update(&mut self, slot: Slot, settings: &LoopSettings) -> Result<Entry>;

    /// Copia un bucle completo a una posición libre de una tarjeta local.
    fn backup_loop(
        &mut self,
        slot: Slot,
        dest: &Card,
        dest_slot: Slot,
        progress: Progress,
    ) -> Result<()>;

    /// Trae un bucle de una tarjeta local (una copia) a una posición libre.
    fn restore_loop(
        &mut self,
        source: &Card,
        source_slot: Slot,
        slot: Slot,
        progress: Progress,
    ) -> Result<()>;

    /// Espacio `(libres, totales)` en bytes, si el almacén lo sabe.
    fn space(&mut self) -> Result<Option<(u64, u64)>> {
        Ok(None)
    }

    /// Comprueba que un bucle recién escrito, cuyo audio sale de
    /// `audio_source`, está de verdad en el almacén. Solo la tarjeta del PC
    /// puede leerse sin caché (ver [`crate::card::verify`]); el pedal, no.
    fn verify_loop(&mut self, _slot: Slot, _audio_source: &Path) -> Result<()> {
        Ok(())
    }
}

impl LoopStore for Card {
    fn list(&mut self) -> Result<Vec<Listing>> {
        Card::list(self)
    }

    fn read(&mut self, slot: Slot) -> Result<Entry> {
        Card::read(self, slot)
    }

    fn slots(&mut self) -> Result<Vec<Slot>> {
        Ok(Card::list(self)?.into_iter().map(|l| l.slot).collect())
    }

    fn first_free_slot(&mut self) -> Result<Option<Slot>> {
        Ok(Card::first_free_slot(self))
    }

    fn import_wav_from(
        &mut self,
        slot: Slot,
        wav: &Path,
        name: &str,
        original: &Path,
        progress: Progress,
    ) -> Result<Entry> {
        Card::import_wav_from(self, slot, wav, name, original, progress)
    }

    fn export_wav(&mut self, slot: Slot, dest: &Path, progress: Progress) -> Result<u64> {
        Card::export_wav(self, slot, dest, progress)
    }

    fn remove(&mut self, slot: Slot) -> Result<()> {
        Card::remove(self, slot)
    }

    fn update(&mut self, slot: Slot, settings: &LoopSettings) -> Result<Entry> {
        Card::update(self, slot, settings)
    }

    fn backup_loop(
        &mut self,
        slot: Slot,
        dest: &Card,
        dest_slot: Slot,
        progress: Progress,
    ) -> Result<()> {
        Card::backup_loop(self, slot, dest, dest_slot, progress)
    }

    fn restore_loop(
        &mut self,
        source: &Card,
        source_slot: Slot,
        slot: Slot,
        progress: Progress,
    ) -> Result<()> {
        Card::restore_loop(self, source, source_slot, slot, progress)
    }

    fn verify_loop(&mut self, slot: Slot, audio_source: &Path) -> Result<()> {
        Card::verify_loop(self, slot, audio_source)
    }
}

impl<T: Read + Write> LoopStore for PedalStore<T> {
    fn list(&mut self) -> Result<Vec<Listing>> {
        PedalStore::list(self)
    }

    fn read(&mut self, slot: Slot) -> Result<Entry> {
        PedalStore::read(self, slot)
    }

    fn slots(&mut self) -> Result<Vec<Slot>> {
        PedalStore::slots(self)
    }

    fn first_free_slot(&mut self) -> Result<Option<Slot>> {
        PedalStore::first_free_slot(self)
    }

    fn import_wav_from(
        &mut self,
        slot: Slot,
        wav: &Path,
        name: &str,
        original: &Path,
        progress: Progress,
    ) -> Result<Entry> {
        PedalStore::import_wav_from(self, slot, wav, name, original, progress)
    }

    fn export_wav(&mut self, slot: Slot, dest: &Path, progress: Progress) -> Result<u64> {
        PedalStore::export_wav(self, slot, dest, progress)
    }

    fn remove(&mut self, slot: Slot) -> Result<()> {
        PedalStore::remove(self, slot)
    }

    fn update(&mut self, slot: Slot, settings: &LoopSettings) -> Result<Entry> {
        PedalStore::update(self, slot, settings)
    }

    fn backup_loop(
        &mut self,
        slot: Slot,
        dest: &Card,
        dest_slot: Slot,
        progress: Progress,
    ) -> Result<()> {
        PedalStore::backup_loop(self, slot, dest, dest_slot, progress)
    }

    fn restore_loop(
        &mut self,
        source: &Card,
        source_slot: Slot,
        slot: Slot,
        progress: Progress,
    ) -> Result<()> {
        PedalStore::restore_loop(self, source, source_slot, slot, progress)
    }

    fn space(&mut self) -> Result<Option<(u64, u64)>> {
        PedalStore::space(self).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::Error;
    use crate::card::testutil::pedal_wav;
    use crate::pedal::Storage;
    use crate::protocol::Client;
    use crate::protocol::fake::FakePedal;

    fn slot(n: u8) -> Slot {
        Slot::new(n).unwrap()
    }

    /// El mismo recorrido para cualquier almacén: así la tarjeta y el pedal
    /// no pueden divergir sin que se note.
    fn exercise(store: &mut dyn LoopStore) {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("a.wav");
        let audio = pedal_wav(3_000);
        fs::write(&wav, &audio).unwrap();

        assert!(store.list().unwrap().is_empty());
        let free = store.first_free_slot().unwrap().unwrap();
        assert_eq!(free, slot(1));
        let entry = store
            .import_wav_from(free, &wav, "Uno", &wav, &mut |_, _| {})
            .unwrap();
        assert_eq!(entry.patch.name, "Uno");
        assert_eq!(store.read(free).unwrap().patch.id, entry.patch.id);
        assert!(matches!(
            store.import_wav_from(free, &wav, "Otro", &wav, &mut |_, _| {}),
            Err(Error::SlotOccupied(1))
        ));
        assert_eq!(store.slots().unwrap(), [slot(1)]);
        assert_eq!(store.first_free_slot().unwrap(), Some(slot(2)));

        let mut settings = LoopSettings::from_entry(&entry);
        settings.name = "Renombrado".into();
        assert_eq!(
            store.update(free, &settings).unwrap().patch.name,
            "Renombrado"
        );

        let out = dir.path().join("salida.wav");
        store.export_wav(free, &out, &mut |_, _| {}).unwrap();
        assert_eq!(fs::read(&out).unwrap(), audio);

        let backup = Card::create(&dir.path().join("copia")).unwrap();
        store
            .backup_loop(free, &backup, slot(7), &mut |_, _| {})
            .unwrap();
        store
            .restore_loop(&backup, slot(7), slot(9), &mut |_, _| {})
            .unwrap();
        assert_eq!(store.slots().unwrap(), [slot(1), slot(9)]);

        store.remove(free).unwrap();
        assert!(matches!(store.remove(free), Err(Error::SlotNotFound(1))));
        let names: Vec<String> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|l| l.entry.unwrap().patch.name)
            .collect();
        assert_eq!(names, ["Renombrado"]);
    }

    #[test]
    fn a_card_behaves_as_a_loop_store() {
        let dir = tempfile::tempdir().unwrap();
        let mut card = Card::create(dir.path()).unwrap();
        exercise(&mut card);
        assert_eq!(LoopStore::space(&mut card).unwrap(), None);
    }

    #[test]
    fn the_pedal_behaves_as_a_loop_store() {
        let mut store = PedalStore::new(Client::new(FakePedal::new()), Storage::Card);
        exercise(&mut store);
        assert!(LoopStore::space(&mut store).unwrap().is_some());
    }
}
