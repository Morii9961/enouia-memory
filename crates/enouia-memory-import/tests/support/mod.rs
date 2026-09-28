#![allow(dead_code)]
//! Synthetic inputs only: a tiny ZIP writer that can also produce hostile
//! archives, a synthetic ChatGPT-shaped export, and a Vault in a temporary
//! root. Nothing here reads a real export or touches a real sync folder.

use enouia_memory_contract::common::{ActorRef, TrustedSurface};
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::ids::PolicyId;
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_import::ImportOptions;
use enouia_memory_vault::service::new_genesis;
use enouia_memory_vault::{RootPolicy, Vault, VaultOptions, verify_data_root};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct Temp(PathBuf);

impl Temp {
    pub fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join("enouia-memory-import-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let path = base.join(format!(
            "{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    pub fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct Env {
    pub temp: Temp,
    pub clock: Arc<FakeClock>,
    pub vault: Vault,
    pub options: ImportOptions,
}

pub const T0: i64 = 1_790_000_000_000;

impl Env {
    pub fn new(name: &str) -> Self {
        let temp = Temp::new(name);
        let root = temp.dir("vault-root");
        let verified = verify_data_root(&root, &RootPolicy::default()).unwrap();
        let clock = Arc::new(FakeClock::new(T0));
        let ids = Arc::new(SequentialIdSource::new(0x7000));
        let owner = ActorRef {
            actor_id: enouia_memory_contract::ids::PrincipalId::parse(
                "prn_00000001-0000-4000-8000-000000000001",
            )
            .unwrap(),
            actor_type: enouia_memory_contract::common::ActorType::Owner,
        };
        let now = enouia_memory_contract::time::Timestamp::from_unix_ms(T0).unwrap();
        let genesis = new_genesis(
            ids.as_ref(),
            owner.clone(),
            TrustedSurface::TrustedLocalCli,
            &now,
        )
        .unwrap();
        let vault = Vault::create(
            &verified,
            genesis,
            clock.clone(),
            ids,
            VaultOptions::default(),
        )
        .unwrap();
        let pin = vault.pin_current().unwrap();
        let policy = vault
            .read_manifest(&pin)
            .unwrap()
            .catalog
            .iter()
            .find(|e| e.record_kind == RecordKind::Policy)
            .map(|e| PolicyId::parse(&e.record_id).unwrap())
            .unwrap();
        let options = ImportOptions::new(owner, "acct-main", policy);
        Self {
            temp,
            clock,
            vault,
            options,
        }
    }

    pub fn tick(&self) {
        self.clock.set(self.clock.now_ms() + 1_000);
    }
}

trait Now {
    fn now_ms(&self) -> i64;
}

impl Now for FakeClock {
    fn now_ms(&self) -> i64 {
        use enouia_memory_contract::foundation::Clock;
        self.now_unix_ms()
    }
}

// ------------------------------------------------------------------ ZIP

#[derive(Clone)]
pub struct Member {
    pub name: String,
    pub data: Vec<u8>,
    pub deflate: bool,
    pub flags: u16,
    pub symlink: bool,
    pub bad_crc: bool,
}

impl Member {
    pub fn new(name: &str, data: &[u8]) -> Self {
        Self {
            name: name.to_owned(),
            data: data.to_vec(),
            deflate: true,
            flags: 0,
            symlink: false,
            bad_crc: false,
        }
    }
}

#[derive(Default)]
pub struct ZipOptions {
    pub zip64_marker: bool,
    pub overlap_last: bool,
}

pub fn zip(members: &[Member], options: &ZipOptions) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut offsets = Vec::new();
    for m in members {
        let compressed = if m.deflate {
            miniz_oxide::deflate::compress_to_vec(&m.data, 6)
        } else {
            m.data.clone()
        };
        let mut crc = crc32fast::hash(&m.data);
        if m.bad_crc {
            crc ^= 0xDEAD_BEEF;
        }
        let method: u16 = if m.deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        offsets.push(offset);
        let name = m.name.as_bytes();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&(m.flags | 0x0800).to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x21u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(m.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&compressed);
        let made_by: u16 = if m.symlink { (3 << 8) | 20 } else { 20 };
        let external: u32 = if m.symlink { 0o120_777 << 16 } else { 0 };
        let local = if options.overlap_last && std::ptr::eq(m, members.last().unwrap()) {
            offsets[0]
        } else {
            offset
        };
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&made_by.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&(m.flags | 0x0800).to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0x21u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        central.extend_from_slice(&(m.data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0u8; 8]);
        central.extend_from_slice(&external.to_le_bytes());
        central.extend_from_slice(&local.to_le_bytes());
        central.extend_from_slice(name);
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    if options.zip64_marker {
        out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0u8; 16]);
    }
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

// ------------------------------------------------------ ChatGPT-shaped

pub struct Msg<'a> {
    pub id: &'a str,
    pub parent: Option<&'a str>,
    pub role: &'a str,
    pub text: &'a str,
    pub time: Option<f64>,
    pub extra_parts: Vec<Value>,
    pub attachments: Vec<Value>,
}

pub fn msg<'a>(
    id: &'a str,
    parent: Option<&'a str>,
    role: &'a str,
    text: &'a str,
    time: Option<f64>,
) -> Msg<'a> {
    Msg {
        id,
        parent,
        role,
        text,
        time,
        extra_parts: Vec::new(),
        attachments: Vec::new(),
    }
}

/// One conversation: a message-less root node plus the given messages.
pub fn conversation(id: &str, messages: &[Msg<'_>], current: &str) -> Value {
    let mut mapping = serde_json::Map::new();
    mapping.insert(
        "root".into(),
        json!({"id": "root", "message": null, "parent": null, "children": []}),
    );
    for m in messages {
        let mut parts = vec![json!(m.text)];
        parts.extend(m.extra_parts.iter().cloned());
        mapping.insert(
            m.id.into(),
            json!({
                "id": m.id,
                "parent": m.parent.unwrap_or("root"),
                "children": [],
                "message": {
                    "id": m.id,
                    "author": {"role": m.role, "name": null, "metadata": {}},
                    "create_time": m.time,
                    "content": {"content_type": "text", "parts": parts},
                    "metadata": {"attachments": m.attachments, "future_field": {"kept": "in raw"}},
                    "status": "finished_successfully"
                }
            }),
        );
    }
    json!({
        "title": "（合成）不应出现在报告中的标题",
        "create_time": 1_719_900_000.0,
        "update_time": 1_719_990_000.0,
        "mapping": mapping,
        "current_node": current,
        "conversation_id": id,
        "id": id,
    })
}

/// A synthetic export: a linear chat, an edited branch, a missing parent,
/// and a message without a time.
pub fn sample_export() -> Value {
    json!([
        conversation(
            "conv-a",
            &[
                msg(
                    "a1",
                    None,
                    "user",
                    "（合成）我在做一个相册应用。",
                    Some(1_719_910_000.5)
                ),
                msg(
                    "a2",
                    Some("a1"),
                    "assistant",
                    "（合成）好的，需要什么功能？",
                    Some(1_719_910_010.0)
                ),
                msg(
                    "a3",
                    Some("a2"),
                    "user",
                    "（合成）先做离线相册。",
                    Some(1_719_910_020.0)
                ),
            ],
            "a3",
        ),
        conversation(
            "conv-b",
            &[
                msg(
                    "b1",
                    None,
                    "user",
                    "（合成）原问题。",
                    Some(1_719_920_000.0)
                ),
                msg(
                    "b2",
                    Some("b1"),
                    "assistant",
                    "（合成）第一次回答。",
                    Some(1_719_920_005.0)
                ),
                // The user edited b1's follow-up: two siblings under b2.
                msg(
                    "b3",
                    Some("b2"),
                    "user",
                    "（合成）追问版本一。",
                    Some(1_719_920_010.0)
                ),
                msg(
                    "b3e",
                    Some("b2"),
                    "user",
                    "（合成）追问版本二（编辑后）。",
                    Some(1_719_920_011.0)
                ),
                msg(
                    "b4",
                    Some("b3e"),
                    "assistant",
                    "（合成）针对版本二的回答。",
                    Some(1_719_920_015.0)
                ),
            ],
            "b4",
        ),
        conversation(
            "conv-c",
            &[
                msg(
                    "c1",
                    Some("gone"),
                    "user",
                    "（合成）父消息不在导出里。",
                    None
                ),
                msg(
                    "c2",
                    Some("c1"),
                    "assistant",
                    "（合成）回答。",
                    Some(1_719_930_000.0)
                ),
            ],
            "c2",
        ),
    ])
}

pub fn to_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(value).unwrap()
}
