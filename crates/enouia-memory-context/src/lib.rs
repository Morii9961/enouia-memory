//! MV-5: deterministic local context, durable sessions, and an offline Mock.
//! No provider connection, tool execution, automatic approval, or network I/O.
//! The in-process owner boundary must be authenticated by a future Host.

pub mod compiler;
pub mod mock;
pub mod session;
mod util;

pub use compiler::{CompileInput, Compiled, compile};
pub use mock::{MockAnswer, answer_saved};
