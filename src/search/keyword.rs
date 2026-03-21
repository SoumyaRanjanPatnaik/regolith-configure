use std::{fmt::Display, path::PathBuf};

use crate::FullConfig;

#[derive(Debug)]
pub struct KeywordDef {
    pub file_path: PathBuf,
    pub line_number: usize,
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
