//! Full configuration loading and aggregation.
//!
//! This module provides [`FullConfig`] for loading a complete window manager
//! configuration by recursively discovering all included config files.

use anyhow::Result;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, LinkedList},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use crate::cli_args::Session;
use crate::search;

use super::partial::ConfigPartial;

/// Mapping from session types to their root config paths.
///
/// Each tuple maps a [`Session`] variant to the path of its root
/// configuration file (e.g., `/etc/regolith/i3/config` for X11).
pub type SessionMappings = [(Session, &'static Path)];

/// A complete window manager configuration.
///
/// Contains all config partials discovered by recursively following
/// `include` directives from the root config file.
#[derive(Debug)]
pub struct FullConfig {
    _config_root: PathBuf,
    /// All discovered config partials, including the root config.
    pub partials: Vec<ConfigPartial>,
}

impl FullConfig {
    /// Loads a complete configuration for the given session.
    ///
    /// # Arguments
    ///
    /// * `session` - The session type (X11 or Wayland)
    /// * `session_mappings` - Mapping from session types to root config paths
    ///
    /// # Returns
    ///
    /// A `FullConfig` containing the root config and all included configs.
    /// Includes are discovered recursively; cyclic includes are skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if the root config file cannot be opened or read.
    pub fn load_for_session<'a>(
        session: Session,
        session_mappings: &'a SessionMappings,
    ) -> Result<Self> {
        let root_config_path = session_mappings
            .iter()
            .find_map(|&(sess, ref path)| {
                if sess == session {
                    Some(Path::new(path))
                } else {
                    None
                }
            })
            .expect("Invalid session type provided");

        let root_config = {
            let mut config_str = String::new();
            let mut root_config_file_handle = File::open(&root_config_path)?;
            root_config_file_handle.read_to_string(&mut config_str)?;
            ConfigPartial::new(&root_config_path, &config_str)
        };

        Ok(Self {
            _config_root: root_config_path.to_path_buf(),
            partials: Self::resolve_includes(root_config)?,
        })
    }

    fn resolve_includes(root_config: ConfigPartial) -> Result<Vec<ConfigPartial>> {
        let mut discovered_config_partials = Vec::new();

        let mut bfs_queue = LinkedList::from([root_config]);
        let mut seen_paths = BTreeSet::new();
        while bfs_queue.len() > 0 {
            let Some(current_partial) = bfs_queue.pop_front() else {
                break;
            };

            for import_path in current_partial.resolve_imports()? {
                if seen_paths.contains(&import_path) {
                    continue;
                }

                seen_paths.insert(import_path.clone());

                let mut import_config = String::new();
                let mut import_file_handle = File::open(&import_path)?;
                import_file_handle.read_to_string(&mut import_config)?;

                let import_partial = ConfigPartial::new(&import_path, &import_config);
                bfs_queue.push_back(import_partial);
            }

            discovered_config_partials.push(current_partial);
        }

        Ok(discovered_config_partials)
    }

    /// Resolves all variables defined across all config partials.
    ///
    /// Variables are defined via `set` or `set_from_resource` directives.
    /// For `set_from_resource`, the value is resolved from the provided
    /// resources map, falling back to the default if not found.
    ///
    /// # Arguments
    ///
    /// * `trawl_resources` - Map of resource names to their runtime values
    ///
    /// # Returns
    ///
    /// A `BTreeMap` of variable names to their resolved values.
    pub fn resolve_variables(
        &self,
        trawl_resources: &HashMap<String, String>,
    ) -> BTreeMap<String, String> {
        self.partials
            .iter()
            .flat_map(|partial| partial.extract_variables(trawl_resources))
            .collect()
    }

    /// Resolves all keybindings defined across all config partials.
    ///
    /// Bindings are defined via `bindsym` or `bindcode` directives.
    /// Variable references in bindings are resolved using the provided
    /// variables map.
    ///
    /// # Arguments
    ///
    /// * `variables` - Map of variable names to their resolved values
    ///
    /// # Returns
    ///
    /// A `BindingsSearchResult` containing all discovered bindings.
    pub fn resolve_bindings(
        &'_ self,
        variables: &BTreeMap<String, String>,
    ) -> search::bindings::BindingsSearchResult<'_> {
        let bindings: Vec<_> = self
            .partials
            .iter()
            .flat_map(|partial| partial.extract_bindings(variables))
            .collect();

        search::bindings::BindingsSearchResult::from(bindings)
    }
}
