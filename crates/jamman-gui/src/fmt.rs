//! Formato de cantidades para mostrar.

/// `m:ss` o `h:mm:ss`.
pub fn duration(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Bytes en MB o GB (base 1000, como los fabricantes de tarjetas).
pub fn bytes(n: u64) -> String {
    let n = n as f64;
    if n >= 1e9 {
        format!("{:.1} GB", n / 1e9)
    } else {
        format!("{:.0} MB", n / 1e6)
    }
}

/// Tempo sin decimales innecesarios.
pub fn bpm(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(duration(304.2), "5:04");
        assert_eq!(duration(11_454.0), "3:10:54");
        assert_eq!(bytes(2_020_000_000), "2.0 GB");
        assert_eq!(bytes(53_000_000), "53 MB");
        assert_eq!(bpm(120.0), "120");
        assert_eq!(bpm(51.063_831), "51.1");
    }
}
