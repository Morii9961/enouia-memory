//! Markdown archive adapter (IMPORT_REVIEW §3): a Markdown file has no
//! message IDs, so every source is located by the file's bytes and a byte
//! span. The file is split at ATX headings (and long sections at blank
//! lines) so each source is a readable span; spans are never merged across
//! files and no global conversation is invented from titles.
//!
//! Each span becomes an `imported_document` source with unknown speaker,
//! unknown evidence class, and unknown time: a migrated note says what it
//! says, not who asserted it or when.

use crate::parsed::{ParentLink, ParsedMessage, Unit};
use enouia_memory_contract::common::{EvidenceClass, Locator, SpeakerRole, TimePrecision};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::source::{Completeness, SourceKind};

pub const ADAPTER: &str = "markdown-byte-span";
pub const VERSION: &str = "1";
pub const SCHEMA: &str = "markdown-atx-sections-v1";
pub const MAX_SPAN: usize = 16 * 1024;

/// Plausibly Markdown text: valid UTF-8 without NUL bytes.
pub fn probe(bytes: &[u8]) -> bool {
    !bytes.is_empty() && !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

fn is_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes) && line[hashes..].starts_with([' ', '\t'])
}

/// Byte spans of the sections of `text`, each non-blank.
pub fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut starts = vec![0];
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if offset > 0 && is_heading(line) {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts.push(text.len());
    let mut out = Vec::new();
    for window in starts.windows(2) {
        let (mut start, end) = (window[0], window[1]);
        // Split long sections at blank lines, never inside a UTF-8 character.
        while end - start > MAX_SPAN {
            let mut limit = start + MAX_SPAN;
            while !text.is_char_boundary(limit) {
                limit -= 1;
            }
            let cut = text[start..limit]
                .rfind("\n\n")
                .map(|i| start + i + 2)
                .filter(|&c| c > start)
                .unwrap_or(limit);
            if !text[start..cut].trim().is_empty() {
                out.push((start, cut));
            }
            start = cut;
        }
        if !text[start..end].trim().is_empty() {
            out.push((start, end));
        }
    }
    out
}

/// Parse one Markdown file. `member` is set when it came from an archive.
pub fn parse_file(bytes: &[u8], member: Option<(&str, &Sha256Hex)>) -> Unit {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Unit {
            conversation_id: None,
            messages: Vec::new(),
            current_leaf: None,
            threaded: false,
            unparseable: 1,
        };
    };
    let messages = spans(text)
        .into_iter()
        .map(|(start, end)| {
            let inner = Locator::ByteRange {
                start: start as u64,
                end: end as u64,
            };
            ParsedMessage {
                kind: SourceKind::ImportedDocument,
                upstream_id: None,
                parent: ParentLink::Root,
                locator: match member {
                    Some((name, hash)) => Locator::ArchiveMember {
                        member_name: name.to_owned(),
                        member_hash: hash.clone(),
                        inner: Box::new(inner),
                    },
                    None => inner,
                },
                content_hash: sha256(&bytes[start..end]),
                original_time: None,
                occurred_at: None,
                time_precision: TimePrecision::Unknown,
                role: SpeakerRole::Unknown,
                evidence: EvidenceClass::Unknown,
                completeness: Completeness::Complete,
                attachments: Vec::new(),
                warnings: Vec::new(),
            }
        })
        .collect();
    Unit {
        conversation_id: None,
        messages,
        current_leaf: None,
        threaded: false,
        unparseable: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_split_at_headings_and_cover_the_text() {
        let text = "intro line\n\n# One\nbody 1\n## Two\nbody 2\n#not a heading\n";
        let spans = spans(text);
        assert_eq!(spans.len(), 3);
        assert_eq!(&text[spans[1].0..spans[1].1], "# One\nbody 1\n");
        assert!(text[spans[2].0..spans[2].1].contains("#not a heading"));
        assert_eq!(spans.last().unwrap().1, text.len());
    }

    #[test]
    fn long_sections_split_on_character_boundaries() {
        let text = "字".repeat(MAX_SPAN);
        let spans = spans(&text);
        assert!(spans.len() > 1);
        for (start, end) in spans {
            assert!(end - start <= MAX_SPAN);
            assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        }
    }
}
