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
    /// None retains parsed availability; metadata-only also marks non-selection.
    pub original: Option<InputObject>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentRetention {
    #[default]
    SelectedOriginals,
    MetadataOnly,
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
    #[serde(default)]
    pub visible_text_only: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshManifest {
    pub format: String,
    /// Explicit run-wide choice; metadata-only never opens attachment files.
    #[serde(default)]
    pub attachment_retention: AttachmentRetention,
    pub shards: Vec<ShardInput>,
    /// Analysis indexes address this table. All entries must be dependencies.
    pub sources: Vec<SourceInput>,
    pub analysis: AnalyzedBatch,
    /// Unicode scalar [start, end) pairs, primary then every supporting quote,
    /// in original statement order. Repeated text never guesses an occurrence.
    pub quote_ranges: Vec<Vec<(u32, u32)>>,
    #[serde(default)]
    pub context_quotes: Vec<Vec<ContextQuote>>,
    #[serde(default)]
    pub authored_records: Option<InputObject>,
    #[serde(default)]
    pub record_origins: Vec<usize>,
    pub formed_at: Timestamp,
    pub formation_until: Timestamp,
    pub project_id: Option<ProjectId>,
}
pub struct FreshInput {
    pub manifest: FreshManifest,
    pub run_hash: Sha256Hex,
    pub batches: Vec<AnalyzedPlanBatch>,
    pub intake: Value,
    contexts: Vec<Vec<ContextQuote>>,
    origins: Vec<Value>,
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
    let metadata_only = manifest.attachment_retention == AttachmentRetention::MetadataOnly;
    if metadata_only
        && manifest
            .sources
            .iter()
            .flat_map(|s| &s.attachments)
            .any(|a| a.original.is_some())
    {
        return Err("fresh.attachment_retention".into());
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
    let mut metadata_only_attachments = Vec::new();
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
        message.visible_text_only = source.visible_text_only;
        message.attachments_metadata_only = metadata_only;
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
            if metadata_only {
                // Do not redact an embedded payload or silently call it missing.
                // Fresh selected JSON must already be a text/metadata carrier.
                if attachment.bytes.is_some() {
                    return Err("fresh.embedded_attachment_payload".into());
                }
                metadata_only_attachments.push(json!({"source_index":source_position,
                    "slot":binding.slot,"parsed_availability":attachment.availability}));
            } else if let Some(object) = &binding.original {
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
    let mut contexts = manifest.context_quotes.clone();
    if !contexts.is_empty() && contexts.len() != analysis.statements.len() {
        return Err("fresh.context_quotes".into());
    }
    for (index, quotes) in contexts.iter_mut().enumerate() {
        for quote in quotes {
            let statement = &analysis.statements[index];
            if !statement
                .statement
                .context_sources
                .contains(&quote.source_index)
                && statement.statement.source_index != quote.source_index
                && !statement
                    .supporting_quotes
                    .iter()
                    .any(|q| q.source_index == quote.source_index)
            {
                return Err("fresh.context_scope".into());
            }
            let source = &mapped
                .get(quote.source_index)
                .ok_or("fresh.analysis_source_index")?
                .1;
            if quote.start >= quote.end
                || quote.end as usize > source.text.chars().count()
                || source
                    .text
                    .chars()
                    .skip(quote.start as usize)
                    .take((quote.end - quote.start) as usize)
                    .collect::<String>()
                    != quote.quote
            {
                return Err("fresh.context_anchor".into());
            }
            quote.source_index = remap[quote.source_index];
        }
    }
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
    let mut batches = pack_analyzed(&prep, &analysis)?;
    let mut origins = Vec::new();
    if let Some(object) = &manifest.authored_records {
        if manifest.record_origins.len() != analysis.statements.len() {
            return Err("fresh.record_origins".into());
        }
        let raw = read_object(base, object, MAX_MATERIAL_BYTES)?;
        let text = std::str::from_utf8(&raw).map_err(|_| "fresh.authored_records")?;
        let records = text
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "fresh.authored_records")?;
        for (&index, statement) in manifest
            .record_origins
            .iter()
            .zip(&manifest.analysis.statements)
        {
            let record = records.get(index).ok_or("fresh.record_origins")?;
            let id = record["id"]
                .as_str()
                .filter(|id| {
                    !id.trim().is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
                })
                .ok_or("fresh.authored_record_id")?;
            let revision = record["revision"]
                .as_u64()
                .filter(|&r| r > 0 && r <= enouia_memory_contract::json::MAX_SAFE_INTEGER)
                .ok_or("fresh.authored_record_revision")?;
            if record["content"].as_str() != Some(&statement.content)
                || record["title"].as_str() != Some(&statement.statement.title)
            {
                return Err("fresh.authored_record_content".into());
            }
            origins.push(json!({"object_hash":object.sha256,"record_index":index,
                "record_hash":digest(record),"id":id,"revision":revision}));
        }
        for batch in &mut batches {
            batch
                .preparation
                .objects
                .insert(object.sha256.clone(), raw.clone());
        }
    } else if !manifest.record_origins.is_empty() {
        return Err("fresh.record_origins".into());
    }
    let intake = json!({"parsed_messages":parsed_messages,"selected_sources":prep.messages.len(),
        "statements":analysis.statements.len(),"batches":batches.len(),
        "attachments_present":present,"attachments_explicitly_unavailable":unavailable,
        "unavailable_attachments":unavailable_attachments,
        "attachment_retention":manifest.attachment_retention,
        "attachments_metadata_only":metadata_only_attachments.len(),
        "metadata_only_attachments":metadata_only_attachments,
        "visible_text_views":manifest.sources.iter().filter(|s|s.visible_text_only).count(),
        "context_quote_ranges":contexts.iter().map(Vec::len).sum::<usize>(),
        "authored_record_origins":origins.len(),
        "unsupported":prep.unsupported,"unparseable":prep.unparseable,
        "raw_privacy":"complete selected conversations, including surrounding unselected messages; not redacted",
        "external_provider_calls":0,"read_grants":0});
    Ok(FreshInput {
        manifest,
        run_hash,
        batches,
        intake,
        contexts,
        origins,
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
                let mut plan = Plan::build_analyzed_anchored(
                    &batch.preparation,
                    &batch.analysis,
                    policy,
                    FormationMode::Backfill,
                    self.manifest.formed_at.clone(),
                    ids,
                    Some(&ranges),
                )
                .map_err(|e| format!("fresh.plan: {e:?}"))?;
                let mut contexts = Vec::new();
                if !self.contexts.is_empty() {
                    for &i in &batch.statement_indices {
                        let mut quotes = self.contexts[i].clone();
                        for quote in &mut quotes {
                            quote.source_index = batch
                                .source_indices
                                .iter()
                                .position(|&source| source == quote.source_index)
                                .ok_or("fresh.context_scope")?;
                        }
                        contexts.push(quotes);
                    }
                }
                let origins = if self.origins.is_empty() {
                    vec![]
                } else {
                    batch
                        .statement_indices
                        .iter()
                        .map(|&i| self.origins[i].clone())
                        .collect()
                };
                if !contexts.is_empty() || !origins.is_empty() {
                    plan.bind_fresh_provenance(&contexts, &origins)
                        .map_err(|e| format!("fresh.provenance: {e:?}"))?;
                }
                Ok(plan)
            })
            .collect()
    }
}
