//! History import and rescue (MV-2, IMPORT_REVIEW).
//!
//! Received bytes are archived first, exactly as received, and verified; only
//! then are they parsed. Parsing is deterministic, resumable, and separate
//! from archiving: an unknown or damaged format stays archived and is
//! reported, never guessed at. Imports produce *sources* (evidence), never
//! memories; nothing here extracts, summarizes, calls a model, or fetches a
//! URL. All tests use synthetic exports.

pub mod audit;
pub mod chatgpt;
pub mod detect;
pub mod locate;
pub mod markdown;
pub mod parsed;
pub mod pipeline;
pub mod runtime;
pub mod zip;

pub use audit::{ImportAudit, audit_import};
pub use pipeline::{ImportOptions, ImportReport, import_file, resume_import};
