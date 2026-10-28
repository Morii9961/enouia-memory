//! Pure text location only. Callers must authorize the exact source revision
//! and bind its actual speaker/class before unchanged strict evidence checks.
use icu_normalizer::{DecomposingNormalizer, properties::CanonicalCombiningClassMap};

pub const ALGORITHM: &str = "memory-quote-nfkd-1";
const MAX_SOURCE_BYTES: usize = 16 * 1024;
const MAX_QUOTE_SCALARS: usize = 500;
const MAX_NORMALIZED: usize = MAX_SOURCE_BYTES * 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocatedQuote {
    pub algorithm: &'static str,
    pub quote: String,
    /// Original Unicode scalar offsets, [start, end), not UTF-8/UTF-16 offsets.
    pub start: u32,
    pub end: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocateFault {
    InvalidInput,
    NotFound,
    Ambiguous,
    NormalizationMismatch,
}
#[derive(Clone, Copy)]
struct Scalar {
    ch: char,
    start: usize,
    end: usize,
}
fn fold(ch: char) -> char {
    match ch {
        '\u{2018}'..='\u{201b}' => '\'',
        '\u{201c}'..='\u{201f}' | '\u{300c}'..='\u{300f}' => '"',
        c if c.is_whitespace() => ' ',
        c => c,
    }
}
fn mapped(text: &str) -> Result<Vec<Scalar>, LocateFault> {
    let nfkd = DecomposingNormalizer::new_nfkd();
    let ccc = CanonicalCombiningClassMap::new();
    let mut scalars = Vec::new();
    for (start, ch) in text.chars().enumerate() {
        for decomposed in nfkd.normalize_iter(std::iter::once(ch)) {
            if scalars.len() == MAX_NORMALIZED {
                return Err(LocateFault::InvalidInput);
            }
            scalars.push(Scalar {
                ch: decomposed,
                start,
                end: start + 1,
            });
        }
    }
    // Canonical ordering never moves a mark across a starter (CCC=0).
    // Stable sorting preserves equal combining classes, including leading marks.
    let mut run = 0;
    for i in 0..scalars.len() {
        if ccc.get_u8(scalars[i].ch) == 0 {
            scalars[run..i].sort_by_key(|s| ccc.get_u8(s.ch));
            run = i + 1;
        }
    }
    scalars[run..].sort_by_key(|s| ccc.get_u8(s.ch));
    if !scalars
        .iter()
        .map(|s| s.ch)
        .eq(nfkd.normalize_iter(text.chars()))
    {
        return Err(LocateFault::NormalizationMismatch);
    }
    let mut folded: Vec<Scalar> = Vec::new();
    for mut s in scalars {
        s.ch = fold(s.ch);
        if s.ch == ' '
            && let Some(last) = folded.last_mut().filter(|last| last.ch == ' ')
        {
            last.start = last.start.min(s.start);
            last.end = last.end.max(s.end);
        } else {
            folded.push(s);
        }
    }
    Ok(folded)
}
fn hint_chars(text: &str) -> Result<Vec<char>, LocateFault> {
    let mut chars: Vec<char> = mapped(text)?.into_iter().map(|s| s.ch).collect();
    if chars.last() == Some(&' ') {
        chars.pop();
    }
    if chars.first() == Some(&' ') {
        chars.remove(0);
    }
    Ok(chars)
}
/// Locate a unique whole hint in one already authorized captured message.
/// No disk, cache, policy, classification or publication is performed here.
pub fn locate(source: &str, hint: &str) -> Result<LocatedQuote, LocateFault> {
    if source.len() > MAX_SOURCE_BYTES || hint.chars().count() > MAX_QUOTE_SCALARS {
        return Err(LocateFault::InvalidInput);
    }
    let needle = hint_chars(hint)?;
    if needle.is_empty() {
        return Err(LocateFault::InvalidInput);
    }
    let haystack = mapped(source)?;
    let original: Vec<char> = source.chars().collect();
    let mut prefix = vec![0; needle.len()];
    for i in 1..needle.len() {
        let mut j = prefix[i - 1];
        while j > 0 && needle[i] != needle[j] {
            j = prefix[j - 1];
        }
        if needle[i] == needle[j] {
            j += 1;
        }
        prefix[i] = j;
    }
    let mut matched = 0;
    let mut found: Option<LocatedQuote> = None;
    for (i, scalar) in haystack.iter().enumerate() {
        while matched > 0 && scalar.ch != needle[matched] {
            matched = prefix[matched - 1];
        }
        if scalar.ch == needle[matched] {
            matched += 1;
        }
        if matched != needle.len() {
            continue;
        }
        let window = &haystack[i + 1 - matched..=i];
        let start = window.iter().map(|s| s.start).min().unwrap();
        let end = window.iter().map(|s| s.end).max().unwrap();
        matched = prefix[matched - 1];
        if end - start > MAX_QUOTE_SCALARS {
            continue;
        }
        let quote: String = original[start..end].iter().collect();
        // Whole original span must match, never a partial ligature expansion
        // or marks reordered around omitted/intervening original characters.
        if hint_chars(&quote)? != needle {
            continue;
        }
        let candidate = LocatedQuote {
            algorithm: ALGORITHM,
            quote,
            start: start as u32,
            end: end as u32,
        };
        if let Some(prior) = &found {
            if prior != &candidate {
                return Err(LocateFault::Ambiguous);
            }
        } else {
            found = Some(candidate);
        }
    }
    found.ok_or(LocateFault::NotFound)
}

#[cfg(test)]
#[path = "quote_locator_tests.rs"]
mod tests;
