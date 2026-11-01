//! End-to-end tests that run the built binary.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn timeliner(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_timeliner"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("runs");
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().expect("finishes")
}

/// Width and height from a PNG's IHDR chunk.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let be = |i: usize| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn renders_the_civil_war_example() {
    let out = std::env::temp_dir().join("timeliner-civil-war.png");
    let o = timeliner(&["examples/civil-war.json5", out.to_str().unwrap()], None);
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{stderr}");
    assert!(o.stdout.is_empty());
    assert!(
        stderr.contains("timeliner: wrote")
            && stderr.contains("12 events")
            && stderr.contains("0 warnings"),
        "{stderr}"
    );
    let (w, h) = png_size(&std::fs::read(&out).unwrap());
    assert_eq!(w, 2376);
    assert!(h > 500, "{h}");
}

#[test]
fn stdin_to_stdout() {
    let o = timeliner(
        &["-", "-"],
        Some(r#"{start: 2000, stop: 2010, events: [{label: "a", start: 2005}]}"#),
    );
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{stderr}");
    assert!(stderr.contains("wrote <stdout>"), "{stderr}");
    assert_eq!(png_size(&o.stdout).0, 2376);
}

#[test]
fn style_changes_the_size() {
    let style = std::env::temp_dir().join("timeliner-test-style.json5");
    std::fs::write(&style, r#"{pixels_per_point: 1, image_width: "6in"}"#).unwrap();
    let o = timeliner(
        &[&format!("--style={}", style.display()), "-", "-"],
        Some(r#"{start: 1, stop: 2, events: []}"#),
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(png_size(&o.stdout).0, 432);
}

#[test]
fn bad_input_fails_without_output() {
    let o = timeliner(
        &["-", "-"],
        Some(r#"{events: [{label: "a", start: "1862-13-01"}]}"#),
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(
        stderr.contains("error: <stdin>: events[0].start: invalid date"),
        "{stderr}"
    );
}

#[test]
fn licenses() {
    let o = timeliner(&["--licenses"], None);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("SIL OPEN FONT LICENSE"));
}
