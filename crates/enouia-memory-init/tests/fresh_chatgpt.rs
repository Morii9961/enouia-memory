use enouia_memory_contract::{
    common::{EvidenceClass, SpeakerRole},
    hash::sha256,
};
use enouia_memory_import::{locate, parsed::ParentLink};
use enouia_memory_init::{
    fresh::{ChatGptSelection, prepare_chatgpt_selection},
    parse::{Format, MAX_MATERIAL_BYTES, Material},
};
use serde_json::json;

fn shard(alias: &str, bytes: Vec<u8>) -> Material {
    Material {
        alias: alias.into(),
        format: Format::Auto,
        bytes,
        account_alias: "synthetic".into(),
    }
}
fn selection(alias: &str, id: &str) -> ChatGptSelection {
    ChatGptSelection {
        shard_alias: alias.into(),
        conversation_id: id.into(),
    }
}
fn conversation(id: &str) -> String {
    json!({"conversation_id":id,"title":"Synthetic conversation","current_node":"n",
        "mapping":{"n":{"parent":null,"message":{"id":"message",
            "author":{"role":"user"},"create_time":1700000000,
            "content":{"parts":["Synthetic owner statement."]}}}}})
    .to_string()
}

#[test]
fn large_shards_keep_exact_selected_spans_and_resolvable_source_lineage() {
    let original = r#"{ "conversation_id" : "selected", "title": "Synthetic 中文",
      "mapping" : {
        "u": {"parent":null, "message":{"id":"owner", "author":{"role":"user"},
          "create_time":1700000000, "content_lang":"zh",
          "content":{"parts":["（合成）我喜欢\u5b89静。\n第二行。"]}}},
        "a": {"parent":"u", "message":{"id":"assistant", "author":{"role":"assistant"},
          "create_time":1700000001, "metadata":{"model_slug":"synthetic-model"},
          "content":{"parts":["Synthetic reply."]}}}
      }, "current_node":"a", "unknown_field": {"keep":[true,"\\\"{}[]"]} }"#;
    let excluded = json!({"conversation_id":"excluded","mapping":{},
        "private_not_selected":"x".repeat(MAX_MATERIAL_BYTES + 10)});
    let bytes = format!(" [\n{excluded},\t{original}\r\n] \n").into_bytes();
    assert!(bytes.len() > MAX_MATERIAL_BYTES);
    let input = shard("first", bytes);
    let before = sha256(&input.bytes);
    let prep = prepare_chatgpt_selection(
        std::slice::from_ref(&input),
        &[selection("first", "selected")],
    )
    .unwrap();
    assert_eq!(sha256(&input.bytes), before);
    assert_eq!((prep.objects.len(), prep.messages.len()), (1, 2));
    assert_eq!((prep.unsupported, prep.unparseable), (0, 0));
    let wrapped = format!("[{original}]").into_bytes();
    assert_eq!(prep.objects.values().next().unwrap(), &wrapped);
    assert!(!prep.objects.values().any(|b| b == &input.bytes));
    for message in &prep.messages {
        assert_eq!(message.conversation.as_deref(), Some("selected"));
        assert_eq!(
            locate::resolve(&prep.objects[&message.raw_hash], &message.locator).unwrap(),
            message.text.as_bytes()
        );
        let origin = &message.metadata["fresh_chatgpt_origin"];
        assert_eq!(origin["shard_hash"], before.as_str());
        assert_eq!(origin["array_index"], 1);
        let start = origin["byte_start"].as_u64().unwrap() as usize;
        let end = origin["byte_end"].as_u64().unwrap() as usize;
        assert_eq!(&input.bytes[start..end], original.as_bytes());
        assert_eq!(
            message.metadata["raw_message"]["id"],
            message.upstream_id.as_deref().unwrap()
        );
    }
    let owner = prep
        .messages
        .iter()
        .find(|m| m.role == SpeakerRole::User)
        .unwrap();
    assert_eq!(owner.evidence, EvidenceClass::UserStatement);
    assert_eq!(owner.text, "（合成）我喜欢安静。\n第二行。");
    let assistant = prep
        .messages
        .iter()
        .find(|m| m.role == SpeakerRole::Assistant)
        .unwrap();
    assert_eq!(assistant.evidence, EvidenceClass::ModelClaim);
    assert_eq!(
        assistant.assistant_model.as_deref(),
        Some("synthetic-model")
    );
    assert_eq!(assistant.parent, ParentLink::Upstream("owner".into()));
    assert!(assistant.original_timezone.is_none());
    assert_eq!(
        prep.selection_metadata["format"],
        "fresh-chatgpt-selection/1"
    );
    let mut changed = input;
    changed.bytes.push(b' ');
    let other = prepare_chatgpt_selection(&[changed], &[selection("first", "selected")]).unwrap();
    assert_eq!(other.messages[0].text, prep.messages[0].text);
    assert_eq!(other.messages[0].raw_hash, prep.messages[0].raw_hash);
    assert_ne!(other.watermark(), prep.watermark());
}

#[test]
fn selection_is_complete_and_unambiguous_across_shards() {
    let one = shard("one", format!("[{}]", conversation("a")).into_bytes());
    let two = shard("two", format!("[{}]", conversation("b")).into_bytes());
    let prep = prepare_chatgpt_selection(
        &[one.clone(), two],
        &[selection("one", "a"), selection("two", "b")],
    )
    .unwrap();
    assert_eq!(prep.messages.len(), 2);
    for (scope, error) in [
        (vec![], "init.fresh_selection_scope"),
        (
            vec![selection("unknown", "a")],
            "init.fresh_selection_scope",
        ),
        (
            vec![selection("one", "missing")],
            "init.fresh_selection_missing",
        ),
        (
            vec![selection("one", "a"), selection("one", "a")],
            "init.fresh_selection_scope",
        ),
    ] {
        assert_eq!(
            prepare_chatgpt_selection(std::slice::from_ref(&one), &scope).err(),
            Some(error)
        );
    }
    let duplicate = shard(
        "one",
        format!("[{},{}]", conversation("a"), conversation("a")).into_bytes(),
    );
    assert_eq!(
        prepare_chatgpt_selection(&[duplicate], &[selection("one", "a")]).err(),
        Some("init.fresh_selection_ambiguous")
    );
    let duplicate_identity = shard("two", format!("[{}]", conversation("a")).into_bytes());
    assert_eq!(
        prepare_chatgpt_selection(
            &[one, duplicate_identity],
            &[selection("one", "a"), selection("two", "a")]
        )
        .err(),
        Some("init.fresh_selection_ambiguous")
    );
}

#[test]
fn malformed_array_boundaries_refuse_the_entire_preparation() {
    let c = conversation("a");
    for invalid in [
        c.clone(),
        format!("[{c},]"),
        format!("[{c}] false"),
        format!("[{c} {c}]"),
        format!("[{c}"),
        format!("[{c},{{]"),
        format!("\u{c}[{c}]"),
    ] {
        assert_eq!(
            prepare_chatgpt_selection(
                &[shard("one", invalid.into_bytes())],
                &[selection("one", "a")]
            )
            .err(),
            Some("init.fresh_shard_json")
        );
    }
}

#[test]
fn selected_conversation_still_obeys_the_existing_material_limit() {
    let oversized = json!({"conversation_id":"a","mapping":{},
        "padding":"x".repeat(MAX_MATERIAL_BYTES)});
    assert_eq!(
        prepare_chatgpt_selection(
            &[shard("one", format!("[{oversized}]").into_bytes())],
            &[selection("one", "a")]
        )
        .err(),
        Some("init.material_scope")
    );
    let bad_shape = shard("one", br#"[{"conversation_id":"a"}]"#.to_vec());
    assert_eq!(
        prepare_chatgpt_selection(&[bad_shape], &[selection("one", "a")]).err(),
        Some("init.fresh_selected_shape")
    );
}
