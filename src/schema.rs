//! JSON Schemas for the two input files, printed by `--schema`. Tests keep
//! them in sync with the keys the loaders accept.

use serde_json::{Value, json};

const LENGTH: &str = r"^\s*-?(\d+(\.\d*)?|\.\d+)\s*(pt|px|in|mm|cm)\s*$";

fn date() -> Value {
    json!({
        "description": "A year (1066 or \"-2560\"), year-month (\"1066-10\"), date (\"2024-03-05\"), date-time (\"2024-03-05T14:32\"), or date-time with a zone (\"2024-03-05T14:32-05:00\", \"…Z\", \"…[America/New_York]\"). Years -9999 to 9999; year 0 is 1 BCE. A year or year-month means the middle of that period.",
        "oneOf": [{"type": "string"}, {"type": "integer", "minimum": -9999, "maximum": 9999}]
    })
}

pub fn desc() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "timeliner description",
        "description": "The timeline's data. Parsed as JSON5, so strict JSON also works.",
        "type": "object",
        "additionalProperties": false,
        "required": ["events"],
        "properties": {
            "start": {"description": "Left end of the timeline. Omit for an open end (drawn as an arrow).", "$ref": "#/$defs/date"},
            "stop": {"description": "Right end of the timeline. Omit for an open end (drawn as an arrow).", "$ref": "#/$defs/date"},
            "start_label": {"type": "string", "description": "Label at the left end; needs start."},
            "stop_label": {"type": "string", "description": "Label at the right end; needs stop."},
            "datetime_format": {"type": "string", "description": "strftime format for dates, e.g. \"%b %-d, %Y\". Default: \"%B %-d, %Y\", or \"%B %-d, %Y %-I:%M %p\" if any value has a time."},
            "time_zone": {"type": "string", "description": "IANA zone (e.g. \"America/New_York\") to display zoned times in."},
            "shows_stop": {"type": "boolean", "default": false, "description": "For every event with a stop: draw a second marker at stop and color the line between."},
            "event_text": {"type": "string", "default": "**{label}**\n\n{start}", "description": "Markdown template for each event's box. {field} or {field:spec} substitutes an event's key; specs are Python format specs for numbers and strftime for start/stop. A paragraph that is empty after substitution is left out."},
            "events": {"type": "array", "items": {"$ref": "#/$defs/event"}, "description": "May be listed in any order; sorted by start."}
        },
        "$defs": {
            "date": date(),
            "event": {
                "type": "object",
                "required": ["label", "start"],
                "properties": {
                    "label": {"type": "string"},
                    "start": {"$ref": "#/$defs/date"},
                    "stop": {"$ref": "#/$defs/date"},
                    "shows_stop": {"type": "boolean", "description": "Overrides the top-level shows_stop."}
                },
                "additionalProperties": {
                    "type": ["string", "number", "boolean"],
                    "description": "Any other key is available to event_text as {key}."
                }
            }
        }
    })
}

pub fn style() -> Value {
    let length = |d: &str| json!({"type": "string", "pattern": LENGTH, "description": d});
    let color = |d: &str| json!({"type": "string", "description": format!("{d} A CSS color: a name like \"white\" or hex like \"#1f3a68\".")});
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "timeliner style",
        "description": "Appearance. Every key is optional; see `timeliner --print-default-style` for defaults. Lengths need a unit (pt, px, in, mm, cm).",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "pixels_per_point": {"type": "number", "exclusiveMinimum": 0},
            "image_width": length("Total image width."),
            "default_font_family": {"type": "string", "description": "\"Open Sans\" (built in) or any installed font."},
            "monospace_font_family": {"type": "string", "description": "For `code` spans. \"Inconsolata\" (built in) or any installed font."},
            "background": color("Image background."),
            "top_margin": length("Space above everything."),
            "bottom_margin": length("Space below everything."),
            "left_margin": length("Where the line starts."),
            "right_margin": length("Where the line ends, from the right edge."),
            "event_text_place": {"enum": ["alternating", "above", "below"]},
            "rect_line_color": color("Event box outline."),
            "rect_fill_color": color("Event box background."),
            "rect_line_width": length("Event box outline width."),
            "rect_margin": length("Padding inside event boxes."),
            "max_event_width": {"type": "number", "minimum": 1, "description": "No box is wider than image_width / max_event_width."},
            "rect_gap": length("Minimum space between boxes."),
            "rounded_corner": length("Box corner radius."),
            "text_color": color("Event text and end labels."),
            "timeline_color": color("The line and its end markers."),
            "timeline_width": length("Thickness of the line."),
            "connector_color": color("Lines from markers to boxes."),
            "connector_width": length("Thickness of connectors."),
            "connector_corner": length("Radius of connector bends."),
            "connector_dashed": {"type": "boolean"},
            "marker": {"enum": ["dot", "square", "diamond", "bar", "none"]},
            "marker_color": color("Event markers and ranges."),
            "marker_size": length("Marker diameter or side."),
            "event_text_size": length("Event text and end label size.")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn keys(v: &Value) -> BTreeSet<String> {
        v["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn style_schema_matches_the_default_style() {
        let defaults: Value = json5::from_str(crate::style::DEFAULT_STYLE).unwrap();
        let default_keys: BTreeSet<String> =
            defaults.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys(&style()), default_keys);
    }

    #[test]
    fn desc_schema_matches_the_loader() {
        let loader: BTreeSet<String> = crate::desc::TOP_KEYS
            .iter()
            .map(|k| k.to_string())
            .collect();
        assert_eq!(keys(&desc()), loader);
    }
}
