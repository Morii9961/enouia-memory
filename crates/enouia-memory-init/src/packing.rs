//! Lossless bounded grouping of caller-authored fresh statements (ADR-MEM-144).
use crate::{
    parse::Preparation,
    pipeline::{AnalyzedBatch, AnalyzedStatement},
};
use enouia_memory_contract::{hash::sha256, next::digest};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub struct AnalyzedPlanBatch {
    pub preparation: Preparation,
    pub analysis: AnalyzedBatch,
    /// Original analysis positions, in unchanged caller order.
    pub statement_indices: Vec<usize>,
}

/// Every explicit quote and context source stays with its statement. Never
/// shorten content, cut quotes or silently drop a dependency to fit a plan.
/// Cross-statement revisions/derivations require a separately authored batch.
/// This prepares no authority, policy, journal, root or model invocation.
pub fn pack_analyzed(
    prep: &Preparation,
    analysis: &AnalyzedBatch,
) -> Result<Vec<AnalyzedPlanBatch>, &'static str> {
    if analysis.statements.is_empty() {
        return Err("init.packing_empty_analysis");
    }
    let mut groups: Vec<(BTreeSet<usize>, Vec<usize>)> = Vec::new();
    let mut used = BTreeSet::new();
    for (index, analyzed) in analysis.statements.iter().enumerate() {
        let statement = &analyzed.statement;
        if statement.revises_statement.is_some() || !statement.derives_from_statements.is_empty() {
            return Err("init.packing_statement_links");
        }
        let sources: BTreeSet<_> = std::iter::once(statement.source_index)
            .chain(statement.context_sources.iter().copied())
            .chain(analyzed.supporting_quotes.iter().map(|q| q.source_index))
            .collect();
        if sources.len() > 64 {
            return Err("init.packing_statement_capacity");
        }
        let append = groups.last().is_some_and(|(old, statements)| {
            old.union(&sources).count() <= 64 && statements.len() < 128
        });
        if !append {
            groups.push((BTreeSet::new(), Vec::new()));
        }
        let (group_sources, statements) = groups.last_mut().unwrap();
        group_sources.extend(&sources);
        statements.push(index);
        used.extend(sources);
    }
    let parent = prep.watermark();
    let mut checked_objects = BTreeSet::new();
    let mut branches = BTreeMap::new();
    for &i in &used {
        let message = prep.messages.get(i).ok_or("init.packing_source_index")?;
        if message.text.len() > 16 * 1024 {
            return Err("init.packing_source_capacity");
        }
        if checked_objects.insert(&message.raw_hash) {
            let object = prep
                .objects
                .get(&message.raw_hash)
                .ok_or("init.packing_raw_missing")?;
            if object.len() > 16 * 1024 * 1024 || sha256(object) != message.raw_hash {
                return Err("init.packing_raw_invalid");
            }
        }
        branches.insert(
            i,
            message
                .branch
                .clone()
                .or_else(|| crate::pipeline::branch_lineage(prep, i, 0)),
        );
    }
    // The source selection must bind actual source values, not only positions:
    // filtering a Preparation does not otherwise change its input watermark.
    let source_bindings: Vec<_> = used.iter().map(|&i| {
        let m = &prep.messages[i];
        json!({"index":i,"raw_hash":m.raw_hash,"platform":m.platform,
            "account_alias":m.account_alias,"conversation":m.conversation,"conversation_title":m.conversation_title,
            "message_id":m.upstream_id,"locator":m.locator,"text_hash":sha256(m.text.as_bytes()),
            "sensitivity":m.sensitivity,"metadata":m.metadata,"branch":branches[&i],
            "parent":format!("{:?}",m.parent),"role":m.role,"evidence":m.evidence,
            "original_time":m.original_time,"occurred_at":m.occurred_at,
            "original_timezone":m.original_timezone,"precision":m.precision,
            "author":m.author,"assistant_model":m.assistant_model,"language":m.language,
            "warnings":m.warnings,"agent_submission":m.agent_submission,"completeness":m.completeness,
            "attachments":m.attachments.iter().map(|a|json!({"name":a.original_name,
                "claimed_media_type":a.claimed_media_type,"size_bytes":a.size_bytes,
                "availability":a.availability,"external_reference":a.external_reference,
                "bytes_hash":a.bytes.as_deref().map(sha256)})).collect::<Vec<_>>()})
    }).collect();
    let source_scope = digest(&source_bindings);
    let mut result = Vec::new();
    for (sources, statement_indices) in groups {
        let map: BTreeMap<_, _> = sources
            .iter()
            .enumerate()
            .map(|(i, old)| (*old, i))
            .collect();
        let messages: Vec<_> = sources
            .iter()
            .map(|&i| {
                let mut message = prep.messages[i].clone();
                message.branch = branches[&i].clone();
                message
            })
            .collect();
        let hashes: BTreeSet<_> = messages.iter().map(|m| m.raw_hash.clone()).collect();
        let objects = prep
            .objects
            .iter()
            .filter(|(hash, _)| hashes.contains(*hash))
            .map(|(h, bytes)| (h.clone(), bytes.clone()))
            .collect();
        let inputs = prep
            .inputs
            .iter()
            .filter(|(_, hash, ..)| hashes.contains(hash))
            .cloned()
            .collect();
        let statements: Vec<AnalyzedStatement> = statement_indices
            .iter()
            .map(|&i| {
                let mut s = analysis.statements[i].clone();
                s.statement.source_index = map[&s.statement.source_index];
                for source in &mut s.statement.context_sources {
                    *source = map[source];
                }
                for quote in &mut s.supporting_quotes {
                    quote.source_index = map[&quote.source_index];
                }
                s
            })
            .collect();
        let preparation = Preparation {
            messages,
            objects,
            inputs,
            selection_metadata: json!({"format":"fresh-analyzed-packing/1",
                "parent_watermark":parent,"source_scope":source_scope,
                "source_indices":sources,"statement_indices":statement_indices,
                "parent_selection_metadata":prep.selection_metadata}),
            // These are preparation-wide intake counts; duplicating them across
            // packed plans would fabricate corpus totals. Caller retains prep.
            ..Preparation::default()
        };
        result.push(AnalyzedPlanBatch {
            preparation,
            analysis: AnalyzedBatch {
                provenance: analysis.provenance.clone(),
                statements,
            },
            statement_indices,
        });
    }
    Ok(result)
}
