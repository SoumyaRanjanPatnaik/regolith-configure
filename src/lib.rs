//! Regolith configuration management library.
//!
//! This crate provides tools for searching, reading, and modifying Regolith
//! window manager configurations across X11 and Wayland sessions.
//!
//! # Modules
//!
//! - [`cli_args`] - CLI argument types and session detection
//! - [`commands`] - High-level operations (search, set resource)
//! - [`config`] - Configuration parsing and representation
//! - [`resources`] - X resource providers for different display systems
//! - [`search`] - Search functionality for bindings, keywords, and resources
//!
//! # Example
//!
//! ```no_run
//! use regolith_config::{FullConfig, execute_search, set_user_xresource};
//! use regolith_config::resources::XrdbResourceProvider;
//! use regolith_config::cli_args::{Session, FilterType};
//! use std::path::Path;
//!
//! // Load configuration for a session
//! let mappings = [(Session::X11, Path::new("/etc/regolith/i3/config"))];
//! let config = FullConfig::load_for_session(Session::X11, &mappings)?;
//!
//! // Search for keybindings
//! let provider = XrdbResourceProvider;
//! let result = execute_search(FilterType::Bindings, "Super+Enter", &config, &provider);
//!
//! // Set a user X resource
//! let path = set_user_xresource("regolithwm.border.width", "2")?;
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod cli_args;
pub mod commands;
pub mod config;
pub mod output;
pub mod resources;
pub mod search;

pub use cli_args::get_session_type;
pub use commands::{execute_search, set_user_xresource, SearchResult};
pub use config::{ConfigPartial, FullConfig, SessionMappings};

#[cfg(test)]
pub mod test_utils;
#[cfg(test)]
pub mod tests;
