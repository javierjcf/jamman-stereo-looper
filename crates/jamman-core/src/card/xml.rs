//! Lector y escritor XML mínimo para los ficheros `patch.xml` y `phrase.xml`.
//!
//! Los ficheros de la tarjeta son planos y con nombres de etiqueta únicos, así
//! que basta con localizar `<Etiqueta>texto</Etiqueta>`. Se toleran los dos
//! estilos observados: el del software de PC (etiquetas vacías como
//! `<Artist></Artist>`) y el del propio pedal (`<Metadata />` autocerrado y
//! otra indentación). No es un analizador XML general: no soporta atributos
//! con significado, CDATA ni espacios de nombres con prefijo.

/// Devuelve el texto (con las entidades resueltas) de la primera etiqueta
/// `name`. `None` si no existe; `Some("")` si está vacía o autocerrada.
pub(crate) fn text(doc: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(rel) = doc[from..].find(&open) {
        let start = from + rel + open.len();
        from = start;
        // La etiqueta debe terminar justo ahí: `<ID>` no es `<IDX>`.
        let rest = &doc[start..];
        let next = rest.chars().next()?;
        if !(next == '>' || next == '/' || next.is_whitespace()) {
            continue;
        }
        let tag_end = rest.find('>')?;
        if rest[..tag_end].ends_with('/') {
            return Some(String::new());
        }
        let body = &rest[tag_end + 1..];
        let close = format!("</{name}>");
        let end = body.find(&close)?;
        return Some(unescape(&body[..end]));
    }
    None
}

/// Resuelve las entidades XML predefinidas y las numéricas.
pub(crate) fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').and_then(|end| {
            let ch = match &rest[1..end] {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                num => num
                    .strip_prefix("#x")
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| num.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((ch, end + 1))
        });
        match decoded {
            Some((ch, used)) => {
                out.push(ch);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Escapa los cinco caracteres especiales de XML.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PC_STYLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>

<JamManPatch device="JamManStereo" version="1" xmlns="http://schemas.digitech.com/JamMan/Patch">
  <PatchName>01 Nirvana &amp; Co</PatchName>
  <ID>aaa</ID>
  <OriginID>bbb</OriginID>
  <Metadata>
    <Artist></Artist>
  </Metadata>
</JamManPatch>
"#;

    const PEDAL_STYLE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<JamManPatch xmlns="http://schemas.digitech.com/JamMan/Patch" device="JamManStereo" version="1">
    <PatchName></PatchName>
    <ID>ccc</ID>
    <Metadata />
</JamManPatch>
"#;

    #[test]
    fn reads_text_and_resolves_entities() {
        assert_eq!(text(PC_STYLE, "PatchName").unwrap(), "01 Nirvana & Co");
    }

    #[test]
    fn id_does_not_match_origin_id() {
        assert_eq!(text(PC_STYLE, "ID").unwrap(), "aaa");
        assert_eq!(text(PC_STYLE, "OriginID").unwrap(), "bbb");
    }

    #[test]
    fn empty_and_self_closing_tags_are_empty_strings() {
        assert_eq!(text(PC_STYLE, "Artist").unwrap(), "");
        assert_eq!(text(PEDAL_STYLE, "PatchName").unwrap(), "");
        assert_eq!(text(PEDAL_STYLE, "Metadata").unwrap(), "");
    }

    #[test]
    fn missing_tag_is_none() {
        assert_eq!(text(PEDAL_STYLE, "OriginID"), None);
        assert_eq!(text("", "ID"), None);
    }

    #[test]
    fn unescape_handles_numeric_and_unknown_entities() {
        assert_eq!(unescape("a&#65;&#x42;&zzz;c &"), "aAB&zzz;c &");
    }

    #[test]
    fn escape_then_unescape_is_identity() {
        let s = r#"a & b < c > "d" 'e' ñ"#;
        assert_eq!(unescape(&escape(s)), s);
    }
}
