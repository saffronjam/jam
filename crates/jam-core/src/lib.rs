pub mod protocol;
pub mod server;
pub mod vt;
pub use protocol::*;

pub mod client;

pub mod status;

pub mod config;
pub mod server_control;
pub mod workspaces;
pub mod worktree;
pub mod worktree_template;

mod pane_move;
mod process_args;

pub mod search;

pub mod remote;

pub mod agents;
mod clipboard;
pub mod file_preview;
pub mod input;

pub mod reviews;

pub mod usage;

pub mod coordination;

mod codex_session;

pub mod model_catalog;

pub mod recovery;

/// Application release version, distinct from wire protocol and source build.
pub const RELEASE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod release_version;

pub mod upgrade;

mod services;
