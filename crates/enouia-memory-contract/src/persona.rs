//! Public persona projection v1 (ADR-MEM-128): the closed document an owner
//! exports for a downstream public-audience consumer.
//!
//! Only two kinds of content may appear: the public block of one exact
//! Identity revision, and Memory records that are `public` and explicitly
//! selected by the owner for the audience. Everything the consumer reads may
//! be extracted by chat participants and is sent to a model provider, so
//! every string is checked here once, at the boundary that owns it.

use crate::error::{ContractError, Violation};
use crate::hash::{Sha256Hex, sha256};
use crate::ids::{IdentityId, MemoryId};
use crate::json::{Revision, canonical_bytes, check_safe_integers};
use crate::scan::{contains_local_path, contains_secret_material};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The Markdown fence info string that marks an Identity revision's public part.
pub const IDENTITY_BLOCK_INFO: &str = "enouia-public-persona-1";
pub const MAX_KNOWLEDGE_ITEMS: usize = 64;
pub const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ContractName {
    #[serde(rename = "public-persona-projection-1")]
    V1,
}

/// What the projection is for. v1 defines one audience.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    PublicGroup,
}

impl Audience {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PublicGroup => "public_group",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonaSettings {
    pub background: String,
    pub personality: String,
    pub voice: String,
    pub motivation: String,
    pub values: String,
    pub rules: String,
    pub limits: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub rules: Vec<String>,
    pub anti_examples: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    pub speaker: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub turns: Vec<Turn>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundaries {
    pub refuse: Vec<String>,
    pub ai_disclosure: String,
    pub avoid_topics: Vec<String>,
    pub forbidden_words: Vec<String>,
}

/// The JSON body of the `enouia-public-persona-1` fence in Identity Markdown.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityBlock {
    pub display_name: String,
    pub persona: PersonaSettings,
    pub style: Style,
    pub examples: Vec<Example>,
    pub interests: Vec<String>,
    pub boundaries: Boundaries,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionSource {
    pub projection_id: String,
    pub identity_id: IdentityId,
    pub identity_revision: Revision,
    pub contract: ContractName,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSource {
    pub memory_id: MemoryId,
    pub memory_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeItem {
    pub item_id: String,
    pub text: String,
    pub source: KnowledgeSource,
    pub tags: Vec<String>,
}

/// Everything the consumer may use. `projection_sha256` covers exactly this.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    pub projection_id: String,
    pub audience: Audience,
    pub source: ProjectionSource,
    pub display_name: String,
    pub persona: PersonaSettings,
    pub style: Style,
    pub examples: Vec<Example>,
    pub interests: Vec<String>,
    pub public_knowledge: Vec<KnowledgeItem>,
    pub boundaries: Boundaries,
}

/// The exported file: the projection plus its digest, version and export facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub contract: ContractName,
    pub projection: Projection,
    pub projection_sha256: Sha256Hex,
    pub version: u64,
    pub exported_at: Timestamp,
    pub memory_version: String,
}

fn hex32(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|rest| {
        rest.len() == 32 && rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn derive(prefix: &str, input: &str) -> String {
    format!("{prefix}{}", &sha256(input.as_bytes()).as_str()[..32])
}

/// Stable per Vault and audience, without stored state.
pub fn projection_id(vault_id: &str, audience: Audience) -> String {
    derive(
        "ppj_",
        &format!(
            "public-persona-projection-1\n{vault_id}\n{}",
            audience.as_str()
        ),
    )
}

/// Stable across revisions of the same record within one projection.
pub fn item_id(projection_id: &str, memory_id: &MemoryId) -> String {
    derive(
        "pkn_",
        &format!("public-persona-item-1\n{projection_id}\n{memory_id}"),
    )
}

/// SHA-256 over the canonical bytes of the projection object.
pub fn projection_sha256(projection: &Projection) -> Sha256Hex {
    sha256(&canonical_bytes(projection).expect("projection serializes"))
}

/// One text field: bounded, no control characters except LF, no credential- or
/// path-shaped material.
fn text(value: &str, max: usize, path: &str, out: &mut Vec<Violation>) {
    let chars = value.chars().count();
    if chars == 0 || chars > max || value.trim().is_empty() {
        out.push(Violation::new("persona.text_length", path));
    }
    if value.chars().any(|c| c.is_control() && c != '\n') {
        out.push(Violation::new("persona.control_character", path));
    }
    if contains_secret_material(value) {
        out.push(Violation::new("persona.secret_material", path));
    }
    if contains_local_path(value) {
        out.push(Violation::new("persona.local_path", path));
    }
}

fn list(values: &[String], max_items: usize, max: usize, path: &str, out: &mut Vec<Violation>) {
    if values.len() > max_items {
        out.push(Violation::new("persona.list_length", path));
    }
    if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        out.push(Violation::new("persona.list_duplicate", path));
    }
    for (i, value) in values.iter().enumerate() {
        text(value, max, &format!("{path}/{i}"), out);
    }
}

impl IdentityBlock {
    pub fn validate(&self, base: &str, out: &mut Vec<Violation>) {
        text(&self.display_name, 64, &format!("{base}/display_name"), out);
        let p = &self.persona;
        for (name, value) in [
            ("background", &p.background),
            ("personality", &p.personality),
            ("voice", &p.voice),
            ("motivation", &p.motivation),
            ("values", &p.values),
            ("rules", &p.rules),
            ("limits", &p.limits),
        ] {
            text(value, 4000, &format!("{base}/persona/{name}"), out);
        }
        list(
            &self.style.rules,
            32,
            500,
            &format!("{base}/style/rules"),
            out,
        );
        list(
            &self.style.anti_examples,
            32,
            500,
            &format!("{base}/style/anti_examples"),
            out,
        );
        if !(3..=8).contains(&self.examples.len()) {
            out.push(Violation::new(
                "persona.example_count",
                format!("{base}/examples"),
            ));
        }
        for (i, example) in self.examples.iter().enumerate() {
            let at = format!("{base}/examples/{i}/turns");
            if !(2..=16).contains(&example.turns.len()) {
                out.push(Violation::new("persona.turn_count", &at));
            }
            for (j, turn) in example.turns.iter().enumerate() {
                text(&turn.speaker, 64, &format!("{at}/{j}/speaker"), out);
                text(&turn.text, 2000, &format!("{at}/{j}/text"), out);
            }
        }
        list(&self.interests, 32, 200, &format!("{base}/interests"), out);
        let b = &self.boundaries;
        list(
            &b.refuse,
            64,
            500,
            &format!("{base}/boundaries/refuse"),
            out,
        );
        text(
            &b.ai_disclosure,
            1000,
            &format!("{base}/boundaries/ai_disclosure"),
            out,
        );
        list(
            &b.avoid_topics,
            64,
            200,
            &format!("{base}/boundaries/avoid_topics"),
            out,
        );
        list(
            &b.forbidden_words,
            128,
            64,
            &format!("{base}/boundaries/forbidden_words"),
            out,
        );
    }
}

impl Projection {
    /// Build from a validated block, the exact Identity revision and the
    /// eligible items (sorted here by item ID).
    pub fn new(
        projection_id: String,
        audience: Audience,
        identity_id: IdentityId,
        identity_revision: Revision,
        block: IdentityBlock,
        mut public_knowledge: Vec<KnowledgeItem>,
    ) -> Self {
        public_knowledge.sort_by(|a, b| a.item_id.cmp(&b.item_id));
        Self {
            source: ProjectionSource {
                projection_id: projection_id.clone(),
                identity_id,
                identity_revision,
                contract: ContractName::V1,
            },
            projection_id,
            audience,
            display_name: block.display_name,
            persona: block.persona,
            style: block.style,
            examples: block.examples,
            interests: block.interests,
            public_knowledge,
            boundaries: block.boundaries,
        }
    }

    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if !hex32(&self.projection_id, "ppj_") {
            out.push(Violation::new("persona.projection_id", "/projection_id"));
        }
        if self.source.projection_id != self.projection_id {
            out.push(Violation::new(
                "persona.source_projection",
                "/source/projection_id",
            ));
        }
        let block = IdentityBlock {
            display_name: self.display_name.clone(),
            persona: self.persona.clone(),
            style: self.style.clone(),
            examples: self.examples.clone(),
            interests: self.interests.clone(),
            boundaries: self.boundaries.clone(),
        };
        block.validate("", &mut out);
        if self.public_knowledge.len() > MAX_KNOWLEDGE_ITEMS {
            out.push(Violation::new(
                "persona.knowledge_count",
                "/public_knowledge",
            ));
        }
        for (i, item) in self.public_knowledge.iter().enumerate() {
            let at = format!("/public_knowledge/{i}");
            if !hex32(&item.item_id, "pkn_") {
                out.push(Violation::new("persona.item_id", format!("{at}/item_id")));
            }
            if i > 0 && self.public_knowledge[i - 1].item_id >= item.item_id {
                out.push(Violation::new(
                    "persona.item_order",
                    format!("{at}/item_id"),
                ));
            }
            text(&item.text, 20_000, &format!("{at}/text"), &mut out);
            if item.tags.len() > 32
                || item.tags.iter().any(|t| !crate::memory::is_label(t))
                || item.tags.iter().collect::<BTreeSet<_>>().len() != item.tags.len()
            {
                out.push(Violation::new("persona.tags", format!("{at}/tags")));
            }
        }
        out
    }
}

impl Document {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out: Vec<Violation> = self
            .projection
            .validate()
            .into_iter()
            .map(|v| Violation::new(v.rule, format!("/projection{}", v.path)))
            .collect();
        if projection_sha256(&self.projection) != self.projection_sha256 {
            out.push(Violation::new(
                "persona.projection_sha256",
                "/projection_sha256",
            ));
        }
        if self.version == 0 {
            out.push(Violation::new("persona.version", "/version"));
        }
        let version = &self.memory_version;
        if version.is_empty()
            || version.len() > 64
            || !version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.+/".contains(&b))
        {
            out.push(Violation::new("persona.memory_version", "/memory_version"));
        }
        out
    }
}

/// Parse a projection document from its bytes, as a consumer would: closed
/// shape, safe integers, every semantic rule and the digest.
pub fn parse_document(bytes: &[u8]) -> Result<Document, ContractError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(ContractError::Invalid(vec![Violation::new(
            "persona.document_size",
            "/",
        )]));
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ContractError::Malformed)?;
    if !value.is_object() {
        return Err(ContractError::Malformed);
    }
    let mut out = Vec::new();
    check_safe_integers(&value, "", &mut out);
    if !out.is_empty() {
        return Err(ContractError::Invalid(out));
    }
    let document: Document =
        serde_json::from_value(value).map_err(|e| ContractError::Shape(e.to_string()))?;
    let out = document.validate();
    if out.is_empty() {
        Ok(document)
    } else {
        Err(ContractError::Invalid(out))
    }
}

/// The public block of an Identity revision and how much was left out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractedBlock {
    pub block: IdentityBlock,
    /// Non-blank Markdown lines outside the block (fence lines excluded).
    pub lines_outside: u64,
}

/// Find exactly one `enouia-public-persona-1` fence and parse its closed body.
/// Rules name the failure; the Markdown text never appears in an error.
pub fn extract_block(markdown: &str) -> Result<ExtractedBlock, ContractError> {
    let invalid = |rule| ContractError::Invalid(vec![Violation::new(rule, "/identity_markdown")]);
    let opener = format!("```{IDENTITY_BLOCK_INFO}");
    let lines: Vec<&str> = markdown
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let openers: Vec<usize> = (0..lines.len())
        .filter(|i| lines[*i].trim_end() == opener)
        .collect();
    let start = match openers.as_slice() {
        [] => return Err(invalid("persona.block_missing")),
        [one] => *one,
        _ => return Err(invalid("persona.block_repeated")),
    };
    let end = (start + 1..lines.len())
        .find(|i| lines[*i].trim_end() == "```")
        .ok_or_else(|| invalid("persona.block_unclosed"))?;
    let body = lines[start + 1..end].join("\n");
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| invalid("persona.block_json"))?;
    if !value.is_object() {
        return Err(invalid("persona.block_json"));
    }
    let mut out = Vec::new();
    check_safe_integers(&value, "/identity_block", &mut out);
    let block: IdentityBlock = serde_json::from_value(value)
        .map_err(|e| ContractError::Shape(format!("identity block: {e}")))?;
    block.validate("/identity_block", &mut out);
    if !out.is_empty() {
        return Err(ContractError::Invalid(out));
    }
    let lines_outside = lines
        .iter()
        .enumerate()
        .filter(|(i, l)| (*i < start || *i > end) && !l.trim().is_empty())
        .count() as u64;
    Ok(ExtractedBlock {
        block,
        lines_outside,
    })
}
