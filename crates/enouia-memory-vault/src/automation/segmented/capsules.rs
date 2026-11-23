//! ADR-MEM-173: Morii's saved recall capsules, read from his own owner
//! actions. Read-only; saving and discarding are ordinary owner actions.
use super::*;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapsuleMember {
    /// The memory revision as saved.
    pub saved: TargetRef,
    /// Its current revision, or `None` when it was withdrawn, forgotten or
    /// deleted since.
    pub current: Option<TargetRef>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SavedCapsule {
    pub name: String,
    pub query: String,
    pub members: Vec<CapsuleMember>,
    /// The owner action that saved this version.
    pub saved_by: OwnerActionProof,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SavedCapsules {
    pub snapshot: SnapshotBinding,
    /// The latest version of every capsule not discarded, by name.
    pub capsules: Vec<SavedCapsule>,
}

impl SegmentedAutomationVault {
    /// Morii's own records, read unfiltered so that deleting a member never
    /// brings back an older version; members report their current revision
    /// in his full-history read at `max`.
    pub fn capsules(&self, owner: &ActorRef, max: Sensitivity) -> Result<SavedCapsules> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let d = state.authorized_for(
            &self.native.descriptor,
            max,
            self.native.now()?,
            ReadBasis::FullHistory,
        )?;
        let current: BTreeMap<&MemoryId, &TargetRef> = d
            .memories
            .iter()
            .map(|m| (&m.target.memory.memory_id, &m.target))
            .collect();
        let mut records: Vec<_> = state.owner_actions.values().map(|(r, _)| r).collect();
        records.sort_by_key(|r| r.sequence);
        let mut latest: BTreeMap<&str, SavedCapsule> = BTreeMap::new();
        {
            for record in records {
                let OwnerAction::Capsule {
                    name,
                    query,
                    members,
                } = &record.plan.action
                else {
                    continue;
                };
                if members.is_empty() {
                    latest.remove(name.as_str());
                    continue;
                }
                latest.insert(
                    name,
                    SavedCapsule {
                        name: name.clone(),
                        query: query.clone(),
                        members: members
                            .iter()
                            .map(|t| CapsuleMember {
                                saved: t.clone(),
                                current: current.get(&t.memory.memory_id).map(|c| (*c).clone()),
                            })
                            .collect(),
                        saved_by: OwnerActionProof::from_record(record),
                    },
                );
            }
        }
        Ok(SavedCapsules {
            snapshot: d.snapshot.clone(),
            capsules: latest.into_values().collect(),
        })
    }
}
