//! Search (MV-4.2 to MV-4.4).
//!
//! The query is literal text. It is folded and split into terms; terms of
//! three or more characters go to the FTS5 trigram index (each as a quoted
//! phrase, so no operator a user types takes effect), shorter ones to a
//! bounded substring scan (CONTEXT_MODEL §3). Plain ASCII words are also
//! looked up as whole words; a term equal to a project's name or alias
//! recalls that project's memories, and projects that merely contain a term
//! are reported as ambiguous, never merged.
//!
//! Every candidate passes the filters before anything about it is returned:
//! requested types and projects, current tombstones (for any snapshot),
//! broken provenance, the principal's read permission, and time (`as_of`
//! currency; historical-only facts only on request). Ranking is the frozen,
//! explainable key of `RANKING_VERSION`; no raw scores are added. A page is
//! verified against the Vault before its snippets are made, and a cursor is
//! bound to the snapshot, the epochs, the principal, and the filters.

use crate::fold::{fold, is_long, is_word, phrase_query, terms};
use crate::store::{Index, Result, db, not_ready};
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::Tombstone;
use enouia_memory_contract::common::{ActorRef, Priority};
use enouia_memory_contract::context::Currency;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{CommitId, MemoryId, ProjectId};
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::memory::{CanonicalMemory, MemoryType, ProjectEntity, ProvenanceState};
use enouia_memory_contract::policy::{
    AccessContext, PolicyDecision, PolicyRecord, ResourceContext, Scope, evaluate,
};
use enouia_memory_contract::ports::CommitPin;
use enouia_memory_contract::record::{RecordKind, RecordRef, parse_record};
use enouia_memory_contract::temporal::currency;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_vault::{Vault, VaultError};
use rusqlite::params_from_iter;
use serde_json::{Value, json};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

/// Version of the ranking key below. Any change to recall or ordering
/// changes it (and the golden expectations).
pub const RANKING_VERSION: &str = "rank-1";
pub const DEFAULT_LIMIT: u32 = 20;
pub const MAX_LIMIT: u32 = 100;
/// Candidates taken from each recall path; beyond it a result is `partial`.
pub const CANDIDATE_CAP: usize = 500;
pub const MAX_TERMS: usize = 16;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchRequest {
    pub query: String,
    pub types: Vec<MemoryType>,
    pub project_ids: Vec<ProjectId>,
    pub include_historical: bool,
    pub as_of: Option<Timestamp>,
    pub known_at: Option<CommitId>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl SearchRequest {
    pub fn text(query: &str) -> Self {
        Self {
            query: query.to_owned(),
            ..Self::default()
        }
    }
}

/// Why a hit was found (for inspection and ranking).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Matched {
    /// The memory belongs to a project the query names exactly.
    pub entity: bool,
    /// Every ASCII word term matched as a whole word.
    pub word: bool,
    /// Position among text matches (None: found by entity only).
    pub text_rank: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub memory_id: MemoryId,
    pub revision: Revision,
    pub memory_type: MemoryType,
    pub project_id: Option<ProjectId>,
    pub snippet: String,
    pub currency: Currency,
    pub evidence_count: u32,
    pub updated_at: Timestamp,
    pub matched: Matched,
}

/// A project the query names (`exact`) or only overlaps (ambiguous).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectMatch {
    pub project_id: ProjectId,
    pub display_name: String,
    pub exact: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchPage {
    pub items: Vec<Hit>,
    pub next_cursor: Option<String>,
    /// A recall path hit its cap: narrow the query or filters.
    pub partial: bool,
    pub projects: Vec<ProjectMatch>,
    /// The commit sequence the page describes.
    pub snapshot: u64,
}

fn invalid(rule: &'static str) -> VaultError {
    VaultError::invalid(vec![rule])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > 4096 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

fn request_hash(principal: &ActorRef, request: &SearchRequest) -> Sha256Hex {
    let value = json!({
        "principal": principal,
        "query": request.query,
        "types": request.types,
        "project_ids": request.project_ids,
        "include_historical": request.include_historical,
        "as_of": request.as_of,
        "known_at": request.known_at,
        "limit": request.limit,
        "ranking": RANKING_VERSION,
    });
    sha256(&canonical_bytes(&value).expect("request serializes"))
}

struct CursorState {
    sequence: u64,
    policy_epoch: u64,
    deletion_epoch: u64,
    request: Sha256Hex,
    offset: usize,
}

fn encode_cursor(state: &CursorState) -> String {
    let value = json!({
        "v": 1,
        "sequence": state.sequence,
        "policy_epoch": state.policy_epoch,
        "deletion_epoch": state.deletion_epoch,
        "request": state.request,
        "offset": state.offset,
    });
    hex(&canonical_bytes(&value).expect("cursor serializes"))
}

fn decode_cursor(text: &str) -> Result<CursorState> {
    let bytes = unhex(text).ok_or_else(|| invalid("search.cursor_invalid"))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("search.cursor_invalid"))?;
    let number = |key: &str| {
        value[key]
            .as_u64()
            .ok_or_else(|| invalid("search.cursor_invalid"))
    };
    if value["v"] != 1 {
        return Err(invalid("search.cursor_invalid"));
    }
    Ok(CursorState {
        sequence: number("sequence")?,
        policy_epoch: number("policy_epoch")?,
        deletion_epoch: number("deletion_epoch")?,
        request: value["request"]
            .as_str()
            .and_then(Sha256Hex::parse)
            .ok_or_else(|| invalid("search.cursor_invalid"))?,
        offset: number("offset")? as usize,
    })
}

const VISIBLE: &str = "known_from <= ?1 AND (known_until IS NULL OR known_until > ?1)";

/// Every memory row visible at the snapshot: row ID and parsed record.
fn visible_memories(index: &Index, snapshot: i64) -> Result<BTreeMap<i64, CanonicalMemory>> {
    let mut statement = index
        .conn
        .prepare(&format!("SELECT row, record FROM memories WHERE {VISIBLE}"))
        .map_err(db)?;
    let rows = statement
        .query_map([snapshot], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(db)?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (row, text) = row.map_err(db)?;
        let memory: CanonicalMemory =
            parse_record(text.as_bytes()).map_err(|_| not_ready("index record"))?;
        out.insert(row, memory);
    }
    Ok(out)
}

fn visible_projects(index: &Index, snapshot: i64) -> Result<Vec<ProjectEntity>> {
    let mut statement = index
        .conn
        .prepare(&format!("SELECT record FROM projects WHERE {VISIBLE}"))
        .map_err(db)?;
    let rows = statement
        .query_map([snapshot], |r| r.get::<_, String>(0))
        .map_err(db)?;
    let mut out = Vec::new();
    for row in rows {
        let text = row.map_err(db)?;
        out.push(parse_record(text.as_bytes()).map_err(|_| not_ready("index record"))?);
    }
    Ok(out)
}

/// Every tombstone the Vault has now: deletions bind every snapshot.
fn tombstones(index: &Index) -> Result<Vec<Tombstone>> {
    let mut statement = index
        .conn
        .prepare("SELECT record FROM tombstones")
        .map_err(db)?;
    let rows = statement
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(db)?;
    let mut out = Vec::new();
    for row in rows {
        let text = row.map_err(db)?;
        out.push(parse_record(text.as_bytes()).map_err(|_| not_ready("index record"))?);
    }
    Ok(out)
}

fn deleted(tombstones: &[Tombstone], memory: &CanonicalMemory) -> bool {
    tombstones.iter().any(|t| {
        t.targets.iter().any(|x| {
            x.record_kind == RecordKind::Memory
                && x.record_id == memory.memory_id.as_str()
                && x.revision.is_none_or(|r| r == memory.revision)
        })
    })
}

/// The latest policy revisions at the head, read from the Vault.
fn policies(vault: &Vault, pin: &CommitPin) -> Result<Vec<PolicyRecord>> {
    let mut out = Vec::new();
    for entry in vault.record_entries(pin, RecordKind::Policy)? {
        let bytes = vault.read_record(
            pin,
            &RecordRef::new(RecordKind::Policy, &entry.record_id, entry.revision),
        )?;
        out.push(parse_record(&bytes).map_err(|_| VaultError::corrupt("policy"))?);
    }
    Ok(out)
}

fn allowed(
    policies: &[&PolicyRecord],
    principal: &ActorRef,
    record: RecordRef,
    project_id: Option<ProjectId>,
    sensitivity: enouia_memory_contract::common::Sensitivity,
    now: &Timestamp,
) -> bool {
    let resource = ResourceContext {
        record,
        project_id,
        sensitivity,
    };
    let context = AccessContext {
        principal,
        scope: Scope::MemoryRead,
        purpose: None,
        destination: None,
        resource: &resource,
    };
    matches!(
        evaluate(policies, &context, now),
        PolicyDecision::Allow { .. }
    )
}

fn currency_order(c: Currency) -> u8 {
    match c {
        Currency::CurrentSupported => 0,
        Currency::NeedsReverification => 1,
        Currency::Conflicted => 2,
        Currency::HistoricalOnly => 3,
    }
}

fn priority_order(p: Priority) -> u8 {
    match p {
        Priority::P0 => 0,
        Priority::P1 => 1,
        Priority::P2 => 2,
    }
}

/// Up to `CANDIDATE_CAP + 1` rows matching every term, best first.
fn text_rows(index: &Index, snapshot: i64, terms: &[String]) -> Result<Vec<i64>> {
    let long: Vec<&str> = terms
        .iter()
        .filter(|t| is_long(t))
        .map(String::as_str)
        .collect();
    let short: Vec<&str> = terms
        .iter()
        .filter(|t| !is_long(t))
        .map(String::as_str)
        .collect();
    let mut sql;
    let mut values: Vec<Value> = vec![json!(snapshot)];
    if long.is_empty() {
        sql = format!("SELECT row FROM memories WHERE {VISIBLE}");
        for term in &short {
            values.push(json!(term));
            sql.push_str(&format!(" AND instr(folded, ?{}) > 0", values.len()));
        }
        sql.push_str(" ORDER BY (project_id IS NULL), row");
    } else {
        values.push(json!(phrase_query(&long)));
        sql = String::from(
            "SELECT m.row FROM memories_tri JOIN memories m ON m.row = memories_tri.rowid
             WHERE memories_tri MATCH ?2 AND m.known_from <= ?1
               AND (m.known_until IS NULL OR m.known_until > ?1)",
        );
        for term in &short {
            values.push(json!(term));
            sql.push_str(&format!(" AND instr(m.folded, ?{}) > 0", values.len()));
        }
        sql.push_str(" ORDER BY bm25(memories_tri), m.row");
    }
    sql.push_str(&format!(" LIMIT {}", CANDIDATE_CAP + 1));
    let params: Vec<rusqlite::types::Value> = values
        .iter()
        .map(|v| match v {
            Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().unwrap_or(0)),
            other => rusqlite::types::Value::Text(other.as_str().unwrap_or_default().to_owned()),
        })
        .collect();
    let mut statement = index.conn.prepare(&sql).map_err(db)?;
    let rows = statement
        .query_map(params_from_iter(params), |r| r.get::<_, i64>(0))
        .map_err(db)?;
    rows.collect::<std::result::Result<_, _>>().map_err(db)
}

/// Rows where every ASCII word term matches as a whole word.
fn word_rows(index: &Index, words: &[&str]) -> Result<BTreeSet<i64>> {
    if words.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut statement = index
        .conn
        .prepare("SELECT rowid FROM memories_word WHERE memories_word MATCH ?1")
        .map_err(db)?;
    let rows = statement
        .query_map([phrase_query(words)], |r| r.get::<_, i64>(0))
        .map_err(db)?;
    rows.collect::<std::result::Result<_, _>>().map_err(db)
}

/// A snippet of the stored text around the first term, never of folded text.
fn snippet(content: &str, terms: &[String]) -> String {
    let chars: Vec<char> = content.chars().collect();
    // Folded characters with the index of the original character each came from.
    let mut folded: Vec<(char, usize)> = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        for f in fold(&c.to_string()).chars() {
            folded.push((f, i));
        }
    }
    let haystack: Vec<char> = folded.iter().map(|(c, _)| *c).collect();
    let start = terms.iter().find_map(|term| {
        let needle: Vec<char> = term.chars().collect();
        haystack
            .windows(needle.len().max(1))
            .position(|w| w == needle.as_slice())
            .map(|p| folded[p].1)
    });
    let at = start.unwrap_or(0);
    let from = at.saturating_sub(20);
    let to = (at + 60).min(chars.len());
    let mut text: String = chars[from..to].iter().collect();
    if from > 0 {
        text.insert(0, '…');
    }
    if to < chars.len() {
        text.push('…');
    }
    text
}

/// Search the snapshot for `principal`. The index must be at the Vault's
/// head (`index_not_ready` otherwise: update it, or show the owner the
/// source browser; results are never mixed across versions).
pub fn search(
    index: &Index,
    vault: &Vault,
    principal: &ActorRef,
    request: &SearchRequest,
) -> Result<SearchPage> {
    let terms = terms(&request.query);
    if terms.is_empty() {
        return Err(invalid("search.empty_query"));
    }
    if terms.len() > MAX_TERMS {
        return Err(invalid("search.too_many_terms"));
    }
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(invalid("search.limit"));
    }
    let pin = vault.pin_current()?;
    let mark = index.watermark()?.ok_or_else(|| not_ready("index empty"))?;
    if mark.sequence != pin.sequence || mark.commit_id != pin.commit_id {
        return Err(VaultError::new(
            MemoryErrorCode::IndexNotReady,
            enouia_memory_vault::Fault::Corrupt("index behind the vault"),
        ));
    }
    let hash = request_hash(principal, request);
    let (snapshot, offset) = match &request.cursor {
        Some(text) => {
            let cursor = decode_cursor(text)?;
            if cursor.request != hash
                || cursor.policy_epoch != pin.policy_epoch
                || cursor.deletion_epoch != pin.deletion_epoch
                || cursor.sequence > pin.sequence
            {
                return Err(invalid("search.cursor_stale"));
            }
            (cursor.sequence, cursor.offset)
        }
        None => match &request.known_at {
            Some(commit) => (
                index
                    .sequence_of(commit)?
                    .ok_or_else(|| invalid("search.known_at_unknown"))?,
                0,
            ),
            None => (pin.sequence, 0),
        },
    };
    let now = vault.now()?;
    let as_of = request.as_of.clone().unwrap_or_else(|| now.clone());
    let snap = snapshot as i64;
    let policy_records = policies(vault, &pin)?;
    let policy_refs: Vec<&PolicyRecord> = policy_records.iter().collect();

    // Projects the query names, among those the principal may read.
    let whole = terms.join(" ");
    let mut projects = Vec::new();
    let mut entity: BTreeSet<ProjectId> = BTreeSet::new();
    for project in visible_projects(index, snap)? {
        let reference = RecordRef::new(
            RecordKind::Project,
            project.project_id.as_str(),
            project.revision,
        );
        if !allowed(
            &policy_refs,
            principal,
            reference,
            Some(project.project_id.clone()),
            project.sensitivity,
            &now,
        ) {
            continue;
        }
        let names = project.names();
        let exact = names.iter().any(|n| *n == whole || terms.contains(n));
        let overlaps = names
            .iter()
            .any(|n| terms.iter().any(|t| n.contains(t.as_str())));
        if exact {
            entity.insert(project.project_id.clone());
        }
        if exact || overlaps {
            projects.push(ProjectMatch {
                project_id: project.project_id.clone(),
                display_name: project.display_name.clone(),
                exact,
            });
        }
    }
    projects
        .sort_by(|a, b| (!a.exact, a.project_id.as_str()).cmp(&(!b.exact, b.project_id.as_str())));

    let memories = visible_memories(index, snap)?;
    let text = text_rows(index, snap, &terms)?;
    let mut partial = text.len() > CANDIDATE_CAP;
    let words: Vec<&str> = terms
        .iter()
        .filter(|t| is_word(t))
        .map(String::as_str)
        .collect();
    let word = word_rows(index, &words)?;
    let mut candidates: BTreeMap<i64, Matched> = BTreeMap::new();
    for (rank, row) in text.into_iter().take(CANDIDATE_CAP).enumerate() {
        candidates.entry(row).or_default().text_rank = Some(rank);
    }
    let mut from_entity = 0;
    for (row, memory) in &memories {
        if memory
            .project_id
            .as_ref()
            .is_some_and(|p| entity.contains(p))
        {
            from_entity += 1;
            if from_entity > CANDIDATE_CAP {
                partial = true;
                break;
            }
            candidates.entry(*row).or_default().entity = true;
        }
    }
    let latest: Vec<&CanonicalMemory> = memories.values().collect();
    let deletions = tombstones(index)?;
    let mut ranked = Vec::new();
    for (row, mut matched) in candidates {
        let Some(memory) = memories.get(&row) else {
            continue;
        };
        matched.word = !words.is_empty() && word.contains(&row);
        if (!request.types.is_empty() && !request.types.contains(&memory.memory_type()))
            || (!request.project_ids.is_empty()
                && !memory
                    .project_id
                    .as_ref()
                    .is_some_and(|p| request.project_ids.contains(p)))
            || deleted(&deletions, memory)
            || memory.provenance_state == ProvenanceState::Broken
        {
            continue;
        }
        let reference = RecordRef::new(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            memory.revision,
        );
        if !allowed(
            &policy_refs,
            principal,
            reference,
            memory.project_id.clone(),
            memory.sensitivity,
            &now,
        ) {
            continue;
        }
        let Some(currency) = currency(memory, &latest, &as_of, &now) else {
            continue;
        };
        if currency == Currency::HistoricalOnly && !request.include_historical {
            continue;
        }
        let date = memory
            .valid_from
            .clone()
            .or(memory.observed_at.clone())
            .unwrap_or(memory.created_at.clone());
        let key = (
            !matched.entity,
            currency_order(currency),
            !matched.word,
            matched.text_rank.unwrap_or(usize::MAX),
            priority_order(memory.priority),
            Reverse(date.as_str().to_owned()),
            memory.memory_id.to_string(),
            memory.revision.get(),
        );
        ranked.push((key, memory, currency, matched));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0));
    let end = (offset + limit as usize).min(ranked.len());
    let mut items = Vec::new();
    for (_, memory, currency, matched) in ranked.iter().take(end).skip(offset) {
        // The index only finds IDs; what is returned is read and checked
        // against the Vault's pinned revision.
        let reference = RecordRef::new(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            memory.revision,
        );
        let bytes = vault.read_revision(&pin, &reference)?;
        let stored: CanonicalMemory =
            parse_record(&bytes).map_err(|_| VaultError::corrupt("memory"))?;
        if &stored != *memory {
            return Err(not_ready("index disagrees with the vault"));
        }
        items.push(Hit {
            memory_id: memory.memory_id.clone(),
            revision: memory.revision,
            memory_type: memory.memory_type(),
            project_id: memory.project_id.clone(),
            snippet: snippet(&stored.content, &terms),
            currency: *currency,
            evidence_count: memory.evidence.len() as u32,
            updated_at: memory.updated_at.clone(),
            matched: *matched,
        });
    }
    let next_cursor = (end < ranked.len()).then(|| {
        encode_cursor(&CursorState {
            sequence: snapshot,
            policy_epoch: pin.policy_epoch,
            deletion_epoch: pin.deletion_epoch,
            request: hash,
            offset: end,
        })
    });
    Ok(SearchPage {
        items,
        next_cursor,
        partial,
        projects,
        snapshot,
    })
}

impl SearchPage {
    /// The IPC shape of `memory_search` (contract `MemorySearchResult`).
    /// A page is never stale: a lagging index is refused, not served.
    pub fn to_ipc(&self) -> enouia_memory_contract::ipc::MemorySearchResult {
        enouia_memory_contract::ipc::MemorySearchResult {
            items: self
                .items
                .iter()
                .map(|h| enouia_memory_contract::ipc::SearchHit {
                    memory_id: h.memory_id.clone(),
                    revision: h.revision,
                    memory_type: h.memory_type,
                    snippet: h.snippet.clone(),
                    currency: h.currency,
                    evidence_count: h.evidence_count,
                    updated_at: h.updated_at.clone(),
                })
                .collect(),
            next_cursor: self.next_cursor.clone(),
            stale: false,
            partial: self.partial,
        }
    }
}
