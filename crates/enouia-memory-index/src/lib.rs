//! Local index and retrieval (MV-4): a rebuildable SQLite/FTS5 projection
//! of the Vault, and literal, permission-filtered, snapshot-bound search.
//!
//! The index only finds candidate IDs. Everything returned is filtered by
//! the Vault's current tombstones and policies and checked against the
//! pinned revision; deleting the index loses nothing (R01). There is no
//! embedding, cloud search, or automatic raw-text injection here.

pub mod fold;
pub mod search;
pub mod store;

pub use search::{Hit, Matched, ProjectMatch, RANKING_VERSION, SearchPage, SearchRequest, search};
pub use store::{Index, UpdateReport, Watermark};
