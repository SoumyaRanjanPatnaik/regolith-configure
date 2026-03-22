//! Individual config file representation.
//!
//! This module provides [`ConfigPartial`] for representing a single
//! configuration file and extracting its imports, variables, and bindings.

use anyhow::{anyhow, Context, Result};
use glob::glob;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use crate::search;

/// A single configuration file.
///
/// Represents the contents of a window manager config file (i3 or sway)
/// and provides methods to extract its components.
#[derive(Debug)]
pub struct ConfigPartial {
    /// Path to the config file.
    pub file_name: PathBuf,
    /// Raw contents of the config file.
    pub config: String,
}

impl ConfigPartial {
    /// Creates a new config partial from a file path and contents.
    pub fn new(file_name: &Path, config: &str) -> Self {
        Self {
            file_name: file_name.to_path_buf(),
            config: config.to_string(),
        }
    }

    /// Resolves all `include` directives in this config.
    ///
    /// Handles both relative and absolute paths, as well as glob patterns.
    /// Relative paths are resolved relative to this config's directory.
    ///
    /// # Returns
    ///
    /// A vector of paths to included files. Non-existent paths matching
    /// a glob pattern are silently skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if a glob pattern is invalid.
    pub fn get_imported_paths(&self) -> Result<Vec<PathBuf>> {
        let mut imports = Vec::new();
        for line in self.config.lines() {
            if !line.trim().starts_with("include") {
                continue;
            }

            let mut import_path = line
                .trim()
                .strip_prefix("include")
                .ok_or_else(|| anyhow!("Invalid include statement - missing 'include' prefix"))?
                .trim()
                .trim_matches('"')
                .to_string();

            if !import_path.starts_with('/') {
                import_path = self
                    .file_name
                    .parent()
                    .ok_or_else(|| anyhow!("Config file has no parent directory"))?
                    .join(import_path)
                    .to_string_lossy()
                    .to_string();
            }

            let paths_matching_import_pattern = glob(&import_path)
                .with_context(|| "Failed to read glob pattern")?
                .filter(|entry| match entry {
                    Ok(path) => path.is_file(),
                    Err(_) => false,
                });

            for path_result in paths_matching_import_pattern {
                let path = path_result.with_context(|| "Failed to read import path")?;
                imports.push(path);
            }
        }
        Ok(imports)
    }

    /// Extracts variable definitions from this config.
    ///
    /// Recognizes `set $var value` and `set_from_resource $var name default`
    /// directives. For `set_from_resource`, the value is resolved from the
    /// provided resources map, falling back to the default if not found.
    ///
    /// # Arguments
    ///
    /// * `trawl_resources` - Map of resource names to their runtime values
    ///
    /// # Returns
    ///
    /// An iterator of (variable_name, resolved_value) pairs.
    pub fn config_variables(
        &self,
        trawl_resources: &HashMap<String, String>,
    ) -> impl Iterator<Item = (String, String)> {
        self.config.lines().filter_map(|line: &str| {
            let mut args = line.trim().split_whitespace();
            let (command, var_declaration) = (args.next()?, args.next()?);
            let var_name = var_declaration.strip_prefix('$').unwrap_or(var_declaration);

            let var_value = match command {
                "set" => args.next()?.to_string(),
                "set_from_resource" => {
                    let (resource_name, default_value) = (args.next()?, args.next()?);
                    trawl_resources
                        .get(resource_name)
                        .cloned()
                        .unwrap_or_else(|| default_value.to_string())
                }
                _ => return None,
            };

            Some((var_name.to_string(), var_value))
        })
    }

    /// Extracts keybinding definitions from this config.
    ///
    /// Recognizes `bindsym` and `bindcode` directives. Variable references
    /// in bindings (e.g., `$mod+Return`) are resolved using the provided
    /// variables map. Options like `--release` are stripped from the binding.
    ///
    /// # Arguments
    ///
    /// * `variables` - Map of variable names to their resolved values
    ///
    /// # Returns
    ///
    /// An iterator of `BindingDef` instances with both original and
    /// normalized binding strings.
    pub fn config_bindings<'a>(
        &'a self,
        variables: &BTreeMap<String, String>,
    ) -> impl Iterator<Item = search::bindings::BindingDef<'a>> {
        self.config.lines().enumerate().filter_map(|(index, line)| {
            let mut args = line
                .trim()
                .split_whitespace()
                .filter(|arg| !arg.starts_with("--"));

            let (command, binding) = (args.next()?, args.next()?);
            if command != "bindsym" && command != "bindcode" {
                return None;
            }
            let resolved_binding = search::bindings::normalize_binding(binding, variables);

            Some(search::bindings::BindingDef {
                orig_binding: binding,
                normalized_binding: resolved_binding,
                src_config: self,
                line_no: index + 1,
                line_contents: line.to_string(),
            })
        })
    }
}
