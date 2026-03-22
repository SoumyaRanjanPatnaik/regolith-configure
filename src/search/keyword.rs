//! Keyword search functionality.

use std::{fmt::Display, path::PathBuf};

use crate::FullConfig;

/// A single line matching a keyword search.
#[derive(Debug)]
pub struct KeywordDef {
    /// Path to the file containing the match.
    pub file_path: PathBuf,
    /// Line number (1-indexed) of the match.
    pub line_number: usize,
    /// The full line contents.
    pub line_contents: String,
}

impl Display for KeywordDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - Line {}:\n\t{}",
            self.file_path.to_string_lossy(),
            self.line_number,
            self.line_contents
        )
    }
}

/// Result of a keyword search.
#[derive(Debug)]
pub struct KeywordSearchResult(pub Vec<KeywordDef>);

impl Display for KeywordSearchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keywords_string = self
            .0
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{}", keywords_string)
    }
}

/// Searches for lines containing the given keyword.
///
/// Matching is case-insensitive substring matching. All lines in all
/// config partials are searched.
///
/// # Arguments
///
/// * `keyword` - The keyword to search for
/// * `config` - The configuration to search within
///
/// # Returns
///
/// A `KeywordSearchResult` containing all matching lines.
pub fn search_keyword_result(keyword: &str, config: &FullConfig) -> KeywordSearchResult {
    let keyword_lower = keyword.to_lowercase();
    let results: Vec<_> = config
        .partials
        .iter()
        .flat_map(|partial| {
            partial
                .config
                .lines()
                .enumerate()
                .filter_map(|(index, line)| {
                    if line.to_lowercase().contains(&keyword_lower) {
                        Some(KeywordDef {
                            file_path: partial.file_name.clone(),
                            line_number: index + 1,
                            line_contents: line.to_string(),
                        })
                    } else {
                        None
                    }
                })
        })
        .collect();
    KeywordSearchResult(results)
}
