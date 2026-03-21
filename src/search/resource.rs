use std::{
    cmp::Ordering,
    collections::BTreeSet,
    fmt::Display,
    path::{Path, PathBuf},
};

use crate::{config::xresources::XresourceConfig, FullConfig};

const MAX_SIMILAR_EDIT_SCORE: usize = 10;
const MAX_SIMILAR_RESULTS: usize = 5;

#[derive(Debug)]
pub struct ResourceUsageDef {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub line_contents: String,
}

#[derive(Debug)]
pub struct ResourceOverrideDef {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub value: String,
}

#[derive(Debug)]
pub struct ResourceSearchResult {
    pub resource_name: String,
    pub has_exact_match: bool,
    pub runtime_value: Option<String>,
    pub default_value: Option<String>,
    pub usages: Vec<ResourceUsageDef>,
    pub overrides: Vec<ResourceOverrideDef>,
    pub matched_resources: Vec<String>,
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

    if let Ok(xconfig) = XresourceConfig::new(xresources_path) {
        for entry in xconfig.get_all_entries() {
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
            if !is_substring && distance > MAX_SIMILAR_EDIT_SCORE {
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

pub fn search_resource_result(
    resource: &str,
    config: &FullConfig,
    provider: &dyn crate::resources::ResourceProvider,
) -> ResourceSearchResult {
    let query_lower = resource.to_lowercase();
    let all_runtime_resources = provider.get_all_resources().unwrap_or_default();
    let xresources_path = std::env::var("HOME")
        .ok()
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("regolith3")
                .join("Xresources")
        })
        .unwrap_or_else(|| {
            PathBuf::from(".config")
                .join("regolith3")
                .join("Xresources")
        });

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

    let overrides: Vec<_> = XresourceConfig::new(&xresources_path)
        .ok()
        .map(|xconfig| {
            xconfig
                .get_all_entries()
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

impl Display for ResourceSearchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Resource Query: {}", self.resource_name)?;

        if !self.matched_resources.is_empty() {
            writeln!(f)?;
            writeln!(f, "Matched Resources:")?;
            for matched in &self.matched_resources {
                writeln!(f, "  - {}", matched)?;
            }
        }

        writeln!(f)?;
        writeln!(
            f,
            "Runtime Value: {}",
            self.runtime_value.as_deref().unwrap_or("Not found")
        )?;

        let default_val = match &self.default_value {
            Some(v) if self.runtime_value.is_none() => format!("{} (In use)", v),
            Some(v) => v.to_string(),
            None => "Not found".to_string(),
        };
        writeln!(f, "Default Value: {}", default_val)?;

        let override_val = self
            .overrides
            .first()
            .map(|o| o.value.clone())
            .unwrap_or_else(|| "Not found".to_string());
        writeln!(f, "Custom Override: {}", override_val)?;

        writeln!(f)?;
        writeln!(f, "Related Configuration Lines:")?;
        for usage in &self.usages {
            writeln!(
                f,
                "{} - Line {}",
                usage.file_path.to_string_lossy(),
                usage.line_number
            )?;
            writeln!(f, "    {}", usage.line_contents)?;
        }

        if self.has_exact_match {
            writeln!(f)?;
            writeln!(
                f,
                "To override this resource, add the following line to your ~/.config/regolith3/Xresources file:"
            )?;
            writeln!(f, "{}: <custom_value>", self.resource_name)?;
        }

        if !self.similar_resources.is_empty() {
            writeln!(f)?;
            writeln!(f, "Similar Resources:")?;
            for resource in &self.similar_resources {
                writeln!(f, "  - {}", resource)?;
            }
        }

        Ok(())
    }
}
