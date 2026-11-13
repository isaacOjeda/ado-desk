//! Assistant panel (roadmap phase 7): the user's own Claude Code running in an embedded
//! terminal, with ADO Desk's context available to it.

pub mod confirm;
pub mod context;
pub mod launch;
pub mod mcp;
pub mod server;
pub mod terminal;

pub use terminal::AssistantState;
