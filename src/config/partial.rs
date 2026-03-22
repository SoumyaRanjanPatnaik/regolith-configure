use anyhow::{anyhow, Context, Result};
use glob::glob;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use crate::search;

#[derive(Debug)]
pub struct ConfigPartial {
    pub file_name: PathBuf,
    pub config: String,
}

impl ConfigPartial {
    pub fn new(file_name: &Path, config: &str) -> Self {
        Self {
            file_name: file_name.to_path_buf(),
            config: config.to_string(),
        }
    }

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
