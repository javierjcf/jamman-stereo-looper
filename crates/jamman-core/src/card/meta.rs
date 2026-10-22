//! Metadatos de un bucle: `patch.xml` y `PhraseA/phrase.xml`.
//!
//! Al **generar** estos ficheros se reproduce el formato exacto del software
//! de PC (CRLF, sin BOM, indentación de 2 espacios, línea en blanco tras la
//! declaración XML), que es el que el pedal ya lee en las tarjetas existentes.
//! Al **leer** se toleran también los ficheros que escribe el propio pedal.

use super::xml;
use crate::{Error, Result};

const NS_PATCH: &str = "http://schemas.digitech.com/JamMan/Patch";
const NS_PHRASE: &str = "http://schemas.digitech.com/JamMan/Phrase";

/// Valor de `RhythmType` sin ritmo de acompañamiento.
pub const RHYTHM_SILENCE: &str = "Silence";
/// Valor de `StopMode` que detiene la reproducción al instante.
pub const STOP_INSTANTLY: &str = "StopInstantly";

/// Contenido de `patch.xml`.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchMeta {
    /// Nombre del bucle (puede estar vacío en los grabados por el pedal).
    pub name: String,
    /// Ritmo de acompañamiento (`Silence`, `StudioKickAndHighHat`…).
    pub rhythm_type: String,
    /// Modo de parada (`StopInstantly`…).
    pub stop_mode: String,
    /// Versión de los ajustes.
    pub settings_version: u32,
    /// Identificador (GUID) del bucle.
    pub id: String,
    /// GUID del bucle original; solo lo escribe el software de PC.
    pub origin_id: Option<String>,
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

/// Contenido de `PhraseA/phrase.xml`.
#[derive(Debug, Clone, PartialEq)]
pub struct PhraseMeta {
    /// Pulsos por minuto (admite decimales).
    pub bpm: f64,
    /// Pulsos por compás.
    pub beats_per_measure: u32,
    /// `true` si se reproduce en bucle.
    pub is_loop: bool,
    /// `true` si se reproduce del revés.
    pub is_reversed: bool,
    /// Versión de los ajustes.
    pub settings_version: u32,
    /// Versión del audio.
    pub audio_version: u32,
    /// Identificador (GUID) de la frase.
    pub id: String,
}

fn required(doc: &str, tag: &str, file: &str) -> Result<String> {
    xml::text(doc, tag).ok_or_else(|| Error::InvalidMetadata(format!("falta <{tag}> en {file}")))
}

fn number<T: std::str::FromStr>(doc: &str, tag: &str, file: &str) -> Result<T> {
    let raw = required(doc, tag, file)?;
    raw.trim().parse().map_err(|_| {
        Error::InvalidMetadata(format!("<{tag}> no es un número válido en {file}: {raw:?}"))
    })
}

fn flag(doc: &str, tag: &str) -> bool {
    matches!(
        xml::text(doc, tag).as_deref().map(str::trim),
        Some("1" | "true")
    )
}

impl PatchMeta {
    /// Metadatos para un bucle nuevo importado desde un fichero de audio.
    pub fn imported(name: &str, description: &str, id: &str) -> Self {
        Self {
            name: name.into(),
            rhythm_type: RHYTHM_SILENCE.into(),
            stop_mode: STOP_INSTANTLY.into(),
            settings_version: 0,
            id: id.into(),
            origin_id: Some(id.into()),
            description: description.into(),
            artist: String::new(),
            genre: "Unspecified".into(),
            rating: 0,
            tags: String::new(),
        }
    }

    /// Interpreta un `patch.xml`.
    pub fn parse(doc: &str) -> Result<Self> {
        const FILE: &str = "patch.xml";
        let text_or_empty = |tag| xml::text(doc, tag).unwrap_or_default();
        Ok(Self {
            name: text_or_empty("PatchName"),
            rhythm_type: text_or_empty("RhythmType"),
            stop_mode: text_or_empty("StopMode"),
            settings_version: xml::text(doc, "SettingsVersion")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            id: required(doc, "ID", FILE)?,
            origin_id: xml::text(doc, "OriginID"),
            description: text_or_empty("Description"),
            artist: text_or_empty("Artist"),
            genre: text_or_empty("Genre"),
            rating: xml::text(doc, "Rating")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            tags: text_or_empty("Tags"),
        })
    }

    /// Genera el `patch.xml` en el formato del software de PC.
    pub fn render(&self) -> String {
        let e = xml::escape;
        let origin = self
            .origin_id
            .as_ref()
            .map(|o| format!("  <OriginID>{}</OriginID>\r\n", e(o)))
            .unwrap_or_default();
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\r\n\
             <JamManPatch device=\"JamManStereo\" version=\"1\" xmlns=\"{NS_PATCH}\">\r\n  \
             <PatchName>{}</PatchName>\r\n  \
             <RhythmType>{}</RhythmType>\r\n  \
             <StopMode>{}</StopMode>\r\n  \
             <SettingsVersion>{}</SettingsVersion>\r\n  \
             <ID>{}</ID>\r\n\
             {origin}  \
             <Metadata>\r\n    \
             <Description>{}</Description>\r\n    \
             <Artist>{}</Artist>\r\n    \
             <Genre>{}</Genre>\r\n    \
             <Rating>{}</Rating>\r\n    \
             <Tags>{}</Tags>\r\n  \
             </Metadata>\r\n\
             </JamManPatch>\r\n",
            e(&self.name),
            e(&self.rhythm_type),
            e(&self.stop_mode),
            self.settings_version,
            e(&self.id),
            e(&self.description),
            e(&self.artist),
            e(&self.genre),
            self.rating,
            e(&self.tags),
        )
    }
}

impl PhraseMeta {
    /// Metadatos de frase para un bucle importado (120 BPM, 4/4).
    pub fn imported(id: &str) -> Self {
        Self {
            bpm: 120.0,
            beats_per_measure: 4,
            is_loop: false,
            is_reversed: false,
            settings_version: 0,
            audio_version: 0,
            id: id.into(),
        }
    }

    /// Interpreta un `phrase.xml`.
    pub fn parse(doc: &str) -> Result<Self> {
        const FILE: &str = "phrase.xml";
        Ok(Self {
            bpm: number(doc, "BeatsPerMinute", FILE)?,
            beats_per_measure: number(doc, "BeatsPerMeasure", FILE)?,
            is_loop: flag(doc, "IsLoop"),
            is_reversed: flag(doc, "IsReversed"),
            settings_version: xml::text(doc, "SettingsVersion")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            audio_version: xml::text(doc, "AudioVersion")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            id: required(doc, "ID", FILE)?,
        })
    }

    /// Genera el `phrase.xml` en el formato del software de PC.
    pub fn render(&self) -> String {
        let b = |v: bool| u8::from(v);
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\r\n\
             <JamManPhrase xmlns=\"{NS_PHRASE}\" version=\"1\">\r\n  \
             <BeatsPerMinute>{}</BeatsPerMinute>\r\n  \
             <BeatsPerMeasure>{}</BeatsPerMeasure>\r\n  \
             <IsLoop>{}</IsLoop>\r\n  \
             <IsReversed>{}</IsReversed>\r\n  \
             <SettingsVersion>{}</SettingsVersion>\r\n  \
             <AudioVersion>{}</AudioVersion>\r\n  \
             <ID>{}</ID>\r\n\
             </JamManPhrase>\r\n",
            self.bpm,
            self.beats_per_measure,
            b(self.is_loop),
            b(self.is_reversed),
            self.settings_version,
            self.audio_version,
            xml::escape(&self.id),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "337c3976-2dec-4c32-a4b9-46a9d4004d42";
    const NAME: &str = "01 Nirvana - Smells like teen spirit BT (vocals)";
    const DESC: &str = "Imported from D:/GUITARRA/BACKING TRACKS/BT Canciones/01 Nirvana - Smells like teen spirit BT (vocals).wav";

    /// `patch.xml` real de una tarjeta (721 bytes, CRLF).
    fn real_patch_xml() -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\r\n\
             <JamManPatch device=\"JamManStereo\" version=\"1\" xmlns=\"http://schemas.digitech.com/JamMan/Patch\">\r\n  \
             <PatchName>{NAME}</PatchName>\r\n  \
             <RhythmType>Silence</RhythmType>\r\n  \
             <StopMode>StopInstantly</StopMode>\r\n  \
             <SettingsVersion>0</SettingsVersion>\r\n  \
             <ID>{ID}</ID>\r\n  \
             <OriginID>{ID}</OriginID>\r\n  \
             <Metadata>\r\n    \
             <Description>{DESC}</Description>\r\n    \
             <Artist></Artist>\r\n    \
             <Genre>Unspecified</Genre>\r\n    \
             <Rating>0</Rating>\r\n    \
             <Tags></Tags>\r\n  \
             </Metadata>\r\n\
             </JamManPatch>\r\n"
        )
    }

    /// `phrase.xml` real de la misma tarjeta (392 bytes, CRLF).
    fn real_phrase_xml() -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\r\n\
             <JamManPhrase xmlns=\"http://schemas.digitech.com/JamMan/Phrase\" version=\"1\">\r\n  \
             <BeatsPerMinute>120</BeatsPerMinute>\r\n  \
             <BeatsPerMeasure>4</BeatsPerMeasure>\r\n  \
             <IsLoop>0</IsLoop>\r\n  \
             <IsReversed>0</IsReversed>\r\n  \
             <SettingsVersion>0</SettingsVersion>\r\n  \
             <AudioVersion>0</AudioVersion>\r\n  \
             <ID>{ID}</ID>\r\n\
             </JamManPhrase>\r\n"
        )
    }

    #[test]
    fn real_files_have_the_sizes_seen_on_the_card() {
        assert_eq!(real_patch_xml().len(), 721);
        assert_eq!(real_phrase_xml().len(), 392);
    }

    #[test]
    fn rendering_an_imported_patch_reproduces_the_original_bytes() {
        let meta = PatchMeta::imported(NAME, DESC, ID);
        assert_eq!(meta.render(), real_patch_xml());
        assert_eq!(PhraseMeta::imported(ID).render(), real_phrase_xml());
    }

    #[test]
    fn parse_then_render_is_the_identity_for_pc_files() {
        let meta = PatchMeta::parse(&real_patch_xml()).unwrap();
        assert_eq!(meta.name, NAME);
        assert_eq!(meta.origin_id.as_deref(), Some(ID));
        assert_eq!(meta.render(), real_patch_xml());
        let phrase = PhraseMeta::parse(&real_phrase_xml()).unwrap();
        assert_eq!(phrase.render(), real_phrase_xml());
    }

    #[test]
    fn special_characters_survive_a_roundtrip() {
        let meta = PatchMeta::imported("Rock & Roll <live> \"v2\"", "ñandú's", ID);
        assert_eq!(PatchMeta::parse(&meta.render()).unwrap(), meta);
    }

    #[test]
    fn parses_the_style_written_by_the_pedal() {
        let patch = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n\
            <JamManPatch xmlns=\"http://schemas.digitech.com/JamMan/Patch\" device=\"JamManStereo\" version=\"1\">\n\
            \x20   <PatchName></PatchName>\n    <RhythmType>StudioKickAndHighHat</RhythmType>\n\
            \x20   <StopMode>StopInstantly</StopMode>\n    <SettingsVersion>1</SettingsVersion>\n\
            \x20   <ID>81017a6a-1dd6-11b2-bd63-77c859c64f3c</ID>\n    <Metadata />\n</JamManPatch>\n";
        let meta = PatchMeta::parse(patch).unwrap();
        assert_eq!(meta.name, "");
        assert_eq!(meta.rhythm_type, "StudioKickAndHighHat");
        assert_eq!(meta.settings_version, 1);
        assert_eq!(meta.origin_id, None);

        let phrase = "<JamManPhrase>\n    <BeatsPerMinute>51.0638313293</BeatsPerMinute>\n\
            \x20   <BeatsPerMeasure>4</BeatsPerMeasure>\n    <BpmValidated>1</BpmValidated>\n\
            \x20   <IsLoop>1</IsLoop>\n    <IsReversed>0</IsReversed>\n    <SettingsVersion>1</SettingsVersion>\n\
            \x20   <AudioVersion>1</AudioVersion>\n    <ID>81091b76</ID>\n    <Metadata />\n</JamManPhrase>\n";
        let phrase = PhraseMeta::parse(phrase).unwrap();
        assert!((phrase.bpm - 51.063_831_329_3).abs() < 1e-9);
        assert!(phrase.is_loop && !phrase.is_reversed);
        assert_eq!(phrase.audio_version, 1);
    }

    #[test]
    fn fractional_bpm_roundtrips() {
        let mut p = PhraseMeta::imported(ID);
        p.bpm = 51.063_831_329_3;
        assert_eq!(PhraseMeta::parse(&p.render()).unwrap().bpm, p.bpm);
    }

    #[test]
    fn missing_mandatory_fields_are_reported() {
        assert!(matches!(
            PatchMeta::parse("<JamManPatch/>"),
            Err(Error::InvalidMetadata(_))
        ));
        assert!(matches!(
            PhraseMeta::parse("<JamManPhrase/>"),
            Err(Error::InvalidMetadata(_))
        ));
        let bad_bpm =
            "<BeatsPerMinute>rápido</BeatsPerMinute><BeatsPerMeasure>4</BeatsPerMeasure><ID>x</ID>";
        assert!(matches!(
            PhraseMeta::parse(bad_bpm),
            Err(Error::InvalidMetadata(_))
        ));
    }
}
