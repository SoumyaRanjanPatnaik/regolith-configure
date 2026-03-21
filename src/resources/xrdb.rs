use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Command;

use super::ResourceProvider;

pub struct XrdbResourceProvider;

impl ResourceProvider for XrdbResourceProvider {
    fn get_all_resources(&self) -> Result<HashMap<String, String>> {
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
