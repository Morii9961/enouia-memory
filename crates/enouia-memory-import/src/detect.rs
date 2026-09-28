//! Format detection by content (never by file name or extension alone).
//! Unrecognized input is reported as unsupported with its members listed;
//! the pipeline has already archived its bytes.

use crate::chatgpt::{self, FileLookup};
use crate::markdown;
use crate::parsed::{ParsedInput, is_dangerous_name, warning};
use crate::runtime;
use crate::zip::{Archive, ZipLimits, looks_like_zip};
use enouia_memory_contract::common::Warning;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::import::{
    AdapterRef, ArchiveMember, CursorUnit, InputKind, MemberDisposition,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub enum Detection {
    Parsed(ParsedInput),
    Unsupported {
        input_kind: InputKind,
        members: Vec<ArchiveMember>,
        warnings: Vec<Warning>,
    },
}

fn adapter(name: &str, version: &str) -> AdapterRef {
    AdapterRef {
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

/// Member bytes read from an archive, with the dispositions to report.
struct Members {
    entries: Vec<ArchiveMember>,
    bytes: BTreeMap<String, (Vec<u8>, Sha256Hex)>,
}

fn read_members(archive: &Archive<'_>) -> Members {
    let mut entries = Vec::new();
    let mut bytes = BTreeMap::new();
    for (index, entry) in archive.entries.iter().enumerate() {
        if entry.is_dir && entry.problem.is_none() {
            continue;
        }
        let problem = entry
            .problem
            .map(|p| p.code())
            .or_else(|| is_dangerous_name(&entry.name).then_some("executable_member"));
        let safe_name = if entry.problem == Some(crate::zip::MemberProblem::UnsafeName) {
            format!("unsafe-member-{index}")
        } else {
            entry.name.trim_end_matches('/').to_owned()
        };
        let read = match problem {
            Some(code) => Err(code),
            None => archive.read(entry).map_err(|p| p.code()),
        };
        match read {
            Ok(data) => {
                let hash = sha256(&data);
                entries.push(ArchiveMember {
                    member_name: safe_name.clone(),
                    member_hash: Some(hash.clone()),
                    size_bytes: data.len() as u64,
                    disposition: MemberDisposition::Skipped,
                    reason_code: None,
                });
                bytes.insert(safe_name, (data, hash));
            }
            Err(code) => entries.push(ArchiveMember {
                member_name: safe_name,
                member_hash: None,
                size_bytes: entry.uncompressed_size(),
                disposition: MemberDisposition::Quarantined,
                reason_code: Some(code.to_owned()),
            }),
        }
    }
    Members { entries, bytes }
}

struct ArchiveFiles<'a> {
    members: &'a Members,
}

impl FileLookup for ArchiveFiles<'_> {
    fn find(&self, file_id: &str) -> Option<Result<(String, Vec<u8>), ()>> {
        if file_id.is_empty() {
            return None;
        }
        for member in &self.members.entries {
            let base = member.member_name.rsplit('/').next().unwrap_or("");
            if base == file_id
                || base.starts_with(&format!("{file_id}-"))
                || base.starts_with(&format!("{file_id}."))
            {
                return Some(match self.members.bytes.get(&member.member_name) {
                    Some((data, _)) if !is_dangerous_name(base) => {
                        Ok((member.member_name.clone(), data.clone()))
                    }
                    _ => Err(()),
                });
            }
        }
        None
    }
}

fn mark(members: &mut [ArchiveMember], name: &str, disposition: MemberDisposition) {
    if let Some(m) = members.iter_mut().find(|m| m.member_name == name)
        && m.disposition != MemberDisposition::Quarantined
    {
        m.disposition = disposition;
    }
}

fn detect_archive(bytes: &[u8], limits: &ZipLimits) -> Detection {
    let archive = match Archive::open(bytes, limits.clone()) {
        Ok(archive) => archive,
        Err(error) => {
            return Detection::Unsupported {
                input_kind: InputKind::UnknownArchive,
                members: Vec::new(),
                warnings: vec![warning(error.code(), None)],
            };
        }
    };
    let members = read_members(&archive);
    let mut entries = members.entries.clone();
    let conversations = entries
        .iter()
        .find(|m| {
            m.member_name == "conversations.json" && m.disposition != MemberDisposition::Quarantined
        })
        .map(|m| m.member_name.clone());
    if let Some(name) = conversations {
        let (data, hash) = &members.bytes[&name];
        let files = ArchiveFiles { members: &members };
        match chatgpt::parse(data, Some((&name, hash)), &files) {
            Ok(units) => {
                mark(&mut entries, &name, MemberDisposition::Parsed);
                for m in entries.iter_mut() {
                    if m.disposition == MemberDisposition::Skipped {
                        m.disposition = MemberDisposition::PreservedOnly;
                    }
                }
                return Detection::Parsed(ParsedInput {
                    input_kind: InputKind::ChatgptExportZip,
                    adapter: adapter(chatgpt::ADAPTER, chatgpt::VERSION),
                    source_schema_observed: chatgpt::SCHEMA.to_owned(),
                    provider: Some("chatgpt".to_owned()),
                    unit_kind: CursorUnit::Conversation,
                    members: entries,
                    units,
                    warnings: Vec::new(),
                });
            }
            Err(code) => {
                return Detection::Unsupported {
                    input_kind: InputKind::ChatgptExportZip,
                    members: entries,
                    warnings: vec![warning(code, Some("conversations.json".to_owned()))],
                };
            }
        }
    }
    let usable: Vec<&ArchiveMember> = entries
        .iter()
        .filter(|m| m.disposition != MemberDisposition::Quarantined)
        .collect();
    let markdown_only = !usable.is_empty()
        && usable.iter().all(|m| {
            let lower = m.member_name.to_lowercase();
            lower.ends_with(".md") || lower.ends_with(".markdown")
        });
    if markdown_only {
        let names: Vec<String> = usable.iter().map(|m| m.member_name.clone()).collect();
        let mut units = Vec::new();
        for name in &names {
            let (data, hash) = &members.bytes[name];
            units.push(markdown::parse_file(data, Some((name, hash))));
            mark(&mut entries, name, MemberDisposition::Parsed);
        }
        return Detection::Parsed(ParsedInput {
            input_kind: InputKind::MarkdownArchiveZip,
            adapter: adapter(markdown::ADAPTER, markdown::VERSION),
            source_schema_observed: markdown::SCHEMA.to_owned(),
            provider: None,
            unit_kind: CursorUnit::File,
            members: entries,
            units,
            warnings: Vec::new(),
        });
    }
    for m in entries.iter_mut() {
        if m.disposition == MemberDisposition::Skipped {
            m.disposition = MemberDisposition::PreservedOnly;
            m.reason_code = Some("no_adapter".to_owned());
        }
    }
    Detection::Unsupported {
        input_kind: InputKind::UnknownArchive,
        members: entries,
        warnings: vec![warning("unsupported_format", None)],
    }
}

/// Recognize and parse the received bytes.
pub fn detect(bytes: &[u8], limits: &ZipLimits) -> Detection {
    if looks_like_zip(bytes) {
        return detect_archive(bytes, limits);
    }
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        if chatgpt::probe(&value) {
            return match chatgpt::parse(bytes, None, &chatgpt::NoFiles) {
                Ok(units) => Detection::Parsed(ParsedInput {
                    input_kind: InputKind::ChatgptConversationsJson,
                    adapter: adapter(chatgpt::ADAPTER, chatgpt::VERSION),
                    source_schema_observed: chatgpt::SCHEMA.to_owned(),
                    provider: Some("chatgpt".to_owned()),
                    unit_kind: CursorUnit::Conversation,
                    members: Vec::new(),
                    units,
                    warnings: Vec::new(),
                }),
                Err(code) => Detection::Unsupported {
                    input_kind: InputKind::ChatgptConversationsJson,
                    members: Vec::new(),
                    warnings: vec![warning(code, None)],
                },
            };
        }
        if runtime::probe(&value) {
            return Detection::Parsed(ParsedInput {
                input_kind: InputKind::RuntimeNativeSession,
                adapter: adapter(runtime::ADAPTER, runtime::VERSION),
                source_schema_observed: runtime::FORMAT.replace('/', "-v"),
                provider: Some(runtime::PROVIDER.to_owned()),
                unit_kind: CursorUnit::Conversation,
                members: Vec::new(),
                units: runtime::parse(&value),
                warnings: Vec::new(),
            });
        }
        return Detection::Unsupported {
            input_kind: InputKind::Unknown,
            members: Vec::new(),
            warnings: vec![warning("unsupported_format", None)],
        };
    }
    if markdown::probe(bytes) {
        return Detection::Parsed(ParsedInput {
            input_kind: InputKind::MarkdownFile,
            adapter: adapter(markdown::ADAPTER, markdown::VERSION),
            source_schema_observed: markdown::SCHEMA.to_owned(),
            provider: None,
            unit_kind: CursorUnit::File,
            members: Vec::new(),
            units: vec![markdown::parse_file(bytes, None)],
            warnings: Vec::new(),
        });
    }
    Detection::Unsupported {
        input_kind: InputKind::Unknown,
        members: Vec::new(),
        warnings: vec![warning("unsupported_format", None)],
    }
}
