mod content;
mod datefmt;
mod desc;
mod diag;
mod layout;
mod plan;
mod richtext;
mod style;
mod template;
mod text;
mod timepoint;
mod units;

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde_json::Value;

use crate::diag::Diagnostics;

/// Render a timeline described in JSON5 as a PNG image.
#[derive(Parser, Debug)]
#[command(version)]
struct Cli {
    /// Style file (JSON5). Keys it leaves out use the built-in defaults.
    #[arg(long, value_name = "style.json")]
    style: Option<PathBuf>,

    /// Timeline description (JSON5), or "-" to read stdin.
    #[arg(value_name = "desc.json", required_unless_present = "licenses")]
    desc: Option<String>,

    /// Output PNG path, or "-" to write to stdout.
    #[arg(value_name = "output.png", required_unless_present = "licenses")]
    output: Option<String>,

    /// Print the licenses of the embedded fonts and exit.
    #[arg(long)]
    licenses: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.licenses {
        print!("{}", text::FONT_LICENSES);
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

    let plan = plan::plan(&desc_name, &desc, &style, &texts, &mut fonts, &mut diags);
    report(&diags);

    // Rendering arrives in milestone 5.
    let px = style.pixels_per_point;
    eprintln!(
        "timeliner: laid out {}×{} px ({} events, {} jogs, {} warnings); rendering is not implemented yet",
        (plan.width * px).round(),
        (plan.height * px).round(),
        desc.events.len(),
        plan.geometry.jogs,
        diags.warnings.len()
    );
    ExitCode::from(2)
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
