//! Wayland resource provider using Trawl D-Bus service.

use anyhow::{Context, Result};
use std::collections::HashMap;
use zbus::{blocking::fdo::PropertiesProxy, blocking::Connection, names::InterfaceName};

use super::ResourceProvider;

/// Resource provider for Wayland sessions using the Trawl D-Bus service.
///
/// Retrieves resources from the `org.regolith.Trawl` D-Bus service,
/// which provides X resource compatibility for Wayland sessions.
pub struct TrawlResourceProvider;

impl ResourceProvider for TrawlResourceProvider {
    /// Queries all X resources from the Trawl D-Bus service.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The D-Bus session bus cannot be connected
    /// - The Trawl service is not available
    /// - The `Resources` property cannot be read
    fn query_resources(&self) -> Result<HashMap<String, String>> {
        let connection = Connection::session().context("Failed to connect to D-Bus session bus")?;
        let props = PropertiesProxy::builder(&connection)
            .destination("org.regolith.Trawl")?
            .path("/org/regolith/Trawl")?
            .build()
            .context("Failed to connect to Trawl D-Bus service")?;

        let resources_map: HashMap<String, String> = props
            .get(InterfaceName::try_from("org.regolith.trawl1")?, "Resources")
            .context("Failed to read property 'Resources' from Trawl D-Bus service")?
            .try_into()?;

        Ok(resources_map)
    }
}
