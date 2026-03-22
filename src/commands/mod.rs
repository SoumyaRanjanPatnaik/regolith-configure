//! High-level operations for configuration management.
//!
//! This module provides the main operations exposed by the library:
//! searching configurations, setting resources, and ejecting configs.

use std::fmt::Display;

use crate::cli_args::{EjectArgs, Session};

pub mod search;
pub mod set_resource;

pub use search::{execute_search, SearchResult};
pub use set_resource::set_user_xresource;

/// Ejects a config partial from the system configuration.
///
/// Creates a local copy of a system config and disables the system instance.
///
/// # Note
///
/// This operation is not yet implemented.
pub fn eject_config(_args: &EjectArgs, _session: Session) -> Box<dyn Display> {
    todo!()
}
