//! ADR-MEM-135: turn a v1 library into initialization batches. The original
//! export bytes are parsed again; the v1 snapshot only says which messages the
//! old library held and which candidates it extracted. Every live candidate
//! becomes one statement that quotes the owner's own cited message exactly, so
//! no model is called and nothing waits for review.
use crate::{parse::Preparation, pipeline::*};
use enouia_memory_contract::{
    candidate::{CandidateStatus, ProposedType},
    hash::sha256,
    next::*,
    record::RecordKind,
    set::RecordSet,
    source::SourceRecord,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Messages of one conversation that stay together unless a batch needs room.
const CHUNK: usize = 32;

pub struct LegacyBatch {
    pub preparation: Preparation,
    pub statements: Vec<Statement>,
}

/// `max_messages` and `max_statements` are the per-plan capacity (64 and 128
/// for `Plan::build`). Batches follow conversation chunks; a candidate's other
/// cited messages become context only when they share its batch.
pub fn legacy_batches(
    prep: &Preparation,
    set: &RecordSet,
    max_messages: usize,
    max_statements: usize,
) -> Result<(Vec<LegacyBatch>, Value), &'static str> {
    let deleted = |kind: RecordKind, id: &str, revision: u64| {
        set.tombstones.iter().any(|t| {
            t.targets.iter().any(|r| {
                r.record_kind == kind
                    && r.record_id == id
                    && r.revision.is_none_or(|v| v.get() == revision)
            })
        })
    };
    let mut sources: BTreeMap<&str, &SourceRecord> = BTreeMap::new();
    for s in &set.sources {
        let entry = sources.entry(s.source_id.as_str()).or_insert(s);
        if s.revision > entry.revision {
            *entry = s;
        }
    }
    sources.retain(|id, s| !deleted(RecordKind::Source, id, s.revision.get()));
    // Old source -> index of the re-parsed message holding the same words.
    let mut matched: BTreeMap<&str, usize> = BTreeMap::new();
    for (id, s) in &sources {
        let found = prep
            .messages
            .iter()
            .position(|m| match &s.original_message_id {
                Some(message) => {
                    m.upstream_id.as_ref() == Some(message)
                        && m.conversation == s.original_conversation_id
                }
                None => sha256(m.text.as_bytes()) == s.content_hash,
            });
        if let Some(index) = found {
            matched.insert(id, index);
        }
    }
    let mut candidates = BTreeMap::new();
    for c in &set.candidates {
        let entry = candidates.entry(c.candidate_id.as_str()).or_insert(c);
        if c.revision > entry.revision {
            *entry = c;
        }
    }
    let (mut rejected, mut unmatched, mut uncited_evidence, mut notes_cut) = (0, 0, 0, 0);
    let mut excerpted = 0;
    // (primary message, context messages, statement without indexes)
    let mut drafts = Vec::new();
    for (id, c) in &candidates {
        if c.status == CandidateStatus::Rejected
            || deleted(RecordKind::Candidate, id, c.revision.get())
        {
            rejected += 1;
            continue;
        }
        let mut cited: Vec<usize> = vec![];
        for e in &c.evidence {
            match matched.get(e.source_id.as_str()) {
                Some(i) if !cited.contains(i) => cited.push(*i),
                Some(_) => {}
                None => uncited_evidence += 1,
            }
        }
        let Some((&primary, context)) = cited.split_first() else {
            unmatched += 1;
            continue;
        };
        let message = &prep.messages[primary];
        let quote = excerpt(&message.text, &c.proposed_content);
        if quote.len() < message.text.trim().len() {
            excerpted += 1;
        }
        let mut d = dimensions(match c.proposed_type {
            ProposedType::Preference => FormationKind::Preference,
            ProposedType::Episode => FormationKind::Episode,
            // Project state needs a stage the candidate never stated.
            _ => FormationKind::Fact,
        });
        if d.kind == FormationKind::Episode {
            d.occurrence = OccurrenceAssertion::ReportedOccurred;
        }
        let mut note = format!("Legacy candidate {id}: {}", c.proposed_content);
        if note.chars().count() > 500 {
            note = note.chars().take(499).collect::<String>() + "…";
            notes_cut += 1;
        }
        drafts.push((
            primary,
            context.to_vec(),
            Statement {
                source_index: 0,
                quote: quote.clone(),
                // The worker keeps a title only if the quote contains it, and
                // titles may not hold control characters such as line breaks.
                title: if quote.contains(c.proposed_content.as_str())
                    && c.proposed_content.chars().count() <= 60
                    && !c.proposed_content.chars().any(char::is_control)
                {
                    c.proposed_content.clone()
                } else {
                    quote
                        .split(char::is_control)
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                        .unwrap_or_default()
                        .chars()
                        .take(60)
                        .collect::<String>()
                        .trim_end()
                        .to_owned()
                },
                interpretation: if message.role == enouia_memory_contract::common::SpeakerRole::User
                    && message.evidence.is_user_evidence()
                {
                    Interpretation::ClearSelfReport
                } else {
                    Interpretation::Inference
                },
                dimensions: d,
                event_time: unknown_time(),
                valid_time: unknown_time(),
                entities: vec![],
                note: Some(note),
                annotations: AnalysisAnnotations::default(),
                revises_statement: None,
                derives_from_statements: vec![],
                context_sources: vec![],
            },
        ));
    }
    // Units: conversation chunks, merged when one candidate cites several.
    let selected: BTreeSet<usize> = matched.values().copied().collect();
    // A unit is (first message of its conversation, chunk number).
    let mut unit_of: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut runs: BTreeMap<(&Option<String>, &str), (usize, usize)> = BTreeMap::new();
    for &i in &selected {
        let m = &prep.messages[i];
        let unit = match &m.conversation {
            Some(_) => {
                let (first, n) = runs
                    .entry((&m.conversation, m.platform.as_str()))
                    .or_insert((i, 0));
                *n += 1;
                (*first, (*n - 1) / CHUNK)
            }
            None => (i, 0),
        };
        unit_of.insert(i, unit);
    }
    // Candidates never merge units: cross-conversation citations would chain
    // most of the library into one oversized batch. A cited message outside
    // the quoted message's batch is dropped from the context and counted.
    let mut groups: BTreeMap<(usize, usize), (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for &i in &selected {
        groups.entry(unit_of[&i]).or_default().0.push(i);
    }
    for (di, (primary, _, _)) in drafts.iter().enumerate() {
        groups.get_mut(&unit_of[primary]).unwrap().1.push(di);
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|(_, (messages, _))| messages[0]);
    // A plan allows 32 branches per conversation, and each selected message
    // whose omitted assistant parent is missing is its own branch. So one
    // conversation contributes at most one chunk (CHUNK messages) per batch.
    let mut packed: Vec<(BTreeSet<usize>, Vec<usize>, Vec<usize>)> = vec![];
    for ((conversation, _), (messages, statements)) in groups {
        if messages.len() > max_messages || statements.len() > max_statements {
            return Err("legacy.group_exceeds_batch");
        }
        match packed.last_mut() {
            Some((c, m, s))
                if !c.contains(&conversation)
                    && m.len() + messages.len() <= max_messages
                    && s.len() + statements.len() <= max_statements =>
            {
                c.insert(conversation);
                m.extend(messages);
                s.extend(statements);
            }
            _ => packed.push((BTreeSet::from([conversation]), messages, statements)),
        }
    }
    let count = packed.len();
    let (mut batches, mut dropped) = (vec![], 0);
    for (bi, (_, mut messages, statements)) in packed.into_iter().enumerate() {
        messages.sort();
        let local = |i: &usize| messages.iter().position(|m| m == i).unwrap();
        let mut batch = prep.clone();
        batch.messages = messages.iter().map(|i| prep.messages[*i].clone()).collect();
        let used: BTreeSet<_> = batch.messages.iter().map(|m| m.raw_hash.clone()).collect();
        batch.objects.retain(|hash, _| used.contains(hash));
        // A distinct selection gives each batch its own input watermark.
        batch.selection_metadata = json!({
            "legacy_batch": bi, "legacy_batches": count, "selection": prep.selection_metadata,
            "messages": batch.messages.iter().map(|m| json!([m.raw_hash, m.locator])).collect::<Vec<_>>(),
        });
        let mut list: Vec<Statement> = statements
            .iter()
            .map(|di| {
                let (primary, context, statement) = &drafts[*di];
                let mut s = statement.clone();
                s.source_index = local(primary);
                s.context_sources = context
                    .iter()
                    .filter(|c| messages.contains(c))
                    .map(local)
                    .collect();
                dropped += context.len() - s.context_sources.len();
                s
            })
            .collect();
        list.sort_by_key(|s| s.source_index);
        batches.push(LegacyBatch {
            preparation: batch,
            statements: list,
        });
    }
    let report = json!({
        "old_sources": sources.len(),
        "matched_sources": matched.len(),
        "missing_sources": sources.len() - matched.len(),
        "candidates": candidates.len(),
        "statements": drafts.len(),
        "self_reports": drafts.iter().filter(|(_, _, s)| s.interpretation == Interpretation::ClearSelfReport).count(),
        "skipped_rejected_or_deleted": rejected,
        "skipped_without_matched_evidence": unmatched,
        "evidence_without_match": uncited_evidence,
        "notes_truncated": notes_cut,
        "quotes_excerpted": excerpted,
        "context_citations_kept": drafts.iter().map(|(_, c, _)| c.len()).sum::<usize>() - dropped,
        "context_citations_dropped": dropped,
        "batches": count,
    });
    Ok((batches, report))
}

/// Longest quote a memory may carry (`QuotedEvidence`).
const MAX_QUOTE: usize = 500;

/// The whole message when it fits; otherwise the sentence sharing the most
/// character pairs with the candidate's wording, widened by neighbouring
/// sentences up to `MAX_QUOTE` characters. Always an exact substring.
fn excerpt(text: &str, hint: &str) -> String {
    let whole = text.trim();
    if whole.chars().count() <= MAX_QUOTE {
        return whole.to_owned();
    }
    let pairs = |s: &str| -> BTreeSet<(char, char)> {
        let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let wanted = pairs(hint);
    // Sentences as byte ranges, ending after their terminator.
    let mut sentences = vec![];
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, '。' | '！' | '？' | '!' | '?' | '.' | '\n' | '；' | ';') {
            sentences.push((start, i + c.len_utf8()));
            start = i + c.len_utf8();
        }
    }
    sentences.push((start, text.len()));
    sentences.retain(|(a, b)| !text[*a..*b].trim().is_empty());
    let best = (0..sentences.len())
        .max_by_key(|i| {
            let (a, b) = sentences[*i];
            (
                wanted.intersection(&pairs(&text[a..b])).count(),
                std::cmp::Reverse(*i),
            )
        })
        .unwrap_or(0);
    let fits = |a: usize, b: usize| text[a..b].trim().chars().count() <= MAX_QUOTE;
    let (mut first, mut last) = (best, best);
    loop {
        let grown_after =
            last + 1 < sentences.len() && fits(sentences[first].0, sentences[last + 1].1);
        if grown_after {
            last += 1;
        }
        let grown_before = first > 0 && fits(sentences[first - 1].0, sentences[last].1);
        if grown_before {
            first -= 1;
        }
        if !grown_after && !grown_before {
            break;
        }
    }
    let quote = text[sentences[first].0..sentences[last].1].trim();
    if quote.chars().count() <= MAX_QUOTE {
        return quote.to_owned();
    }
    // One sentence longer than the limit: its first MAX_QUOTE characters.
    quote
        .chars()
        .take(MAX_QUOTE)
        .collect::<String>()
        .trim()
        .to_owned()
}
