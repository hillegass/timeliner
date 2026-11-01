//! The description file: the timeline's data.

use jiff::tz::TimeZone;
use serde_json::{Map, Value};

use crate::diag::{self, Diagnostics};
use crate::timepoint::{self, TimePoint, kind};

pub const DEFAULT_EVENT_TEXT: &str = "**{label}**\n\n{start}";

const TOP_KEYS: &[&str] = &[
    "datetime_format",
    "time_zone",
    "start",
    "start_label",
    "stop",
    "stop_label",
    "event_text",
    "shows_stop",
    "events",
];

// Fields are read by the rendering milestones.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Desc {
    pub datetime_format: Option<String>,
    pub time_zone: Option<TimeZone>,
    pub start: Option<TimePoint>,
    pub start_label: Option<String>,
    pub stop: Option<TimePoint>,
    pub stop_label: Option<String>,
    pub event_text: String,
    /// Sorted by start instant; ties keep input order.
    pub events: Vec<Event>,
}

// Fields are read by the rendering milestones.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Event {
    /// Position in the input file's `events` array, used in messages.
    pub index: usize,
    pub label: String,
    pub start: TimePoint,
    pub stop: Option<TimePoint>,
    /// The effective value: the event's own `shows_stop`, else the top-level one.
    pub shows_stop: bool,
    /// Every key of the event except `start`, `stop`, and `shows_stop`,
    /// including `label`. These are the fields available to `event_text`.
    pub fields: Map<String, Value>,
}

impl Event {
    pub fn path(&self) -> String {
        format!("events[{}] \"{}\"", self.index, self.label)
    }
}

pub fn load(file: &str, text: &str, diags: &mut Diagnostics) -> Option<Desc> {
    let value = crate::parse_json5(file, text, diags)?;
    let Value::Object(map) = value else {
        diags.error(
            file,
            "",
            format!("expected an object at the top level, got {}", kind(&value)),
        );
        return None;
    };
    let errors_before = diags.errors.len();
    let mut r = Reader { file, diags };

    for key in map.keys() {
        if !TOP_KEYS.contains(&key.as_str()) {
            r.diags
                .error(file, key, diag::unknown_setting(key, TOP_KEYS));
        }
    }

    let datetime_format = r.opt_string(&map, "datetime_format", "datetime_format");
    let time_zone = r.opt_string(&map, "time_zone", "time_zone").and_then(|name| {
        match jiff::tz::db().get(&name) {
            Ok(tz) => Some(tz),
            Err(_) => {
                r.diags.error(file, "time_zone", format!(
                    "unknown time zone \"{name}\"; use an IANA name such as \"America/New_York\" or \"UTC\""
                ));
                None
            }
        }
    });
    let start = r.opt_time(&map, "start", "start");
    let stop = r.opt_time(&map, "stop", "stop");
    let start_label = r.opt_string(&map, "start_label", "start_label");
    let stop_label = r.opt_string(&map, "stop_label", "stop_label");
    let event_text = r
        .opt_string(&map, "event_text", "event_text")
        .unwrap_or_else(|| DEFAULT_EVENT_TEXT.to_string());
    let shows_stop = r
        .opt_bool(&map, "shows_stop", "shows_stop")
        .unwrap_or(false);

    let mut events = Vec::new();
    match map.get("events") {
        None => r.diags.error(
            file,
            "events",
            "missing; give an array of events (it may be empty)",
        ),
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                if let Some(e) = r.event(i, item, shows_stop) {
                    events.push(e);
                }
            }
        }
        Some(v) => r.diags.error(
            file,
            "events",
            format!("expected an array, got {}", kind(v)),
        ),
    }

    if start_label.is_some() && !map.contains_key("start") {
        r.diags.error(
            file,
            "start_label",
            "only allowed when \"start\" is given (an open end has no label)",
        );
    }
    if stop_label.is_some() && !map.contains_key("stop") {
        r.diags.error(
            file,
            "stop_label",
            "only allowed when \"stop\" is given (an open end has no label)",
        );
    }

    // Cross-value checks still run on whatever parsed, so range errors are
    // reported alongside structural ones.
    let structural_errors = diags.errors.len() > errors_before;
    let mut desc = Desc {
        datetime_format,
        time_zone,
        start,
        start_label,
        stop,
        stop_label,
        event_text,
        events,
    };
    check_time_values(file, &desc, structural_errors, diags);
    if diags.errors.len() > errors_before {
        return None;
    }
    // `sort_by_key` is stable, so equal starts keep their input order.
    desc.events.sort_by_key(|e| e.start.instant);
    Some(desc)
}

/// Cross-value checks: zone mixing, ordering, and the event range.
fn check_time_values(file: &str, desc: &Desc, structural_errors: bool, diags: &mut Diagnostics) {
    let mut all: Vec<(String, &TimePoint)> = Vec::new();
    if let Some(t) = &desc.start {
        all.push(("start".into(), t));
    }
    if let Some(t) = &desc.stop {
        all.push(("stop".into(), t));
    }
    for e in &desc.events {
        all.push((format!("events[{}].start", e.index), &e.start));
        if let Some(t) = &e.stop {
            all.push((format!("events[{}].stop", e.index), t));
        }
    }
    let zoned = all.iter().find(|(_, t)| t.is_zoned());
    let zoneless = all.iter().find(|(_, t)| !t.is_zoned());
    if let (Some((zp, zt)), Some((np, nt))) = (zoned, zoneless) {
        diags.error(file, "", format!(
            "{zp} \"{}\" has a time zone but {np} \"{}\" does not; they can't be ordered reliably, so give every value a zone or offset, or none",
            zt.describe(),
            nt.describe()
        ));
        return;
    }

    // Skipped after structural errors: events that failed to parse are missing.
    if !structural_errors && desc.events.is_empty() && (desc.start.is_none() || desc.stop.is_none())
    {
        diags.error(file, "events", "an empty timeline needs both \"start\" and \"stop\", because an open end extends past the nearest event");
    }

    if let (Some(start), Some(stop)) = (&desc.start, &desc.stop)
        && stop.instant <= start.instant
    {
        diags.error(
            file,
            "stop",
            format!(
                "\"{}\" must be after start \"{}\"",
                stop.describe(),
                start.describe()
            ),
        );
        return;
    }

    for e in &desc.events {
        if let Some(stop) = &e.stop
            && stop.instant < e.start.instant
        {
            diags.error(
                file,
                &e.path(),
                format!(
                    "stop {} is before start {}",
                    stop.describe(),
                    e.start.describe()
                ),
            );
        }
        for (name, t) in
            std::iter::once(("start", &e.start)).chain(e.stop.as_ref().map(|t| ("stop", t)))
        {
            if let Some(ts) = &desc.start
                && t.instant < ts.instant
            {
                diags.error(
                    file,
                    &e.path(),
                    format!(
                        "{name} {} is before timeline start {}",
                        t.describe(),
                        ts.describe()
                    ),
                );
            }
            if let Some(ts) = &desc.stop
                && t.instant > ts.instant
            {
                diags.error(
                    file,
                    &e.path(),
                    format!(
                        "{name} {} is after timeline stop {}",
                        t.describe(),
                        ts.describe()
                    ),
                );
            }
        }
    }
}

struct Reader<'a> {
    file: &'a str,
    diags: &'a mut Diagnostics,
}

impl Reader<'_> {
    fn opt_string(&mut self, map: &Map<String, Value>, key: &str, path: &str) -> Option<String> {
        match map.get(key)? {
            Value::String(s) => Some(s.clone()),
            v => {
                self.diags.error(
                    self.file,
                    path,
                    format!("expected a string, got {}", kind(v)),
                );
                None
            }
        }
    }

    fn opt_bool(&mut self, map: &Map<String, Value>, key: &str, path: &str) -> Option<bool> {
        match map.get(key)? {
            Value::Bool(b) => Some(*b),
            v => {
                self.diags.error(
                    self.file,
                    path,
                    format!("expected true or false, got {}", kind(v)),
                );
                None
            }
        }
    }

    fn opt_time(&mut self, map: &Map<String, Value>, key: &str, path: &str) -> Option<TimePoint> {
        match timepoint::parse_value(map.get(key)?) {
            Ok(t) => Some(t),
            Err(msg) => {
                self.diags.error(self.file, path, msg);
                None
            }
        }
    }

    fn event(&mut self, i: usize, item: &Value, default_shows_stop: bool) -> Option<Event> {
        let path = format!("events[{i}]");
        let Value::Object(map) = item else {
            self.diags.error(
                self.file,
                &path,
                format!("expected an object, got {}", kind(item)),
            );
            return None;
        };
        let errors_before = self.diags.errors.len();

        let label = match map.get("label") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(v) => {
                self.diags.error(
                    self.file,
                    &format!("{path}.label"),
                    format!("expected a string, got {}", kind(v)),
                );
                None
            }
            None => {
                self.diags.error(self.file, &path, "missing \"label\"");
                None
            }
        };
        let start = if map.contains_key("start") {
            self.opt_time(map, "start", &format!("{path}.start"))
        } else {
            self.diags.error(self.file, &path, "missing \"start\"");
            None
        };
        let stop = self.opt_time(map, "stop", &format!("{path}.stop"));
        let shows_stop = self
            .opt_bool(map, "shows_stop", &format!("{path}.shows_stop"))
            .unwrap_or(default_shows_stop);

        let mut fields = Map::new();
        for (key, value) in map {
            if matches!(key.as_str(), "start" | "stop" | "shows_stop") {
                continue;
            }
            match value {
                Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                    fields.insert(key.clone(), value.clone());
                }
                v => self.diags.error(
                    self.file,
                    &format!("{path}.{key}"),
                    format!(
                        "fields must be a string, number, or boolean, got {}",
                        kind(v)
                    ),
                ),
            }
        }

        if self.diags.errors.len() > errors_before {
            return None;
        }
        Some(Event {
            index: i,
            label: label?,
            start: start?,
            stop,
            shows_stop,
            fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_text(text: &str) -> (Option<Desc>, Diagnostics) {
        let mut d = Diagnostics::default();
        let desc = load("desc.json", text, &mut d);
        (desc, d)
    }

    fn errors(text: &str) -> String {
        let (desc, d) = load_text(text);
        assert!(desc.is_none(), "expected errors");
        d.errors.join("\n")
    }

    #[test]
    fn civil_war_example_loads() {
        let (desc, d) = load_text(include_str!("../examples/civil-war.json5"));
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let desc = desc.unwrap();
        assert_eq!(desc.events.len(), 12);
        assert_eq!(desc.events[0].label, "Shiloh");
        assert_eq!(desc.events[0].fields["union"], 1754);
        assert!(desc.event_text.contains("{union:,d}"));
    }

    #[test]
    fn defaults_and_sorting() {
        let (desc, d) = load_text(
            r#"{start: 1860, stop: 1870, events: [
                {label: "b", start: "1865-01-01"},
                {label: "a", start: "1862-01-01"},
                {label: "a2", start: "1862-01-01"},
            ]}"#,
        );
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let desc = desc.unwrap();
        assert_eq!(desc.event_text, DEFAULT_EVENT_TEXT);
        let order: Vec<_> = desc.events.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(order, ["a", "a2", "b"]);
        assert_eq!(desc.events[2].index, 0);
    }

    #[test]
    fn empty_events() {
        assert!(
            load_text(r#"{start: 1861, stop: 1865, events: []}"#)
                .0
                .is_some()
        );
        assert!(errors(r#"{start: 1861, events: []}"#).contains("needs both"));
    }

    #[test]
    fn open_ends_are_unlimited() {
        let (desc, d) =
            load_text(r#"{events: [{label: "x", start: -2560}, {label: "y", start: 1066}]}"#);
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        assert!(desc.unwrap().start.is_none());
    }

    #[test]
    fn range_errors_are_all_reported() {
        let all = errors(
            r#"{start: "1861-04-12", stop: "1865-04-09", events: [
                {label: "Early", start: "1860-12-11"},
                {label: "Late", start: "1865-01-01", stop: "1866-01-01"},
            ]}"#,
        );
        assert!(
            all.contains(
                "events[0] \"Early\": start 1860-12-11 is before timeline start 1861-04-12"
            ),
            "{all}"
        );
        assert!(
            all.contains("events[1] \"Late\": stop 1866-01-01 is after timeline stop 1865-04-09"),
            "{all}"
        );
    }

    #[test]
    fn structural_errors() {
        let all = errors(
            r#"{stat: "x", title: "T", start_label: "a", events: [
                {start: "1862-01-01"},
                {label: "x", start: "1862-13-01"},
                {label: "y", start: "1862-01-01", extra: [1, 2]},
                "nope",
            ]}"#,
        );
        assert!(
            all.contains("unknown key \"stat\" (did you mean \"start\"?)")
                && all.contains("title\": timeliner doesn't draw titles"),
            "{all}"
        );
        assert!(
            all.contains("start_label: only allowed when \"start\" is given"),
            "{all}"
        );
        assert!(all.contains("events[0]: missing \"label\""), "{all}");
        assert!(
            all.contains("events[1].start: invalid date \"1862-13-01\""),
            "{all}"
        );
        assert!(all.contains("events[2].extra: fields must be"), "{all}");
        assert!(all.contains("events[3]: expected an object"), "{all}");
    }

    #[test]
    fn zone_mixing_is_an_error() {
        let all = errors(
            r#"{events: [
                {label: "a", start: "2024-03-05T09:00-05:00"},
                {label: "b", start: "2024-03-05T10:00"},
            ]}"#,
        );
        assert!(all.contains("has a time zone but"), "{all}");
        let (desc, d) = load_text(
            r#"{time_zone: "America/New_York", events: [
                {label: "a", start: "2024-03-05T09:00-05:00"},
                {label: "b", start: "2024-03-05T06:30-08:00"},
            ]}"#,
        );
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let desc = desc.unwrap();
        assert_eq!(desc.events[0].label, "a");
        assert!(desc.time_zone.is_some());
        assert!(
            errors(r#"{time_zone: "Mars/Olympus", start: 1, stop: 2, events: []}"#)
                .contains("unknown time zone")
        );
    }

    #[test]
    fn stop_before_start() {
        assert!(errors(r#"{start: 1865, stop: 1861, events: []}"#).contains("must be after start"));
        assert!(
            errors(r#"{events: [{label: "x", start: "1862-05-01", stop: "1862-04-01"}]}"#)
                .contains("stop 1862-04-01 is before start 1862-05-01")
        );
    }
}
