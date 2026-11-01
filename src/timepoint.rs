//! Dates and times from the description file.
//!
//! A value can be a year, a year-month, a date, a date-time, or a date-time
//! with a zone. Every value gets an `instant` (nanoseconds since 1970-01-01)
//! used for sorting and placement on the line. Zoneless values are measured
//! as if they were UTC; that is consistent because zoned and zoneless values
//! may not be mixed in one file.

use jiff::Zoned;
use jiff::civil::{Date, DateTime, Time};
use jiff::fmt::temporal::Pieces;
use jiff::tz::TimeZone;
use serde_json::Value;

const NANOS_PER_DAY: i128 = 86_400 * 1_000_000_000;

#[derive(Debug, Clone, PartialEq)]
pub enum TimeValue {
    Year(i16),
    YearMonth(i16, i8),
    Date(Date),
    DateTime(DateTime),
    Zoned(Zoned),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimePoint {
    pub value: TimeValue,
    pub instant: i128,
    /// The value as written in the input, for messages.
    pub source: String,
}

impl TimePoint {
    pub fn is_zoned(&self) -> bool {
        matches!(self.value, TimeValue::Zoned(_))
    }

    /// The value as written in the input, for messages.
    pub fn describe(&self) -> &str {
        &self.source
    }
}

/// Parses a JSON value (string, or integer year) into a `TimePoint`.
pub fn parse_value(v: &Value) -> Result<TimePoint, String> {
    match v {
        Value::Number(n) => match n.as_i64() {
            Some(y) => year(y).map(|t| TimePoint {
                source: n.to_string(),
                ..t
            }),
            None => Err(format!("{n} is not a whole year")),
        },
        Value::String(s) => parse(s),
        other => Err(format!(
            "expected a date string or a year, got {}",
            kind(other)
        )),
    }
}

pub fn parse(input: &str) -> Result<TimePoint, String> {
    parse_str(input).map(|t| TimePoint {
        source: input.trim().to_string(),
        ..t
    })
}

fn parse_str(input: &str) -> Result<TimePoint, String> {
    let s = input.trim();
    let bad = |why: &str| format!("invalid date \"{input}\": {why}");

    let (sign, digits_start) = match s.as_bytes().first() {
        Some(b'-') => (-1, 1),
        Some(b'+') => (1, 1),
        _ => (1, 0),
    };
    let year_digits = s[digits_start..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let year_end = digits_start + year_digits;
    if year_digits == 0 {
        return Err(bad("expected it to start with a year"));
    }
    let rest = &s[year_end..];

    // Year only: "1066", "-2560".
    if rest.is_empty() {
        let y: i64 = s[digits_start..year_end]
            .parse()
            .map_err(|_| bad("year is too large"))?;
        return year(sign * y).map_err(|e| bad(&e));
    }

    // Year and month: "1066-10", "-0043-03".
    if rest.len() == 3 && rest.starts_with('-') && rest[1..].bytes().all(|b| b.is_ascii_digit()) {
        let y: i64 = s[digits_start..year_end]
            .parse()
            .map_err(|_| bad("year is too large"))?;
        let m: i64 = rest[1..].parse().unwrap();
        return year_month(sign * y, m).map_err(|e| bad(&e));
    }

    // Everything else goes to jiff. Its parser wants signed years written
    // with six digits, so expand the four-digit short form first.
    let normalized = if digits_start == 1 && year_digits == 4 {
        format!("{}00{}", &s[..1], &s[1..])
    } else {
        s.to_string()
    };
    let pieces = Pieces::parse(&normalized).map_err(|e| bad(&e.to_string()))?;
    let date = pieces.date();
    let Some(time) = pieces.time() else {
        if pieces.offset().is_some() || pieces.time_zone_annotation().is_some() {
            return Err(bad("a time zone needs a time of day"));
        }
        return Ok(TimePoint {
            source: String::new(),
            instant: civil_instant(date, Time::midnight()),
            value: TimeValue::Date(date),
        });
    };
    let dt = date.to_datetime(time);

    let tz = pieces.to_time_zone().map_err(|e| bad(&e.to_string()))?;
    let offset = pieces.offset().map(|o| o.to_numeric_offset());
    let zoned = match (tz, offset) {
        (None, None) => {
            return Ok(TimePoint {
                source: String::new(),
                instant: civil_instant(date, time),
                value: TimeValue::DateTime(dt),
            });
        }
        // An offset pins the exact instant; a zone name (if any) is used for display.
        (tz, Some(offset)) => {
            let ts = offset.to_timestamp(dt).map_err(|e| bad(&e.to_string()))?;
            ts.to_zoned(tz.unwrap_or_else(|| TimeZone::fixed(offset)))
        }
        (Some(tz), None) => tz
            .to_ambiguous_zoned(dt)
            .compatible()
            .map_err(|e| bad(&e.to_string()))?,
    };
    Ok(TimePoint {
        source: String::new(),
        instant: zoned.timestamp().as_nanosecond(),
        value: TimeValue::Zoned(zoned),
    })
}

fn year(y: i64) -> Result<TimePoint, String> {
    let y = check_year(y)?;
    let start = days_from_civil(y.into(), 1, 1);
    let end = days_from_civil(i64::from(y) + 1, 1, 1);
    Ok(TimePoint {
        source: String::new(),
        value: TimeValue::Year(y),
        instant: (start + end) as i128 * NANOS_PER_DAY / 2,
    })
}

fn year_month(y: i64, m: i64) -> Result<TimePoint, String> {
    let y = check_year(y)?;
    if !(1..=12).contains(&m) {
        return Err(format!("month {m} is not between 1 and 12"));
    }
    let start = days_from_civil(y.into(), m, 1);
    let end = if m == 12 {
        days_from_civil(i64::from(y) + 1, 1, 1)
    } else {
        days_from_civil(y.into(), m + 1, 1)
    };
    Ok(TimePoint {
        source: String::new(),
        value: TimeValue::YearMonth(y, m as i8),
        instant: (start + end) as i128 * NANOS_PER_DAY / 2,
    })
}

fn check_year(y: i64) -> Result<i16, String> {
    if (-9999..=9999).contains(&y) {
        Ok(y as i16)
    } else {
        Err(format!("year {y} is outside -9999 to 9999"))
    }
}

fn civil_instant(date: Date, time: Time) -> i128 {
    let days = days_from_civil(date.year().into(), date.month().into(), date.day().into());
    let time_nanos = (i128::from(time.hour()) * 3600
        + i128::from(time.minute()) * 60
        + i128::from(time.second()))
        * 1_000_000_000
        + i128::from(time.subsec_nanosecond());
    days as i128 * NANOS_PER_DAY + time_nanos
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (Howard Hinnant's
/// algorithm). Works for any year, including year 10000 when computing the end
/// of 9999.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> i128 {
        parse(s).unwrap().instant / NANOS_PER_DAY
    }

    #[test]
    fn epoch_math() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(day("1970-01-02"), 1);
    }

    #[test]
    fn year_and_month_are_midpoints() {
        // 1066 is a common year: midpoint is July 2, 12:00.
        let mid = parse("1066").unwrap().instant;
        let jul2 = parse("1066-07-02T12:00").unwrap().instant;
        assert_eq!(mid, jul2);
        // 2024 is a leap year: midpoint is July 2, 00:00.
        assert_eq!(
            parse("2024").unwrap().instant,
            parse("2024-07-02").unwrap().instant
        );
        // October has 31 days: midpoint is October 16, 12:00.
        assert_eq!(
            parse("1066-10").unwrap().instant,
            parse("1066-10-16T12:00").unwrap().instant
        );
    }

    #[test]
    fn negative_years_short_and_long() {
        let short = parse("-0043-03-15").unwrap();
        let long = parse("-000043-03-15").unwrap();
        assert_eq!((&short.value, short.instant), (&long.value, long.instant));
        assert_eq!(short.describe(), "-0043-03-15");
        assert_eq!(parse("-0043").unwrap().value, TimeValue::Year(-43));
        assert_eq!(
            parse("-0043-03").unwrap().value,
            TimeValue::YearMonth(-43, 3)
        );
        assert_eq!(
            parse("+1862-04-06").unwrap().instant,
            parse("1862-04-06").unwrap().instant
        );
        assert!(parse("-9999").is_ok());
        assert!(parse("9999-12").is_ok());
        assert!(parse("10000").is_err());
    }

    #[test]
    fn integers_are_years() {
        assert_eq!(
            parse_value(&Value::from(1066)).unwrap().value,
            TimeValue::Year(1066)
        );
        assert!(parse_value(&Value::from(1.5)).is_err());
        assert!(parse_value(&Value::Bool(true)).is_err());
    }

    #[test]
    fn date_times() {
        let a = parse("2024-03-05T14:32").unwrap();
        let b = parse("2024-03-05 14:32").unwrap();
        assert_eq!((&a.value, a.instant), (&b.value, b.instant));
        assert!(matches!(a.value, TimeValue::DateTime(_)));
        assert!(parse("2024-03-05T14:32:07").is_ok());
    }

    #[test]
    fn zones_place_by_instant() {
        let ny = parse("2024-03-05T09:00-05:00").unwrap();
        let la = parse("2024-03-05T06:00-08:00").unwrap();
        let utc = parse("2024-03-05T14:00Z").unwrap();
        let named = parse("2024-03-05T09:00-05:00[America/New_York]").unwrap();
        assert!(ny.is_zoned());
        assert_eq!(ny.instant, la.instant);
        assert_eq!(ny.instant, utc.instant);
        assert_eq!(ny.instant, named.instant);
        // Zoned and zoneless measure the same wall time the same way in UTC.
        assert_eq!(utc.instant, parse("2024-03-05T14:00").unwrap().instant);
    }

    #[test]
    fn bad_dates() {
        assert!(parse("1862-13-01").unwrap_err().contains("1862-13-01"));
        assert!(parse("1066-13").is_err());
        assert!(parse("April 1862").is_err());
        assert!(parse("").is_err());
        assert!(parse("2024-03-05T14:32[Mars/Olympus]").is_err());
    }
}
