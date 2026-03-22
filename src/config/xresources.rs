//! Xresources file parsing.
//!
//! This module provides types for parsing Xresources files, which store
//! X11 resource definitions in `key: value` format with support for
//! `#include` directives.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A single key-value entry from an Xresources file.
#[derive(Debug, Clone)]
pub struct XresourceEntry {
    /// The resource key (left of the colon).
    pub key: String,
    /// The resource value (right of the colon).
    pub value: String,
    /// Path to the file containing this entry.
    pub file_path: PathBuf,
    /// Line number (1-indexed) where this entry was found.
    pub line_number: usize,
}

/// A parsed Xresources configuration.
///
/// Parses Xresources files following the standard format:
/// - Lines starting with `!` are comments
/// - Empty lines are ignored
/// - `#include "path"` directives are followed recursively
/// - `key: value` lines define resource entries
#[derive(Debug)]
pub struct XresourceConfig {
    entries: Vec<XresourceEntry>,
}

impl XresourceConfig {
    /// Parses an Xresources file and all its includes.
    ///
    /// # Arguments
    ///
    /// * `root_path` - Path to the root Xresources file
    ///
    /// # Returns
    ///
    /// A `XresourceConfig` containing all entries from the root file
    /// and all included files. Cyclic includes are handled by skipping
    /// already-visited files.
    ///
    /// # Errors
    ///
    /// Returns an error if the root file cannot be opened or read,
    /// or if any included file cannot be resolved.
    pub fn new<P: AsRef<Path>>(root_path: P) -> Result<Self> {
        let root_path = root_path.as_ref();
        let mut entries = Vec::new();
        let mut seen_paths = BTreeSet::new();
        Self::parse_file_recursive(root_path, &mut entries, &mut seen_paths)?;
        Ok(Self { entries })
    }

    fn parse_file_recursive(
        file_path: &Path,
        entries: &mut Vec<XresourceEntry>,
        seen_paths: &mut BTreeSet<PathBuf>,
    ) -> Result<()> {
        let canonical_path = file_path
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize path: {:?}", file_path))?;

        if seen_paths.contains(&canonical_path) {
            return Ok(());
        }
        seen_paths.insert(canonical_path.clone());

        let mut content = String::new();
        let mut file = File::open(file_path)
            .with_context(|| format!("Failed to open file: {:?}", file_path))?;
        file.read_to_string(&mut content)
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        let file_dir = file_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("File has no parent directory: {:?}", file_path))?;

        for (line_number, line) in content.lines().enumerate() {
            let line_number = line_number + 1;
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('!') {
                continue;
            }

            if trimmed.starts_with("#include") {
                let include_path = trimmed
                    .strip_prefix("#include")
                    .ok_or_else(|| anyhow::anyhow!("Invalid include directive"))?
                    .trim()
                    .trim_matches('"');

                let resolved_path = if include_path.starts_with('/') {
                    PathBuf::from(include_path)
                } else {
                    file_dir.join(include_path)
                };

                Self::parse_file_recursive(&resolved_path, entries, seen_paths)?;
                continue;
            }

            if let Some(colon_pos) = trimmed.find(':') {
                let key = trimmed[..colon_pos].trim().to_string();
                let value = trimmed[colon_pos + 1..].trim().to_string();

                entries.push(XresourceEntry {
                    key,
                    value,
                    file_path: file_path.to_path_buf(),
                    line_number,
                });
            }
        }

        Ok(())
    }

    /// Returns all parsed resource entries.
    pub fn get_all_entries(&self) -> &[XresourceEntry] {
        &self.entries
    }

    /// Finds an entry by exact key match.
    ///
    /// Returns the first entry with a matching key, or `None` if not found.
    /// Key matching is case-sensitive.
    pub fn get_entry(&self, key: &str) -> Option<&XresourceEntry> {
        self.entries.iter().find(|entry| entry.key == key)
    }
}

/// Returns the path to the user's Xresources file.
///
/// The path is `$HOME/.config/regolith3/Xresources`, or
/// `.config/regolith3/Xresources` relative to the current directory
/// if `$HOME` is not set.
pub fn get_user_xresources_path() -> PathBuf {
    match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home)
            .join(".config")
            .join("regolith3")
            .join("Xresources"),
        Err(_) => PathBuf::from(".config")
            .join("regolith3")
            .join("Xresources"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{create_temp_config_dir, create_xresources_file};

    #[test]
    fn new_parses_key_value_pairs_and_skips_invalid_lines() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "! comment\n\nregolithwm.border.width: 2\n\nregolithwm.font.size: 12\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entries = config.get_all_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "regolithwm.border.width");
        assert_eq!(entries[0].value, "2");
        assert_eq!(entries[1].key, "regolithwm.font.size");
        assert_eq!(entries[1].value, "12");
    }

    #[test]
    fn new_follows_include_directives() {
        let dir = create_temp_config_dir();
        create_xresources_file(dir.path(), "extra.xr", "regolithwm.font.size: 14\n");
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "regolithwm.border.width: 2\n#include extra.xr\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entries = config.get_all_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "regolithwm.border.width");
        assert_eq!(entries[1].key, "regolithwm.font.size");
    }

    #[test]
    fn new_follows_nested_and_quoted_includes() {
        let dir = create_temp_config_dir();
        create_xresources_file(dir.path(), "level2.xr", "regolithwm.gamma: 1.0\n");
        create_xresources_file(
            dir.path(),
            "level1.xr",
            "regolithwm.font.size: 12\n#include level2.xr\n",
        );
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "regolithwm.border.width: 2\n#include level1.xr\n#include \"level2.xr\"\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        // level2.xr is included twice but duplicate is skipped
        assert_eq!(config.get_all_entries().len(), 3);
    }

    #[test]
    fn new_returns_error_for_nonexistent_file() {
        let result = XresourceConfig::new("/nonexistent/path/Xresources");
        assert!(result.is_err());
    }

    #[test]
    fn new_records_file_path_and_line_number() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "! comment\nregolithwm.border.width: 2\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entry = &config.get_all_entries()[0];

        assert_eq!(entry.file_path, path);
        assert_eq!(entry.line_number, 2);
    }

    #[test]
    fn get_entry_finds_existing_key() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "regolithwm.border.width: 2\nregolithwm.font.size: 12\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entry = config.get_entry("regolithwm.font.size").unwrap();

        assert_eq!(entry.value, "12");
    }

    #[test]
    fn get_entry_returns_none_for_missing_or_empty_key() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(dir.path(), "Xresources", "regolithwm.border.width: 2\n");

        let config = XresourceConfig::new(&path).unwrap();
        assert!(config.get_entry("nonexistent.key").is_none());
        assert!(config.get_entry("").is_none());
    }

    #[test]
    fn get_entry_is_case_sensitive() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(dir.path(), "Xresources", "regolithwm.border.width: 2\n");

        let config = XresourceConfig::new(&path).unwrap();
        assert!(config.get_entry("Regolithwm.border.width").is_none());
        assert!(config.get_entry("regolithwm.border.width").is_some());
    }
}
