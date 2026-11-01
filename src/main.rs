mod content;
mod datefmt;
mod desc;
mod diag;
mod layout;
mod plan;
mod render;
mod richtext;
mod schema;
mod style;
mod template;
mod text;
mod timepoint;
mod units;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde_json::Value;

use crate::diag::Diagnostics;

/// The complete guide shown by `--help`. Agents read this to learn the tool,
/// so it covers both file formats.
const GUIDE: &str = r##"Render a horizontal timeline of dated events as a PNG.

Use "-" for desc.json to read stdin, or for output.png to write the PNG to
stdout. The image has no title: put the title and caption in the document or
page that shows it.

DESCRIPTION FILE (JSON5, so strict JSON also works). Only "events" is required.
  {
    start: "1861-04-12", stop: "1865-04-09",  // ends of the line; omit for an
                                              // open end (drawn as an arrow)
    start_label: "Fort Sumter",               // labels at the ends
    stop_label: "Appomattox Court House",
    datetime_format: "%b %-d, %Y",            // strftime, for {start} etc.
    time_zone: "America/New_York",            // show zoned times in this zone
    shows_stop: true,                         // draw each event's start-stop range
    event_text: "**{label}**\n\n{start}",     // box template (this is the default)
    events: [
      {label: "Shiloh", start: "1862-04-06", stop: "1862-04-07", killed: 3482},
    ],
  }
  Events may be in any order. Every key on an event can be used in event_text.

DATES
  1066 and "-2560" (years), "1066-10" (year-month), "2024-03-05",
  "2024-03-05T14:32", "2024-03-05T14:32-05:00", "2024-03-05T19:32Z",
  "-0043-03-15" (44 BCE). Years and year-months mean the middle of the period
  and display as "1066" and "October 1066". Years 0 and below display as BCE.
  Values with and without a time zone can't be mixed.

EVENT TEXT (event_text)
  CommonMark: **bold**, *italic*, `code` (monospace), "- " bullets, "1. " numbers,
  blank line = new paragraph, backslash at the end of a line = line break.
  {field} substitutes an event's key; {field:spec} formats it:
    numbers  Python format specs: {n:,d} -> 1,754   {x:.1f}   {p:.0%}
    dates    strftime: {start:%B %Y} -> April 1862  (start and stop only)
  A paragraph or list item that is empty after substitution is left out, so
  events that lack a detail can set that key to "".

STYLE FILE (optional, JSON5)
  Any subset of the keys printed by --print-default-style. Lengths need a unit
  (pt, px, in, mm, cm); colors are CSS names or hex like "#1f3a68".

OUTPUT
  Errors, warnings, and a one-line summary go to stderr. Exit status is 0 on
  success, 1 for problems with the input files, and 2 for command-line
  mistakes. Crowded timelines are still drawn, with one warning per problem
  (boxes or connectors too close) and a hint about making room. Check the
  result by viewing the PNG.
"##;

const EXAMPLES: &str = r#"Examples:
  timeliner --print-example > desc.json5 && timeliner desc.json5 out.png
  timeliner --style=dark.json5 desc.json5 out.png
  echo '{events: [{label: "Launch", start: "2024-06-01"}]}' | timeliner - out.png
"#;

/// Render a horizontal timeline of dated events as a PNG.
#[derive(Parser, Debug)]
#[command(version, long_about = GUIDE, after_long_help = EXAMPLES)]
struct Cli {
    /// Style file (JSON5). Keys it leaves out use the built-in defaults.
    #[arg(long, value_name = "style.json")]
    style: Option<PathBuf>,

    /// Timeline description (JSON5), or "-" to read stdin.
    #[arg(value_name = "desc.json", required_unless_present_any = INFO_FLAGS)]
    desc: Option<String>,

    /// Output PNG path, or "-" to write to stdout.
    #[arg(value_name = "output.png", required_unless_present_any = INFO_FLAGS)]
    output: Option<String>,

    /// Print a complete example description file and exit.
    #[arg(long)]
    print_example: bool,

    /// Print the default style, with every key explained, and exit.
    #[arg(long)]
    print_default_style: bool,

    /// Print the JSON Schema for a description or style file and exit.
    #[arg(long, value_name = "desc|style", value_parser = ["desc", "style"])]
    schema: Option<String>,

    /// Print the licenses of the embedded fonts and exit.
    #[arg(long)]
    licenses: bool,
}

/// Flags that print something and exit, so no files are needed.
const INFO_FLAGS: [&str; 4] = ["licenses", "print_example", "print_default_style", "schema"];

fn main() -> ExitCode {
    let cli = Cli::parse();
    let info = if cli.licenses {
        Some(text::FONT_LICENSES.to_string())
    } else if cli.print_example {
        Some(include_str!("../examples/civil-war.json5").to_string())
    } else if cli.print_default_style {
        Some(style::DEFAULT_STYLE.to_string())
    } else {
        cli.schema.as_deref().map(|which| {
            let schema = if which == "desc" {
                schema::desc()
            } else {
                schema::style()
            };
            serde_json::to_string_pretty(&schema).expect("schemas serialize") + "\n"
        })
    };
    if let Some(text) = info {
        print!("{text}");
        return ExitCode::SUCCESS;
    }
    let desc_arg = cli.desc.expect("clap requires desc");
    let mut diags = Diagnostics::default();

    let desc_name = if desc_arg == "-" {
        "<stdin>".to_string()
    } else {
        desc_arg.clone()
    };
    let desc_text = read_input(&desc_arg, &desc_name, &mut diags);
    let style_input = cli.style.as_ref().map(|p| {
        let name = p.display().to_string();
        let text = read_input(&name, &name, &mut diags);
        (name, text)
    });

    let style = match &style_input {
        Some((name, Some(text))) => style::load(Some((name, text)), &mut diags),
        Some((_, None)) => None,
        None => style::load(None, &mut diags),
    };
    let desc = desc_text.and_then(|text| desc::load(&desc_name, &text, &mut diags));
    let texts = desc
        .as_ref()
        .and_then(|d| content::event_texts(&desc_name, d, &mut diags));

    let style_name = style_input
        .as_ref()
        .map_or("<default style>", |(n, _)| n.as_str());
    let fonts = style.as_ref().and_then(|s| match text::Fonts::new(s) {
        Ok(f) => Some(f),
        Err(errors) => {
            for e in errors {
                diags.error(style_name, "", e);
            }
            None
        }
    });

    let (Some(style), Some(desc), Some(texts), Some(mut fonts)) = (style, desc, texts, fonts)
    else {
        report(&diags);
        return ExitCode::FAILURE;
    };

    let mut plan_diags = Diagnostics::default();
    let mut plan = plan::plan(
        &desc_name,
        &desc,
        &style,
        &texts,
        &mut fonts,
        &mut plan_diags,
    );
    // Characters the embedded fonts lack (e.g. CJK) need system fonts.
    if !fonts.uses_system() && plan.has_missing_glyphs() {
        fonts = text::Fonts::with_system(&style, true).expect("the embedded families exist");
        plan_diags = Diagnostics::default();
        plan = plan::plan(
            &desc_name,
            &desc,
            &style,
            &texts,
            &mut fonts,
            &mut plan_diags,
        );
    }
    diags.warnings.extend(plan_diags.warnings);
    let crowded = !plan.geometry.warnings.is_empty();
    report(&diags);
    if crowded {
        let alternate = if style.event_text_place == style::Place::Alternating {
            ""
        } else {
            "set event_text_place to \"alternating\", "
        };
        eprintln!(
            "hint: to make room, shorten event_text, or in the style {alternate}lower \
             event_text_size, raise max_event_width (narrower boxes), or raise image_width"
        );
    }

    let png = match render::render(&plan, &style) {
        Ok(png) => png,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let output = cli.output.expect("clap requires output");
    let written = if output == "-" {
        std::io::stdout()
            .write_all(&png)
            .and_then(|()| std::io::stdout().flush())
    } else {
        std::fs::write(&output, &png)
    };
    if let Err(e) = written {
        eprintln!("error: can't write {output}: {e}");
        return ExitCode::FAILURE;
    }

    let px = style.pixels_per_point;
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    eprintln!(
        "timeliner: wrote {} ({}×{} px, {}, {}, {})",
        if output == "-" { "<stdout>" } else { &output },
        (plan.width * px).round(),
        (plan.height * px).round(),
        plural(desc.events.len(), "event"),
        plural(plan.geometry.jogs, "jog"),
        plural(diags.warnings.len(), "warning"),
    );
    ExitCode::SUCCESS
}

/// Reads a file, or stdin for "-". Read failures are reported as errors.
fn read_input(arg: &str, name: &str, diags: &mut Diagnostics) -> Option<String> {
    let result = if arg == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map(|_| s)
    } else {
        std::fs::read_to_string(arg)
    };
    result
        .map_err(|e| diags.error(name, "", format!("can't read: {e}")))
        .ok()
}

/// Parses JSON5 text, reporting syntax errors with a line and column.
pub fn parse_json5(file: &str, text: &str, diags: &mut Diagnostics) -> Option<Value> {
    match json5::from_str::<Value>(text) {
        Ok(v) => Some(v),
        Err(e) => {
            let what = e.code().map_or_else(|| e.to_string(), |c| c.to_string());
            let hint = match e.code() {
                Some(json5::ErrorCode::LineTerminatorInString) => {
                    " (strings can't contain raw line breaks; write \\n instead)"
                }
                _ => "",
            };
            let msg = match e.position() {
                Some(p) => format!(
                    "syntax error at line {}, column {}: {what}{hint}",
                    p.line + 1,
                    p.column + 1
                ),
                None => format!("syntax error: {what}{hint}"),
            };
            diags.error(file, "", msg);
            None
        }
    }
}

fn report(diags: &Diagnostics) {
    for w in &diags.warnings {
        eprintln!("warning: {w}");
    }
    for e in &diags.errors {
        eprintln!("error: {e}");
    }
}
