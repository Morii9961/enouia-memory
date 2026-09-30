//! Conservative content scans used by validators. These are heuristics that
//! block obvious leaks at the contract boundary; they are not a guarantee that
//! text is free of secrets. Importer quarantine and log scanning are MV-2/B04.

use serde_json::Value;

fn run_len(rest: &str, allowed: impl Fn(char) -> bool) -> usize {
    rest.chars().take_while(|c| allowed(*c)).count()
}

fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Credential-shaped material: private-key armor, common API token prefixes,
/// cloud access key IDs, and bearer tokens.
pub fn contains_secret_material(text: &str) -> bool {
    if text.contains("-----BEGIN") && text.contains("PRIVATE KEY") {
        return true;
    }
    for (prefix, min) in [
        ("sk-", 20),
        ("ghp_", 20),
        ("gho_", 20),
        ("github_pat_", 20),
        ("xoxb-", 10),
        ("xoxp-", 10),
        ("AKIA", 16),
        ("Bearer ", 20),
    ] {
        for (index, _) in text.match_indices(prefix) {
            let rest = &text[index + prefix.len()..];
            if run_len(rest, token_char) >= min {
                return true;
            }
        }
    }
    false
}

/// Local absolute paths must not appear in capsules, dispatch records, audit,
/// or IPC detail: drive letters, UNC shares, and common home-directory roots.
pub fn contains_local_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    let drive = bytes.windows(3).enumerate().any(|(i, w)| {
        w[0].is_ascii_alphabetic()
            && w[1] == b':'
            && (w[2] == b'\\' || w[2] == b'/')
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
    });
    drive
        || text.contains("\\\\")
        || text.contains("/Users/")
        || text.contains("/home/")
        || text.to_ascii_lowercase().contains("%localappdata%")
        || text.to_ascii_lowercase().contains("appdata\\")
}

/// Visit every string (keys and values) in a JSON tree.
pub fn any_string(value: &Value, predicate: &dyn Fn(&str) -> bool) -> bool {
    match value {
        Value::String(text) => predicate(text),
        Value::Array(items) => items.iter().any(|item| any_string(item, predicate)),
        Value::Object(map) => map
            .iter()
            .any(|(key, item)| predicate(key) || any_string(item, predicate)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_credential_shapes_without_flagging_prose() {
        assert!(contains_secret_material(
            "key sk-abcdefghijklmnopqrstuvwxyz0123"
        ));
        assert!(contains_secret_material(
            "Authorization: Bearer abcdefghijklmnopqrstuvwxyz"
        ));
        assert!(!contains_secret_material(
            "选用 Professional Darkroom 视觉方向"
        ));
        assert!(!contains_secret_material("task-sk- short"));
    }

    #[test]
    fn detects_local_paths() {
        assert!(contains_local_path(r"C:\Users\someone\vault"));
        assert!(contains_local_path(r"\\server\share"));
        assert!(!contains_local_path("json_pointer /mapping/2/message"));
        assert!(!contains_local_path("https://example.invalid/path"));
        assert!(contains_local_path("stored at C:/private/vault"));
    }
}
