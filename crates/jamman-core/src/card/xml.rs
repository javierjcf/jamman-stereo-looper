//! Lector y escritor XML mínimo para los ficheros `patch.xml` y `phrase.xml`.
//!
//! Los ficheros de la tarjeta son planos y con nombres de etiqueta únicos, así
//! que basta con localizar `<Etiqueta>texto</Etiqueta>`. Se toleran los dos
//! estilos observados: el del software de PC (etiquetas vacías como
//! `<Artist></Artist>`) y el del propio pedal (`<Metadata />` autocerrado y
//! otra indentación). No es un analizador XML general: no soporta atributos
//! con significado, CDATA ni espacios de nombres con prefijo.

/// Posición de una etiqueta dentro del documento.
enum Found {
    /// Etiqueta autocerrada (`<X />`): rango de la etiqueta completa.
    SelfClosed { start: usize, end: usize },
    /// Etiqueta con contenido: rango del texto entre apertura y cierre.
    Body { start: usize, end: usize },
}

/// Localiza la primera etiqueta llamada exactamente `name`.
fn locate(doc: &str, name: &str) -> Option<Found> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(rel) = doc[from..].find(&open) {
        let tag_start = from + rel;
        let after_name = tag_start + open.len();
        from = after_name;
        // La etiqueta debe terminar justo ahí: `<ID>` no es `<IDX>`.
        let rest = &doc[after_name..];
        let next = rest.chars().next()?;
        if !(next == '>' || next == '/' || next.is_whitespace()) {
            continue;
        }
        let tag_end = rest.find('>')?;
        if rest[..tag_end].ends_with('/') {
            return Some(Found::SelfClosed {
                start: tag_start,
                end: after_name + tag_end + 1,
            });
        }
        let body_start = after_name + tag_end + 1;
        let end = doc[body_start..].find(&format!("</{name}>"))?;
        return Some(Found::Body {
            start: body_start,
            end: body_start + end,
        });
    }
    None
}

/// Devuelve el texto (con las entidades resueltas) de la primera etiqueta
/// `name`. `None` si no existe; `Some("")` si está vacía o autocerrada.
pub(crate) fn text(doc: &str, name: &str) -> Option<String> {
    match locate(doc, name)? {
        Found::SelfClosed { .. } => Some(String::new()),
        Found::Body { start, end } => Some(unescape(&doc[start..end])),
    }
}

/// Sustituye el texto de la primera etiqueta `name` conservando el resto del
/// documento tal cual. `None` si la etiqueta no existe.
pub(crate) fn replace_text(doc: &str, name: &str, value: &str) -> Option<String> {
    let value = escape(value);
    Some(match locate(doc, name)? {
        Found::SelfClosed { start, end } => {
            format!("{}<{name}>{value}</{name}>{}", &doc[..start], &doc[end..])
        }
        Found::Body { start, end } => format!("{}{value}{}", &doc[..start], &doc[end..]),
    })
}

/// Fija el texto de la etiqueta `name`. Si no existe, la crea como última hija
/// de `parent` respetando el estilo del documento (salto de línea e
/// indentación: CRLF y 2 espacios en el software de PC, LF y 4 en el pedal). Si `parent` está autocerrado
/// (`<Metadata />`) lo expande. `None` si tampoco existe `parent`.
pub(crate) fn set_text(doc: &str, name: &str, value: &str, parent: &str) -> Option<String> {
    if let Some(updated) = replace_text(doc, name, value) {
        return Some(updated);
    }
    let crlf = doc.contains("\r\n");
    let nl = if crlf { "\r\n" } else { "\n" };
    let unit = indent_unit(doc);
    let element = format!("<{name}>{}</{name}>", escape(value));

    match locate(doc, parent)? {
        Found::SelfClosed { start, end } => {
            let indent = line_indent(doc, start);
            Some(format!(
                "{}<{parent}>{nl}{indent}{unit}{element}{nl}{indent}</{parent}>{}",
                &doc[..start],
                &doc[end..]
            ))
        }
        Found::Body { end, .. } => {
            // `end` apunta a `</parent>`; se inserta al principio de su línea.
            let line_start = doc[..end].rfind('\n').map_or(0, |i| i + 1);
            let indent = line_indent(doc, end);
            let insert_at = if doc[line_start..end].trim().is_empty() {
                line_start
            } else {
                end
            };
            let (before, after) = doc.split_at(insert_at);
            let prefix = if insert_at == end { nl } else { "" };
            Some(format!(
                "{before}{prefix}{indent}{unit}{element}{nl}{after}"
            ))
        }
    }
}

/// Unidad de indentación del documento: la de la primera línea indentada.
fn indent_unit(doc: &str) -> String {
    doc.lines()
        .map(|l| l.chars().take_while(|c| *c == ' ').count())
        .find(|&n| n > 0)
        .map_or_else(|| "  ".to_owned(), |n| " ".repeat(n))
}

/// Espacios al principio de la línea que contiene `pos`.
fn line_indent(doc: &str, pos: usize) -> String {
    let line_start = doc[..pos].rfind('\n').map_or(0, |i| i + 1);
    doc[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
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
    fn replace_text_changes_only_the_target() {
        let out = replace_text(PC_STYLE, "PatchName", "Nuevo & mejor").unwrap();
        assert_eq!(text(&out, "PatchName").unwrap(), "Nuevo & mejor");
        assert_eq!(
            out.replace("Nuevo &amp; mejor", "01 Nirvana &amp; Co"),
            PC_STYLE
        );
    }

    #[test]
    fn replace_text_fills_empty_and_self_closed_tags() {
        let out = replace_text(PEDAL_STYLE, "PatchName", "Canción").unwrap();
        assert_eq!(text(&out, "PatchName").unwrap(), "Canción");
        let out = replace_text(PEDAL_STYLE, "Metadata", "x").unwrap();
        assert!(out.contains("<Metadata>x</Metadata>"));
        assert_eq!(replace_text(PEDAL_STYLE, "NoExiste", "x"), None);
    }

    #[test]
    fn set_text_replaces_when_the_tag_exists() {
        let out = set_text(PC_STYLE, "Artist", "Hendrix", "Metadata").unwrap();
        assert_eq!(
            out,
            PC_STYLE.replace("<Artist></Artist>", "<Artist>Hendrix</Artist>")
        );
    }

    #[test]
    fn set_text_expands_a_self_closed_parent_in_pedal_style() {
        let out = set_text(PEDAL_STYLE, "Artist", "Hendrix", "Metadata").unwrap();
        assert!(
            out.contains("    <Metadata>\n        <Artist>Hendrix</Artist>\n    </Metadata>\n"),
            "{out}"
        );
        assert_eq!(text(&out, "ID").unwrap(), "ccc");
    }

    #[test]
    fn set_text_appends_inside_an_existing_parent() {
        let out = set_text(PC_STYLE, "Genre", "Blues", "Metadata").unwrap();
        assert!(
            out.contains("    <Artist></Artist>\n    <Genre>Blues</Genre>\n  </Metadata>"),
            "{out}"
        );
        let crlf = PC_STYLE.replace('\n', "\r\n");
        let out = set_text(&crlf, "Genre", "Blues", "Metadata").unwrap();
        assert!(
            out.contains("    <Genre>Blues</Genre>\r\n  </Metadata>"),
            "{out}"
        );
    }

    #[test]
    fn set_text_without_parent_is_none() {
        assert_eq!(set_text(PEDAL_STYLE, "X", "1", "NoExiste"), None);
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
