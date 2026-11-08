//! Public persona projection v1 (ADR-MEM-128): the synthetic sample corpus
//! agrees with the schema and the Rust boundary, including the canonical
//! digest a downstream consumer recomputes.

mod support;

use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::persona::{
    Document, IdentityBlock, Projection, extract_block, parse_document, projection_sha256,
};
use serde_json::Value;
use support::schema::SchemaStore;

fn path(name: &str) -> std::path::PathBuf {
    support::fixtures_root().join("../persona").join(name)
}

fn native(kind: &str, value: &Value) -> Result<(), Vec<&'static str>> {
    let rules = |e: enouia_memory_contract::ContractError| e.rules();
    match kind {
        "document" => parse_document(&canonical_bytes(value).unwrap())
            .map(|_| ())
            .map_err(rules),
        "identity_block" => {
            let block: IdentityBlock =
                serde_json::from_value(value.clone()).map_err(|_| vec!["shape"])?;
            let mut out = Vec::new();
            block.validate("", &mut out);
            if out.is_empty() {
                Ok(())
            } else {
                Err(out.iter().map(|v| v.rule).collect())
            }
        }
        "preview" => {
            let projection: Projection =
                serde_json::from_value(value["projection"].clone()).map_err(|_| vec!["shape"])?;
            assert!(projection.validate().is_empty());
            assert_eq!(
                serde_json::json!(projection_sha256(&projection)),
                value["projectionSha256"]
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

#[test]
fn persona_corpus_agrees_with_schema_and_native_rules() {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(path("manifest.json")).unwrap()).unwrap();
    let schemas = SchemaStore::load();
    let load = |file: &str| -> Value {
        serde_json::from_slice(&std::fs::read(path(file)).unwrap()).unwrap()
    };
    for entry in manifest["valid"].as_array().unwrap() {
        let file = entry["file"].as_str().unwrap();
        let value = load(file);
        let errors = schemas.validate(entry["schema"].as_str().unwrap(), &value);
        assert!(errors.is_empty(), "{file}: {errors:?}");
        native(entry["type"].as_str().unwrap(), &value).unwrap_or_else(|r| panic!("{file}: {r:?}"));
    }
    for case in manifest["invalid"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let entry = manifest["valid"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["file"] == case["base"])
            .unwrap();
        let mut value = load(case["base"].as_str().unwrap());
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate(entry["schema"].as_str().unwrap(), &value)
                .is_empty(),
            case["schema"] == "accept",
            "{id}"
        );
        if let Some(rule) = case["rust_rule"].as_str() {
            let rules = native(entry["type"].as_str().unwrap(), &value).expect_err(id);
            assert!(rules.contains(&rule), "{id}: {rules:?}");
        }
    }
}

/// The sample file is the exact canonical bytes; its digests are reproducible
/// from the bytes alone.
#[test]
fn persona_sample_is_canonical_and_its_digests_recompute() {
    let bytes = std::fs::read(path("projection-sample.json")).unwrap();
    let document: Document = parse_document(&bytes).unwrap();
    assert_eq!(canonical_bytes(&document).unwrap(), bytes);
    assert_eq!(
        sha256(&canonical_bytes(&document.projection).unwrap()),
        document.projection_sha256
    );
    let receipt: Value =
        serde_json::from_slice(&std::fs::read(path("receipt.json")).unwrap()).unwrap();
    assert_eq!(serde_json::json!(sha256(&bytes)), receipt["fileSha256"]);
    assert_eq!(receipt["bytes"], bytes.len());
    assert_eq!(receipt["version"], document.version);
}

#[test]
fn persona_block_is_found_once_inside_identity_markdown() {
    let block = std::fs::read_to_string(path("identity-block.json")).unwrap();
    let markdown = format!(
        "# Synthetic\r\n\r\nprivate prose\r\n```enouia-public-persona-1\r\n{block}```\r\nmore\r\n"
    );
    let found = extract_block(&markdown).unwrap();
    assert_eq!(found.lines_outside, 3);
    assert_eq!(found.block.display_name, "合成伙伴");
    for (text, rule) in [
        ("no block".to_owned(), "persona.block_missing"),
        (format!("{markdown}{markdown}"), "persona.block_repeated"),
        (
            "```enouia-public-persona-1\n{}\n".to_owned(),
            "persona.block_unclosed",
        ),
        (
            "```enouia-public-persona-1\nnot json\n```\n".to_owned(),
            "persona.block_json",
        ),
    ] {
        let error = extract_block(&text).unwrap_err();
        assert_eq!(error.rules(), vec![rule]);
        assert!(!error.to_string().contains("private prose"));
    }
}
