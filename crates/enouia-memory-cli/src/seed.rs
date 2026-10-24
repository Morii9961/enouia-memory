//! Offline bridge from an inspected, hash-bound seed package to native
//! imports and pending agent proposals. Never approves or sends anything.
use super::{Failure, Outcome, genesis_policy, owner_of};
use enouia_memory_contract::candidate::ProposedType;
use enouia_memory_contract::common::{
    ActorRef, ActorType, EvidenceClass, Locator, Sensitivity, SourceRevisionRef, SpeakerRole,
};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::PrincipalId;
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::record::{RecordKind, RecordRef, parse_record};
use enouia_memory_contract::source::SourceRecord;
use enouia_memory_govern::{EvidenceSpec, Origin, Proposal, Proposed, propose_bound_at_head};
use enouia_memory_import::{ImportOptions, import_file};
use enouia_memory_vault::{Vault, VaultError};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    name: String,
    sha256: Sha256Hex,
    sensitivity: Sensitivity,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Citation {
    input: String,
    locator: Locator,
    content_hash: Sha256Hex,
    evidence_class: EvidenceClass,
    speaker_role: SpeakerRole,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Seed {
    seed_id: String,
    revision: u64,
    proposed_type: ProposedType,
    content: String,
    details: Map<String, Value>,
    evidence: Vec<Citation>,
    context: Vec<Citation>,
    sensitivity: Sensitivity,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    format: String,
    account_alias: String,
    inputs: Vec<Input>,
    seeds: Vec<Seed>,
}

pub(super) struct Loaded {
    package: Package,
    digest: Sha256Hex,
    directory: PathBuf,
}

fn invalid(rule: &'static str) -> Failure {
    Failure::Vault(VaultError::invalid(vec![rule]))
}

fn bytes(path: &Path) -> Result<Vec<u8>, Failure> {
    let meta = std::fs::symlink_metadata(path).map_err(VaultError::from)?;
    if !meta.is_file()
        || enouia_memory_vault::platform::is_reparse_point(&meta)
        || meta.len() > MAX_BYTES
    {
        return Err(invalid("seed.input_file"));
    }
    let read = || -> Result<Vec<u8>, Failure> {
        let mut out = Vec::new();
        std::fs::File::open(path)
            .map_err(VaultError::from)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(VaultError::from)?;
        if out.len() as u64 != meta.len() {
            return Err(invalid("seed.input_changed"));
        }
        Ok(out)
    };
    let first = read()?;
    if sha256(&first) != sha256(&read()?) {
        return Err(invalid("seed.input_changed"));
    }
    Ok(first)
}

pub(super) fn load(path: &Path, expected: Option<&str>) -> Result<Loaded, Failure> {
    let raw = bytes(path)?;
    let digest = sha256(&raw);
    if expected.is_some_and(|hash| hash != digest.as_str()) {
        return Err(invalid("seed.package_hash"));
    }
    let package: Package =
        serde_json::from_slice(&raw).map_err(|_| invalid("seed.package_format"))?;
    if package.format != "enouia.native-seed-stage/1"
        || package.seeds.is_empty()
        || package.seeds.len() > 1000
        || !enouia_memory_contract::source::is_account_alias(&package.account_alias)
    {
        return Err(invalid("seed.package_format"));
    }
    let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut parsed = BTreeMap::new();
    let mut raw_inputs = BTreeMap::new();
    for input in &package.inputs {
        if input.name.is_empty()
            || input.name.contains(['/', '\\', ':'])
            || input.name == "."
            || input.name == ".."
            || parsed.contains_key(&input.name)
            || input.sensitivity < Sensitivity::Private
        {
            return Err(invalid("seed.input_name"));
        }
        let raw = bytes(&directory.join(&input.name))?;
        if sha256(&raw) != input.sha256 {
            return Err(invalid("seed.input_hash"));
        }
        let enouia_memory_import::detect::Detection::Parsed(data) =
            enouia_memory_import::detect::detect(&raw, &Default::default())
        else {
            return Err(invalid("seed.input_format"));
        };
        if data.units.iter().any(|unit| unit.unparseable != 0) {
            return Err(invalid("seed.unparseable"));
        }
        parsed.insert(input.name.clone(), data);
        raw_inputs.insert(input.name.clone(), raw);
    }
    let mut ids = BTreeSet::new();
    for seed in &package.seeds {
        if let Some(subjects) = seed.details.get("subject_ids") {
            serde_json::from_value::<Vec<enouia_memory_contract::ids::SubjectId>>(subjects.clone())
                .map_err(|_| invalid("seed.subject_ids"))?;
        }
        if let Some(participants) = seed.details.get("participants") {
            serde_json::from_value::<Vec<enouia_memory_contract::ids::SubjectId>>(
                participants.clone(),
            )
            .map_err(|_| invalid("seed.participants"))?;
        }
        for field in ["state", "decisions", "open_loops"] {
            if let Some(items) = seed.details.get(field).and_then(Value::as_array) {
                for item in items {
                    let id = item
                        .get("item_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("seed.item_id"))?;
                    enouia_memory_contract::ids::ItemId::parse(id)
                        .map_err(|_| invalid("seed.item_id"))?;
                }
            }
        }
        if seed.seed_id.is_empty()
            || seed.seed_id.len() > 100
            || seed.revision == 0
            || !ids.insert(&seed.seed_id)
            || seed.evidence.is_empty()
            || matches!(
                seed.proposed_type,
                ProposedType::Identity | ProposedType::SessionCheckpoint
            )
        {
            return Err(invalid("seed.record"));
        }
        for cite in seed.evidence.iter().chain(&seed.context) {
            let input = package
                .inputs
                .iter()
                .find(|i| i.name == cite.input)
                .ok_or_else(|| invalid("seed.input_missing"))?;
            if seed.sensitivity < input.sensitivity {
                return Err(invalid("seed.sensitivity_downgrade"));
            }
            let matches = parsed[&cite.input]
                .units
                .iter()
                .flat_map(|u| &u.messages)
                .filter(|m| {
                    m.locator == cite.locator
                        && m.content_hash == cite.content_hash
                        && m.evidence == cite.evidence_class
                        && m.role == cite.speaker_role
                })
                .count();
            if matches != 1 {
                return Err(invalid("seed.citation_mismatch"));
            }
            let text =
                enouia_memory_import::locate::resolve(&raw_inputs[&cite.input], &cite.locator)
                    .map_err(|_| invalid("seed.locator"))?;
            if sha256(&text) != cite.content_hash
                || enouia_memory_contract::scan::contains_secret_material(&String::from_utf8_lossy(
                    &text,
                ))
            {
                return Err(invalid("seed.source_content"));
            }
        }
    }
    Ok(Loaded {
        package,
        digest,
        directory,
    })
}

pub(super) fn preview(path: &Path) -> Outcome {
    let loaded = load(path, None)?;
    Ok(
        json!({"package_sha256":loaded.digest,"inputs":loaded.package.inputs.len(),"candidates":loaded.package.seeds.len(),
        "canonical_writes":0,"external_sends":0,"requires_owner_review":true}),
    )
}

fn resolve<'a>(
    cite: &Citation,
    loaded: &Loaded,
    sources: &'a [SourceRecord],
) -> Result<&'a SourceRecord, Failure> {
    let input = loaded
        .package
        .inputs
        .iter()
        .find(|i| i.name == cite.input)
        .ok_or_else(|| invalid("seed.input_missing"))?;
    let mut found = sources.iter().filter(|s| {
        s.raw_object_hash.as_ref() == Some(&input.sha256)
            && s.locator == cite.locator
            && s.content_hash == cite.content_hash
            && s.evidence_class == cite.evidence_class
            && s.speaker_role == cite.speaker_role
    });
    let source = found
        .next()
        .ok_or_else(|| invalid("seed.native_source_missing"))?;
    if found.next().is_some() {
        return Err(invalid("seed.native_source_ambiguous"));
    }
    Ok(source)
}

pub(super) fn stage(vault: &Vault, loaded: &Loaded) -> Outcome {
    let owner = owner_of(vault);
    for input in &loaded.package.inputs {
        let mut options = ImportOptions::new(
            owner.clone(),
            &loaded.package.account_alias,
            genesis_policy(vault)?,
        );
        options.sensitivity = input.sensitivity;
        let report = import_file(
            vault,
            &loaded.directory.join(&input.name),
            &options,
            &enouia_memory_import::pipeline::NeverCancel,
        )?;
        if report.manifest.input_object_hash != input.sha256
            || !enouia_memory_import::audit_import(vault, &report.import_id)?.is_consistent()
        {
            return Err(invalid("seed.native_import_mismatch"));
        }
    }
    let pin = vault.pin_current()?;
    let mut sources = Vec::new();
    for entry in vault.record_entries(&pin, RecordKind::Source)? {
        sources.push(
            parse_record::<SourceRecord>(&vault.read_record(
                &pin,
                &RecordRef::new(RecordKind::Source, &entry.record_id, entry.revision),
            )?)
            .map_err(|_| invalid("seed.native_source"))?,
        );
    }
    // Stable agent principal per Vault; never impersonate the genesis owner.
    let actor_hash = sha256(format!("seed-agent:{}", vault.vault_id()).as_bytes());
    let mut id = [0u8; 16];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&actor_hash.as_str()[index * 2..index * 2 + 2], 16)
            .expect("hex hash");
    }
    let origin = Origin::agent(ActorRef {
        actor_id: PrincipalId::from_random(id),
        actor_type: ActorType::Agent,
    });
    let mut proposals = Vec::new();
    for seed in &loaded.package.seeds {
        let cited: Vec<_> = seed
            .evidence
            .iter()
            .map(|c| resolve(c, loaded, &sources))
            .collect::<Result<_, _>>()?;
        let evidence = cited
            .iter()
            .map(|s| EvidenceSpec::content(s.source_id.clone(), s.revision))
            .collect();
        let mut details = seed.details.clone();
        if seed.proposed_type == ProposedType::ProjectState {
            for field in ["state", "decisions", "open_loops"] {
                if let Some(items) = details.get_mut(field).and_then(Value::as_array_mut) {
                    for item in items {
                        item["evidence_refs"] = json!(
                            cited
                                .iter()
                                .map(|s| SourceRevisionRef {
                                    source_id: s.source_id.clone(),
                                    source_revision: s.revision
                                })
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
        let mut proposal = Proposal::create(seed.proposed_type, &seed.content, details, evidence);
        proposal.context_source_refs = seed
            .context
            .iter()
            .map(|c| {
                resolve(c, loaded, &sources).map(|s| SourceRevisionRef {
                    source_id: s.source_id.clone(),
                    source_revision: s.revision,
                })
            })
            .collect::<Result<_, _>>()?;
        proposal.context_source_refs.sort();
        proposal.context_source_refs.dedup();
        proposal.sensitivity = Some(seed.sensitivity);
        proposal.reason = seed.reason.clone();
        // Bind all inspected bytes, including source classes and context, to
        // this publication key. Changed packages cannot silently reuse it.
        let payload = sha256(
            &canonical_bytes(
                &json!({"package":loaded.digest,"id":seed.seed_id,"revision":seed.revision}),
            )
            .map_err(|_| invalid("seed.payload"))?,
        );
        proposals.push((seed, proposal, payload));
    }
    let mut outcomes = Vec::new();
    for (seed, proposal, payload) in proposals {
        let key = format!("seed-stage:{}:{}", seed.seed_id, seed.revision);
        let result = propose_bound_at_head(
            vault,
            &proposal,
            &origin,
            key.as_bytes(),
            &payload,
            &vault.pin_current()?.commit_id,
        )?;
        let (id, state) = match result {
            Proposed::Stored(w) => (w.id, "stored_or_replayed"),
            Proposed::DuplicateOf(id) => (id, "duplicate_of"),
        };
        outcomes.push(json!({"seed_id":seed.seed_id,"candidate_id":id,"state":state}));
    }
    Ok(
        json!({"package_sha256":loaded.digest,"candidates":outcomes,"canonical_writes":0,"external_sends":0,"requires_owner_review":true}),
    )
}

/// Validate every pending seed's exact native accept plan without confirming
/// it. Private content remains in memory; stdout contains counts/IDs only.
pub(super) fn review_check(vault: &Vault) -> Outcome {
    let candidates = enouia_memory_govern::pending_candidates(vault, &vault.pin_current()?)?;
    let mut checked = 0;
    let mut failed = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|c| c.origin_kind == enouia_memory_contract::candidate::OriginKind::AgentProposal)
    {
        let decision = enouia_memory_govern::Decision::Accept {
            candidate_id: candidate.candidate_id.clone(),
            revision: candidate.revision,
        };
        match enouia_memory_govern::plan(
            vault,
            &[decision],
            &owner_of(vault),
            enouia_memory_contract::common::TrustedSurface::TrustedLocalCli,
        ) {
            Ok(_) => checked += 1,
            Err(error) => failed.push(
                json!({"candidate_id":candidate.candidate_id,"fault":format!("{:?}",error.fault)}),
            ),
        }
    }
    Ok(json!({"checked":checked,"failed":failed,"canonical_writes":0,"confirmed":false}))
}
