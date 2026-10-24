//! Tipo de error común de la librería.

use std::fmt;

/// Errores que puede devolver `jamman-core`.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Número de bucle fuera del rango admitido por el pedal.
    InvalidSlot(u8),
    /// El mensaje recibido o por enviar está incompleto.
    Truncated {
        /// Bytes que se necesitaban.
        needed: usize,
        /// Bytes disponibles.
        available: usize,
    },
    /// Una cadena del protocolo no es UTF-8 válido.
    InvalidUtf8,
    /// El pedal respondió con un tipo de mensaje desconocido.
    UnknownMessageType(u16),
    /// La ruta supera el máximo del protocolo (255 bytes).
    PathTooLong(usize),
    /// El bloque de datos supera el máximo por mensaje.
    ChunkTooLarge(usize),
    /// El mensaje declara una carga mayor que el límite aceptado.
    PayloadTooLarge(u32),
    /// El pedal respondió con un error (`FailureResponse`).
    Device(String),
    /// La respuesta no es del tipo que correspondía a la petición.
    UnexpectedResponse {
        /// Tipo de respuesta esperado.
        expected: &'static str,
        /// Tipo de mensaje recibido.
        got: u16,
    },
    /// El fichero no es un WAV válido o está dañado.
    InvalidWav(&'static str),
    /// El audio no tiene el formato que acepta el pedal.
    UnsupportedAudio(String),
    /// La carpeta indicada no contiene una tarjeta del JamMan (`JamManStereo/`).
    NotACard(std::path::PathBuf),
    /// No hay ningún bucle en esa posición de la tarjeta.
    SlotNotFound(u8),
    /// Ya hay un bucle en esa posición de la tarjeta.
    SlotOccupied(u8),
    /// Los metadatos (`patch.xml` o `phrase.xml`) están incompletos o dañados.
    InvalidMetadata(String),
    /// Un ajuste del bucle está fuera de rango.
    InvalidSetting(String),
    /// La posición tiene ajustes guardados pero ningún audio grabado.
    MissingAudio(u8),
    /// Lo leído directamente de la tarjeta no es lo que se escribió en ese
    /// fichero: la tarjeta parece averiada (ver [`crate::card::verify`]).
    WriteNotPersisted(std::path::PathBuf),
    /// No hay espacio libre suficiente para copiar.
    NoSpace {
        /// Bytes que se necesitan.
        needed: u64,
        /// Bytes libres.
        free: u64,
    },
    /// El almacén del pedal no está disponible (falta la tarjeta).
    StorageUnavailable(crate::pedal::Storage),
    /// No hay ningún JamMan conectado por USB.
    PedalNotFound,
    /// Error del puerto serie al abrirlo o configurarlo.
    Serial(String),
    /// Falló la comunicación con el pedal (leer o escribir en el puerto).
    Connection(std::io::Error),
    /// Error de entrada/salida.
    Io(std::io::Error),
}

impl Error {
    /// `true` si, tras este error, la conversación con el pedal ya no es
    /// fiable (el puerto falló o llegó un mensaje que no encaja) y no tiene
    /// sentido seguir enviándole peticiones.
    pub fn breaks_connection(&self) -> bool {
        matches!(
            self,
            Self::Connection(_)
                | Self::Serial(_)
                | Self::Truncated { .. }
                | Self::InvalidUtf8
                | Self::UnknownMessageType(_)
                | Self::PayloadTooLarge(_)
                | Self::UnexpectedResponse { .. }
        )
    }
}

/// Alias de resultado de la librería.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSlot(n) => write!(f, "posición de bucle inválida: {n} (válido: 1..=99)"),
            Self::Truncated { needed, available } => {
                write!(
                    f,
                    "mensaje incompleto: faltan bytes ({needed} necesarios, {available} disponibles)"
                )
            }
            Self::InvalidUtf8 => write!(f, "cadena del protocolo con UTF-8 inválido"),
            Self::UnknownMessageType(t) => write!(f, "tipo de mensaje desconocido: {t:#06x}"),
            Self::PathTooLong(n) => write!(f, "ruta demasiado larga: {n} bytes (máximo 255)"),
            Self::ChunkTooLarge(n) => write!(f, "bloque demasiado grande: {n} bytes"),
            Self::PayloadTooLarge(n) => write!(f, "carga declarada demasiado grande: {n} bytes"),
            Self::Device(msg) => write!(f, "error del dispositivo: {msg}"),
            Self::UnexpectedResponse { expected, got } => {
                write!(
                    f,
                    "respuesta inesperada: se esperaba {expected}, llegó el tipo {got:#06x}"
                )
            }
            Self::InvalidWav(why) => write!(f, "WAV inválido: {why}"),
            Self::UnsupportedAudio(why) => write!(f, "audio no admitido por el pedal: {why}"),
            Self::NotACard(p) => write!(
                f,
                "{} no contiene una tarjeta JamMan (falta JamManStereo/)",
                p.display()
            ),
            Self::SlotNotFound(n) => write!(f, "no hay ningún bucle en la posición {n}"),
            Self::SlotOccupied(n) => write!(f, "la posición {n} ya está ocupada"),
            Self::InvalidMetadata(why) => write!(f, "metadatos inválidos: {why}"),
            Self::InvalidSetting(why) => write!(f, "ajuste no válido: {why}"),
            Self::MissingAudio(n) => write!(
                f,
                "la posición {n} tiene ajustes guardados pero ningún audio grabado"
            ),
            Self::WriteNotPersisted(p) => write!(
                f,
                "la tarjeta parece averiada: no guarda lo que se escribe ({} no contiene lo \
                 que se copió al leerlo directamente de la tarjeta). Prueba con otra tarjeta",
                p.display()
            ),
            Self::NoSpace { needed, free } => write!(
                f,
                "no hay espacio suficiente: hacen falta {:.1} MB y quedan {:.1} MB libres",
                *needed as f64 / 1e6,
                *free as f64 / 1e6
            ),
            Self::StorageUnavailable(s) => write!(
                f,
                "{} no disponible: ¿está la tarjeta metida en el pedal?",
                s.label()
            ),
            Self::PedalNotFound => write!(
                f,
                "no se encuentra ningún JamMan conectado por USB (1210:0020)"
            ),
            Self::Serial(why) => write!(f, "puerto serie: {why}"),
            Self::Connection(e) if e.kind() == std::io::ErrorKind::TimedOut => write!(
                f,
                "el pedal no responde; si no se recupera, apágalo y vuelve a encenderlo"
            ),
            Self::Connection(e) => write!(f, "comunicación con el pedal: {e}"),
            Self::Io(e) => write!(f, "error de E/S: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) | Self::Connection(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_transport_and_framing_errors_break_the_connection() {
        let timeout = Error::Connection(std::io::Error::from(std::io::ErrorKind::TimedOut));
        assert!(timeout.breaks_connection());
        assert!(timeout.to_string().contains("no responde"));
        let local = Error::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(
            !local.breaks_connection(),
            "un fichero del equipo no corta el pedal"
        );
        assert!(
            Error::Truncated {
                needed: 4,
                available: 0
            }
            .breaks_connection()
        );
        assert!(!Error::Device("The folder does not exist.".into()).breaks_connection());
        assert!(!Error::SlotOccupied(3).breaks_connection());
        assert!(
            !Error::NoSpace {
                needed: 10,
                free: 5
            }
            .breaks_connection()
        );
    }
}
