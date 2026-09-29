//! Memory governance (MV-3): candidates, owner review, supersession,
//! conflicts, identity revisions, and deletion.
//!
//! Models and agents only propose candidates. A canonical memory, identity
//! revision, or tombstone is written only by an owner review confirmed on a
//! trusted surface: the reviewer sees an exact plan (every record it will
//! write), and the confirmation is bound to that plan's hash, a single-use
//! nonce, and the expected revisions of everything it touches. Nothing here
//! ranks, auto-accepts, or uses confidence as approval.
//!
//! The in-process boundary is the owner check against the Vault's genesis
//! owner and the trusted-surface enum; which process may speak for the owner
//! is transport authentication (MV-8), not something a caller asserts.

pub mod delete;
pub mod evidence;
pub mod propose;
pub mod review;
pub mod view;

mod util;

pub use evidence::EvidenceSpec;
pub use propose::{
    CandidateEdit, Origin, Proposal, Proposed, edit_candidate, pending_candidates, propose,
    withdraw,
};
pub use review::{Decision, OwnerConfirmation, ReviewPlan, confirm, plan};
pub use view::{Canonical, canonical_memories};
