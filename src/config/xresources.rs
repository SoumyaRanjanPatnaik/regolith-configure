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
