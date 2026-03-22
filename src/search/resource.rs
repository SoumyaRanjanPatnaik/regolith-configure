//! X resource search functionality.

use std::{
    cmp::Ordering,
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{
    cli_args::OutputMode,
    config::xresources::{get_user_xresources_path, XresourceConfig},
    output, FullConfig,
};

const MAX_SIMILAR_RESULTS: usize = 5;
const SIMILAR_EDIT_DISTANCE_PERCENT: usize = 15;

/// A line in a config file that uses a resource.
#[derive(Debug)]
pub struct ResourceUsageDef {
    /// Path to the file containing the usage.
    pub file_path: PathBuf,
    /// Line number (1-indexed) of the usage.
    pub line_number: usize,
    /// The full line contents.
    pub line_contents: String,
}

/// A user override for a resource in their Xresources file.
#[derive(Debug)]
pub struct ResourceOverrideDef {
    /// Path to the Xresources file.
    pub file_path: PathBuf,
    /// Line number (1-indexed) of the override.
    pub line_number: usize,
    /// The override value.
    pub value: String,
}

/// Result of a resource search.
#[derive(Debug)]
pub struct ResourceSearchResult {
    /// The queried resource name.
    pub resource_name: String,
    /// Whether an exact match was found.
    pub has_exact_match: bool,
    /// The current runtime value, if available.
    pub runtime_value: Option<String>,
    /// The default value from `set_from_resource`, if defined.
    pub default_value: Option<String>,
    /// All config lines that use this resource.
    pub usages: Vec<ResourceUsageDef>,
    /// User overrides in the Xresources file.
    pub overrides: Vec<ResourceOverrideDef>,
    /// Resources where the query appears as a substring (case-insensitive).
    /// These are direct/partial matches based on the search term.
    pub matched_resources: Vec<String>,
    /// Resources with similar names using fuzzy matching (Levenshtein distance).
    /// Only included if edit distance ≤ 15% of max(query, candidate) length.
    /// Intended for typo suggestions when no exact match is found.
    pub similar_resources: Vec<String>,
}

#[derive(Debug)]
struct SetFromResourceDef {
    resource_name: String,
    variable_name: String,
    default_value: String,
}

fn parse_set_from_resource(line: &str) -> Option<SetFromResourceDef> {
    let mut args = line.split_whitespace();
    let command = args.next()?;
    if command != "set_from_resource" {
        return None;
    }

    let variable_name = args.next()?.to_string();
    let resource_name = args.next()?.to_string();
    let default_value = args.collect::<Vec<_>>().join(" ");
    if default_value.is_empty() {
        return None;
    }
    Some(SetFromResourceDef {
        resource_name,
        variable_name,
        default_value,
    })
}

fn resource_candidates(config: &FullConfig, xresources_path: &Path) -> BTreeSet<String> {
    let mut candidates = BTreeSet::new();

    for partial in &config.partials {
        for line in partial.config.lines() {
            if let Some(def) = parse_set_from_resource(line.trim()) {
                candidates.insert(def.resource_name);
            }
        }
    }

    if let Ok(xconfig) = XresourceConfig::load(xresources_path) {
        for entry in xconfig.entries() {
            candidates.insert(entry.key.clone());
        }
    }

    candidates
}

fn collect_similar_resources(query: &str, candidates: &BTreeSet<String>) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let mut scored = candidates
        .iter()
        .filter(|candidate| candidate.to_lowercase() != query_lower)
        .filter_map(|candidate| {
            let candidate_lower = candidate.to_lowercase();
            let is_substring = candidate_lower.contains(&query_lower);
            let distance = strsim::levenshtein(&query_lower, &candidate_lower);
            let max_len = query.len().max(candidate.len());
            let threshold = max_len * SIMILAR_EDIT_DISTANCE_PERCENT / 100;
            if !is_substring && distance > threshold {
                return None;
            }

            let len_diff = candidate.len().abs_diff(query.len());
            Some((candidate.clone(), is_substring, len_diff, distance))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| match b.1.cmp(&a.1) {
        Ordering::Equal => a.2.cmp(&b.2).then(a.3.cmp(&b.3)).then(a.0.cmp(&b.0)),
        order => order,
    });

    scored
        .into_iter()
        .take(MAX_SIMILAR_RESULTS)
        .map(|(candidate, _, _, _)| candidate)
        .collect()
}

/// Searches for information about an X resource.
///
/// Returns comprehensive information about a resource including:
/// - Its runtime value (if available from the provider)
/// - Its default value (from `set_from_resource` directives)
/// - All config lines that use it
/// - User overrides in the Xresources file
/// - Similar resources (for fuzzy matching/typos)
///
/// # Arguments
///
/// * `resource` - The resource name to search for
/// * `config` - The configuration to search within
/// * `provider` - Resource provider for runtime values
///
/// # Returns
///
/// A `ResourceSearchResult` with all available information about the resource.
pub fn search_resources(
    resource: &str,
    config: &FullConfig,
    provider: &dyn crate::resources::ResourceProvider,
) -> ResourceSearchResult {
    let query_lower = resource.to_lowercase();
    let all_runtime_resources = provider.query_resources().unwrap_or_default();
    let xresources_path = get_user_xresources_path();

    let mut candidates = resource_candidates(config, &xresources_path);
    for runtime_resource in all_runtime_resources.keys() {
        candidates.insert(runtime_resource.clone());
    }

    let matched_resources: Vec<String> = candidates
        .iter()
        .filter(|candidate| candidate.to_lowercase().contains(&query_lower))
        .cloned()
        .collect();

    let has_exact_match = candidates
        .iter()
        .any(|candidate| candidate.to_lowercase() == query_lower);
    let runtime_value = all_runtime_resources
        .iter()
        .find(|(name, _)| name.to_lowercase() == query_lower)
        .map(|(_, value)| value.clone());

    let similar_resources = collect_similar_resources(resource, &candidates);

    let mut default_value = None;

    let all_set_from_resources: Vec<SetFromResourceDef> = config
        .partials
        .iter()
        .flat_map(|partial| partial.config.lines())
        .filter_map(|line| parse_set_from_resource(line.trim()))
        .collect();

    for def in &all_set_from_resources {
        if def.resource_name == resource
            || (has_exact_match && def.resource_name.eq_ignore_ascii_case(resource))
        {
            default_value = Some(def.default_value.clone());
            break;
        }
    }

    let matched_variables: BTreeSet<String> = if has_exact_match {
        all_set_from_resources
            .iter()
            .filter(|def| def.resource_name == resource)
            .map(|def| def.variable_name.clone())
            .collect()
    } else {
        all_set_from_resources
            .iter()
            .filter(|def| {
                def.resource_name.to_lowercase().contains(&query_lower)
                    || matched_resources
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(&def.resource_name))
            })
            .map(|def| def.variable_name.clone())
            .collect()
    };

    let usages: Vec<_> = config
        .partials
        .iter()
        .flat_map(|partial| {
            partial
                .config
                .lines()
                .enumerate()
                .filter_map(|(index, line)| {
                    let trimmed = line.trim();
                    let lower_line = trimmed.to_lowercase();

                    let is_match = if has_exact_match {
                        let has_exact_resource = lower_line.contains(&query_lower);
                        let has_variable = matched_variables
                            .iter()
                            .any(|variable| lower_line.contains(variable));
                        has_exact_resource || has_variable
                    } else {
                        let has_resource_match = matched_resources
                            .iter()
                            .any(|name| lower_line.contains(&name.to_lowercase()));
                        let has_variable_match = matched_variables
                            .iter()
                            .any(|variable| lower_line.contains(&variable.to_lowercase()));
                        let has_query_match = lower_line.contains(&query_lower);
                        has_resource_match || has_variable_match || has_query_match
                    };

                    if is_match {
                        return Some(ResourceUsageDef {
                            file_path: partial.file_name.clone(),
                            line_number: index + 1,
                            line_contents: line.to_string(),
                        });
                    }
                    None
                })
        })
        .collect();

    let overrides: Vec<_> = XresourceConfig::load(&xresources_path)
        .ok()
        .map(|xconfig| {
            xconfig
                .entries()
                .iter()
                .filter(|entry| {
                    if has_exact_match {
                        entry.key.eq_ignore_ascii_case(resource)
                    } else {
                        let key = entry.key.to_lowercase();
                        key.contains(&query_lower)
                            || matched_resources
                                .iter()
                                .any(|name| name.eq_ignore_ascii_case(&entry.key))
                    }
                })
                .map(|entry| ResourceOverrideDef {
                    file_path: entry.file_path.clone(),
                    line_number: entry.line_number,
                    value: entry.value.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    ResourceSearchResult {
        resource_name: resource.to_string(),
        has_exact_match,
        runtime_value,
        default_value,
        usages,
        overrides,
        matched_resources,
        similar_resources,
    }
}

impl ResourceSearchResult {
    pub fn format(&self, mode: OutputMode) -> String {
        match mode {
            OutputMode::Minimal => self.format_minimal(),
            OutputMode::Summary => self.format_summary(),
            OutputMode::Full => self.format_full(),
        }
    }

    fn format_minimal(&self) -> String {
        let mut output = String::new();

        if !self.has_exact_match {
            output.push_str(&format!(
                "No exact match found for '{}'. Try --summary or --full to see related resources.\n",
                self.resource_name
            ));
            return output;
        }

        if let Some(v) = &self.runtime_value {
            output.push_str(&format!("Runtime Value: {}\n", output::value_found(v)));
        }

        if let Some(v) = &self.default_value {
            let default_str = if self.runtime_value.is_none() {
                format!(
                    "{} {}",
                    output::default_value(v),
                    output::in_use("(In use)")
                )
            } else {
                output::default_value(v).to_string()
            };
            output.push_str(&format!("Default Value: {}\n", default_str));
        }

        if let Some(o) = self.overrides.first() {
            output.push_str(&format!(
                "Custom Override: {}\n",
                output::override_value(&o.value)
            ));
        }

        output
    }

    fn format_summary(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "Resource Query: {}\n",
            output::resource_name(&self.resource_name)
        ));

        output.push('\n');
        let runtime_display = match &self.runtime_value {
            Some(v) => output::value_found(v),
            None => output::value_not_found("Not found"),
        };
        output.push_str(&format!("Runtime Value: {}\n", runtime_display));

        let default_val = match &self.default_value {
            Some(v) if self.runtime_value.is_none() => {
                format!(
                    "{} {}",
                    output::default_value(v),
                    output::in_use("(In use)")
                )
            }
            Some(v) => output::default_value(v).to_string(),
            None => output::value_not_found("Not found").to_string(),
        };
        output.push_str(&format!("Default Value: {}\n", default_val));

        let has_matched = !self.matched_resources.is_empty();
        let has_similar = !self.similar_resources.is_empty();
        if has_matched || has_similar {
            output.push('\n');
            output.push_str(&format!(
                "{}\n",
                output::section_header("Related Resources:")
            ));
            if has_matched {
                output.push_str("  Matched (substring):\n");
                for matched in &self.matched_resources {
                    output.push_str(&format!("    - {}\n", output::resource_name(matched)));
                }
            }
            if has_similar {
                output.push_str("  Similar (fuzzy/typo):\n");
                for resource in &self.similar_resources {
                    output.push_str(&format!("    - {}\n", output::similar_item(resource)));
                }
            }
        }

        output
    }

    fn format_full(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "Resource Query: {}\n",
            output::resource_name(&self.resource_name)
        ));

        output.push('\n');
        let runtime_display = match &self.runtime_value {
            Some(v) => output::value_found(v),
            None => output::value_not_found("Not found"),
        };
        output.push_str(&format!("Runtime Value: {}\n", runtime_display));

        let default_val = match &self.default_value {
            Some(v) if self.runtime_value.is_none() => {
                format!(
                    "{} {}",
                    output::default_value(v),
                    output::in_use("(In use)")
                )
            }
            Some(v) => output::default_value(v).to_string(),
            None => output::value_not_found("Not found").to_string(),
        };
        output.push_str(&format!("Default Value: {}\n", default_val));

        let override_val = self
            .overrides
            .first()
            .map(|o| output::override_value(&o.value).to_string())
            .unwrap_or_else(|| output::value_not_found("Not found").to_string());
        output.push_str(&format!("Custom Override: {}\n", override_val));

        let has_matched = !self.matched_resources.is_empty();
        let has_similar = !self.similar_resources.is_empty();
        if has_matched || has_similar {
            output.push('\n');
            output.push_str(&format!(
                "{}\n",
                output::section_header("Related Resources:")
            ));
            if has_matched {
                output.push_str("  Matched (substring):\n");
                for matched in &self.matched_resources {
                    output.push_str(&format!("    - {}\n", output::resource_name(matched)));
                }
            }
            if has_similar {
                output.push_str("  Similar (fuzzy/typo):\n");
                for resource in &self.similar_resources {
                    output.push_str(&format!("    - {}\n", output::similar_item(resource)));
                }
            }
        }

        output.push('\n');
        output.push_str(&format!(
            "{}\n",
            output::section_header("Related Configuration Lines:")
        ));
        for usage in &self.usages {
            output.push_str(&format!(
                "{} - Line {}\n",
                output::file_path(&usage.file_path.to_string_lossy()),
                output::line_number(usage.line_number)
            ));
            output.push_str(&format!("    {}\n", usage.line_contents));
        }

        if self.has_exact_match {
            output.push('\n');
            output.push_str(&format!(
                "{}\n",
                output::section_header("To override this resource, run the following command:")
            ));
            output.push_str(&format!(
                "{}\n",
                output::command(&format!(
                    "regolith-configure set-resource {} \"<custom_value>\"",
                    self.resource_name
                ))
            ));
        }

        output
    }
}
