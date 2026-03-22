//! X resource providers for different display systems.
//!
//! This module provides the [`ResourceProvider`] trait and implementations
//! for retrieving X resources from X11 (via xrdb) and Wayland (via Trawl D-Bus).

mod trawl;
mod xrdb;

pub use trawl::TrawlResourceProvider;
pub use xrdb::XrdbResourceProvider;

/// Provider for X resources at runtime.
///
/// Implementations retrieve the current X resource database, which contains
/// key-value pairs used by X applications for configuration.
pub trait ResourceProvider {
    /// Queries the provider for all available X resources.
    ///
    /// # Returns
    ///
    /// A `HashMap` mapping resource names to their current values.
    ///
    /// # Errors
    ///
    /// Returns an error if the resources cannot be retrieved (e.g.,
    /// the display server is unavailable or the D-Bus service is not running).
    fn query_resources(&self) -> anyhow::Result<std::collections::HashMap<String, String>>;
}
