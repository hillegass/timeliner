//! Builds each event's box text from `event_text` and the event's fields.

use std::collections::BTreeMap;

use crate::datefmt::DateStyle;
use crate::desc::{Desc, Event};
use crate::diag::{self, Diagnostics};
use crate::richtext::{RichText, Template};
use crate::template::format_value;

/// Returns one `RichText` per event, in `desc.events` order, or `None` after
/// reporting errors. A problem that affects every event is reported once.
pub fn event_texts(file: &str, desc: &Desc, diags: &mut Diagnostics) -> Option<Vec<RichText>> {
    let template = match Template::parse(&desc.event_text) {
        Ok(t) => t,
        Err(e) => {
            diags.error(file, "event_text", e);
            return None;
        }
    };
    let dates = DateStyle::for_desc(desc);

    let mut texts = Vec::with_capacity(desc.events.len());
    // Error message -> paths of the events it applies to.
    let mut problems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for event in &desc.events {
        match template.render(|name, spec| field(event, &dates, name, spec)) {
            Ok(t) => texts.push(t),
            Err(errors) => {
                for e in errors {
                    problems.entry(e).or_default().push(event.path());
                }
            }
        }
    }
    if problems.is_empty() {
        return Some(texts);
    }
    for (msg, paths) in problems {
        if paths.len() == desc.events.len() && paths.len() > 1 {
            diags.error(
                file,
                "event_text",
                format!("in all {} events: {msg}", paths.len()),
            );
        } else {
            for path in paths {
                diags.error(file, &format!("{path}: event_text"), &msg);
            }
        }
    }
    None
}

fn field(
    event: &Event,
    dates: &DateStyle,
    name: &str,
    spec: Option<&str>,
) -> Result<String, String> {
    let fail = |e: String| {
        format!(
            "field {{{name}{}}}: {e}",
            spec.map(|s| format!(":{s}")).unwrap_or_default()
        )
    };
    match name {
        "start" => dates.format(&event.start, spec).map_err(fail),
        "stop" => match &event.stop {
            Some(t) => dates.format(t, spec).map_err(fail),
            None => Err(fail("this event has no \"stop\"".into())),
        },
        _ => match event.fields.get(name) {
            Some(v) => format_value(v, spec.unwrap_or("")).map_err(fail),
            None => {
                let mut known: Vec<&str> = event.fields.keys().map(String::as_str).collect();
                known.extend(["start", "stop"]);
                Err(fail(
                    diag::unknown_key(name, &known).replacen("key", "field", 1),
                ))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc;
    use crate::richtext::{BlockKind, ListMarker};

    fn texts(text: &str) -> (Option<Vec<RichText>>, Diagnostics) {
        let mut d = Diagnostics::default();
        let desc = desc::load("desc.json", text, &mut d).expect("valid desc");
        (event_texts("desc.json", &desc, &mut d), d)
    }

    fn plain(t: &RichText) -> Vec<String> {
        t.blocks
            .iter()
            .map(|b| b.runs.iter().map(|r| r.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn civil_war() {
        let (t, d) = texts(include_str!("../examples/civil-war.json5"));
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let t = t.unwrap();
        assert_eq!(
            plain(&t[0]),
            ["Shiloh\nApr 6, 1862", "North: 1,754", "South: 1,728"]
        );
        assert!(t[0].blocks[0].runs[0].style.bold);
        assert_eq!(
            t[0].blocks[1].kind,
            BlockKind::Item {
                depth: 0,
                marker: Some(ListMarker::Bullet)
            }
        );
        assert_eq!(plain(&t[7])[0], "Gettysburg\nJul 1, 1863");
    }

    #[test]
    fn default_template() {
        let (t, d) = texts(
            r#"{events: [{label: "Hastings", start: "1066-10-14"}, {label: "Caesar", start: -43}]}"#,
        );
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let t = t.unwrap();
        assert_eq!(plain(&t[0]), ["Caesar", "44 BCE"]);
        assert_eq!(plain(&t[1]), ["Hastings", "October 14, 1066"]);
    }

    #[test]
    fn times_switch_the_default_format() {
        let (t, _) = texts(
            r#"{events: [{label: "Call", start: "2024-03-05T14:32"}, {label: "Text", start: "2024-03-06"}]}"#,
        );
        let t = t.unwrap();
        assert_eq!(plain(&t[0])[1], "March 5, 2024 2:32 PM");
        assert_eq!(plain(&t[1])[1], "March 6, 2024 12:00 AM");
    }

    #[test]
    fn missing_field_everywhere_is_reported_once() {
        let (t, d) = texts(
            r#"{event_text: "{label} {unoin:,d}", events: [
                {label: "a", start: 1, union: 1},
                {label: "b", start: 2, union: 2},
            ]}"#,
        );
        assert!(t.is_none());
        assert_eq!(
            d.errors,
            [
                "desc.json: event_text: in all 2 events: field {unoin:,d}: unknown field \"unoin\" (did you mean \"union\"?)"
            ]
        );
    }

    #[test]
    fn per_event_errors() {
        let (t, d) = texts(
            r#"{event_text: "{n:,d} {stop}", events: [
                {label: "a", start: 1, stop: 2, n: 5},
                {label: "b", start: 2, stop: 3, n: "five"},
                {label: "c", start: 3, n: 7},
            ]}"#,
        );
        assert!(t.is_none());
        let all = d.errors.join("\n");
        assert!(all.contains("events[1] \"b\": event_text: field {n:,d}: format code 'd' needs a number, but the value is text"), "{all}");
        assert!(
            all.contains("events[2] \"c\": event_text: field {stop}: this event has no \"stop\""),
            "{all}"
        );
        assert_eq!(d.errors.len(), 2, "{all}");
    }

    #[test]
    fn template_syntax_errors() {
        let (t, d) = texts(r#"{event_text: "{label", events: [{label: "a", start: 1}]}"#);
        assert!(t.is_none());
        assert!(
            d.errors[0].contains("event_text: unclosed field"),
            "{:?}",
            d.errors
        );
    }
}
