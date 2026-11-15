use super::{sanitize_paths, sanitize_secrets};

pub(super) struct Sanitized {
    pub text: String,
    pub changed: bool,
}

pub(super) fn text(input: &str, multiline: bool) -> Sanitized {
    let mut normalized = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\r' => {}
            '\n' if multiline => normalized.push('\n'),
            '\n' | '\t' => normalized.push(' '),
            // Directional overrides and invisible separators can conceal paths or
            // credentials from both the scanner and the person reviewing them.
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}' => {}
            c if c.is_control() => {}
            c => normalized.push(c),
        }
    }
    let secrets = sanitize_secrets::redact(&normalized);
    let sanitized = sanitize_paths::redact(&secrets);
    Sanitized {
        changed: sanitized != input,
        text: sanitized,
    }
}
