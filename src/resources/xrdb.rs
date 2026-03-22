//! X11 resource provider using xrdb.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Command;

use super::ResourceProvider;

/// Resource provider for X11 sessions using the `xrdb` command.
///
/// Retrieves resources by querying the X resource database via `xrdb -query`.
pub struct XrdbResourceProvider;

impl ResourceProvider for XrdbResourceProvider {
    /// Queries all X resources from the X resource database.
    ///
    /// # Errors
    ///
    /// Returns an error if the `xrdb` command cannot be executed.
    fn query_resources(&self) -> Result<HashMap<String, String>> {
        let output = Command::new("xrdb")
            .arg("-query")
            .output()
            .context("Failed to execute xrdb command")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut resources = HashMap::new();

        for line in stdout.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                if !key.is_empty() {
                    resources.insert(key.to_string(), value.to_string());
                }
            }
        }

        Ok(resources)
    }
}
