//! Deterministic origin grouping shared by authored preparation and admission.
use crate::{common::SourceRevisionRef, source::SourceRecord};
use std::collections::{BTreeMap, BTreeSet};

/// Canonical non-singleton components of distinct, bounded supporting sources.
/// Equal content or an identical upstream message joins origins transitively.
/// This does not validate sources, grant authority or claim corroboration.
pub fn support_origin_groups(sources: &[&SourceRecord]) -> Vec<Vec<SourceRevisionRef>> {
    let mut roots: Vec<_> = (0..sources.len()).collect();
    for (i, source) in sources.iter().enumerate() {
        for (j, other) in sources[..i].iter().enumerate() {
            let same_upstream = source.original_message_id.is_some()
                && source.provider == other.provider
                && source.account_scope == other.account_scope
                && source.original_conversation_id == other.original_conversation_id
                && source.original_message_id == other.original_message_id;
            if source.content_hash == other.content_hash || same_upstream {
                let (from, to) = (roots[i], roots[j]);
                for root in &mut roots {
                    if *root == from {
                        *root = to;
                    }
                }
            }
        }
    }
    let mut components: BTreeMap<usize, BTreeSet<SourceRevisionRef>> = BTreeMap::new();
    for (source, root) in sources.iter().zip(roots) {
        components
            .entry(root)
            .or_default()
            .insert(SourceRevisionRef {
                source_id: source.source_id.clone(),
                source_revision: source.revision,
            });
    }
    let mut groups: Vec<Vec<_>> = components
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|g| g.into_iter().collect())
        .collect();
    groups.sort();
    groups
}
