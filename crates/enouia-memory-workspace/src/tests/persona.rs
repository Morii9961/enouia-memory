//! Public persona projection export (ADR-MEM-128) through the page channel.
//! Synthetic Identity text and records in temporary roots only.

use super::*;
use enouia_memory_contract::ids::IdentityId;
use enouia_memory_contract::persona::parse_document;
use enouia_memory_govern::Proposed;

const PRIVATE_PROSE: &str = "（合成私密）只在主人面前说的关系语境。";
const PRIVATE_FACT: &str = "（合成私密）主人家里的门牌号。";
const UNSELECTED_PUBLIC: &str = "（合成公开）没有被勾选的公开事实。";

fn block(display_name: &str) -> Value {
    let example = |n: u32| {
        json!({"turns": [
            {"speaker": "群友甲", "text": format!("（合成）问题{n}")},
            {"speaker": display_name, "text": format!("（合成）回答{n}")},
        ]})
    };
    json!({
        "display_name": display_name,
        "persona": {
            "background": "（合成）一个虚构的数字伙伴。",
            "personality": "（合成）温和、好奇。",
            "voice": "（合成）短句，先说结论。",
            "motivation": "（合成）帮群友把问题想清楚。",
            "values": "（合成）诚实，不装懂。",
            "rules": "（合成）不评价群友的私事。",
            "limits": "（合成）不能联网，不记得群聊。",
        },
        "style": {"rules": ["（合成）一次只回答一个问题"], "anti_examples": ["（合成）长篇大论"]},
        "examples": [example(1), example(2), example(3)],
        "interests": ["（合成）天文", "（合成）做饭"],
        "boundaries": {
            "refuse": ["（合成）透露主人的私人信息"],
            "ai_disclosure": "（合成）我是 AI，由主人设定。",
            "avoid_topics": ["（合成）政治"],
            "forbidden_words": ["（合成）禁词"],
        },
    })
}

fn markdown(block: &Value) -> String {
    format!(
        "# 合成身份\n\n{PRIVATE_PROSE}\n\n```enouia-public-persona-1\n{}\n```\n\n结尾的私密段落。\n",
        serde_json::to_string_pretty(block).unwrap()
    )
}

fn source(
    env: &Env,
    text: &str,
    sensitivity: Sensitivity,
) -> enouia_memory_contract::ids::SourceId {
    let open = env.ws.open().unwrap();
    open.vault
        .record_manual_assertion(
            &ManualAssertionInput {
                text: text.to_owned(),
                operator: open.owner.clone(),
                trusted_surface: SURFACE,
                confirmation: ConfirmationMethod::ExactTextConfirmDialog,
                sensitivity,
                access_policy_id: env.ws.policy(&open).unwrap(),
                time_precision: TimePrecision::Millisecond,
            },
            format!("persona-test-source\n{text}").as_bytes(),
        )
        .unwrap()
        .id
}

/// Owner proposal, then the page's plan/confirm with the exact diff.
fn accept(env: &Env, proposal: &Proposal) {
    let open = env.ws.open().unwrap();
    let key = format!("persona-test\n{}", proposal.content);
    let Proposed::Stored(written) = propose(
        &open.vault,
        proposal,
        &enouia_memory_govern::Origin::owner(open.owner.clone()),
        key.as_bytes(),
    )
    .unwrap() else {
        panic!("duplicate");
    };
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": written.id, "revision": 1, "action": "accept",
            "editedContent": null, "mergeTarget": null}]}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
}

fn public_memory(env: &Env, text: &str, claim: &str, tags: &[&str]) -> String {
    let src = source(env, text, Sensitivity::Public);
    let subject = SubjectId::parse(
        &env.ws
            .open()
            .unwrap()
            .owner
            .actor_id
            .as_str()
            .replacen("prn_", "sub_", 1),
    )
    .unwrap();
    let mut details = Map::new();
    details.insert("claim_key".into(), json!(claim));
    details.insert("subject_ids".into(), json!([subject]));
    details.insert("tags".into(), json!(tags));
    accept(
        env,
        &Proposal::create(
            ProposedType::Fact,
            text,
            details,
            vec![EvidenceSpec::content(src, one())],
        ),
    );
    memory_id(env, text)
}

fn memory_id(env: &Env, text: &str) -> String {
    let list = env.ok(
        "memory_list",
        json!({"includeInactive": true, "cursor": null, "limit": 100}),
    );
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["snippet"] == text)
        .unwrap_or_else(|| panic!("{text}: {list}"))["memoryId"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A new Identity record, or the next revision of `target`.
fn identity(env: &Env, text: &str, target: Option<(&str, u64)>) -> (String, u64) {
    let src = source(env, text, Sensitivity::Private);
    let mut details = Map::new();
    if target.is_none() {
        details.insert("slug".into(), json!("style"));
        details.insert("title".into(), json!("合成身份"));
    }
    accept(
        env,
        &Proposal {
            kind: ProposalKind::IdentityChange,
            proposed_type: ProposedType::Identity,
            content: text.to_owned(),
            details: Some(details),
            evidence: vec![EvidenceSpec::content(src, one())],
            reason: "identity change".into(),
            context_source_refs: vec![],
            sensitivity: None,
            target_memory_id: None,
            target_identity_id: target.map(|(id, _)| IdentityId::parse(id).unwrap()),
            expected_revision: target.map(|(_, r)| Revision::new(r).unwrap()),
            effective_from: None,
            reopens_candidate_id: None,
        },
    );
    let options = env.ok("persona_options", json!({"audience": "public_group"}));
    let row = options["identities"][0].clone();
    (
        row["identityId"].as_str().unwrap().to_owned(),
        row["revision"].as_u64().unwrap(),
    )
}

fn selection(identity: &(String, u64), ids: &[&str]) -> Value {
    json!({"audience": "public_group", "identityId": identity.0,
        "identityRevision": identity.1, "memoryIds": ids})
}

fn export(env: &Env, preview: &Value, selection: &Value, folder: &str, key: &str) -> Value {
    std::fs::create_dir_all(env.base.join(folder)).unwrap();
    let mut args = selection.clone();
    args["previewDigest"] = preview["previewDigest"].clone();
    args["destinationToken"] = json!(env.pick(PickKind::PersonaDestination, folder));
    env.send_keyed("persona_export", args, key)
}

struct World {
    env: Env,
    identity: (String, u64),
    selected_public: String,
    selected_private: String,
}

fn world(name: &str) -> World {
    let env = Env::new(name);
    let identity = identity(&env, &markdown(&block("合成伙伴")), None);
    let selected_public = public_memory(
        &env,
        "（合成公开）喜欢看星星。",
        "synthetic.stars",
        &["hobby.astronomy"],
    );
    public_memory(&env, UNSELECTED_PUBLIC, "synthetic.unselected", &[]);
    let selected_private = env.remember(PRIVATE_FACT, "synthetic.private_selected");
    env.remember("（合成私密）另一个未勾选的私密事实。", "synthetic.private");
    World {
        env,
        identity,
        selected_public,
        selected_private,
    }
}

#[test]
fn persona_preview_projects_only_the_public_block_and_selected_public_records() {
    let w = world("persona-scope");
    let options = w
        .env
        .ok("persona_options", json!({"audience": "public_group"}));
    assert_eq!(options["identities"][0]["publicBlock"], "valid");
    let listed: Vec<&Value> = options["publicMemories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| &m["memoryId"])
        .collect();
    assert_eq!(
        listed.len(),
        2,
        "only public records are offered: {options}"
    );
    assert!(!options.to_string().contains(PRIVATE_FACT));
    assert_eq!(options["lastExport"], Value::Null);

    let preview = w.env.ok(
        "persona_preview",
        selection(&w.identity, &[&w.selected_public, &w.selected_private]),
    );
    let knowledge = preview["projection"]["public_knowledge"]
        .as_array()
        .unwrap();
    assert_eq!(knowledge.len(), 1);
    assert_eq!(knowledge[0]["text"], "（合成公开）喜欢看星星。");
    assert_eq!(
        knowledge[0]["source"]["memory_id"],
        json!(w.selected_public)
    );
    assert_eq!(preview["projection"]["display_name"], "合成伙伴");
    assert_eq!(
        preview["projection"]["source"]["identity_id"],
        json!(w.identity.0)
    );
    assert_eq!(preview["version"], 1);
    let excluded = &preview["exclusions"];
    assert_eq!(excluded["identityLinesOutsideBlock"], 3);
    assert_eq!(excluded["otherIdentityRecords"], 0);
    assert_eq!(excluded["selectedNotPublic"], 1);
    assert_eq!(excluded["publicMemoriesNotSelected"], 1);
    assert_eq!(excluded["memoriesNotPublic"], json!({"private": 1}));
    assert_eq!(excluded["selectedUnavailable"], 0);
    assert_eq!(preview["warnings"][0]["code"], "names_people");
    // No body of excluded content reaches the preview.
    let text = preview.to_string();
    for hidden in [
        PRIVATE_PROSE,
        PRIVATE_FACT,
        UNSELECTED_PUBLIC,
        "结尾的私密段落",
    ] {
        assert!(!text.contains(hidden), "{hidden} leaked");
    }
    assert_eq!(preview["diff"]["baselineVersion"], Value::Null);
    assert_eq!(preview["diff"]["knowledge"][0]["change"], "added");
    // The digest is the projection's canonical bytes.
    let projection: enouia_memory_contract::persona::Projection =
        serde_json::from_value(preview["projection"].clone()).unwrap();
    assert_eq!(
        json!(enouia_memory_contract::persona::projection_sha256(
            &projection
        )),
        preview["projectionSha256"]
    );
    // An identical request gives an identical preview.
    let again = w.env.ok(
        "persona_preview",
        selection(&w.identity, &[&w.selected_private, &w.selected_public]),
    );
    assert_eq!(again["previewDigest"], preview["previewDigest"]);
}

#[test]
fn persona_export_writes_once_replays_and_keeps_version_for_unchanged_content() {
    let w = world("persona-export");
    let chosen = selection(&w.identity, &[&w.selected_public]);
    let preview = w.env.ok("persona_preview", chosen.clone());
    let response = export(&w.env, &preview, &chosen, "out-a", "persona-export-one");
    assert_eq!(response["error"], Value::Null, "{response}");
    let receipt = &response["result"];
    assert_eq!(receipt["version"], 1);
    assert_eq!(receipt["replayed"], false);
    assert_eq!(receipt["fileName"], "public-persona-public_group-v1.json");
    let bytes =
        std::fs::read(w.env.base.join("out-a/public-persona-public_group-v1.json")).unwrap();
    assert_eq!(json!(sha256(&bytes)), receipt["fileSha256"]);
    let document = parse_document(&bytes).unwrap();
    assert_eq!(json!(document.projection), preview["projection"]);
    assert_eq!(
        json!(document.projection_sha256),
        preview["projectionSha256"]
    );
    assert!(!String::from_utf8_lossy(&bytes).contains(PRIVATE_FACT));

    // The same key replays the receipt without touching any folder.
    let mut args = chosen.clone();
    args["previewDigest"] = preview["previewDigest"].clone();
    args["destinationToken"] = json!("tok_00000000000000000000000000000000");
    let replay = w
        .env
        .send_keyed("persona_export", args.clone(), "persona-export-one");
    assert_eq!(replay["result"]["replayed"], true, "{replay}");
    assert_eq!(replay["result"]["fileSha256"], receipt["fileSha256"]);
    // A different request under the same key is a conflict.
    args["memoryIds"] = json!([]);
    let reused = w
        .env
        .send_keyed("persona_export", args, "persona-export-one");
    assert_eq!(reused["error"]["code"], "idempotency_conflict", "{reused}");

    // Unchanged content: same version and digest, an empty diff.
    let options = w
        .env
        .ok("persona_options", json!({"audience": "public_group"}));
    assert_eq!(options["lastExport"]["version"], 1);
    assert_eq!(
        options["lastExport"]["memoryIds"],
        json!([w.selected_public])
    );
    let same = w.env.ok("persona_preview", chosen.clone());
    assert_eq!(same["version"], 1);
    assert_eq!(same["projectionSha256"], preview["projectionSha256"]);
    assert_eq!(same["diff"]["baselineVersion"], 1);
    assert_eq!(same["diff"]["fields"], json!([]));
    assert_eq!(same["diff"]["knowledge"], json!([]));
    assert_eq!(same["diff"]["unchangedKnowledge"], 1);
    // The existing file is never replaced; another folder takes the same version.
    let refused = export(&w.env, &same, &chosen, "out-a", "persona-export-two");
    assert_eq!(
        refused["error"]["rules"][0], "persona.destination_exists",
        "{refused}"
    );
    assert_eq!(
        std::fs::read(w.env.base.join("out-a/public-persona-public_group-v1.json")).unwrap(),
        bytes
    );
    let second = export(&w.env, &same, &chosen, "out-b", "persona-export-three");
    assert_eq!(second["result"]["version"], 1, "{second}");
    assert_eq!(
        second["result"]["projectionSha256"],
        preview["projectionSha256"]
    );
}

#[test]
fn persona_diff_reports_changes_and_withdrawals_as_removals() {
    let w = world("persona-diff");
    let env = &w.env;
    let revised = public_memory(env, "（合成公开）会做番茄炒蛋。", "synthetic.cooking", &[]);
    let forgotten = public_memory(env, "（合成公开）养过一只猫。", "synthetic.cat", &[]);
    let ids = [
        w.selected_public.as_str(),
        revised.as_str(),
        forgotten.as_str(),
    ];
    let chosen = selection(&w.identity, &ids);
    let preview = env.ok("persona_preview", chosen.clone());
    assert_eq!(
        preview["projection"]["public_knowledge"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let first = export(env, &preview, &chosen, "out", "persona-diff-one");
    assert_eq!(first["result"]["version"], 1, "{first}");

    // Reclassified to non-public: an owner correction cites a private source.
    env.ok(
        "correction_propose",
        json!({"memoryId": w.selected_public, "revision": 1, "text": "（合成私密）改为私密。"}),
    );
    let pending = env.ok("candidate_list", json!({"cursor": null, "limit": null}));
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": pending["items"][0]["candidateId"], "revision": 1,
            "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    // Revised publicly: the same item changes.
    let src = source(
        env,
        "（合成公开）会做番茄炒蛋和蛋炒饭。",
        Sensitivity::Public,
    );
    accept(
        env,
        &Proposal {
            kind: ProposalKind::Revise,
            proposed_type: ProposedType::Fact,
            content: "（合成公开）会做番茄炒蛋和蛋炒饭。".into(),
            details: None,
            evidence: vec![EvidenceSpec::content(src, one())],
            reason: "owner_correction".into(),
            context_source_refs: vec![],
            sensitivity: None,
            target_memory_id: Some(MemoryId::parse(&revised).unwrap()),
            target_identity_id: None,
            expected_revision: Some(one()),
            effective_from: None,
            reopens_candidate_id: None,
        },
    );
    // Forgotten: logically deleted.
    let forget = env.ok(
        "forget_plan",
        json!({"memoryId": forgotten, "mode": "forget", "withDependents": false}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": forget["planId"], "diffHash": forget["diffHash"]}),
    );
    // A new approved Identity revision with a changed public name.
    let identity = identity(
        env,
        &markdown(&block("合成伙伴二号")),
        Some((&w.identity.0, w.identity.1)),
    );
    assert_eq!(identity.1, 2);

    let stale = env.err("persona_preview", chosen.clone());
    assert_eq!(stale["rules"][0], "persona.identity_stale");
    let next = env.ok("persona_preview", selection(&identity, &ids));
    assert_eq!(next["version"], 2);
    assert_ne!(next["projectionSha256"], preview["projectionSha256"]);
    let knowledge = next["projection"]["public_knowledge"].as_array().unwrap();
    assert_eq!(knowledge.len(), 1);
    assert_eq!(knowledge[0]["text"], "（合成公开）会做番茄炒蛋和蛋炒饭。");
    assert_eq!(knowledge[0]["source"]["memory_revision"], 2);
    let changes: BTreeMap<String, String> = next["diff"]["knowledge"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["source"]["memory_id"].as_str().unwrap().to_owned(),
                c["change"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(changes[&w.selected_public], "removed");
    assert_eq!(changes[&forgotten], "removed");
    assert_eq!(changes[&revised], "changed");
    let fields: Vec<&Value> = next["diff"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["field"])
        .collect();
    assert_eq!(
        fields,
        [
            &json!("/source"),
            &json!("/display_name"),
            &json!("/examples")
        ]
    );
    assert_eq!(next["exclusions"]["selectedNotPublic"], 1);
    assert_eq!(next["exclusions"]["selectedUnavailable"], 1);
    // Removed items carry no text: their content is excluded now.
    let text = next.to_string();
    assert!(!text.contains("改为私密") && !text.contains("养过一只猫"));
    // An item ID survives a revision of the same record.
    assert_eq!(
        knowledge[0]["item_id"],
        preview["projection"]["public_knowledge"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["source"]["memory_id"] == json!(revised))
            .unwrap()["item_id"]
    );
}

#[test]
fn persona_export_refuses_changed_previews_and_unsafe_destinations() {
    let w = world("persona-refusals");
    let env = &w.env;
    let chosen = selection(&w.identity, &[&w.selected_public]);
    let preview = env.ok("persona_preview", chosen.clone());
    let mut stale = preview.clone();
    stale["previewDigest"] = json!(sha256(b"synthetic other preview"));
    let changed = export(env, &stale, &chosen, "out", "persona-refuse-digest");
    assert_eq!(changed["error"]["code"], "revision_conflict", "{changed}");
    assert_eq!(changed["error"]["rules"][0], "persona.preview_changed");

    // A Git repository, a worktree (".git" file) and this repository itself.
    std::fs::create_dir_all(env.base.join("repo/.git")).unwrap();
    std::fs::create_dir_all(env.base.join("tree/sub")).unwrap();
    std::fs::write(env.base.join("tree/.git"), "gitdir: elsewhere\n").unwrap();
    for folder in ["repo", "tree/sub"] {
        let refused = export(
            env,
            &preview,
            &chosen,
            folder,
            &format!("persona-refuse-{}", folder.len()),
        );
        assert_eq!(
            refused["error"]["rules"],
            json!(["persona.destination_rejected", "root.inside_repository"]),
            "{refused}"
        );
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut args = chosen.clone();
    args["previewDigest"] = preview["previewDigest"].clone();
    args["destinationToken"] = json!(
        env.ws
            .register_pick(PickKind::PersonaDestination, repository)
            .unwrap()
            .token
    );
    let refused = env.send_keyed("persona_export", args, "persona-refuse-repository");
    assert_eq!(
        refused["error"]["rules"][1], "root.inside_repository",
        "{refused}"
    );
    // Inside the open Vault.
    std::fs::create_dir_all(env.base.join("vault/exports")).unwrap();
    let refused = export(
        env,
        &preview,
        &chosen,
        "vault/exports",
        "persona-refuse-vault",
    );
    assert_eq!(
        refused["error"]["rules"],
        json!([
            "persona.destination_rejected",
            "persona.destination_in_vault"
        ]),
        "{refused}"
    );
    // Nothing was written or reserved by any refusal.
    for folder in ["repo", "tree/sub", "vault/exports"] {
        assert_eq!(
            std::fs::read_dir(env.base.join(folder))
                .unwrap()
                .filter(|e| e.as_ref().unwrap().file_name() != ".git")
                .count(),
            0
        );
    }
    let options = env.ok("persona_options", json!({"audience": "public_group"}));
    assert_eq!(options["lastExport"], Value::Null);
    let fine = export(env, &preview, &chosen, "out", "persona-refuse-ok");
    assert_eq!(fine["result"]["version"], 1, "{fine}");
    // A selected public record is still checked as consumer-visible text.
    let pathy = public_memory(
        env,
        "（合成公开）笔记放在 D:\\synthetic\\notes",
        "synthetic.path",
        &[],
    );
    let refused = env.err(
        "persona_preview",
        selection(&w.identity, &[&w.selected_public, &pathy]),
    );
    assert_eq!(
        refused["rules"][0], "persona.projection_invalid",
        "{refused}"
    );
    assert!(
        refused["rules"]
            .as_array()
            .unwrap()
            .contains(&json!("persona.local_path"))
    );
}

#[test]
fn persona_identity_block_must_be_single_closed_and_bounded() {
    let env = Env::new("persona-block");
    let id = identity(&env, "# 只有私密文字\n\n没有公开块。\n", None);
    let missing = env.err("persona_preview", selection(&id, &[]));
    assert_eq!(
        missing["rules"],
        json!(["persona.identity_block", "persona.block_missing"])
    );
    let options = env.ok("persona_options", json!({"audience": "public_group"}));
    assert_eq!(options["identities"][0]["publicBlock"], "missing");
    let twice = format!("{}{}", markdown(&block("甲")), markdown(&block("乙")));
    let id = identity(&env, &twice, Some((&id.0, id.1)));
    assert_eq!(
        env.err("persona_preview", selection(&id, &[]))["rules"][1],
        "persona.block_repeated"
    );
    let mut extra = block("甲");
    extra["group_overrides"] = json!({});
    let id = identity(&env, &markdown(&extra), Some((&id.0, id.1)));
    assert_eq!(
        env.err("persona_preview", selection(&id, &[]))["rules"][1],
        "shape"
    );
    let mut short = block("甲");
    short["examples"].as_array_mut().unwrap().truncate(2);
    // Escaped in the Markdown, a control character once parsed.
    short["boundaries"]["ai_disclosure"] = json!("synthetic bell \u{7}");
    let id = identity(&env, &markdown(&short), Some((&id.0, id.1)));
    let rules = env.err("persona_preview", selection(&id, &[]))["rules"].clone();
    assert!(
        rules
            .as_array()
            .unwrap()
            .contains(&json!("persona.example_count")),
        "{rules}"
    );
    assert!(
        rules
            .as_array()
            .unwrap()
            .contains(&json!("persona.control_character")),
        "{rules}"
    );
    let unknown = env.err(
        "persona_preview",
        json!({"audience": "private_chat", "identityId": id.0, "identityRevision": id.1, "memoryIds": []}),
    );
    assert_eq!(unknown["rules"], json!(["shape"]));
}

#[test]
fn persona_owner_resend_completes_an_interrupted_export_with_the_reserved_bytes() {
    let w = world("persona-resume");
    let env = &w.env;
    let chosen = selection(&w.identity, &[&w.selected_public]);
    let preview = env.ok("persona_preview", chosen.clone());
    let done = export(env, &preview, &chosen, "out", "persona-resume-key");
    let file = env.base.join("out/public-persona-public_group-v1.json");
    let bytes = std::fs::read(&file).unwrap();
    // Recreate the state after "reserved, file write unknown": the ledger holds
    // the reserved document, no confirmed export and no file.
    let ledger_path = env.base.join("vault/sync-state/public-persona-1.json");
    let mut ledger: Value = serde_json::from_slice(&std::fs::read(&ledger_path).unwrap()).unwrap();
    let entry = ledger["receipts"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    entry["written"] = json!(false);
    entry["document"] = serde_json::from_slice(&bytes).unwrap();
    ledger["audiences"]["public_group"]["last_export"] = Value::Null;
    std::fs::write(&ledger_path, serde_json::to_vec(&ledger).unwrap()).unwrap();
    std::fs::remove_file(&file).unwrap();
    env.tick();

    // Nothing is retried on its own; the owner resends the same request.
    assert!(!file.exists());
    let resent = export(env, &preview, &chosen, "out", "persona-resume-key");
    assert_eq!(resent["error"], Value::Null, "{resent}");
    assert_eq!(resent["result"]["fileSha256"], done["result"]["fileSha256"]);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        bytes,
        "the reserved bytes, not a new export time"
    );
    let options = env.ok("persona_options", json!({"audience": "public_group"}));
    assert_eq!(options["lastExport"]["version"], 1);
    // A reserved version is never reused for different content.
    let mut ledger: Value = serde_json::from_slice(&std::fs::read(&ledger_path).unwrap()).unwrap();
    ledger["audiences"]["public_group"]["issued"]["projection_sha256"] = json!(sha256(b"other"));
    std::fs::write(&ledger_path, serde_json::to_vec(&ledger).unwrap()).unwrap();
    assert_eq!(env.ok("persona_preview", chosen)["version"], 2);
}

/// Regenerates the contract's synthetic sample corpus from real Core output:
/// `ENOUIA_PERSONA_SAMPLE_DIR=<dir> cargo test -p enouia-memory-workspace
/// persona_contract_samples -- --ignored`. Deterministic clock and IDs.
#[test]
#[ignore = "manual regeneration of tests/fixtures/persona"]
fn persona_contract_samples() {
    let out = PathBuf::from(std::env::var_os("ENOUIA_PERSONA_SAMPLE_DIR").expect("sample dir"));
    let w = world("persona-sample");
    let env = &w.env;
    let changed = public_memory(env, "（合成公开）会做番茄炒蛋。", "synthetic.cooking", &[]);
    let gone = public_memory(env, "（合成公开）养过一只猫。", "synthetic.cat", &[]);
    let ids = [
        w.selected_public.as_str(),
        changed.as_str(),
        gone.as_str(),
        w.selected_private.as_str(),
    ];
    let chosen = selection(&w.identity, &ids);
    let first = env.ok("persona_preview", chosen.clone());
    assert_eq!(
        export(env, &first, &chosen, "v1", "persona-sample-one")["result"]["version"],
        1
    );
    let src = source(
        env,
        "（合成公开）会做番茄炒蛋和蛋炒饭。",
        Sensitivity::Public,
    );
    accept(
        env,
        &Proposal {
            kind: ProposalKind::Revise,
            proposed_type: ProposedType::Fact,
            content: "（合成公开）会做番茄炒蛋和蛋炒饭。".into(),
            details: None,
            evidence: vec![EvidenceSpec::content(src, one())],
            reason: "owner_correction".into(),
            context_source_refs: vec![],
            sensitivity: None,
            target_memory_id: Some(MemoryId::parse(&changed).unwrap()),
            target_identity_id: None,
            expected_revision: Some(one()),
            effective_from: None,
            reopens_candidate_id: None,
        },
    );
    let forget = env.ok(
        "forget_plan",
        json!({"memoryId": gone, "mode": "forget", "withDependents": false}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": forget["planId"], "diffHash": forget["diffHash"]}),
    );
    let added = public_memory(
        env,
        "（合成公开）周末常去爬山。",
        "synthetic.hiking",
        &["hobby.outdoor"],
    );
    let ids = [
        w.selected_public.as_str(),
        changed.as_str(),
        gone.as_str(),
        w.selected_private.as_str(),
        added.as_str(),
    ];
    let chosen = selection(&w.identity, &ids);
    let preview = env.ok("persona_preview", chosen.clone());
    let options = env.ok("persona_options", json!({"audience": "public_group"}));
    let receipt = export(env, &preview, &chosen, "v2", "persona-sample-two")["result"].clone();
    let document = std::fs::read(
        env.base
            .join("v2")
            .join(receipt["fileName"].as_str().unwrap()),
    )
    .unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let pretty = |v: &Value| format!("{}\n", serde_json::to_string_pretty(v).unwrap());
    std::fs::write(out.join("projection-sample.json"), document).unwrap();
    std::fs::write(out.join("identity-block.json"), pretty(&block("合成伙伴"))).unwrap();
    std::fs::write(out.join("preview.json"), pretty(&preview)).unwrap();
    std::fs::write(out.join("options.json"), pretty(&options)).unwrap();
    std::fs::write(out.join("receipt.json"), pretty(&receipt)).unwrap();
}
