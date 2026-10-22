//! Utilidades mínimas para extraer opciones de la línea de comandos.

/// Extrae `--opción valor` de los argumentos si está presente.
pub fn take_option(args: &mut Vec<String>, name: &str) -> Result<Option<String>, String> {
    let Some(i) = args.iter().position(|a| a == name) else {
        return Ok(None);
    };
    if i + 1 >= args.len() {
        return Err(format!("falta el valor de {name}"));
    }
    let value = args.remove(i + 1);
    args.remove(i);
    Ok(Some(value))
}

/// Extrae un indicador sin valor (`--yes`). Devuelve si estaba presente.
pub fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != name);
    args.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn extracts_the_option_and_its_value() {
        let mut a = args(&["ls", "--port", "/dev/ttyACM0", "/"]);
        assert_eq!(
            take_option(&mut a, "--port").unwrap().as_deref(),
            Some("/dev/ttyACM0")
        );
        assert_eq!(a, ["ls", "/"]);
    }

    #[test]
    fn missing_option_is_none() {
        let mut a = args(&["about"]);
        assert_eq!(take_option(&mut a, "--port").unwrap(), None);
    }

    #[test]
    fn option_without_value_is_an_error() {
        let mut a = args(&["ls", "--port"]);
        assert!(take_option(&mut a, "--port").is_err());
    }

    #[test]
    fn flag_is_removed_and_reported() {
        let mut a = args(&["rm", "--yes", "5"]);
        assert!(take_flag(&mut a, "--yes"));
        assert_eq!(a, ["rm", "5"]);
        assert!(!take_flag(&mut a, "--yes"));
    }
}
