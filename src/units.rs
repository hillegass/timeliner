//! Lengths with units. Everything is converted to points.

/// Parses a length such as `"12pt"`, `"0.5in"`, or `"3px"` into points.
/// `pixels_per_point` is needed to convert `px`.
pub fn parse_length(s: &str, pixels_per_point: f64) -> Result<f64, String> {
    let s = s.trim();
    let split = s
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| format!("\"{s}\" needs a unit (pt, px, in, mm, cm)"))?;
    let (num, unit) = s.split_at(split);
    let value: f64 = num
        .trim()
        .parse()
        .map_err(|_| format!("\"{s}\" is not a number followed by a unit"))?;
    if !value.is_finite() {
        return Err(format!("\"{s}\" is not a finite length"));
    }
    let per_unit = match unit {
        "pt" => 1.0,
        "in" => 72.0,
        "cm" => 72.0 / 2.54,
        "mm" => 72.0 / 25.4,
        "px" => 1.0 / pixels_per_point,
        _ => {
            return Err(format!(
                "\"{s}\" has unknown unit \"{unit}\" (use pt, px, in, mm, or cm)"
            ));
        }
    };
    Ok(value * per_unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_units() {
        assert_eq!(parse_length("12pt", 2.0), Ok(12.0));
        assert_eq!(parse_length("1in", 2.0), Ok(72.0));
        assert_eq!(parse_length("2.54cm", 2.0), Ok(72.0));
        assert_eq!(parse_length("25.4mm", 2.0), Ok(72.0));
        assert_eq!(parse_length("4px", 2.0), Ok(2.0));
        assert_eq!(parse_length(" 0.5 in ", 2.0), Ok(36.0));
    }

    #[test]
    fn rejects_bare_numbers_and_bad_units() {
        assert!(
            parse_length("12", 2.0)
                .unwrap_err()
                .contains("needs a unit")
        );
        assert!(
            parse_length("12em", 2.0)
                .unwrap_err()
                .contains("unknown unit")
        );
        assert!(parse_length("pt", 2.0).is_err());
    }
}
