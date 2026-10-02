//! Result shapes for the page (camelCase). Stored records embedded in a
//! result keep their snake_case storage form, as in the Memory IPC.

use enouia_memory_contract::candidate::CandidateRecord;
use enouia_memory_contract::memory::{CanonicalMemory, ProvenanceState};
use enouia_memory_contract::time::Timestamp;
use serde_json::{Value, json};

pub const SNIPPET_CHARS: usize = 160;

pub fn snippet(text: &str) -> String {
    let mut out: String = text.chars().take(SNIPPET_CHARS).collect();
    if text.chars().count() > SNIPPET_CHARS {
        out.push('…');
    }
    out
}

/// One row of the Memory Explorer: every state the page must show
/// (approved, expired, conflicted, historical, source missing).
pub fn memory_row(memory: &CanonicalMemory, now: &Timestamp) -> Value {
    json!({
        "memoryId": memory.memory_id,
        "revision": memory.revision,
        "type": memory.memory_type(),
        "title": memory.title,
        "snippet": snippet(&memory.content),
        "status": memory.status,
        "projectId": memory.project_id,
        "sensitivity": memory.sensitivity,
        "expired": memory.valid_until.as_ref().is_some_and(|t| t <= now),
        "notYetEffective": memory.valid_from.as_ref().is_some_and(|t| t > now),
        "conflicted": memory.conflict_group_id.is_some(),
        "sourceMissing": memory.provenance_state == ProvenanceState::Broken,
        "evidenceCount": memory.evidence.len(),
        "updatedAt": memory.updated_at,
    })
}

pub fn candidate_row(candidate: &CandidateRecord, target: Option<&CanonicalMemory>) -> Value {
    json!({
        "candidateId": candidate.candidate_id,
        "revision": candidate.revision,
        "proposalKind": candidate.proposal_kind,
        "proposedType": candidate.proposed_type,
        "content": candidate.proposed_content,
        "details": candidate.proposed_details,
        "reason": candidate.reason,
        "originKind": candidate.origin_kind,
        "sensitivity": candidate.sensitivity,
        "evidence": candidate.evidence.iter().map(|e| json!({
            "sourceId": e.source_id, "sourceRevision": e.source_revision, "supports": e.supports,
            "locator": e.locator,
        })).collect::<Vec<_>>(),
        "conflicts": candidate.conflicts,
        "targetMemoryId": candidate.target_memory_id,
        "expectedRevision": candidate.expected_revision,
        "target": target.map(|m| json!({
            "memoryId": m.memory_id, "revision": m.revision, "content": m.content, "status": m.status,
        })),
        "createdAt": candidate.created_at,
    })
}

/// Cut `[start, start + max)` of `text` back to UTF-8 boundaries.
pub fn excerpt(text: &str, start: u64, max: u64) -> (String, u64, u64, bool) {
    let len = text.len() as u64;
    let mut begin = start.min(len) as usize;
    while begin > 0 && !text.is_char_boundary(begin) {
        begin -= 1;
    }
    let mut end = (begin as u64).saturating_add(max).min(len) as usize;
    while end > begin && !text.is_char_boundary(end) {
        end -= 1;
    }
    (
        text[begin..end].to_owned(),
        begin as u64,
        end as u64,
        end < text.len() || begin > 0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_never_split_a_character() {
        let text = "函馆的夜景";
        let (part, start, end, truncated) = excerpt(text, 1, 7);
        assert_eq!(part, "函馆");
        assert_eq!((start, end, truncated), (0, 6, true));
        let (all, _, _, truncated) = excerpt(text, 0, 8192);
        assert_eq!((all.as_str(), truncated), (text, false));
        assert_eq!(
            snippet(&"字".repeat(200)).chars().count(),
            SNIPPET_CHARS + 1
        );
    }
}
