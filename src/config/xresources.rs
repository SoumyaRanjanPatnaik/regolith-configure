use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
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

pub fn set_user_xresource(key: &str, value: &str) -> Result<PathBuf> {
    let xresources_path = get_user_xresources_path();

    if let Some(parent) = xresources_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {:?}", parent))?;
    }

    let content = match File::open(&xresources_path) {
        Ok(mut file) => {
            let mut buf = String::new();
            file.read_to_string(&mut buf)
                .with_context(|| format!("Failed to read file: {:?}", xresources_path))?;
            buf
        }
        Err(_) => String::new(),
    };

    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let mut found = false;
    let key_lower = key.to_lowercase();

    for line in &mut lines {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with("#include") {
            continue;
        }

        if let Some(colon_pos) = trimmed.find(':') {
            let existing_key = trimmed[..colon_pos].trim().to_lowercase();
            if existing_key == key_lower {
                *line = format!("{}: {}", key, value);
                found = true;
                break;
            }
        }
    }

    if !found {
        if let Some(last) = lines.last() {
            if !last.trim().is_empty() {
                lines.push(String::new());
            }
        }
        lines.push(format!("{}: {}", key, value));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&xresources_path)
        .with_context(|| format!("Failed to open file for writing: {:?}", xresources_path))?;

    for (i, line) in lines.iter().enumerate() {
        if i < lines.len() - 1 {
            writeln!(file, "{}", line)
        } else {
            write!(file, "{}", line)
        }
        .with_context(|| format!("Failed to write to file: {:?}", xresources_path))?;
    }

    Ok(xresources_path)
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

    #[test]
    fn set_user_xresource_creates_and_updates() {
        let dir = create_temp_config_dir();
        let nested_path = dir.path().join(".config/regolith3/Xresources");

        unsafe {
            std::env::set_var("HOME", dir.path());
        }

        // Creates new file with a key
        let result = set_user_xresource("test.key", "test_value");
        assert!(result.is_ok());
        assert!(nested_path.exists());

        let content = std::fs::read_to_string(&nested_path).unwrap();
        assert!(content.contains("test.key: test_value"));

        // Appends another key
        let result2 = set_user_xresource("new.key", "new_value");
        assert!(result2.is_ok());

        let content2 = std::fs::read_to_string(&nested_path).unwrap();
        assert!(content2.contains("test.key: test_value"));
        assert!(content2.contains("new.key: new_value"));

        // Updates existing key
        let result3 = set_user_xresource("test.key", "updated_value");
        assert!(result3.is_ok());

        let content3 = std::fs::read_to_string(&nested_path).unwrap();
        assert!(content3.contains("test.key: updated_value"));
        assert!(content3.contains("new.key: new_value"));
    }
}
