//! MV-7 text providers. No account discovery, implicit sending, fallback,
//! automatic memory acceptance, or dependency on a desktop host.
pub mod client;
pub mod codec;
pub mod egress;
pub mod extraction;
pub mod policy;
pub mod stream;
pub mod transport;
pub mod vault;
pub mod windows;

use enouia_memory_contract::{MemoryError, MemoryErrorCode, foundation::ComponentId};

pub type Result<T> = std::result::Result<T, MemoryError>;
pub(crate) fn error(code: MemoryErrorCode) -> MemoryError {
    MemoryError::new(code, ComponentId::Provider)
}
