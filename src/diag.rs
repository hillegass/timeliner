//! Error and warning collection. Validation reports every problem it finds
//! instead of stopping at the first one, so an agent can fix them all at once.

use std::fmt;

#[derive(Debug, Default)]
pub struct Diagnostics {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Diagnostics {
    pub fn error(&mut self, file: &str, path: &str, msg: impl fmt::Display) {
        self.errors.push(locate(file, path, msg));
    }

    pub fn warning(&mut self, file: &str, path: &str, msg: impl fmt::Display) {
        self.warnings.push(locate(file, path, msg));
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

fn locate(file: &str, path: &str, msg: impl fmt::Display) -> String {
    if path.is_empty() {
        format!("{file}: {msg}")
    } else {
        format!("{file}: {path}: {msg}")
    }
}

/// Builds the "unknown key" message, with a "did you mean" hint when a known
/// key is a likely typo of `key`.
pub fn unknown_key(key: &str, known: &[&str]) -> String {
    match suggest(key, known) {
        Some(s) => format!("unknown key \"{key}\" (did you mean \"{s}\"?)"),
        None => format!("unknown key \"{key}\""),
    }
}

fn suggest<'a>(word: &str, candidates: &[&'a str]) -> Option<&'a str> {
    // Allow roughly one edit per three characters, and at least two.
    let limit = (word.chars().count() / 3).max(2);
    candidates
        .iter()
        .map(|c| (levenshtein(word, c), *c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_close_keys() {
        let known = ["marker_color", "marker_size", "timeline_color"];
        assert_eq!(suggest("marker_colour", &known), Some("marker_color"));
        assert_eq!(suggest("markercolor", &known), Some("marker_color"));
        assert_eq!(suggest("background", &known), None);
    }
}
