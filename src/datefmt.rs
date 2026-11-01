//! Displaying dates: strftime via jiff, with our own `%Y` so years at or
//! before 0 read "N BCE", and partial dates shown at their input precision.

use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::desc::Desc;
use crate::timepoint::{TimePoint, TimeValue};

const DATE_FORMAT: &str = "%B %-d, %Y";
const DATETIME_FORMAT: &str = "%B %-d, %Y %-I:%M %p";

/// What every date in one description is displayed with.
#[derive(Debug, Clone)]
pub struct DateStyle {
    /// `datetime_format`, or the default chosen from the input.
    pub format: String,
    pub time_zone: Option<TimeZone>,
}

impl DateStyle {
    pub fn for_desc(desc: &Desc) -> DateStyle {
        let format = desc.datetime_format.clone().unwrap_or_else(|| {
            let mut all = desc.start.iter().chain(&desc.stop).chain(
                desc.events
                    .iter()
                    .flat_map(|e| std::iter::once(&e.start).chain(&e.stop)),
            );
            let has_time =
                all.any(|t| matches!(t.value, TimeValue::DateTime(_) | TimeValue::Zoned(_)));
            if has_time {
                DATETIME_FORMAT
            } else {
                DATE_FORMAT
            }
            .to_string()
        });
        DateStyle {
            format,
            time_zone: desc.time_zone.clone(),
        }
    }

    /// Formats `t`. With `spec` (a strftime format from `{start:%b %Y}`),
    /// that format is used as given, even for partial dates.
    pub fn format(&self, t: &TimePoint, spec: Option<&str>) -> Result<String, String> {
        match (&t.value, spec) {
            (TimeValue::Year(y), None) => Ok(year_text(i64::from(*y))),
            (TimeValue::YearMonth(y, m), None) => {
                let first = Date::new(*y, *m, 1).map_err(|e| e.to_string())?;
                strftime(
                    &format!("%B {}", year_text(i64::from(*y))),
                    first.to_datetime(jiff::civil::Time::midnight()),
                )
            }
            (TimeValue::Zoned(z), spec) => {
                let shown = match &self.time_zone {
                    Some(tz) => z.with_time_zone(tz.clone()),
                    None => z.clone(),
                };
                let fmt = spec.unwrap_or(&self.format);
                jiff::fmt::strtime::format(with_year(fmt, i64::from(shown.year())), &shown)
                    .map_err(|e| describe_error(fmt, e))
            }
            (_, spec) => {
                let dt = t.civil();
                let fmt = spec.unwrap_or(&self.format);
                strftime(&with_year(fmt, i64::from(dt.year())), dt)
                    .map_err(|_| describe_error(fmt, "it needs information this date doesn't have"))
            }
        }
    }
}

fn strftime(fmt: &str, dt: jiff::civil::DateTime) -> Result<String, String> {
    jiff::fmt::strtime::format(fmt, dt).map_err(|e| e.to_string())
}

fn describe_error(fmt: &str, e: impl std::fmt::Display) -> String {
    format!("can't format the date with \"{fmt}\": {e}")
}

/// A year as displayed: plain for positive years, "N BCE" for years at or
/// before 0 (astronomical year 0 is 1 BCE).
pub fn year_text(y: i64) -> String {
    if y <= 0 {
        format!("{} BCE", 1 - y)
    } else {
        y.to_string()
    }
}

/// Replaces each `%Y` directive in a strftime format with the year text,
/// leaving everything else (including `%%`) for jiff.
fn with_year(fmt: &str, year: i64) -> String {
    let mut out = String::with_capacity(fmt.len());
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        // A directive: '%', optional flag and width, then a letter.
        let mut directive = String::from("%");
        while let Some(&n) = chars.peek() {
            directive.push(n);
            chars.next();
            if !(matches!(n, '-' | '_' | '0' | '^' | '#') || n.is_ascii_digit()) {
                break;
            }
        }
        if directive.ends_with('Y') {
            out.push_str(&year_text(year).replace('%', "%%"));
        } else {
            out.push_str(&directive);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timepoint::parse;

    fn style(format: &str) -> DateStyle {
        DateStyle {
            format: format.to_string(),
            time_zone: None,
        }
    }

    fn fmt(s: &str, format: &str, spec: Option<&str>) -> String {
        style(format).format(&parse(s).unwrap(), spec).unwrap()
    }

    #[test]
    fn default_formats() {
        assert_eq!(fmt("1862-04-06", DATE_FORMAT, None), "April 6, 1862");
        assert_eq!(fmt("1862-04-06", "%B %d, %Y", None), "April 06, 1862");
        assert_eq!(
            fmt("2024-03-05T14:32", DATETIME_FORMAT, None),
            "March 5, 2024 2:32 PM"
        );
        // A date-only value under a time format reads as midnight.
        assert_eq!(
            fmt("2024-03-05", DATETIME_FORMAT, None),
            "March 5, 2024 12:00 AM"
        );
    }

    #[test]
    fn partial_dates_use_input_precision() {
        assert_eq!(fmt("1066", DATE_FORMAT, None), "1066");
        assert_eq!(fmt("1066-10", DATE_FORMAT, None), "October 1066");
        assert_eq!(fmt("-2560", DATE_FORMAT, None), "2561 BCE");
        assert_eq!(fmt("-0043-03", DATE_FORMAT, None), "March 44 BCE");
        // An explicit format wins and shows the midpoint.
        assert_eq!(fmt("1066", DATE_FORMAT, Some("%B %-d, %Y")), "July 2, 1066");
    }

    #[test]
    fn bce_years() {
        assert_eq!(fmt("-0043-03-15", DATE_FORMAT, None), "March 15, 44 BCE");
        assert_eq!(fmt("0000-06-01", DATE_FORMAT, None), "June 1, 1 BCE");
        assert_eq!(fmt("0476-09-04", DATE_FORMAT, None), "September 4, 476");
        assert_eq!(fmt("1862-04-06", "%Y%%", None), "1862%");
    }

    #[test]
    fn zones() {
        let t = parse("2024-03-05T09:00-05:00[America/New_York]").unwrap();
        let s = style("%-I:%M %p %Z");
        assert_eq!(s.format(&t, None).unwrap(), "9:00 AM EST");
        let utc = DateStyle {
            time_zone: Some(TimeZone::UTC),
            ..s.clone()
        };
        assert_eq!(utc.format(&t, None).unwrap(), "2:00 PM UTC");
        let off = parse("2024-03-05T09:00-05:00").unwrap();
        assert_eq!(style("%H:%M %z").format(&off, None).unwrap(), "09:00 -0500");
    }

    #[test]
    fn bad_formats_are_errors() {
        let err = style("%Z")
            .format(&parse("2024-03-05T09:00").unwrap(), None)
            .unwrap_err();
        assert!(err.contains("can't format the date with \"%Z\""), "{err}");
    }
}
