//! MV-2 acceptance I01–I06 on synthetic exports. I07 (a real export chosen
//! by the owner) is pending: no real export was provided or read.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::foundation::Cancellation;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::ImportId;
use enouia_memory_contract::import::{ImportStatus, InputKind, MemberDisposition};
use enouia_memory_contract::json::Knowable;
use enouia_memory_contract::parse_record;
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_contract::source::{AttachmentRecord, Availability, SourceRecord};
use enouia_memory_import::locate::resolve;
use enouia_memory_import::pipeline::NeverCancel;
use enouia_memory_import::{import_file, resume_import};
use enouia_memory_vault::fault::{FaultAction, FaultPoint};
use serde_json::json;
use std::cell::Cell;
use support::{Env, Member, ZipOptions, msg, sample_export, to_bytes, zip};

fn sources(env: &Env) -> Vec<SourceRecord> {
    let pin = env.vault.pin_current().unwrap();
    env.vault
        .read_manifest(&pin)
        .unwrap()
        .catalog
        .iter()
        .filter(|e| e.record_kind == RecordKind::Source)
        .map(|e| {
            let r = RecordRef::new(e.record_kind, &e.record_id, e.revision);
            parse_record(&env.vault.read_record(&pin, &r).unwrap()).unwrap()
        })
        .collect()
}

fn attachments(env: &Env) -> Vec<AttachmentRecord> {
    let pin = env.vault.pin_current().unwrap();
    env.vault
        .read_manifest(&pin)
        .unwrap()
        .catalog
        .iter()
        .filter(|e| e.record_kind == RecordKind::Attachment)
        .map(|e| {
            let r = RecordRef::new(e.record_kind, &e.record_id, e.revision);
            parse_record(&env.vault.read_record(&pin, &r).unwrap()).unwrap()
        })
        .collect()
}

fn raw(env: &Env, hash: &enouia_memory_contract::hash::Sha256Hex) -> Vec<u8> {
    let pin = env.vault.pin_current().unwrap();
    env.vault.read_object(&pin, hash).unwrap()
}

#[test]
fn i01_received_bytes_are_archived_exactly_and_every_source_resolves() {
    let env = Env::new("i01");
    let bytes = to_bytes(&sample_export());
    let file = env.temp.file("conversations.json", &bytes);
    let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    let m = &report.manifest;
    assert_eq!(m.status, ImportStatus::Completed);
    assert_eq!(m.input_kind, InputKind::ChatgptConversationsJson);
    assert_eq!(m.input_object_hash, sha256(&bytes));
    assert_eq!(
        raw(&env, &m.input_object_hash),
        bytes,
        "archived byte for byte"
    );
    let all = sources(&env);
    assert_eq!(all.len(), 10);
    assert_eq!(m.counts.sources_created, 10);
    for source in &all {
        let content = resolve(&bytes, &source.locator).unwrap();
        assert_eq!(
            sha256(&content),
            source.content_hash,
            "{}",
            source.source_id
        );
        assert_eq!(source.raw_object_hash.as_ref(), Some(&m.input_object_hash));
    }
    // Titles never reach the manifest or sources.
    let manifest_text = serde_json::to_string(m).unwrap();
    assert!(!manifest_text.contains("不应出现"));
    let pin = env.vault.pin_current().unwrap();
    assert!(env.vault.verify(&pin).unwrap().is_clean());
}

#[test]
fn i01_a_parse_failure_still_keeps_the_original_bytes() {
    let env = Env::new("i01-broken");
    let mut bytes = to_bytes(&sample_export());
    bytes.truncate(bytes.len() / 2); // no longer valid JSON; still UTF-8 text
    let broken = zip(
        &[Member::new("conversations.json", &bytes)],
        &ZipOptions::default(),
    );
    let file = env.temp.file("export.zip", &broken);
    let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    assert_eq!(report.manifest.status, ImportStatus::Partial);
    assert!(report.manifest.adapter.is_none());
    assert!(
        report
            .manifest
            .warnings
            .iter()
            .any(|w| w.code == "invalid_json")
    );
    assert_eq!(raw(&env, &report.manifest.input_object_hash), broken);
    assert!(sources(&env).is_empty());
}

#[test]
fn i02_reimport_is_deduplicated_and_changed_messages_become_revisions() {
    let env = Env::new("i02");
    let first = to_bytes(&sample_export());
    let file = env.temp.file("first.json", &first);
    let one = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    env.tick();
    let again = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    assert_eq!(again.manifest.duplicate_of, Some(one.import_id.clone()));
    assert_eq!(again.manifest.counts.sources_created, 0);
    assert_eq!(sources(&env).len(), 10, "identical bytes add nothing");

    // An overlapping later export: one edited message, one new message.
    let mut later = sample_export();
    later[0]["mapping"]["a3"]["message"]["content"]["parts"][0] =
        json!("（合成）先做离线相册和标签。");
    later[0]["mapping"]["a4"] = json!({"id": "a4", "parent": "a3", "children": [],
        "message": {"id": "a4", "author": {"role": "assistant"}, "create_time": 1_719_910_030.0,
                    "content": {"content_type": "text", "parts": ["（合成）好。"]}, "metadata": {}}});
    later[0]["current_node"] = json!("a4");
    let file2 = env.temp.file("later.json", &to_bytes(&later));
    env.tick();
    let two = import_file(&env.vault, &file2, &env.options, &NeverCancel).unwrap();
    assert_eq!(two.manifest.counts.sources_created, 1);
    assert_eq!(two.manifest.counts.sources_revised, 1);
    assert_eq!(two.manifest.counts.sources_unchanged, 9);
    let all = sources(&env);
    assert_eq!(all.len(), 11, "one new logical source");
    let a3 = all
        .iter()
        .find(|s| s.original_message_id.as_deref() == Some("a3"))
        .unwrap();
    assert_eq!(
        a3.revision.get(),
        2,
        "the edit is a revision of the same source"
    );
    assert_eq!(a3.import_id.as_ref(), Some(&two.import_id));
    // The first revision is still readable from history.
    let pin = env.vault.pin_current().unwrap();
    let head = env.vault.read_manifest(&pin).unwrap();
    assert!(
        head.catalog
            .iter()
            .any(|e| e.record_id == a3.source_id.as_str() && e.revision.get() == 2)
    );

    // Another account alias never merges with the first.
    let mut other = env.options.clone();
    other.account_scope = "acct-second".to_owned();
    env.tick();
    let three = import_file(&env.vault, &file2, &other, &NeverCancel);
    // Same bytes as `later.json`: recorded as a duplicate, not re-parsed.
    assert_eq!(three.unwrap().manifest.duplicate_of, Some(two.import_id));
}

#[test]
fn i03_branches_edits_and_missing_parents_are_kept_not_linearized() {
    let env = Env::new("i03");
    let file = env.temp.file("c.json", &to_bytes(&sample_export()));
    let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    let all = sources(&env);
    let by = |id: &str| {
        all.iter()
            .find(|s| s.original_message_id.as_deref() == Some(id))
            .unwrap()
    };
    let (b2, b3, b3e, b4) = (by("b2"), by("b3"), by("b3e"), by("b4"));
    assert_eq!(
        b3.parent_source_ids,
        Knowable::Known(vec![b2.source_id.clone()])
    );
    assert_eq!(
        b3e.parent_source_ids,
        Knowable::Known(vec![b2.source_id.clone()])
    );
    assert_ne!(
        b3.branch_id, b3e.branch_id,
        "edited versions are separate branches"
    );
    assert_eq!(
        b3e.branch_id, b4.branch_id,
        "the current path is one branch"
    );
    assert_eq!(b2.branch_id, b4.branch_id);
    assert_eq!(by("a1").parent_source_ids, Knowable::Known(vec![]));
    let c1 = by("c1");
    assert_eq!(
        c1.parent_source_ids,
        Knowable::Unknown,
        "a missing parent is explicit"
    );
    assert_eq!(c1.occurred_at, None, "no invented time");
    assert_eq!(
        by("c2").parent_source_ids,
        Knowable::Known(vec![c1.source_id.clone()])
    );
    let coverage = &report.manifest.coverage;
    let b = coverage
        .iter()
        .find(|c| c.original_conversation_id.as_deref() == Some("conv-b"))
        .unwrap();
    assert_eq!(b.branch_count, 2);
    let c = coverage
        .iter()
        .find(|c| c.original_conversation_id.as_deref() == Some("conv-c"))
        .unwrap();
    assert_eq!(c.missing_parents, 1);
    assert_eq!(c.unknown_time_count, 1);
    assert_eq!(report.manifest.counts.missing_parents, 1);
    assert!(
        report
            .manifest
            .warnings
            .iter()
            .any(|w| w.code == "missing_parent")
    );
}

#[test]
fn i04_attachments_missing_external_and_dangerous_are_marked_never_fetched() {
    let env = Env::new("i04");
    let mut export = sample_export();
    let mut m = msg(
        "d1",
        None,
        "user",
        "（合成）看这几张图。",
        Some(1_719_940_000.0),
    );
    m.extra_parts = vec![
        json!({"content_type": "image_asset_pointer", "asset_pointer": "file-service://file-present"}),
        json!({"content_type": "image_asset_pointer", "asset_pointer": "file-service://file-missing"}),
        json!({"content_type": "image_url", "url": "https://images.example.invalid/x.png?token=synthetic"}),
    ];
    m.attachments = vec![
        json!({"id": "file-tool", "name": "tool.exe", "mime_type": "application/octet-stream"}),
    ];
    export
        .as_array_mut()
        .unwrap()
        .push(support::conversation("conv-d", &[m], "d1"));
    let png = b"\x89PNG\r\n\x1a\nsynthetic-image-bytes".to_vec();
    let archive = zip(
        &[
            Member::new("conversations.json", &to_bytes(&export)),
            Member::new("file-present-photo.png", &png),
            Member::new("file-tool-tool.exe", b"MZ synthetic"),
        ],
        &ZipOptions::default(),
    );
    let file = env.temp.file("export.zip", &archive);
    let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    assert_eq!(report.manifest.input_kind, InputKind::ChatgptExportZip);
    let found = attachments(&env);
    let by = |a: Availability| found.iter().filter(|x| x.availability == a).count();
    assert_eq!(by(Availability::Present), 1);
    assert_eq!(by(Availability::Missing), 1);
    assert_eq!(by(Availability::ExternalReference), 1);
    assert_eq!(by(Availability::Quarantined), 1);
    let present = found
        .iter()
        .find(|a| a.availability == Availability::Present)
        .unwrap();
    assert_eq!(present.object_hash, Some(sha256(&png)));
    assert_eq!(present.detected_media_type.as_deref(), Some("image/png"));
    assert_eq!(raw(&env, present.object_hash.as_ref().unwrap()), png);
    let external = found
        .iter()
        .find(|a| a.availability == Availability::ExternalReference)
        .unwrap();
    assert!(
        external.object_hash.is_none(),
        "a URL is kept as text, never downloaded"
    );
    let exe = report
        .manifest
        .members
        .iter()
        .find(|m| m.member_name.ends_with(".exe"))
        .unwrap();
    assert_eq!(exe.disposition, MemberDisposition::Quarantined);
    assert_eq!(exe.reason_code.as_deref(), Some("executable_member"));
    let pin = env.vault.pin_current().unwrap();
    let head = env.vault.read_manifest(&pin).unwrap();
    assert!(
        !head
            .objects
            .iter()
            .any(|o| o.object_hash == sha256(b"MZ synthetic"))
    );
    assert_eq!(report.manifest.counts.attachments_present, 1);
    assert_eq!(report.manifest.counts.attachments_quarantined, 1);
}

#[test]
fn i05_hostile_or_unknown_archives_are_bounded_and_never_break_existing_data() {
    let env = Env::new("i05");
    let good = env.temp.file("good.json", &to_bytes(&sample_export()));
    import_file(&env.vault, &good, &env.options, &NeverCancel).unwrap();
    let before = sources(&env).len();

    let bomb = vec![0u8; 64 * 1024 * 1024];
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "garbage",
            b"\x00\x01\x02 not an export".to_vec(),
            "unsupported_format",
        ),
        (
            "truncated",
            zip(
                &[Member::new("conversations.json", b"[]")],
                &ZipOptions::default(),
            )[..40]
                .to_vec(),
            "",
        ),
        (
            "zip64",
            zip(
                &[Member::new("a.md", b"# a\n")],
                &ZipOptions {
                    zip64_marker: true,
                    ..Default::default()
                },
            ),
            "zip64_unsupported",
        ),
        (
            "overlap",
            zip(
                &[Member::new("a.md", b"# a\n"), Member::new("b.md", b"# b\n")],
                &ZipOptions {
                    overlap_last: true,
                    ..Default::default()
                },
            ),
            "overlapping_members",
        ),
    ];
    for (name, bytes, code) in cases {
        env.tick();
        let file = env.temp.file(name, &bytes);
        let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
        assert_eq!(report.manifest.status, ImportStatus::Partial, "{name}");
        if !code.is_empty() {
            assert!(
                report.manifest.warnings.iter().any(|w| w.code == code),
                "{name}: {:?}",
                report.manifest.warnings
            );
        }
        assert_eq!(
            raw(&env, &report.manifest.input_object_hash),
            bytes,
            "{name}: archived anyway"
        );
    }

    // Member-level hazards: quarantined, the rest of the archive still works.
    let mut unsafe_member = Member::new("../../outside.md", b"# escape\n");
    unsafe_member.deflate = false;
    let mut link = Member::new("link.md", b"/etc/passwd");
    link.symlink = true;
    let mut encrypted = Member::new("secret.md", b"# s\n");
    encrypted.flags = 1;
    let mut bad_crc = Member::new("crc.md", b"# c\n");
    bad_crc.bad_crc = true;
    let hostile = zip(
        &[
            Member::new("notes/ok.md", "# 标题\n正文。\n".as_bytes()),
            unsafe_member,
            link,
            encrypted,
            bad_crc,
            Member::new("bomb.md", &bomb),
        ],
        &ZipOptions::default(),
    );
    assert!(hostile.len() < 1024 * 1024, "compressed bomb is small");
    env.tick();
    let file = env.temp.file("hostile.zip", &hostile);
    let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    assert_eq!(report.manifest.input_kind, InputKind::MarkdownArchiveZip);
    assert_eq!(report.manifest.status, ImportStatus::Completed);
    let reason = |name: &str| {
        report
            .manifest
            .members
            .iter()
            .find(|m| m.member_name == name)
            .and_then(|m| m.reason_code.clone())
    };
    assert_eq!(reason("link.md").as_deref(), Some("symlink_member"));
    assert_eq!(reason("secret.md").as_deref(), Some("encrypted_member"));
    assert_eq!(reason("crc.md").as_deref(), Some("member_crc_mismatch"));
    assert_eq!(
        reason("bomb.md").as_deref(),
        Some("compression_ratio_exceeded")
    );
    assert!(
        report
            .manifest
            .members
            .iter()
            .any(|m| m.member_name.starts_with("unsafe-member-")
                && m.reason_code.as_deref() == Some("unsafe_member_name"))
    );
    assert!(!env.temp.path().join("outside.md").exists());
    assert!(
        !env.temp
            .path()
            .parent()
            .unwrap()
            .join("outside.md")
            .exists()
    );
    assert_eq!(
        report.manifest.counts.sources_created, 1,
        "only the safe note"
    );

    assert!(sources(&env).len() == before + 1);
    let pin = env.vault.pin_current().unwrap();
    assert!(
        env.vault.verify(&pin).unwrap().is_clean(),
        "existing data unharmed"
    );
}

struct CancelAfter(Cell<u32>);

impl Cancellation for CancelAfter {
    fn is_cancelled(&self) -> bool {
        let left = self.0.get();
        self.0.set(left.saturating_sub(1));
        left == 0
    }
}

#[test]
fn i06_cancel_and_crash_resume_from_the_committed_cursor() {
    let env = Env::new("i06");
    let mut options = env.options.clone();
    options.batch_units = 1;
    let bytes = to_bytes(&sample_export());
    let file = env.temp.file("c.json", &bytes);
    // Cancelled after the first batch.
    let first = import_file(&env.vault, &file, &options, &CancelAfter(Cell::new(1))).unwrap();
    assert_eq!(first.manifest.status, ImportStatus::Parsing);
    assert_eq!(first.manifest.cursor.completed, 1);
    assert_eq!(first.manifest.cursor.total, Some(3));
    assert_eq!(sources(&env).len(), 3);
    // The original file is gone: resume uses the archived bytes.
    std::fs::remove_file(&file).unwrap();
    // A second import of the same bytes is refused while one is unfinished.
    let again = env.temp.file("c2.json", &bytes);
    let error = import_file(&env.vault, &again, &options, &NeverCancel).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::InvalidRequest);

    // A crash-equivalent failure during the next batch commit.
    let faults = enouia_memory_vault::fault::Faults::armed();
    let crashing = enouia_memory_vault::Vault::open(
        &enouia_memory_vault::verify_data_root(
            &env.temp.path().join("vault-root"),
            &Default::default(),
        )
        .unwrap(),
        None,
        env.clock.clone(),
        std::sync::Arc::new(enouia_memory_contract::ports::SequentialIdSource::new(
            0x9000,
        )),
        enouia_memory_vault::VaultOptions {
            faults: faults.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    faults.arm(FaultPoint::BeforeCurrent, 1, FaultAction::Fail(112));
    assert!(resume_import(&crashing, &first.import_id, &options, &NeverCancel).is_err());
    faults.disarm();
    assert_eq!(sources(&env).len(), 3, "the failed batch is invisible");

    let done = resume_import(&env.vault, &first.import_id, &options, &NeverCancel).unwrap();
    assert_eq!(done.manifest.status, ImportStatus::Completed);
    assert_eq!(done.manifest.cursor.completed, 3);
    assert_eq!(done.manifest.counts.sources_created, 10);
    assert_eq!(done.manifest.coverage.len(), 3);
    assert_eq!(sources(&env).len(), 10, "no duplicates after resume");
    assert_eq!(done.manifest.adapter, first.manifest.adapter);
    // Resuming a completed import does nothing.
    let noop = resume_import(&env.vault, &first.import_id, &options, &NeverCancel).unwrap();
    assert_eq!(noop.commits, 0);
    let missing = ImportId::parse("imp_0000dead-0000-4000-8000-00000000dead").unwrap();
    assert_eq!(
        resume_import(&env.vault, &missing, &options, &NeverCancel)
            .unwrap_err()
            .code(),
        MemoryErrorCode::NotFound
    );
}

#[test]
fn markdown_files_are_located_by_byte_span() {
    let env = Env::new("markdown");
    let text = "开头说明。\n\n# 第一节\n内容一。\n\n## 第二节\n内容二。\n";
    let file = env.temp.file("notes.md", text.as_bytes());
    let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    assert_eq!(report.manifest.input_kind, InputKind::MarkdownFile);
    let all = sources(&env);
    assert_eq!(all.len(), 3);
    for s in &all {
        let span = resolve(text.as_bytes(), &s.locator).unwrap();
        assert_eq!(sha256(&span), s.content_hash);
        assert!(s.provider.is_none() && s.occurred_at.is_none());
    }
}

#[test]
fn runtime_native_sessions_import_as_sources() {
    let env = Env::new("runtime");
    let export = json!({"format": "enouia-runtime-session/1", "sessions": [{
    "session_key": "rt-1",
    "events": [
        {"event_key": "e1", "sequence": 1, "parent_event_key": null, "role": "user",
         "occurred_at": "2026-09-01T10:00:00.000Z", "text": "（合成）运行时里说的话。"},
        {"event_key": "e2", "sequence": 2, "parent_event_key": "e1", "role": "assistant",
         "occurred_at": "2026-09-01T10:00:05.000Z", "text": "（合成）回复。"},
        {"event_key": "e3", "sequence": 3, "role": "user", "text": 42}
    ]}]});
    let bytes = to_bytes(&export);
    let file = env.temp.file("session.json", &bytes);
    let report = import_file(&env.vault, &file, &env.options, &NeverCancel).unwrap();
    assert_eq!(report.manifest.input_kind, InputKind::RuntimeNativeSession);
    assert_eq!(report.manifest.counts.sources_created, 2);
    assert_eq!(report.manifest.counts.unparseable, 1);
    for s in sources(&env) {
        assert_eq!(
            sha256(&resolve(&bytes, &s.locator).unwrap()),
            s.content_hash
        );
        assert_eq!(s.provider.as_deref(), Some("enouia-runtime"));
    }
}
