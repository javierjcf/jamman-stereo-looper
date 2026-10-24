//! Estado de la aplicación, independiente de la interfaz gráfica.
//!
//! Aquí vive todo lo que no es dibujar: qué tarjeta está abierta, qué bucles
//! tiene, cuál está seleccionado, el filtro, el orden y los cambios sin
//! guardar. Las vistas de `egui` solo leen este estado y llaman a sus métodos,
//! de modo que otra interfaz podría reutilizarlo tal cual.

use std::path::{Path, PathBuf};

use jamman_core::Result;

use crate::selection::{ClickMods, Selection};
use jamman_core::card::{Card, Entry, Listing, LoopSettings};
use jamman_core::device::Slot;
use jamman_core::pedal::Storage;

/// Una fila de la tabla: un bucle de la tarjeta.
#[derive(Debug, Clone)]
pub struct Row {
    /// Posición.
    pub slot: Slot,
    /// Ajustes; `None` si el bucle está dañado.
    pub settings: Option<LoopSettings>,
    /// Duración en segundos.
    pub duration_secs: f64,
    /// Bytes de audio.
    pub audio_bytes: u64,
    /// Motivo por el que no se pudo leer, si está dañado.
    pub error: Option<String>,
}

impl Row {
    fn from_entry(e: &Entry) -> Self {
        Self {
            slot: e.slot,
            settings: Some(LoopSettings::from_entry(e)),
            duration_secs: e.audio.duration_secs(),
            audio_bytes: e.audio.data_len,
            error: None,
        }
    }

    /// Nombre para mostrar.
    pub fn name(&self) -> &str {
        match &self.settings {
            Some(s) if !s.name.is_empty() => &s.name,
            Some(_) => "(sin nombre)",
            None => "(dañado)",
        }
    }

    fn matches(&self, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        let in_text = |t: &str| t.to_lowercase().contains(needle);
        format!("{:02}", self.slot.get()).contains(needle)
            || in_text(self.name())
            || self
                .settings
                .as_ref()
                .is_some_and(|s| in_text(&s.artist) || in_text(&s.tags) || in_text(&s.genre))
    }
}

/// Criterio de orden de la tabla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// Por posición.
    #[default]
    Slot,
    /// Por nombre.
    Name,
    /// Por duración.
    Duration,
    /// Por tempo.
    Bpm,
}

/// Ajustes en edición de un bucle.
#[derive(Debug, Clone)]
pub struct Draft {
    /// Bucle que se edita.
    pub slot: Slot,
    /// Ajustes guardados.
    pub original: LoopSettings,
    /// Ajustes con los cambios del usuario.
    pub edited: LoopSettings,
}

impl Draft {
    /// `true` si hay cambios sin guardar.
    pub fn is_dirty(&self) -> bool {
        self.original != self.edited
    }
}

/// Origen de los bucles que se muestran.
#[derive(Debug, Clone)]
enum Source {
    /// Tarjeta (o copia) en el PC: se lee directamente.
    Card {
        /// Tarjeta abierta.
        card: Card,
        /// Ruta que se abrió (punto de montaje o carpeta).
        mount: PathBuf,
    },
    /// Un almacén del pedal por USB: la lista llega en segundo plano.
    Pedal(Storage),
}

/// Biblioteca: la tarjeta abierta y la vista sobre sus bucles.
#[derive(Debug, Default)]
pub struct Library {
    source: Option<Source>,
    /// `true` mientras se espera la lista del pedal.
    pub loading: bool,
    rows: Vec<Row>,
    selection: Selection<Slot>,
    /// Texto de búsqueda.
    pub filter: String,
    /// Columna de orden.
    pub sort: SortKey,
    /// Orden descendente.
    pub sort_desc: bool,
    /// Ajustes en edición del bucle seleccionado.
    pub draft: Option<Draft>,
}

impl Library {
    /// Abre una tarjeta (punto de montaje o carpeta `JamManStereo`).
    pub fn open(&mut self, path: &Path) -> Result<()> {
        let card = Card::open(path)?;
        self.source = Some(Source::Card {
            card,
            mount: path.to_path_buf(),
        });
        self.loading = false;
        self.selection.clear();
        self.draft = None;
        self.reload()
    }

    /// Muestra un almacén del pedal. La lista se rellena con
    /// [`Library::set_listing`] cuando llega del USB.
    pub fn open_pedal(&mut self, storage: Storage) {
        self.source = Some(Source::Pedal(storage));
        self.rows.clear();
        self.selection.clear();
        self.draft = None;
        self.loading = true;
    }

    /// Almacén del pedal mostrado, si es el pedal.
    pub fn pedal(&self) -> Option<Storage> {
        match self.source {
            Some(Source::Pedal(s)) => Some(s),
            _ => None,
        }
    }

    /// `true` si hay algo abierto (tarjeta o pedal).
    pub fn is_open(&self) -> bool {
        self.source.is_some()
    }

    /// Tarjeta abierta.
    pub fn card(&self) -> Option<&Card> {
        match &self.source {
            Some(Source::Card { card, .. }) => Some(card),
            _ => None,
        }
    }

    /// Punto de montaje que se abrió.
    pub fn mount(&self) -> Option<&Path> {
        match &self.source {
            Some(Source::Card { mount, .. }) => Some(mount),
            _ => None,
        }
    }

    /// Vuelve a leer la tarjeta. Conserva la selección de los bucles que sigan
    /// ahí y los cambios sin guardar del bucle principal.
    pub fn reload(&mut self) -> Result<()> {
        // El pedal se recarga en segundo plano (ver `set_listing`).
        let Some(card) = self.card() else {
            return Ok(());
        };
        let listing = card.list()?;
        self.set_listing(listing);
        Ok(())
    }

    /// Sustituye las filas por una lista de bucles leída (de la tarjeta o del
    /// pedal). Conserva la selección y los cambios sin guardar del principal.
    pub fn set_listing(&mut self, listing: Vec<Listing>) {
        self.loading = false;
        self.rows = listing
            .into_iter()
            .map(|l| match l.entry {
                Ok(e) => Row::from_entry(&e),
                Err(err) => Row {
                    slot: l.slot,
                    settings: None,
                    duration_secs: 0.0,
                    audio_bytes: 0,
                    error: Some(err.to_string()),
                },
            })
            .collect();
        let existing: Vec<Slot> = self.rows.iter().map(|r| r.slot).collect();
        self.selection.retain(|s| existing.contains(s));
        let keep_edits =
            self.has_unsaved_changes() && self.draft.as_ref().map(|d| d.slot) == self.selected();
        if !keep_edits {
            self.rebuild_draft();
        }
    }

    /// Todas las filas, por posición.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Fila de una posición.
    pub fn row(&self, slot: Slot) -> Option<&Row> {
        self.rows.iter().find(|r| r.slot == slot)
    }

    /// Filas que cumplen el filtro, en el orden elegido.
    pub fn visible(&self) -> Vec<&Row> {
        let needle = self.filter.trim().to_lowercase();
        let mut rows: Vec<&Row> = self.rows.iter().filter(|r| r.matches(&needle)).collect();
        let bpm = |r: &Row| r.settings.as_ref().map_or(0.0, |s| s.bpm);
        rows.sort_by(|a, b| {
            let order = match self.sort {
                SortKey::Slot => a.slot.cmp(&b.slot),
                SortKey::Name => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
                SortKey::Duration => a.duration_secs.total_cmp(&b.duration_secs),
                SortKey::Bpm => bpm(a).total_cmp(&bpm(b)),
            };
            order.then(a.slot.cmp(&b.slot))
        });
        if self.sort_desc {
            rows.reverse();
        }
        rows
    }

    /// Cambia el orden: misma columna invierte el sentido.
    pub fn sort_by(&mut self, key: SortKey) {
        if self.sort == key {
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort = key;
            self.sort_desc = false;
        }
    }

    /// Bucle principal de la selección: el que se edita y se reproduce.
    pub fn selected(&self) -> Option<Slot> {
        self.selection.primary().copied()
    }

    /// Selección completa.
    pub fn selection(&self) -> &Selection<Slot> {
        &self.selection
    }

    /// Posiciones seleccionadas, en orden ascendente.
    pub fn selected_slots(&self) -> Vec<Slot> {
        self.selection.items().copied().collect()
    }

    /// Selecciona solo un bucle (o ninguno) y prepara sus ajustes para editar.
    /// Los cambios sin guardar del anterior se descartan: la interfaz debe
    /// preguntar antes si [`Draft::is_dirty`].
    pub fn select(&mut self, slot: Option<Slot>) {
        match slot {
            Some(s) => self.selection.set_single(s),
            None => self.selection.clear(),
        }
        self.rebuild_draft();
    }

    /// Aplica un clic (con Ctrl o Mayús) sobre la tabla tal como se ve.
    /// Si cambia el bucle principal, se preparan sus ajustes.
    pub fn click(&mut self, slot: Slot, mods: ClickMods) {
        let order: Vec<Slot> = self.visible().iter().map(|r| r.slot).collect();
        self.selection.click(slot, mods, &order);
        if self.draft.as_ref().map(|d| d.slot) != self.selected() {
            self.rebuild_draft();
        }
    }

    /// Selecciona todos los bucles visibles.
    pub fn select_all_visible(&mut self) {
        let order: Vec<Slot> = self.visible().iter().map(|r| r.slot).collect();
        self.selection.select_all(&order);
        if self.draft.as_ref().map(|d| d.slot) != self.selected() {
            self.rebuild_draft();
        }
    }

    fn rebuild_draft(&mut self) {
        self.draft = self
            .selected()
            .and_then(|s| self.row(s))
            .and_then(|r| r.settings.clone().map(|settings| (r.slot, settings)))
            .map(|(slot, settings)| Draft {
                slot,
                original: settings.clone(),
                edited: settings,
            });
    }

    /// `true` si hay cambios sin guardar.
    pub fn has_unsaved_changes(&self) -> bool {
        self.draft.as_ref().is_some_and(Draft::is_dirty)
    }

    /// Marca los ajustes en edición como guardados (se guardan en segundo
    /// plano); la próxima lista trae los valores nuevos.
    pub fn mark_draft_saved(&mut self) {
        if let Some(d) = &mut self.draft {
            d.original = d.edited.clone();
        }
    }

    /// Descarta los cambios sin guardar.
    pub fn discard_draft(&mut self) {
        if let Some(d) = &mut self.draft {
            d.edited = d.original.clone();
        }
    }

    /// Duración total de los bucles, en segundos.
    pub fn total_duration_secs(&self) -> f64 {
        self.rows.iter().map(|r| r.duration_secs).sum()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use jamman_core::card::RhythmType;

    fn slot(n: u8) -> Slot {
        Slot::new(n).unwrap()
    }

    /// WAV del pedal con `n` bytes de audio.
    fn wav(n: usize) -> Vec<u8> {
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(36 + n as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&44_100u32.to_le_bytes());
        out.extend_from_slice(&176_400u32.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(n as u32).to_le_bytes());
        out.extend(std::iter::repeat_n(0u8, n));
        out
    }

    /// Tarjeta temporal con bucles `(posición, nombre, bytes de audio)`.
    fn library_with(loops: &[(u8, &str, usize)]) -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let card = Card::create(dir.path()).unwrap();
        for (i, (n, name, bytes)) in loops.iter().enumerate() {
            let src = dir.path().join(format!("src{i}.wav"));
            fs::write(&src, wav(*bytes)).unwrap();
            card.import_wav(slot(*n), &src, name).unwrap();
        }
        let mut lib = Library::default();
        lib.open(dir.path()).unwrap();
        (dir, lib)
    }

    fn names(lib: &Library) -> Vec<String> {
        lib.visible().iter().map(|r| r.name().to_owned()).collect()
    }

    #[test]
    fn opening_a_card_lists_its_loops() {
        let (_d, lib) = library_with(&[(3, "c", 4), (1, "a", 4)]);
        assert_eq!(names(&lib), ["a", "c"]);
        assert!(lib.card().is_some());
    }

    #[test]
    fn filter_matches_name_artist_and_slot_ignoring_case() {
        let (_d, mut lib) = library_with(&[(1, "Sweet Home", 4), (12, "Blues A", 4)]);
        lib.filter = "sweet".into();
        assert_eq!(names(&lib), ["Sweet Home"]);
        lib.filter = "12".into();
        assert_eq!(names(&lib), ["Blues A"]);
        lib.filter = "nada".into();
        assert!(lib.visible().is_empty());
    }

    #[test]
    fn sorting_by_the_same_column_toggles_direction() {
        let (_d, mut lib) = library_with(&[(1, "b", 400), (2, "a", 4), (3, "c", 40)]);
        lib.sort_by(SortKey::Name);
        assert_eq!(names(&lib), ["a", "b", "c"]);
        lib.sort_by(SortKey::Name);
        assert_eq!(names(&lib), ["c", "b", "a"]);
        lib.sort_by(SortKey::Duration);
        assert_eq!(names(&lib), ["a", "c", "b"]);
    }

    #[test]
    fn editing_and_saving_a_draft_updates_the_card() {
        let (_d, mut lib) = library_with(&[(5, "viejo", 4)]);
        lib.select(Some(slot(5)));
        assert!(!lib.has_unsaved_changes());

        let draft = lib.draft.as_mut().unwrap();
        draft.edited.name = "nuevo".into();
        draft.edited.rhythm_type = RhythmType::Click.xml().into();
        assert!(lib.has_unsaved_changes());

        // Como la app: se guarda en segundo plano y luego llega la lista.
        let edited = lib.draft.as_ref().unwrap().edited.clone();
        lib.card().unwrap().update(slot(5), &edited).unwrap();
        lib.mark_draft_saved();
        lib.reload().unwrap();
        assert!(!lib.has_unsaved_changes(), "tras guardar no hay cambios");
        assert_eq!(lib.row(slot(5)).unwrap().name(), "nuevo");
        assert_eq!(lib.selected(), Some(slot(5)), "se conserva la selección");
    }

    #[test]
    fn discarding_restores_the_saved_settings() {
        let (_d, mut lib) = library_with(&[(1, "x", 4)]);
        lib.select(Some(slot(1)));
        lib.draft.as_mut().unwrap().edited.bpm = 90.0;
        lib.discard_draft();
        assert!(!lib.has_unsaved_changes());
    }

    #[test]
    fn reload_drops_the_selection_of_a_removed_loop() {
        let (_d, mut lib) = library_with(&[(1, "x", 4), (2, "y", 4)]);
        lib.select(Some(slot(2)));
        lib.card().unwrap().remove(slot(2)).unwrap();
        lib.reload().unwrap();
        assert_eq!(lib.selected(), None);
        assert!(lib.draft.is_none());
    }

    #[test]
    fn multiple_selection_keeps_editing_the_primary_loop() {
        let (_d, mut lib) = library_with(&[(1, "a", 4), (2, "b", 4), (3, "c", 4)]);
        let ctrl = ClickMods {
            toggle: true,
            range: false,
        };
        let shift = ClickMods {
            toggle: false,
            range: true,
        };
        lib.click(slot(1), ClickMods::default());
        lib.click(slot(3), shift);
        assert_eq!(lib.selected_slots(), [slot(1), slot(2), slot(3)]);
        lib.click(slot(2), ctrl);
        assert_eq!(lib.selected_slots(), [slot(1), slot(3)]);
        assert_eq!(lib.draft.as_ref().unwrap().slot, lib.selected().unwrap());
        lib.select_all_visible();
        assert_eq!(lib.selection().len(), 3);
    }

    #[test]
    fn reload_keeps_unsaved_edits_of_the_primary_loop() {
        let (_d, mut lib) = library_with(&[(1, "a", 4)]);
        lib.select(Some(slot(1)));
        lib.draft.as_mut().unwrap().edited.name = "editando".into();
        lib.reload().unwrap();
        assert_eq!(lib.draft.as_ref().unwrap().edited.name, "editando");
        assert!(lib.has_unsaved_changes());
    }

    #[test]
    fn a_pedal_listing_arrives_later_and_keeps_the_selection() {
        let (_d, source) = library_with(&[(1, "a", 4), (2, "b", 4)]);
        let listing = || source.card().unwrap().list().unwrap();

        let mut lib = Library::default();
        lib.open_pedal(Storage::Card);
        assert!(lib.loading && lib.rows().is_empty());
        assert_eq!(lib.pedal(), Some(Storage::Card));
        lib.set_listing(listing());
        assert!(!lib.loading);
        assert_eq!(names(&lib), ["a", "b"]);

        lib.select(Some(slot(2)));
        lib.draft.as_mut().unwrap().edited.name = "editando".into();
        lib.set_listing(listing());
        assert_eq!(lib.selected(), Some(slot(2)));
        assert_eq!(lib.draft.as_ref().unwrap().edited.name, "editando");
        lib.mark_draft_saved();
        assert!(!lib.has_unsaved_changes());
        assert!(lib.reload().is_ok(), "recargar el pedal no hace nada aquí");
    }

    #[test]
    fn damaged_loops_are_listed_but_not_editable() {
        let (_d, mut lib) = library_with(&[(1, "x", 4)]);
        fs::create_dir(lib.card().unwrap().root().join("Patch07")).unwrap();
        lib.reload().unwrap();
        let broken = lib.row(slot(7)).unwrap();
        assert!(broken.error.is_some());
        assert_eq!(broken.name(), "(dañado)");
        lib.select(Some(slot(7)));
        assert!(lib.draft.is_none());
    }
}
