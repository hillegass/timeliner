mod desc;
mod diag;
mod style;
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
    #[arg(value_name = "desc.json")]
    desc: String,

    /// Output PNG path, or "-" to write to stdout.
    #[arg(value_name = "output.png")]
    output: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut diags = Diagnostics::default();

    let desc_name = if cli.desc == "-" {
        "<stdin>".to_string()
    } else {
        cli.desc.clone()
    };
    let desc_text = read_input(&cli.desc, &desc_name, &mut diags);
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

    report(&diags);
    let (Some(_style), Some(desc)) = (style, desc) else {
        return ExitCode::FAILURE;
    };

    // Rendering arrives in later milestones.
    eprintln!(
        "timeliner: input is valid ({} events); rendering is not implemented yet",
        desc.events.len()
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
