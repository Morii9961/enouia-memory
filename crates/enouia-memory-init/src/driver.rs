//! Explicit local fresh input. Files and analysis are data, never authority.
use crate::{
    fresh::{ChatGptSelection, prepare_chatgpt_selection},
    packing::{AnalyzedPlanBatch, pack_analyzed},
    parse::{Format, MAX_MATERIAL_BYTES, Material},
    pipeline::*,
};
use enouia_memory_contract::{
    common::Sensitivity,
    hash::{Sha256Hex, sha256},
    ids::*,
    json::Revision,
    next::*,
    ports::IdSource,
    source::Availability,
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputObject {
    pub path: PathBuf,
    pub sha256: Sha256Hex,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardInput {
    pub alias: String,
    pub account_alias: String,
    pub object: InputObject,
    pub conversation_ids: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentInput {
    pub slot: usize,
    /// None explicitly retains the parsed missing/external attachment.
    pub original: Option<InputObject>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    pub account_alias: String,
    pub conversation_id: String,
    pub message_id: String,
    pub raw_object_hash: Sha256Hex,
    pub text_hash: Sha256Hex,
    pub sensitivity: Sensitivity,
    pub attachments: Vec<AttachmentInput>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshManifest {
    pub format: String,
    pub shards: Vec<ShardInput>,
    /// Analysis indexes address this table. All entries must be dependencies.
    pub sources: Vec<SourceInput>,
    pub analysis: AnalyzedBatch,
    /// Unicode scalar [start, end) pairs, primary then every supporting quote,
    /// in original statement order. Repeated text never guesses an occurrence.
    pub quote_ranges: Vec<Vec<(u32, u32)>>,
    pub formed_at: Timestamp,
    pub formation_until: Timestamp,
    pub project_id: Option<ProjectId>,
}
pub struct FreshInput {
    pub manifest: FreshManifest,
    pub run_hash: Sha256Hex,
    pub batches: Vec<AnalyzedPlanBatch>,
    pub intake: Value,
}

/// Read exactly the caller's file; no discovery, network, Vault or mutation.
fn read_object(base: &Path, object: &InputObject, limit: usize) -> Result<Vec<u8>, String> {
    let bytes = read_file(&base.join(&object.path), limit)?;
    if sha256(&bytes) != object.sha256 {
        return Err("fresh.object_revision".into());
    }
    Ok(bytes)
}
fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|_| "fresh.input_unavailable")?;
    if file
        .metadata()
        .map_err(|_| "fresh.input_unavailable")?
        .len()
        > limit as u64
    {
        return Err("fresh.input_capacity".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "fresh.input_unavailable")?;
    if bytes.len() > limit {
        return Err("fresh.input_capacity".into());
    }
    Ok(bytes)
}

pub fn load_fresh(path: &Path) -> Result<FreshInput, String> {
    let bytes = read_file(path, MAX_MATERIAL_BYTES)?;
    let manifest: FreshManifest =
        serde_json::from_slice(&bytes).map_err(|_| "fresh.manifest_shape")?;
    if manifest.format != "enouia-fresh-input/1"
        || manifest.formation_until.unix_ms() - manifest.formed_at.unix_ms() != WEEK_MS
    {
        return Err("fresh.manifest_scope".into());
    }
    let base = path.parent().ok_or("fresh.manifest_path")?;
    let mut materials = Vec::new();
    let mut selections = Vec::new();
    for shard in &manifest.shards {
        if shard.conversation_ids.is_empty() {
            return Err("fresh.empty_shard_selection".into());
        }
        // Original shards may be larger than a Raw material (ADR140). Bound
        // only each selected conversation envelope with the existing parser.
        materials.push(Material {
            alias: shard.alias.clone(),
            account_alias: shard.account_alias.clone(),
            format: Format::Auto,
            bytes: read_object(base, &shard.object, usize::MAX - 1)?,
        });
        selections.extend(shard.conversation_ids.iter().map(|id| ChatGptSelection {
            shard_alias: shard.alias.clone(),
            conversation_id: id.clone(),
        }));
    }
    let mut prep = prepare_chatgpt_selection(&materials, &selections)?;
    if prep.unparseable != 0 || prep.duplicate_messages != 0 {
        return Err("fresh.incomplete_parse".into());
    }
    let parsed_messages = prep.messages.len();
    let mut seen = BTreeSet::new();
    let mut mapped = Vec::new();
    let mut present = 0;
    let mut unavailable = 0;
    let mut unavailable_attachments = Vec::new();
    for (source_position, source) in manifest.sources.iter().enumerate() {
        let candidates: Vec<_> = prep
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.account_alias == source.account_alias
                    && m.conversation.as_deref() == Some(&source.conversation_id)
                    && m.upstream_id.as_deref() == Some(&source.message_id)
            })
            .collect();
        let [(index, original)] = candidates.as_slice() else {
            return Err("fresh.source_identity".into());
        };
        if !seen.insert(*index)
            || original.raw_hash != source.raw_object_hash
            || sha256(original.text.as_bytes()) != source.text_hash
            || source.sensitivity < original.sensitivity
        {
            return Err("fresh.source_revision".into());
        }
        let mut message = (*original).clone();
        message.branch = message
            .branch
            .clone()
            .or_else(|| branch_lineage(&prep, *index, 0));
        message.sensitivity = source.sensitivity;
        let mut slots = BTreeSet::new();
        for binding in &source.attachments {
            if !slots.insert(binding.slot) {
                return Err("fresh.attachment_duplicate".into());
            }
            let attachment = message
                .attachments
                .get_mut(binding.slot)
                .ok_or("fresh.attachment_slot")?;
            if let Some(object) = &binding.original {
                if attachment.external_reference.is_some()
                    || attachment
                        .original_name
                        .as_deref()
                        .is_some_and(enouia_memory_import::parsed::is_dangerous_name)
                    || object
                        .path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(enouia_memory_import::parsed::is_dangerous_name)
                {
                    return Err("fresh.attachment_original".into());
                }
                let raw = read_object(base, object, MAX_MATERIAL_BYTES)?;
                if enouia_memory_import::parsed::sniff_media_type(&raw)
                    == Some("application/x-msdownload")
                {
                    return Err("fresh.attachment_original".into());
                }
                attachment.size_bytes = Some(raw.len() as u64);
                attachment.bytes = Some(raw);
                attachment.availability = Availability::Present;
                present += 1;
            } else {
                unavailable += 1;
                unavailable_attachments.push(json!({"source_index":source_position,
                    "slot":binding.slot,"availability":attachment.availability}));
            }
        }
        if slots.len() != message.attachments.len() {
            return Err("fresh.attachment_unbound".into());
        }
        mapped.push((*index, message));
    }
    // Preserve original parser time ordering, then remap only analysis indexes.
    let mut order: Vec<_> = (0..mapped.len()).collect();
    order.sort_by_key(|&i| mapped[i].0);
    let mut remap = vec![0; order.len()];
    for (new, &old) in order.iter().enumerate() {
        remap[old] = new;
    }
    let mut analysis = manifest.analysis.clone();
    if manifest.quote_ranges.len() != analysis.statements.len() {
        return Err("fresh.quote_ranges".into());
    }
    let mut used = BTreeSet::new();
    for (analyzed, ranges) in analysis.statements.iter_mut().zip(&manifest.quote_ranges) {
        let quotes = std::iter::once((analyzed.statement.source_index, &analyzed.statement.quote))
            .chain(
                analyzed
                    .supporting_quotes
                    .iter()
                    .map(|q| (q.source_index, &q.quote)),
            )
            .collect::<Vec<_>>();
        if quotes.len() != ranges.len() {
            return Err("fresh.quote_ranges".into());
        }
        for ((index, quote), &(start, end)) in quotes.into_iter().zip(ranges) {
            let message = &mapped.get(index).ok_or("fresh.analysis_source_index")?.1;
            if start >= end
                || end as usize > message.text.chars().count()
                || message
                    .text
                    .chars()
                    .skip(start as usize)
                    .take((end - start) as usize)
                    .collect::<String>()
                    != *quote
            {
                return Err("fresh.quote_anchor".into());
            }
        }
        for index in std::iter::once(&mut analyzed.statement.source_index)
            .chain(&mut analyzed.statement.context_sources)
            .chain(
                analyzed
                    .supporting_quotes
                    .iter_mut()
                    .map(|q| &mut q.source_index),
            )
        {
            used.insert(*index);
            *index = *remap.get(*index).ok_or("fresh.analysis_source_index")?;
        }
    }
    if used.len() != mapped.len() || mapped.is_empty() {
        return Err("fresh.unused_source".into());
    }
    prep.messages = order.iter().map(|&i| mapped[i].1.clone()).collect();
    let run_hash = sha256(&bytes);
    prep.selection_metadata = json!({"format":"fresh-native-driver/1", "manifest_hash":run_hash,
        "original_selection":prep.selection_metadata});
    let batches = pack_analyzed(&prep, &analysis)?;
    let intake = json!({"parsed_messages":parsed_messages,"selected_sources":prep.messages.len(),
        "statements":analysis.statements.len(),"batches":batches.len(),
        "attachments_present":present,"attachments_explicitly_unavailable":unavailable,
        "unavailable_attachments":unavailable_attachments,
        "unsupported":prep.unsupported,"unparseable":prep.unparseable,
        "raw_privacy":"complete selected conversations, including surrounding unselected messages; not redacted",
        "external_provider_calls":0,"read_grants":0});
    Ok(FreshInput {
        manifest,
        run_hash,
        batches,
        intake,
    })
}

impl FreshInput {
    pub fn plans(
        &self,
        descriptor: &AutomationDescriptor,
        ids: &dyn IdSource,
    ) -> Result<Vec<Plan>, String> {
        self.batches
            .iter()
            .map(|batch| {
                let policy = AutomationPolicy {
                    schema_version: NextVersion,
                    kind: PolicyKind::AutomationPolicy,
                    policy_id: PolicyId::from_random(ids.random_16()),
                    revision: Revision::new(1).unwrap(),
                    rule_version: RuleVersion::V3,
                    system_actor: descriptor.system_actor.clone(),
                    vault_id: descriptor.vault_id.clone(),
                    subject_id: descriptor
                        .owner_subject_id
                        .clone()
                        .ok_or("fresh.owner_subject")?,
                    project_id: self.manifest.project_id.clone(),
                    source_ids: vec![],
                    allow_create: true,
                    allow_revise_created: false,
                    target_ids: vec![],
                    allowed_forms: batch
                        .analysis
                        .statements
                        .iter()
                        .map(|s| s.statement.dimensions.kind)
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                    max_sensitivity: batch
                        .preparation
                        .messages
                        .iter()
                        .map(|m| m.sensitivity)
                        .max()
                        .unwrap_or(Sensitivity::Private),
                    access_policy_id: PolicyId::from_random(ids.random_16()),
                    egress_policy_id: PolicyId::from_random(ids.random_16()),
                    active: true,
                    valid_from: self.manifest.formed_at.clone(),
                    valid_until: self.manifest.formation_until.clone(),
                };
                let ranges = batch
                    .statement_indices
                    .iter()
                    .map(|&i| self.manifest.quote_ranges[i].clone())
                    .collect::<Vec<_>>();
                Plan::build_analyzed_anchored(
                    &batch.preparation,
                    &batch.analysis,
                    policy,
                    FormationMode::Backfill,
                    self.manifest.formed_at.clone(),
                    ids,
                    Some(&ranges),
                )
                .map_err(|e| format!("fresh.plan: {e:?}"))
            })
            .collect()
    }
}
