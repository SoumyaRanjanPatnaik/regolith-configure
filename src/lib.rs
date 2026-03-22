pub mod cli_args;
pub mod commands;
pub mod config;
pub mod resources;
pub mod search;
pub use cli_args::get_session_type;
pub use commands::{SearchResult, search_config, set_user_xresource};
pub use config::{ConfigPartial, FullConfig, SessionMappings};

// Testing related modules only compiled when running tests
#[cfg(test)]
pub mod test_utils;
#[cfg(test)]
pub mod tests;
