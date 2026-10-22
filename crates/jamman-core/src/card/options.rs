//! Opciones de reproducción y catálogo de un bucle.
//!
//! Los valores XML salen de las cadenas del software original y las
//! descripciones, del manual del pedal (páginas 36, 40, 45 y 48).

use std::ops::RangeInclusive;

/// Tempo admitido, en BPM ("The value must be between 40 & 240").
pub const BPM_RANGE: RangeInclusive<f64> = 40.0..=240.0;

/// Tiempos por compás admitidos (manual, "Tipo de compás").
pub const BEATS_PER_MEASURE_RANGE: RangeInclusive<u32> = 2..=15;

/// Valoración máxima (estrellas).
pub const MAX_RATING: u8 = 5;

/// Géneros que ofrece el software original.
pub const GENRES: [&str; 14] = [
    "Unspecified",
    "Alternative",
    "Blues",
    "Country",
    "Folk",
    "Funk",
    "Fusion",
    "Hip Hop",
    "Jazz",
    "Latin",
    "Metal",
    "New Age",
    "Rock",
    "Techno",
];

/// Pista guía de ritmo (manual, página 48: `oF` y `r1`..`r9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RhythmType {
    /// `oF`: sin ritmo.
    Off,
    /// `r1`: bloque de madera.
    WoodBlocks,
    /// `r2`: baquetas.
    Sticks,
    /// `r3`: clic.
    Click,
    /// `r4`: bombo / hi-hat alternativo.
    AltKickHiHat,
    /// `r5`: bombo / hi-hat de estudio.
    StudioKickHiHat,
    /// `r6`: bombo / hi-hat techno.
    TechnoKickHiHat,
    /// `r7`: cencerro.
    Cowbell,
    /// `r8`: conga.
    Conga,
    /// `r9`: pandereta.
    Tambourine,
}

impl RhythmType {
    /// Todos los ritmos en el orden del pedal.
    pub const ALL: [Self; 10] = [
        Self::Off,
        Self::WoodBlocks,
        Self::Sticks,
        Self::Click,
        Self::AltKickHiHat,
        Self::StudioKickHiHat,
        Self::TechnoKickHiHat,
        Self::Cowbell,
        Self::Conga,
        Self::Tambourine,
    ];

    /// Valor en `patch.xml`.
    pub fn xml(self) -> &'static str {
        match self {
            Self::Off => "Silence",
            Self::WoodBlocks => "WoodBlocks",
            Self::Sticks => "Sticks",
            Self::Click => "Click",
            Self::AltKickHiHat => "AlternativeKickAndHighHat",
            Self::StudioKickHiHat => "StudioKickAndHighHat",
            Self::TechnoKickHiHat => "TechnoKickAndHighHat",
            Self::Cowbell => "Cowbell",
            Self::Conga => "Conga",
            Self::Tambourine => "Tambourine",
        }
    }

    /// Texto para mostrar, con el código que aparece en la pantalla del pedal.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "oF · Sin ritmo",
            Self::WoodBlocks => "r1 · Bloque de madera",
            Self::Sticks => "r2 · Baquetas",
            Self::Click => "r3 · Clic",
            Self::AltKickHiHat => "r4 · Bombo/hi-hat alternativo",
            Self::StudioKickHiHat => "r5 · Bombo/hi-hat de estudio",
            Self::TechnoKickHiHat => "r6 · Bombo/hi-hat techno",
            Self::Cowbell => "r7 · Cencerro",
            Self::Conga => "r8 · Conga",
            Self::Tambourine => "r9 · Pandereta",
        }
    }

    /// Interpreta el valor de `patch.xml`.
    pub fn from_xml(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.xml() == value.trim())
    }
}

/// Modo de parada (manual, página 36).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopMode {
    /// Stop: se detiene al pulsar.
    Stop,
    /// Finish: termina la vuelta y se detiene.
    Finish,
    /// Fade: fundido hasta silencio.
    Fade,
}

impl StopMode {
    /// Todos los modos.
    pub const ALL: [Self; 3] = [Self::Stop, Self::Finish, Self::Fade];

    /// Valor en `patch.xml`.
    pub fn xml(self) -> &'static str {
        match self {
            Self::Stop => "StopInstantly",
            Self::Finish => "StopAtEndOfLoop",
            Self::Fade => "FadeOut",
        }
    }

    /// Texto para mostrar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stop => "Stop · inmediato",
            Self::Finish => "Finish · al terminar",
            Self::Fade => "Fade · fundido",
        }
    }

    /// Interpreta el valor de `patch.xml`.
    pub fn from_xml(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.xml() == value.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_values_roundtrip() {
        for r in RhythmType::ALL {
            assert_eq!(RhythmType::from_xml(r.xml()), Some(r));
        }
        for m in StopMode::ALL {
            assert_eq!(StopMode::from_xml(m.xml()), Some(m));
        }
    }

    #[test]
    fn values_seen_on_a_real_card_are_known() {
        assert_eq!(RhythmType::from_xml("Silence"), Some(RhythmType::Off));
        assert_eq!(
            RhythmType::from_xml("StudioKickAndHighHat"),
            Some(RhythmType::StudioKickHiHat)
        );
        assert_eq!(StopMode::from_xml("StopInstantly"), Some(StopMode::Stop));
        assert_eq!(RhythmType::from_xml("Desconocido"), None);
    }

    #[test]
    fn ranges_match_the_manual() {
        assert!(BPM_RANGE.contains(&120.0) && !BPM_RANGE.contains(&241.0));
        assert!(BEATS_PER_MEASURE_RANGE.contains(&4) && !BEATS_PER_MEASURE_RANGE.contains(&1));
    }
}
