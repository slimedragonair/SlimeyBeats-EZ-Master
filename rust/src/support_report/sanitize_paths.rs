const REMOVED: &str = "[path removed]";

/// Unquoted paths can contain spaces, so conservatively redact to the end of
/// their line. Quoted paths stop at the closing quote. This is intentionally
/// best-effort: relative filenames and prose naming a person are not detected.
pub(super) fn redact(input: &str) -> String {
    input
        .split('\n')
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut output = String::with_capacity(line.len());
    let mut copied_to = 0;
    for (start, _) in line.char_indices() {
        if start < copied_to {
            continue;
        }
        let boundary = start == 0
            || bytes[start - 1].is_ascii_whitespace()
            || matches!(
                bytes[start - 1],
                b'\'' | b'"' | b'`' | b'(' | b'[' | b'=' | b':'
            );
        if !boundary || !absolute_start(&bytes[start..]) {
            continue;
        }
        let quote = start
            .checked_sub(1)
            .and_then(|i| matches!(bytes[i], b'\'' | b'"' | b'`').then_some(bytes[i] as char));
        let end = quote
            .and_then(|quote| line[start..].find(quote).map(|offset| start + offset))
            .unwrap_or(line.len());
        output.push_str(&line[copied_to..start]);
        output.push_str(REMOVED);
        copied_to = end;
    }
    output.push_str(&line[copied_to..]);
    output
}

fn absolute_start(bytes: &[u8]) -> bool {
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let unc = bytes.starts_with(b"\\\\");
    let posix =
        bytes.first() == Some(&b'/') && bytes.get(1).is_some_and(|b| !b.is_ascii_whitespace());
    drive || unc || posix
}
