//! The style file. The built-in default style is itself a JSON5 document; a
//! user's style file is laid over it key by key, and the result is parsed once.

use color::{AlphaColor, Srgb};
use serde_json::{Map, Value};

use crate::diag::{self, Diagnostics};
use crate::timepoint::kind;
use crate::units::parse_length;

pub type Color = AlphaColor<Srgb>;

/// The default style. Hex and named colors are mixed on purpose so both
/// parsing paths are always exercised.
pub const DEFAULT_STYLE: &str = r##"{
  pixels_per_point: 2,
  image_width: "792pt",
  default_font_family: "Open Sans",
  monospace_font_family: "Inconsolata",
  background: "white",
  top_margin: "16pt",
  bottom_margin: "16pt",
  left_margin: "48pt",
  right_margin: "48pt",
  event_text_place: "alternating",
  rect_line_color: "#1f3a68",
  rect_fill_color: "#ebebeb",
  rect_line_width: "1pt",
  rect_margin: "10pt",
  max_event_width: 6,
  rect_gap: "8pt",
  rounded_corner: "5pt",
  text_color: "black",
  timeline_color: "black",
  timeline_width: "2pt",
  connector_color: "#1f3a68",
  connector_width: "1pt",
  connector_corner: "5pt",
  connector_dashed: false,
  marker: "dot",
  marker_color: "#1f3a68",
  marker_size: "12pt",
  event_text_size: "10pt",
}
"##;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Alternating,
    Above,
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Dot,
    Square,
    Diamond,
    Bar,
    None,
}

/// A fully resolved style. All lengths are in points.
// Fields are read by the rendering milestones.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Style {
    pub pixels_per_point: f64,
    pub image_width: f64,
    pub default_font_family: String,
    pub monospace_font_family: String,
    pub background: Color,
    pub top_margin: f64,
    pub bottom_margin: f64,
    pub left_margin: f64,
    pub right_margin: f64,
    pub event_text_place: Place,
    pub rect_line_color: Color,
    pub rect_fill_color: Color,
    pub rect_line_width: f64,
    pub rect_margin: f64,
    pub max_event_width: f64,
    pub rect_gap: f64,
    pub rounded_corner: f64,
    pub text_color: Color,
    pub timeline_color: Color,
    pub timeline_width: f64,
    pub connector_color: Color,
    pub connector_width: f64,
    pub connector_corner: f64,
    pub connector_dashed: bool,
    pub marker: Marker,
    pub marker_color: Color,
    pub marker_size: f64,
    pub event_text_size: f64,
}

/// Parses a style file's text (or `None` for the defaults alone) into a `Style`.
/// Problems are added to `diags`; `None` is returned if there were any errors.
pub fn load(user: Option<(&str, &str)>, diags: &mut Diagnostics) -> Option<Style> {
    let errors_before = diags.errors.len();
    let defaults = default_map();
    let mut merged = defaults.clone();
    let file = user.map_or("<default style>", |(name, _)| name);

    if let Some((name, text)) = user {
        let value = crate::parse_json5(name, text, diags)?;
        let Value::Object(user_map) = value else {
            diags.error(
                name,
                "",
                format!("expected an object at the top level, got {}", kind(&value)),
            );
            return None;
        };
        let known: Vec<&str> = defaults.keys().map(String::as_str).collect();
        for (key, value) in user_map {
            if defaults.contains_key(&key) {
                merged.insert(key, value);
            } else {
                diags.error(name, &key, diag::unknown_setting(&key, &known));
            }
        }
    }

    let mut r = Reader {
        map: &merged,
        file,
        diags,
        pixels_per_point: 2.0,
    };

    let pixels_per_point = r.positive_number("pixels_per_point");
    r.pixels_per_point = pixels_per_point;
    let style = Style {
        pixels_per_point,
        image_width: r.length("image_width"),
        default_font_family: r.string("default_font_family"),
        monospace_font_family: r.string("monospace_font_family"),
        background: r.color("background"),
        top_margin: r.length("top_margin"),
        bottom_margin: r.length("bottom_margin"),
        left_margin: r.length("left_margin"),
        right_margin: r.length("right_margin"),
        event_text_place: r.choice(
            "event_text_place",
            &[
                ("alternating", Place::Alternating),
                ("above", Place::Above),
                ("below", Place::Below),
            ],
        ),
        rect_line_color: r.color("rect_line_color"),
        rect_fill_color: r.color("rect_fill_color"),
        rect_line_width: r.length("rect_line_width"),
        rect_margin: r.length("rect_margin"),
        max_event_width: r.positive_number("max_event_width"),
        rect_gap: r.length("rect_gap"),
        rounded_corner: r.length("rounded_corner"),
        text_color: r.color("text_color"),
        timeline_color: r.color("timeline_color"),
        timeline_width: r.length("timeline_width"),
        connector_color: r.color("connector_color"),
        connector_width: r.length("connector_width"),
        connector_corner: r.length("connector_corner"),
        connector_dashed: r.boolean("connector_dashed"),
        marker: r.choice(
            "marker",
            &[
                ("dot", Marker::Dot),
                ("square", Marker::Square),
                ("diamond", Marker::Diamond),
                ("bar", Marker::Bar),
                ("none", Marker::None),
            ],
        ),
        marker_color: r.color("marker_color"),
        marker_size: r.length("marker_size"),
        event_text_size: r.length("event_text_size"),
    };

    if diags.errors.len() > errors_before {
        return None;
    }
    if style.max_event_width < 1.0 {
        diags.error(
            file,
            "max_event_width",
            format!(
                "{} would allow boxes wider than the image; use 1 or more",
                style.max_event_width
            ),
        );
    }
    if style.left_margin + style.right_margin >= style.image_width {
        diags.error(
            file,
            "image_width",
            format!(
                "{}pt leaves no room between left_margin ({}pt) and right_margin ({}pt)",
                style.image_width, style.left_margin, style.right_margin
            ),
        );
    }
    (diags.errors.len() == errors_before).then_some(style)
}

fn default_map() -> Map<String, Value> {
    match json5::from_str(DEFAULT_STYLE) {
        Ok(Value::Object(map)) => map,
        _ => unreachable!("DEFAULT_STYLE is a valid JSON5 object"),
    }
}

/// Reads typed values out of the merged style map, reporting errors by key.
struct Reader<'a> {
    map: &'a Map<String, Value>,
    file: &'a str,
    diags: &'a mut Diagnostics,
    pixels_per_point: f64,
}

impl Reader<'_> {
    fn get(&self, key: &str) -> &Value {
        &self.map[key]
    }

    fn fail(&mut self, key: &str, msg: impl std::fmt::Display) {
        self.diags.error(self.file, key, msg);
    }

    fn string(&mut self, key: &str) -> String {
        match self.get(key) {
            Value::String(s) => s.clone(),
            v => {
                let msg = format!("expected a string, got {}", kind(v));
                self.fail(key, msg);
                String::new()
            }
        }
    }

    fn boolean(&mut self, key: &str) -> bool {
        match self.get(key) {
            Value::Bool(b) => *b,
            v => {
                let msg = format!("expected true or false, got {}", kind(v));
                self.fail(key, msg);
                false
            }
        }
    }

    fn positive_number(&mut self, key: &str) -> f64 {
        match self.get(key).as_f64() {
            Some(n) if n > 0.0 && n.is_finite() => n,
            _ => {
                let msg = format!("expected a positive number, got {}", self.get(key));
                self.fail(key, msg);
                1.0
            }
        }
    }

    fn length(&mut self, key: &str) -> f64 {
        let result = match self.get(key) {
            Value::String(s) => parse_length(s, self.pixels_per_point),
            Value::Number(n) => Err(format!(
                "{n} needs a unit (pt, px, in, mm, cm), e.g. \"{n}pt\""
            )),
            v => Err(format!(
                "expected a length such as \"12pt\", got {}",
                kind(v)
            )),
        };
        match result {
            Ok(pt) if pt >= 0.0 => pt,
            Ok(_) => {
                self.fail(key, "lengths can't be negative");
                0.0
            }
            Err(msg) => {
                self.fail(key, msg);
                0.0
            }
        }
    }

    fn color(&mut self, key: &str) -> Color {
        let Value::String(s) = self.get(key) else {
            let msg = format!(
                "expected a color string such as \"#1f3a68\" or \"black\", got {}",
                kind(self.get(key))
            );
            self.fail(key, msg);
            return Color::BLACK;
        };
        match color::parse_color(s) {
            Ok(c) => c.to_alpha_color::<Srgb>(),
            Err(e) => {
                let msg =
                    format!("\"{s}\" is not a color ({e}); use hex like \"#8a2391\" or a CSS name");
                self.fail(key, msg);
                Color::BLACK
            }
        }
    }

    fn choice<T: Copy>(&mut self, key: &str, options: &[(&str, T)]) -> T {
        let names: Vec<String> = options.iter().map(|(n, _)| format!("\"{n}\"")).collect();
        if let Value::String(s) = self.get(key)
            && let Some((_, v)) = options.iter().find(|(n, _)| n == s)
        {
            return *v;
        }
        let msg = format!(
            "expected one of {}, got {}",
            names.join(", "),
            self.get(key)
        );
        self.fail(key, msg);
        options[0].1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_text(text: &str) -> (Option<Style>, Diagnostics) {
        let mut d = Diagnostics::default();
        let s = load(Some(("style.json", text)), &mut d);
        (s, d)
    }

    #[test]
    fn defaults_parse() {
        let mut d = Diagnostics::default();
        let s = load(None, &mut d).expect("default style is valid");
        assert_eq!(s.image_width, 792.0);
        assert_eq!(s.image_width * s.pixels_per_point, 1584.0);
        assert_eq!(s.marker, Marker::Dot);
        assert_eq!(s.timeline_color, Color::BLACK);
        let blue = s.marker_color.to_rgba8();
        assert_eq!((blue.r, blue.g, blue.b), (0x1f, 0x3a, 0x68));
    }

    #[test]
    fn partial_override_and_hex() {
        let (s, d) = load_text(r##"{marker_color: "#8a2391", marker: "diamond"}"##);
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let s = s.unwrap();
        let c = s.marker_color.to_rgba8();
        assert_eq!((c.r, c.g, c.b), (0x8a, 0x23, 0x91));
        assert_eq!(s.marker, Marker::Diamond);
        assert_eq!(s.marker_size, 12.0);
    }

    #[test]
    fn unknown_key_alone_fails() {
        let (s, d) = load_text(r#"{marker_colour: "red"}"#);
        assert!(s.is_none());
        assert_eq!(d.errors.len(), 1);
    }

    #[test]
    fn empty_object_is_valid() {
        let (s, d) = load_text("{}");
        assert!(s.is_some() && d.errors.is_empty());
    }

    #[test]
    fn px_uses_pixels_per_point() {
        let (s, _) = load_text(r#"{pixels_per_point: 4, marker_size: "8px"}"#);
        assert_eq!(s.unwrap().marker_size, 2.0);
    }

    #[test]
    fn reports_every_error() {
        let (s, d) = load_text(
            r#"{marker_colour: "red", marker_size: 12, marker: "circle", background: "notacolor"}"#,
        );
        assert!(s.is_none());
        let all = d.errors.join("\n");
        assert_eq!(d.errors.len(), 4, "{all}");
        assert!(all.contains("did you mean \"marker_color\""));
        assert!(all.contains("marker_size: 12 needs a unit"));
        assert!(all.contains("marker: expected one of \"dot\", \"square\""));
        assert!(all.contains("background: \"notacolor\" is not a color"));
    }

    #[test]
    fn margins_must_leave_room() {
        let (s, d) = load_text(r#"{image_width: "90pt"}"#);
        assert!(s.is_none());
        assert!(d.errors[0].contains("leaves no room"));
    }
}
