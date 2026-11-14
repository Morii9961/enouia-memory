//! Explicit fresh ChatGPT selection, before semantic analysis (ADR-MEM-140).
use crate::parse::{Format, Material, Preparation, prepare};
use enouia_memory_contract::hash::sha256;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct ChatGptSelection {
    pub shard_alias: String,
    pub conversation_id: String,
}

/// Pure, caller-selected input. No file discovery, model, authority or Vault I/O.
/// Original shards stay in the owner's input archive; prepared objects contain
/// exact selected conversation spans inside a derived two-bracket envelope.
pub fn prepare_chatgpt_selection(
    shards: &[Material],
    selections: &[ChatGptSelection],
) -> Result<Preparation, &'static str> {
    let mut aliases = BTreeSet::new();
    for shard in shards {
        if shard.format != Format::Auto
            || shard.alias.is_empty()
            || shard.alias.len() > 64
            || !shard
                .alias
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || !enouia_memory_contract::source::is_account_alias(&shard.account_alias)
            || !aliases.insert(shard.alias.as_str())
        {
            return Err("init.fresh_shard_scope");
        }
    }
    let mut wanted = BTreeSet::new();
    for selection in selections {
        if selection.conversation_id.is_empty()
            || !aliases.contains(selection.shard_alias.as_str())
            || !wanted.insert((
                selection.shard_alias.as_str(),
                selection.conversation_id.as_str(),
            ))
        {
            return Err("init.fresh_selection_scope");
        }
    }
    if wanted.is_empty() {
        return Err("init.fresh_selection_scope");
    }
    let mut found = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut materials = Vec::new();
    let mut origins = BTreeMap::new();
    let mut shard_origins = Vec::new();
    for shard in shards {
        let mut cursor = 0;
        let skip_space = |cursor: &mut usize| {
            while matches!(shard.bytes.get(*cursor), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                *cursor += 1;
            }
        };
        skip_space(&mut cursor);
        if shard.bytes.get(cursor) != Some(&b'[') {
            return Err("init.fresh_shard_json");
        }
        cursor += 1;
        skip_space(&mut cursor);
        let shard_hash = sha256(&shard.bytes);
        let mut index = 0;
        if shard.bytes.get(cursor) == Some(&b']') {
            cursor += 1;
        } else {
            loop {
                let start = cursor;
                let mut stream = serde_json::Deserializer::from_slice(&shard.bytes[start..])
                    .into_iter::<Value>();
                let conversation = stream
                    .next()
                    .ok_or("init.fresh_shard_json")?
                    .map_err(|_| "init.fresh_shard_json")?;
                // byte_offset is meaningful here only after successful parsing.
                let end = start + stream.byte_offset();
                if let Some(id) = conversation["conversation_id"]
                    .as_str()
                    .or_else(|| conversation["id"].as_str())
                {
                    let key = (shard.alias.as_str(), id);
                    if wanted.contains(&key) {
                        if !found.insert((shard.alias.clone(), id.to_owned()))
                            || !identities.insert((shard.account_alias.clone(), id.to_owned()))
                        {
                            return Err("init.fresh_selection_ambiguous");
                        }
                        let mut bytes = Vec::with_capacity(end - start + 2);
                        bytes.push(b'[');
                        bytes.extend_from_slice(&shard.bytes[start..end]);
                        bytes.push(b']');
                        let hash = sha256(&bytes);
                        origins.insert(
                            (hash, shard.account_alias.clone()),
                            json!({"shard_alias":shard.alias,"shard_hash":shard_hash,
                                "shard_bytes":shard.bytes.len(),"account_alias":shard.account_alias,
                                "conversation_id":id,"array_index":index,
                                "byte_start":start,"byte_end":end,
                                "derived_envelope":"two_brackets_around_exact_original_span"}),
                        );
                        materials.push(Material {
                            alias: format!("fresh-{}", materials.len()),
                            format: Format::Auto,
                            bytes,
                            account_alias: shard.account_alias.clone(),
                        });
                    }
                }
                index += 1;
                cursor = end;
                skip_space(&mut cursor);
                match shard.bytes.get(cursor) {
                    Some(b',') => {
                        cursor += 1;
                        skip_space(&mut cursor);
                    }
                    Some(b']') => {
                        cursor += 1;
                        break;
                    }
                    _ => return Err("init.fresh_shard_json"),
                }
            }
        }
        skip_space(&mut cursor);
        if cursor != shard.bytes.len() {
            return Err("init.fresh_shard_json");
        }
        shard_origins.push(json!({"alias":shard.alias,"hash":shard_hash,
            "bytes":shard.bytes.len(),"account_alias":shard.account_alias}));
    }
    if found.len() != wanted.len() {
        return Err("init.fresh_selection_missing");
    }
    let mut prep = prepare(&materials)?;
    if prep.unsupported != 0 {
        return Err("init.fresh_selected_shape");
    }
    for message in &mut prep.messages {
        let origin = &origins[&(message.raw_hash.clone(), message.account_alias.clone())];
        message.metadata = json!({"raw_message":message.metadata.take(),
            "fresh_chatgpt_origin":origin});
    }
    prep.selection_metadata = json!({"format":"fresh-chatgpt-selection/1",
        "original_shards":shard_origins,
        "selected":origins.iter().map(|((hash, _), origin)|
            json!({"derived_object_hash":hash,"origin":origin})).collect::<Vec<_>>()});
    Ok(prep)
}
