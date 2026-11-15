const SECRET: &str = "[sensitive value removed]";
const EMAIL: &str = "[email removed]";

const KEYS: &[&str] = &[
    "authorization",
    "api_key",
    "api-key",
    "api key",
    "apikey",
    "access_token",
    "access-token",
    "access token",
    "refresh_token",
    "refresh-token",
    "refresh token",
    "client_secret",
    "client-secret",
    "client secret",
    "secret_key",
    "secret-key",
    "private_key",
    "private-key",
    "password",
    "passwd",
    "secret",
    "credential",
    "token",
];

pub(super) fn redact(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut private_key = false;
    for (index, line) in input.split('\n').enumerate() {
        if index > 0 {
            result.push('\n');
        }
        if line.contains("-----BEGIN ") && line.contains("PRIVATE KEY-----") {
            private_key = true;
            result.push_str(SECRET);
        } else if private_key {
            if line.contains("-----END ") && line.contains("PRIVATE KEY-----") {
                private_key = false;
            }
        } else {
            result.push_str(&redact_line(line));
        }
    }
    result
}

fn redact_line(line: &str) -> String {
    let assignment = secret_assignment(line);
    let mut result = String::with_capacity(line.len());
    let (prefix, suffix) = assignment.map_or((line, ""), |at| (&line[..at], SECRET));
    for part in prefix.split_inclusive(char::is_whitespace) {
        let word = part.trim_end_matches(char::is_whitespace);
        let whitespace = &part[word.len()..];
        if email(word) {
            result.push_str(EMAIL);
        } else if token_like(word) {
            result.push_str(SECRET);
        } else {
            result.push_str(word);
        }
        result.push_str(whitespace);
    }
    result.push_str(suffix);
    result
}

fn secret_assignment(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (start, _) in line.char_indices() {
        if start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        for key in KEYS {
            let Some(candidate) = line.get(start..start + key.len()) else {
                continue;
            };
            if !candidate.eq_ignore_ascii_case(key) {
                continue;
            }
            let after = start + key.len();
            let rest = line[after..].trim_start();
            if rest.starts_with([':', '=']) {
                let delimiter = line.len() - rest.len();
                return Some(delimiter + 1);
            }
        }
        if let Some(candidate) = line.get(start..start + 7)
            && candidate.eq_ignore_ascii_case("bearer ")
        {
            return Some(start + 7);
        }
    }
    None
}

fn email(word: &str) -> bool {
    let Some((local, domain)) = word.rsplit_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && domain.chars().any(|c| c.is_alphanumeric())
}

fn token_like(word: &str) -> bool {
    // Punctuation may wrap a credential copied into the user-authored text.
    let word = word.trim_matches(['\'', '"', '`', '(', ')', '[', ']', '{', '}', ',', ';']);
    let known_prefix = ["sk-", "ghp_", "github_pat_", "hf_", "AKIA", "ASIA"]
        .iter()
        .any(|prefix| word.starts_with(prefix));
    if known_prefix && word.len() >= 16 {
        return true;
    }
    let candidate = word.len() >= 24
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.+/=".contains(&b));
    candidate
        && word.bytes().any(|b| b.is_ascii_alphabetic())
        && word.bytes().any(|b| b.is_ascii_digit())
}
