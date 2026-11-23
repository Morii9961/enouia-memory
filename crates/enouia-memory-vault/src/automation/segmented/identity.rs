//! ADR-MEM-172: entity identity across windows. Morii's decisions group
//! records (see the `EntityIdentity` handling in `catalog.rs`); records of the
//! same type and name that he has not separated are linked automatically, as
//! a revocable suggestion. Nothing here writes or authorizes anything.
use super::*;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityBasis {
    /// Every record resolves to one identity through Morii's decisions.
    Confirmed,
    /// At least one record is linked only by an identical type and name.
    SameName,
    /// A record linked to nothing else.
    Single,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EntityIdentityRecord {
    pub entity_id: String,
    /// The identity this record resolves to after merges and decisions.
    pub resolved_entity_id: String,
    pub display_name: String,
    /// Current memories in this read that mention the record.
    pub memories: u64,
    /// Morii separated it: it never joins a same-name link.
    pub opted_out: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EntityIdentityGroup {
    pub basis: IdentityBasis,
    pub entity_type: EntityType,
    /// The display name of the group's first record.
    pub name: String,
    pub records: Vec<EntityIdentityRecord>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EntityIdentities {
    pub snapshot: SnapshotBinding,
    /// Confirmed and same-name groups first, then single records, by name.
    pub groups: Vec<EntityIdentityGroup>,
    pub total_groups: u64,
    pub complete: bool,
}

fn normalize(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl AuthorizedSegmentedRead<'_> {
    /// The automatic same-name key of a visible record: its type and its
    /// display name with whitespace collapsed and lowercased. `None` when
    /// Morii separated the record or it is not visible.
    pub fn same_name_key(&self, record: &str) -> Option<String> {
        if self.entity_opted_out.contains(record) {
            return None;
        }
        let e = self.entities.get(record)?;
        Some(format!(
            "{:?}:{}",
            e.entity_type,
            normalize(&e.display_name)
        ))
    }
    /// The identity a memory's mention of `record` resolves to.
    pub fn resolved_entity<'s>(&'s self, memory: &MemoryId, record: &'s str) -> &'s str {
        self.entity_redirects
            .get(&(memory.clone(), record.to_string()))
            .map_or(record, String::as_str)
    }
    fn identity_groups(&self, limit: usize) -> (Vec<EntityIdentityGroup>, u64) {
        // Mentions per record and the identity each resolves to.
        let mut memories: BTreeMap<&str, u64> = BTreeMap::new();
        let mut resolved: BTreeMap<&str, String> = BTreeMap::new();
        for m in &self.memories {
            let id = &m.memory.memory.memory_id;
            for mention in &m
                .memory
                .decision
                .input
                .record_fields
                .as_ref()
                .unwrap()
                .entities
            {
                if let Some((record, _)) = self.entities.get_key_value(&mention.entity_id) {
                    *memories.entry(record.as_str()).or_default() += 1;
                    resolved
                        .entry(record.as_str())
                        .or_insert_with(|| self.resolved_entity(id, record).to_string());
                }
            }
        }
        // Union records that share a resolved identity or a same-name key.
        let records: Vec<&str> = self.entities.keys().map(String::as_str).collect();
        let mut parent: Vec<usize> = (0..records.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut by_key: BTreeMap<String, usize> = BTreeMap::new();
        for (i, record) in records.iter().enumerate() {
            let identity = resolved.get(record).map_or(*record, String::as_str);
            let mut keys = vec![format!("id:{identity}")];
            keys.extend(self.same_name_key(record).map(|k| format!("name:{k}")));
            for key in keys {
                let first = *by_key.entry(key).or_insert(i);
                let (a, b) = (root(&mut parent, first), root(&mut parent, i));
                parent[a] = b;
            }
        }
        let mut components: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
        for (i, record) in records.iter().enumerate() {
            components
                .entry(root(&mut parent, i))
                .or_default()
                .push(record);
        }
        let mut groups: Vec<EntityIdentityGroup> = components
            .into_values()
            .map(|members| {
                let entries: Vec<EntityIdentityRecord> = members
                    .iter()
                    .map(|record| EntityIdentityRecord {
                        entity_id: record.to_string(),
                        resolved_entity_id: resolved
                            .get(record)
                            .cloned()
                            .unwrap_or_else(|| record.to_string()),
                        display_name: self.entities[*record].display_name.clone(),
                        memories: memories.get(record).copied().unwrap_or(0),
                        opted_out: self.entity_opted_out.contains(*record),
                    })
                    .collect();
                let basis = if entries.len() == 1 {
                    IdentityBasis::Single
                } else if entries
                    .iter()
                    .all(|r| r.resolved_entity_id == entries[0].resolved_entity_id)
                {
                    IdentityBasis::Confirmed
                } else {
                    IdentityBasis::SameName
                };
                EntityIdentityGroup {
                    basis,
                    entity_type: self.entities[members[0]].entity_type,
                    name: self.entities[members[0]].display_name.clone(),
                    records: entries,
                }
            })
            .collect();
        groups.sort_by_key(|g| (g.basis == IdentityBasis::Single, normalize(&g.name)));
        let total = groups.len() as u64;
        groups.truncate(limit);
        (groups, total)
    }
}

impl SegmentedAutomationVault {
    /// ADR-MEM-172: Morii's view of entity identities over his full history,
    /// for the Runtime confirmation UI. Read-only; decisions are applied with
    /// the ordinary owner plan, confirmation and commit of `EntityIdentity`.
    pub fn entity_identities(
        &self,
        owner: &ActorRef,
        max: Sensitivity,
        limit: usize,
    ) -> Result<EntityIdentities> {
        self.native.owner(owner)?;
        if !(1..=4096).contains(&limit) {
            return Err(AutomationFault::Denied);
        }
        let system = self.native.descriptor.system_actor.clone();
        self.read_scoped(&system, max, SystemReadScope::MoriiFullHistory, |d| {
            let (groups, total_groups) = d.identity_groups(limit);
            Ok(EntityIdentities {
                snapshot: d.snapshot.clone(),
                complete: groups.len() as u64 == total_groups,
                groups,
                total_groups,
            })
        })?
    }
}
