//! Memory-domain file and IPC contracts for Enouia Runtime (MV-0).
//!
//! This crate is pure: it defines record types, strict parsing, per-record and
//! cross-record validation, deterministic temporal/eligibility rules, and the
//! ports that later Vault, Context, Session, and Provider implementations must
//! satisfy. It performs no filesystem, network, process, or clock access, and it
//! must never depend on the Enouia Runtime checkout, Activity crates, the
//! Moriium checkout, or credentials. Host ports it needs live in `foundation`.
//!
//! JSON Schemas under `contracts/{memory,context,provider,ipc}` describe the
//! same shapes. Constraints a schema cannot express (real dates, time order,
//! reference closure, supersession cycles, status history, idempotency) are
//! enforced here and listed in `docs/contracts/CONSTRAINT_MAP.md`.
//! Passing these validators does not prove transactional durability; that is
//! MV-1 behavior tested against a real store.

pub mod approval;
pub mod candidate;
pub mod catalog;
pub mod commit;
pub mod common;
pub mod context;
pub mod delta;
pub mod error;
pub mod extraction;
pub mod foundation;
pub mod hash;
pub mod identity;
pub mod ids;
pub mod import;
pub mod ipc;
pub mod json;
pub mod layout;
pub mod memory;
pub mod policy;
pub mod ports;
pub mod provider;
pub mod record;
pub mod scan;
pub mod session;
pub mod set;
pub mod source;
pub mod store;
pub mod temporal;
pub mod time;
pub mod workspace;

pub use error::{ContractError, MemoryError, MemoryErrorCode, Violation};
pub use record::{AnyRecord, RecordKind, parse_any, parse_record};
