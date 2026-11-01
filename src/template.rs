//! `{field}` / `{field:spec}` substitution, with Python's format-spec
//! mini-language for numbers and strings. Python's behavior is the reference.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Literal(String),
    Field { name: String, spec: Option<String> },
}

/// Splits text into literals and fields. `{{` and `}}` are literal braces.
pub fn parse_pieces(text: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut lit = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                lit.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                lit.push('}');
            }
            '}' => return Err("single '}' is not allowed; write '}}' for a literal brace".into()),
            '{' => {
                let mut field = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') => {
                            return Err(format!(
                                "nested '{{' in field \"{{{field}\"; formats can't contain fields"
                            ));
                        }
                        Some(c) => field.push(c),
                        None => {
                            return Err(format!(
                                "unclosed field \"{{{field}\"; write '{{{{' for a literal brace"
                            ));
                        }
                    }
                }
                let (name, spec) = match field.split_once(':') {
                    Some((n, s)) => (n, Some(s.to_string())),
                    None => (field.as_str(), None),
                };
                let name = name.trim();
                if name.is_empty() {
                    return Err("empty field \"{}\"; put a field name inside the braces".into());
                }
                if name.contains('!') {
                    return Err(format!("conversions like \"{name}\" are not supported"));
                }
                if !lit.is_empty() {
                    pieces.push(Piece::Literal(std::mem::take(&mut lit)));
                }
                pieces.push(Piece::Field {
                    name: name.to_string(),
                    spec,
                });
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        pieces.push(Piece::Literal(lit));
    }
    Ok(pieces)
}

/// Formats a JSON field value the way Python's `format(value, spec)` would.
/// Booleans display as `true` / `false` and take string specs.
pub fn format_value(v: &Value, spec: &str) -> Result<String, String> {
    let s = Spec::parse(spec)?;
    match v {
        Value::String(text) => format_str(text, &s),
        Value::Bool(b) => format_str(if *b { "true" } else { "false" }, &s),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                format_int(i.into(), &s)
            } else if let Some(u) = n.as_u64() {
                format_int(u.into(), &s)
            } else {
                format_float(n.as_f64().unwrap_or(f64::NAN), &s)
            }
        }
        other => Err(format!("can't format {other}")),
    }
}

#[derive(Debug, Default)]
struct Spec {
    fill: Option<char>,
    align: Option<char>,
    sign: Option<char>,
    z: bool,
    alt: bool,
    zero: bool,
    width: usize,
    grouping: Option<char>,
    precision: Option<usize>,
    ty: Option<char>,
}

impl Spec {
    fn parse(spec: &str) -> Result<Spec, String> {
        let c: Vec<char> = spec.chars().collect();
        let mut i = 0;
        let mut s = Spec::default();
        let is_align = |ch: char| matches!(ch, '<' | '>' | '=' | '^');
        if c.len() >= 2 && is_align(c[1]) {
            s.fill = Some(c[0]);
            s.align = Some(c[1]);
            i = 2;
        } else if !c.is_empty() && is_align(c[0]) {
            s.align = Some(c[0]);
            i = 1;
        }
        if i < c.len() && matches!(c[i], '+' | '-' | ' ') {
            s.sign = Some(c[i]);
            i += 1;
        }
        if i < c.len() && c[i] == 'z' {
            s.z = true;
            i += 1;
        }
        if i < c.len() && c[i] == '#' {
            s.alt = true;
            i += 1;
        }
        if i < c.len() && c[i] == '0' {
            s.zero = true;
            i += 1;
        }
        let digits = |i: &mut usize| -> Option<usize> {
            let start = *i;
            while *i < c.len() && c[*i].is_ascii_digit() {
                *i += 1;
            }
            (start < *i).then(|| {
                c[start..*i]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .unwrap_or(usize::MAX)
            })
        };
        s.width = digits(&mut i).unwrap_or(0);
        if i < c.len() && matches!(c[i], ',' | '_') {
            s.grouping = Some(c[i]);
            i += 1;
        }
        if i < c.len() && c[i] == '.' {
            i += 1;
            s.precision =
                Some(digits(&mut i).ok_or_else(|| {
                    format!("format \"{spec}\" is missing a precision after '.'")
                })?);
        }
        if i < c.len() {
            s.ty = Some(c[i]);
            i += 1;
        }
        if i < c.len() {
            return Err(format!("invalid format \"{spec}\""));
        }
        if s.width > 1000 || s.precision.is_some_and(|p| p > 1000) {
            return Err(format!(
                "format \"{spec}\" asks for more than 1000 characters"
            ));
        }
        // The '0' flag means zero fill, sign-aware unless an alignment is given.
        if s.zero {
            s.fill.get_or_insert('0');
            s.align.get_or_insert('=');
        }
        Ok(s)
    }
}

fn format_str(text: &str, s: &Spec) -> Result<String, String> {
    if !matches!(s.ty, None | Some('s')) {
        return Err(format!(
            "format code '{}' needs a number, but the value is text",
            s.ty.unwrap()
        ));
    }
    if s.sign.is_some() {
        return Err("a sign is not allowed when formatting text".into());
    }
    if s.alt || s.z {
        return Err("'#' and 'z' are not allowed when formatting text".into());
    }
    if s.grouping.is_some() {
        return Err("',' and '_' need a number, but the value is text".into());
    }
    if s.align == Some('=') && !s.zero {
        return Err("'=' alignment needs a number, but the value is text".into());
    }
    let body: String = match s.precision {
        Some(p) => text.chars().take(p).collect(),
        None => text.to_string(),
    };
    // '=' only got here through the '0' flag; Python left-aligns text then.
    let align = match s.align {
        Some('=') | None => '<',
        Some(a) => a,
    };
    Ok(pad("", &body, s.fill.unwrap_or(' '), align, s.width))
}

fn format_int(n: i128, s: &Spec) -> Result<String, String> {
    let ty = s.ty.unwrap_or('d');
    if matches!(ty, 'e' | 'E' | 'f' | 'F' | 'g' | 'G' | '%') {
        return format_float(n as f64, s);
    }
    if s.precision.is_some() {
        return Err(
            "precision is not allowed for integers; use a type like 'f' (e.g. \".2f\")".into(),
        );
    }
    if s.z {
        return Err("'z' is not allowed for integers".into());
    }
    let (radix, prefix) = match ty {
        'd' | 'n' => (10, ""),
        'b' => (2, "0b"),
        'o' => (8, "0o"),
        'x' => (16, "0x"),
        'X' => (16, "0X"),
        'c' => {
            if s.sign.is_some() || s.alt || s.grouping.is_some() {
                return Err("'c' can't be combined with a sign, '#', or grouping".into());
            }
            let ch = u32::try_from(n)
                .ok()
                .and_then(char::from_u32)
                .ok_or("'c' needs a valid character code")?;
            return Ok(pad(
                "",
                &ch.to_string(),
                s.fill.unwrap_or(' '),
                s.align.unwrap_or('<'),
                s.width,
            ));
        }
        's' => return Err("format code 's' needs text, but the value is a number".into()),
        t => return Err(format!("unknown format code '{t}' for a number")),
    };
    if s.grouping == Some(',') && radix != 10 {
        return Err(format!("',' can't be used with '{ty}'"));
    }
    let digits = to_radix(n.unsigned_abs(), radix, ty == 'X');
    let group_size = if radix == 10 { 3 } else { 4 };
    let sign = sign_text(n < 0, s.sign);
    let prefix = if s.alt { prefix } else { "" };
    Ok(finish(
        &format!("{sign}{prefix}"),
        &digits,
        "",
        s,
        group_size,
    ))
}

fn format_float(x: f64, s: &Spec) -> Result<String, String> {
    let ty = s.ty;
    if let Some(t) = ty
        && !matches!(t, 'e' | 'E' | 'f' | 'F' | 'g' | 'G' | 'n' | '%')
    {
        return Err(match t {
            'd' => "format code 'd' needs a whole number; use 'f' for numbers with decimals (e.g. \",.0f\")".into(),
            's' => "format code 's' needs text, but the value is a number".into(),
            t => format!("unknown format code '{t}' for a number with decimals"),
        });
    }
    let upper = matches!(ty, Some('E'..='G'));
    let mut negative = x.is_sign_negative() && !x.is_nan();
    let a = x.abs();

    let (body, suffix) = if !a.is_finite() {
        let t = if a.is_nan() { "nan" } else { "inf" };
        (
            if upper {
                t.to_uppercase()
            } else {
                t.to_string()
            },
            "",
        )
    } else {
        match ty {
            Some('f' | 'F') => (fixed(a, s.precision.unwrap_or(6), s.alt), ""),
            Some('%') => (fixed(a * 100.0, s.precision.unwrap_or(6), s.alt), "%"),
            Some('e' | 'E') => (sci(a, s.precision.unwrap_or(6), s.alt, upper), ""),
            Some('g' | 'G' | 'n') => (
                general(a, s.precision.unwrap_or(6), s.alt, upper, false),
                "",
            ),
            None => match s.precision {
                Some(p) => (general(a, p, s.alt, false, true), ""),
                None => (repr(a), ""),
            },
            Some(_) => unreachable!(),
        }
    };
    if s.z && negative && body.chars().all(|c| matches!(c, '0' | '.')) {
        negative = false;
    }
    let sign = sign_text(negative, s.sign);
    // Group only the integer digits; keep the fraction and exponent as is.
    let split = body
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(body.len());
    let (int, rest) = body.split_at(split);
    let rest = format!("{rest}{suffix}");
    if int.is_empty() {
        return Ok(pad(
            &sign,
            &rest,
            s.fill.unwrap_or(' '),
            s.align.unwrap_or('>'),
            s.width,
        ));
    }
    Ok(finish(&sign, int, &rest, s, 3))
}

/// Applies grouping and padding to a number split into sign/prefix, integer
/// digits, and the rest (fraction, exponent, '%').
fn finish(head: &str, int_digits: &str, rest: &str, s: &Spec, group_size: usize) -> String {
    let fill = s.fill.unwrap_or(' ');
    let align = s.align.unwrap_or('>');
    let mut digits = int_digits.to_string();
    if let Some(sep) = s.grouping {
        // With the '0' flag, Python zero-pads inside the grouping: 00,001,234.
        if s.zero && align == '=' && fill == '0' {
            while head.len() + group(&digits, sep, group_size).len() + rest.chars().count()
                < s.width
            {
                digits.insert(0, '0');
            }
        }
        digits = group(&digits, sep, group_size);
    }
    pad(head, &format!("{digits}{rest}"), fill, align, s.width)
}

fn group(digits: &str, sep: char, size: usize) -> String {
    let mut out = String::new();
    let n = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(size) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

fn pad(head: &str, body: &str, fill: char, align: char, width: usize) -> String {
    let len = head.chars().count() + body.chars().count();
    if len >= width {
        return format!("{head}{body}");
    }
    let n = width - len;
    let fills = |k: usize| std::iter::repeat_n(fill, k).collect::<String>();
    match align {
        '<' => format!("{head}{body}{}", fills(n)),
        '^' => format!("{}{head}{body}{}", fills(n / 2), fills(n - n / 2)),
        '=' => format!("{head}{}{body}", fills(n)),
        _ => format!("{}{head}{body}", fills(n)),
    }
}

fn sign_text(negative: bool, sign: Option<char>) -> String {
    match (negative, sign) {
        (true, _) => "-".into(),
        (false, Some('+')) => "+".into(),
        (false, Some(' ')) => " ".into(),
        _ => String::new(),
    }
}

fn to_radix(mut n: u128, radix: u128, upper: bool) -> String {
    if n == 0 {
        return "0".into();
    }
    let digits = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut out = Vec::new();
    while n > 0 {
        out.push(digits[(n % radix) as usize]);
        n /= radix;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// Fixed-point with `p` decimals (correctly rounded, ties to even like Python).
fn fixed(a: f64, p: usize, alt: bool) -> String {
    let mut s = format!("{a:.p$}");
    if alt && p == 0 {
        s.push('.');
    }
    s
}

/// Splits Rust's `{:.Ne}` output into mantissa and exponent.
fn sci_parts(a: f64, p: usize) -> (String, i32) {
    let s = format!("{a:.p$e}");
    let (m, e) = s.split_once('e').unwrap();
    (m.to_string(), e.parse().unwrap())
}

fn exp_text(e: i32, upper: bool) -> String {
    let sign = if e < 0 { '-' } else { '+' };
    format!("{}{sign}{:02}", if upper { 'E' } else { 'e' }, e.abs())
}

fn sci(a: f64, p: usize, alt: bool, upper: bool) -> String {
    let (mut m, e) = sci_parts(a, p);
    if alt && p == 0 {
        m.push('.');
    }
    format!("{m}{}", exp_text(e, upper))
}

/// Python's 'g', and the no-type form when a precision is given. The no-type
/// form switches to scientific notation one exponent earlier and always
/// keeps a digit after the decimal point in fixed notation.
fn general(a: f64, p: usize, alt: bool, upper: bool, no_type: bool) -> String {
    let p = p.max(1);
    let (mantissa, exp) = sci_parts(a, p - 1);
    let limit = if no_type { p as i32 - 1 } else { p as i32 };
    let strip = |s: String| -> String {
        if alt || !s.contains('.') {
            s
        } else {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    };
    if -4 <= exp && exp < limit {
        let decimals = (p as i32 - 1 - exp) as usize;
        let mut out = strip(format!("{a:.decimals$}"));
        if no_type && !out.contains('.') {
            out.push_str(".0");
        }
        if alt && !out.contains('.') {
            out.push('.');
        }
        out
    } else {
        let mut m = strip(mantissa);
        if alt && !m.contains('.') {
            m.push('.');
        }
        format!("{m}{}", exp_text(exp, upper))
    }
}

/// Python's `repr(float)`: the shortest digits that round-trip, in fixed
/// notation unless the exponent is below -4 or at least 16.
fn repr(a: f64) -> String {
    let s = format!("{a:e}");
    let (m, e) = s.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap();
    if (-4..16).contains(&e) {
        let digits: String = m.chars().filter(char::is_ascii_digit).collect();
        let point = e + 1;

        if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point as usize >= digits.len() {
            format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        }
    } else {
        format!("{m}{}", exp_text(e, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn f(v: Value, spec: &str) -> String {
        format_value(&v, spec).unwrap_or_else(|e| format!("ERR: {e}"))
    }

    #[test]
    fn pieces() {
        assert_eq!(
            parse_pieces("a {x} {{b}} {n:,d}").unwrap(),
            vec![
                Piece::Literal("a ".into()),
                Piece::Field {
                    name: "x".into(),
                    spec: None
                },
                Piece::Literal(" {b} ".into()),
                Piece::Field {
                    name: "n".into(),
                    spec: Some(",d".into())
                },
            ]
        );
        assert!(parse_pieces("{x").is_err());
        assert!(parse_pieces("x}").is_err());
        assert!(parse_pieces("{}").is_err());
        assert!(parse_pieces("{x!r}").is_err());
        assert!(parse_pieces("{x:{w}}").is_err());
        assert_eq!(
            parse_pieces("{start:%b %Y}").unwrap()[0],
            Piece::Field {
                name: "start".into(),
                spec: Some("%b %Y".into())
            }
        );
    }

    /// Expected values were produced by Python 3: `format(value, spec)`.
    #[test]
    fn matches_python() {
        let cases: &[(Value, &str, &str)] = &[
            (json!(1754), ",d", "1,754"),
            (json!(1754), "", "1754"),
            (json!(1234567), "_d", "1_234_567"),
            (json!(-42), "+d", "-42"),
            (json!(42), "+d", "+42"),
            (json!(42), " d", " 42"),
            (json!(42), ">8,", "      42"),
            (json!(1234), "010,", "00,001,234"),
            (json!(1234), "09,", "0,001,234"),
            (json!(-42), "06d", "-00042"),
            (json!(42), "*^9", "***42****"),
            (json!(42), "<5", "42   "),
            (json!(5), "<05", "50000"),
            (json!(255), "x", "ff"),
            (json!(255), "#X", "0XFF"),
            (json!(255), "#b", "0b11111111"),
            (json!(65535), "_x", "ffff"),
            (json!(1048575), "_x", "f_ffff"),
            (json!(8), "o", "10"),
            (json!(65), "c", "A"),
            (json!(1754), ".2f", "1754.00"),
            (json!(0.45), ".0%", "45%"),
            (json!(0.4567), ".1%", "45.7%"),
            (json!(1.23456), ".1f", "1.2"),
            (json!(1.23456), "", "1.23456"),
            (json!(3.0), "", "3.0"),
            (json!(1e16), "", "1e+16"),
            (json!(1.5e-5), "", "1.5e-05"),
            (json!(0.0001), "", "0.0001"),
            (json!(123456.789), ",.2f", "123,456.79"),
            (json!(1234.5), ",", "1,234.5"),
            (json!(0.125), ".2f", "0.12"),
            (json!(0.375), ".2f", "0.38"),
            (json!(2.5), ".0f", "2"),
            (json!(1234.5678), "e", "1.234568e+03"),
            (json!(1234.5678), ".2E", "1.23E+03"),
            (json!(1234.5678), "g", "1234.57"),
            (json!(1234567.0), "g", "1.23457e+06"),
            (json!(0.00001234), "g", "1.234e-05"),
            (json!(100.0), "g", "100"),
            (json!(100.0), "#g", "100.000"),
            (json!(1234.5678), ".3", "1.23e+03"),
            (json!(12.0), ".3", "12.0"),
            (json!(0.5), ".1", "0.5"),
            (json!(-0.0001), "z.1f", "0.0"),
            (json!(-0.0001), ".1f", "-0.0"),
            (json!(2.0), "#.0f", "2."),
            (json!(-3.5), "=+10.2f", "-     3.50"),
            (json!(1754), "f", "1754.000000"),
            (json!("Shiloh"), "", "Shiloh"),
            (json!("Shiloh"), ">10", "    Shiloh"),
            (json!("Shiloh"), "^10", "  Shiloh  "),
            (json!("Shiloh"), ".3", "Shi"),
            (json!("ab"), "05", "ab000"),
            (json!(true), "", "true"),
            (json!(0.0), ".1", "0e+00"),
            (json!(5.0), ".1", "5e+00"),
            (json!(0.0), "g", "0"),
            (json!(1e-5), ".3", "1e-05"),
            (json!(0.0), "", "0.0"),
            (json!(-0.0), "", "-0.0"),
            (json!(1e22), ",", "1e+22"),
            (json!(123.0), ".3", "1.23e+02"),
        ];
        for (v, spec, want) in cases {
            assert_eq!(&f(v.clone(), spec), want, "format({v}, {spec:?})");
        }
    }

    #[test]
    fn type_errors() {
        for (v, spec) in [
            (json!("x"), ",d"),
            (json!("x"), "d"),
            (json!("x"), "+"),
            (json!(1.5), "d"),
            (json!(1.5), ",d"),
            (json!(5), ".2"),
            (json!(5), "s"),
            (json!(255), ",x"),
            (json!(5), "q"),
            (json!(5), "5.x2"),
        ] {
            assert!(
                format_value(&v, spec).is_err(),
                "format({v}, {spec:?}) should fail"
            );
        }
    }
}
