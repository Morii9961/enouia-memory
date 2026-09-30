//! Index-side text folding (normalization version `fold-1`).
//!
//! The stored text is never rewritten; the index keeps a folded copy for
//! matching: full-width ASCII to ASCII, the ideographic space to a space,
//! and lowercase. No NFKC tables are available offline, and no simplified /
//! traditional or Japanese variant conversion is done, so names are never
//! rewritten into something the owner did not write (CONTEXT_MODEL §3).

pub const FOLD_VERSION: &str = "fold-1";

pub fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// Query terms: folded, split on whitespace, empty ones dropped. Quotes and
/// operators are ordinary characters of a term (the query is literal).
pub fn terms(query: &str) -> Vec<String> {
    fold(query).split_whitespace().map(str::to_owned).collect()
}

/// Terms of at least this many characters use the trigram index; shorter
/// ones are matched by a bounded literal substring scan.
pub const TRIGRAM_MIN: usize = 3;

pub fn is_long(term: &str) -> bool {
    term.chars().count() >= TRIGRAM_MIN
}

/// An FTS5 query that matches `terms` literally: each term is one quoted
/// phrase (inner quotes doubled), joined by AND. No FTS5 operator a user
/// types can take effect.
pub fn phrase_query(terms: &[&str]) -> String {
    terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Plain ASCII word (letters and digits): also looked up as a whole word.
pub fn is_word(term: &str) -> bool {
    !term.is_empty() && term.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_keeps_cjk_and_folds_width_and_case() {
        assert_eq!(fold("ＭｏｒｉＭｅｔａ　記憶"), "morimeta 記憶");
        assert_eq!(fold("琉璃光院"), "琉璃光院");
        assert_eq!(terms("  函館\u{3000}ＭoriMeta  "), ["函館", "morimeta"]);
        assert!(is_long("琉璃光") && !is_long("记忆"));
    }

    #[test]
    fn phrases_quote_everything() {
        assert_eq!(phrase_query(&["a\"b", "c*"]), "\"a\"\"b\" AND \"c*\"");
        assert_eq!(phrase_query(&["') or 1=1 --"]), "\"') or 1=1 --\"");
    }
}
