//! Explorador de ficheros del equipo, independiente de la interfaz.
//!
//! Muestra carpetas y ficheros de audio admitidos (los ocultos no) de una
//! carpeta, permite navegar y seleccionar varios elementos.

use std::path::{Path, PathBuf};

use jamman_audio::SUPPORTED_EXTENSIONS;

use crate::selection::Selection;

/// Elemento de la carpeta actual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    /// Ruta completa.
    pub path: PathBuf,
    /// Nombre para mostrar.
    pub name: String,
    /// `true` si es una carpeta.
    pub is_dir: bool,
    /// Tamaño en bytes (0 en carpetas).
    pub size: u64,
}

/// `true` si la extensión es de un formato que se puede importar.
pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

/// Carpeta personal del usuario.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Carpeta inicial: la de música si existe; si no, la personal.
pub fn default_start() -> PathBuf {
    let Some(home) = home() else {
        return PathBuf::from(".");
    };
    ["Música", "Music"]
        .iter()
        .map(|d| home.join(d))
        .find(|d| d.is_dir())
        .unwrap_or(home)
}

/// Estado del explorador.
#[derive(Debug)]
pub struct LocalBrowser {
    dir: PathBuf,
    entries: Vec<LocalEntry>,
    /// Elementos seleccionados.
    pub selection: Selection<PathBuf>,
    /// Último error al leer una carpeta.
    pub error: Option<String>,
}

impl LocalBrowser {
    /// Abre el explorador en `dir`.
    pub fn new(dir: PathBuf) -> Self {
        let mut browser = Self {
            dir,
            entries: Vec::new(),
            selection: Selection::default(),
            error: None,
        };
        browser.refresh();
        browser
    }

    /// Carpeta actual.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Elementos de la carpeta: carpetas primero y luego ficheros, por nombre.
    pub fn entries(&self) -> &[LocalEntry] {
        &self.entries
    }

    /// Rutas en el orden en que se muestran (para los rangos de selección).
    pub fn order(&self) -> Vec<PathBuf> {
        self.entries.iter().map(|e| e.path.clone()).collect()
    }

    /// Vuelve a leer la carpeta. Conserva la selección de lo que siga existiendo.
    pub fn refresh(&mut self) {
        match read_entries(&self.dir) {
            Ok(entries) => {
                self.entries = entries;
                self.error = None;
            }
            Err(e) => {
                self.entries.clear();
                self.error = Some(format!("No se puede leer {}: {e}", self.dir.display()));
            }
        }
        let paths: Vec<PathBuf> = self.order();
        self.selection.retain(|p| paths.contains(p));
    }

    /// Entra en `dir`. Si no se puede leer, se queda donde estaba y anota el error.
    pub fn navigate(&mut self, dir: PathBuf) {
        match read_entries(&dir) {
            Ok(entries) => {
                self.dir = dir;
                self.entries = entries;
                self.error = None;
                self.selection.clear();
            }
            Err(e) => self.error = Some(format!("No se puede abrir {}: {e}", dir.display())),
        }
    }

    /// Sube a la carpeta superior.
    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            self.navigate(parent.to_path_buf());
        }
    }

    /// Va a la carpeta personal.
    pub fn go_home(&mut self) {
        if let Some(home) = home() {
            self.navigate(home);
        }
    }

    /// Carpeta actual para mostrar, con la carpeta personal abreviada como `~`.
    pub fn display_dir(&self) -> String {
        match home().and_then(|h| self.dir.strip_prefix(&h).ok().map(Path::to_path_buf)) {
            Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
            Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
            None => self.dir.display().to_string(),
        }
    }

    /// Elementos seleccionados, en el orden en que se muestran.
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|e| self.selection.contains(&e.path))
            .map(|e| e.path.clone())
            .collect()
    }
}

fn read_entries(dir: &Path) -> std::io::Result<Vec<LocalEntry>> {
    let mut entries: Vec<LocalEntry> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = e.path();
            // `metadata` sigue los enlaces simbólicos.
            let meta = std::fs::metadata(&path).ok()?;
            let is_dir = meta.is_dir();
            (is_dir || is_audio(&path)).then_some(LocalEntry {
                path,
                name,
                is_dir,
                size: if is_dir { 0 } else { meta.len() },
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::selection::ClickMods;

    fn names(b: &LocalBrowser) -> Vec<&str> {
        b.entries().iter().map(|e| e.name.as_str()).collect()
    }

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("Rock")).unwrap();
        fs::create_dir(dir.path().join("blues")).unwrap();
        fs::create_dir(dir.path().join(".oculta")).unwrap();
        for f in ["b.MP3", "a.wav", "notas.txt", ".oculto.wav", "c.flac"] {
            fs::write(dir.path().join(f), b"xx").unwrap();
        }
        dir
    }

    #[test]
    fn lists_folders_first_then_audio_and_hides_the_rest() {
        let dir = sample();
        let b = LocalBrowser::new(dir.path().to_path_buf());
        assert_eq!(names(&b), ["blues", "Rock", "a.wav", "b.MP3", "c.flac"]);
        assert_eq!(b.entries()[2].size, 2);
    }

    #[test]
    fn navigation_moves_in_and_out() {
        let dir = sample();
        let mut b = LocalBrowser::new(dir.path().to_path_buf());
        b.navigate(dir.path().join("Rock"));
        assert_eq!(b.dir(), dir.path().join("Rock"));
        assert!(b.entries().is_empty());
        b.up();
        assert_eq!(b.dir(), dir.path());
    }

    #[test]
    fn a_folder_that_cannot_be_read_keeps_the_current_one() {
        let dir = sample();
        let mut b = LocalBrowser::new(dir.path().to_path_buf());
        b.navigate(dir.path().join("no-existe"));
        assert_eq!(b.dir(), dir.path());
        assert!(b.error.is_some());
        assert_eq!(b.entries().len(), 5);
    }

    #[test]
    fn selection_survives_refresh_and_follows_visible_order() {
        let dir = sample();
        let mut b = LocalBrowser::new(dir.path().to_path_buf());
        let order = b.order();
        let toggle = ClickMods {
            toggle: true,
            range: false,
        };
        b.selection.click(dir.path().join("c.flac"), toggle, &order);
        b.selection.click(dir.path().join("a.wav"), toggle, &order);
        fs::remove_file(dir.path().join("c.flac")).unwrap();
        fs::write(dir.path().join("d.ogg"), b"x").unwrap();
        b.refresh();
        assert_eq!(b.selected_paths(), [dir.path().join("a.wav")]);
        assert!(names(&b).contains(&"d.ogg"));
    }
}
