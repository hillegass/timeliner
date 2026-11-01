//! Golden-image regression tests: render each case and compare it with the
//! reference PNG in `tests/golden/`.
//!
//! When a change to the output is intended, regenerate the references with
//! `TIMELINER_UPDATE_GOLDEN=1 cargo test --test golden`, look at them, and
//! commit them. When a comparison fails, the actual image and a diff (changed
//! pixels in red) are written to `target/golden-failures/`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// (name, description file, style file)
const CASES: &[(&str, &str, &str)] = &[
    (
        "civil-war",
        "examples/civil-war.json5",
        "tests/golden/small.json5",
    ),
    (
        "murder-dark",
        "examples/murder.json",
        "tests/golden/dark.json5",
    ),
    (
        "ancient",
        "tests/golden/ancient.json5",
        "tests/golden/ancient-style.json5",
    ),
];

/// A pixel counts as changed if any channel differs by more than this. Small
/// differences come from SIMD and floating-point differences between CPUs.
const CHANNEL_TOLERANCE: u8 = 24;
/// The share of pixels allowed to change.
const PIXEL_TOLERANCE: f64 = 0.001;

struct Image {
    width: u32,
    height: u32,
    /// RGBA, 8 bits per channel.
    pixels: Vec<u8>,
}

fn decode(bytes: &[u8]) -> Image {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().expect("a PNG");
    let mut buf = vec![0; reader.output_buffer_size().expect("a size")];
    let info = reader.next_frame(&mut buf).expect("a frame");
    buf.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba, "expanded to RGBA");
    Image {
        width: info.width,
        height: info.height,
        pixels: buf,
    }
}

fn encode(path: &Path, image: &Image) {
    let file = std::fs::File::create(path).expect("writable");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&image.pixels)
        .unwrap();
}

fn render(desc: &str, style: &str) -> Vec<u8> {
    let o = Command::new(env!("CARGO_BIN_EXE_timeliner"))
        .args([&format!("--style={style}"), desc, "-"])
        .output()
        .expect("runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    o.stdout
}

fn failures_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/golden-failures");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn golden_images() {
    let update = std::env::var_os("TIMELINER_UPDATE_GOLDEN").is_some();
    let mut failures = Vec::new();
    for (name, desc, style) in CASES {
        let golden_path = Path::new("tests/golden").join(format!("{name}.png"));
        let png = render(desc, style);
        if update || !golden_path.exists() {
            std::fs::write(&golden_path, &png).unwrap();
            eprintln!("wrote {}", golden_path.display());
            continue;
        }
        let (actual, expected) = (decode(&png), decode(&std::fs::read(&golden_path).unwrap()));
        if (actual.width, actual.height) != (expected.width, expected.height) {
            std::fs::write(failures_dir().join(format!("{name}.png")), &png).unwrap();
            failures.push(format!(
                "{name}: size {}×{}, expected {}×{}",
                actual.width, actual.height, expected.width, expected.height
            ));
            continue;
        }
        let mut diff = Image {
            width: actual.width,
            height: actual.height,
            pixels: expected.pixels.clone(),
        };
        let mut changed = 0;
        for (i, (a, e)) in actual
            .pixels
            .chunks(4)
            .zip(expected.pixels.chunks(4))
            .enumerate()
        {
            if a.iter()
                .zip(e)
                .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE)
            {
                changed += 1;
                diff.pixels[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let share = changed as f64 / (actual.width * actual.height) as f64;
        if share > PIXEL_TOLERANCE {
            std::fs::write(failures_dir().join(format!("{name}.png")), &png).unwrap();
            encode(&failures_dir().join(format!("{name}-diff.png")), &diff);
            failures.push(format!(
                "{name}: {changed} pixels changed ({:.2}%)",
                share * 100.0
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "images differ from tests/golden/ (see target/golden-failures/; if the change is intended, \
         rerun with TIMELINER_UPDATE_GOLDEN=1):\n  {}",
        failures.join("\n  ")
    );
}
