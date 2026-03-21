use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct XresourceEntry {
    pub key: String,
    pub value: String,
    pub file_path: PathBuf,
    pub line_number: usize,
}

#[derive(Debug)]
pub struct XresourceConfig {
    entries: Vec<XresourceEntry>,
}

impl XresourceConfig {
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

    pub fn get_all_entries(&self) -> &[XresourceEntry] {
        &self.entries
    }

    pub fn get_entry(&self, key: &str) -> Option<&XresourceEntry> {
        self.entries.iter().find(|entry| entry.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{create_temp_config_dir, create_xresources_file};

    // --- XresourceConfig::new ---

    #[test]
    fn new_parses_simple_key_value_pairs() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "regolithwm.border.width: 2\nregolithwm.font.size: 12\n",
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
    fn new_skips_comment_lines() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "! This is a comment\nregolithwm.border.width: 2\n! Another comment\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entries = config.get_all_entries();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "regolithwm.border.width");
    }

    #[test]
    fn new_skips_empty_lines() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "\nregolithwm.border.width: 2\n\nregolithwm.font.size: 12\n\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entries = config.get_all_entries();

        assert_eq!(entries.len(), 2);
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
    fn new_follows_nested_includes() {
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
            "regolithwm.border.width: 2\n#include level1.xr\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entries = config.get_all_entries();

        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn new_returns_error_for_nonexistent_file() {
        let result = XresourceConfig::new("/nonexistent/path/Xresources");
        assert!(result.is_err());
    }

    #[test]
    fn new_handles_include_with_quoted_path() {
        let dir = create_temp_config_dir();
        create_xresources_file(dir.path(), "extra.xr", "regolithwm.gamma: 1.0\n");
        let path = create_xresources_file(dir.path(), "Xresources", "#include \"extra.xr\"\n");

        let config = XresourceConfig::new(&path).unwrap();
        assert_eq!(config.get_all_entries().len(), 1);
        assert_eq!(config.get_all_entries()[0].key, "regolithwm.gamma");
    }

    #[test]
    fn new_skips_duplicate_includes() {
        let dir = create_temp_config_dir();
        create_xresources_file(dir.path(), "extra.xr", "regolithwm.gamma: 1.0\n");
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "#include extra.xr\n#include extra.xr\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        assert_eq!(config.get_all_entries().len(), 1);
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

    // --- XresourceConfig::get_all_entries ---

    #[test]
    fn get_all_entries_returns_empty_for_empty_file() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(dir.path(), "Xresources", "");

        let config = XresourceConfig::new(&path).unwrap();
        assert!(config.get_all_entries().is_empty());
    }

    #[test]
    fn get_all_entries_returns_entries_from_all_includes() {
        let dir = create_temp_config_dir();
        create_xresources_file(dir.path(), "a.xr", "key.a: val_a\n");
        create_xresources_file(dir.path(), "b.xr", "key.b: val_b\n");
        let path = create_xresources_file(
            dir.path(),
            "Xresources",
            "key.root: val_root\n#include a.xr\n#include b.xr\n",
        );

        let config = XresourceConfig::new(&path).unwrap();
        let entries = config.get_all_entries();

        assert_eq!(entries.len(), 3);
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"key.root"));
        assert!(keys.contains(&"key.a"));
        assert!(keys.contains(&"key.b"));
    }

    #[test]
    fn get_all_entries_correct_count() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(dir.path(), "Xresources", "k1: v1\nk2: v2\nk3: v3\n");

        let config = XresourceConfig::new(&path).unwrap();
        assert_eq!(config.get_all_entries().len(), 3);
    }

    // --- XresourceConfig::get_entry ---

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
    fn get_entry_returns_none_for_missing_key() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(dir.path(), "Xresources", "regolithwm.border.width: 2\n");

        let config = XresourceConfig::new(&path).unwrap();
        assert!(config.get_entry("nonexistent.key").is_none());
    }

    #[test]
    fn get_entry_returns_none_for_empty_key() {
        let dir = create_temp_config_dir();
        let path = create_xresources_file(dir.path(), "Xresources", "regolithwm.border.width: 2\n");

        let config = XresourceConfig::new(&path).unwrap();
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
