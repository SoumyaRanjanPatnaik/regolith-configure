use std::{borrow::Cow, collections::BTreeMap, collections::HashMap, fmt::Display};

use crate::{ConfigPartial, FullConfig};

#[derive(Debug)]
pub struct BindingDef<'a> {
    #[allow(dead_code)]
    pub orig_binding: &'a str,
    pub normalized_binding: Cow<'a, str>,
    pub src_config: &'a ConfigPartial,
    pub line_no: usize,
    pub line_contents: String,
}

impl<'a> Display for BindingDef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - Line {}:\n\t{}",
            self.src_config.file_name.to_string_lossy(),
            self.line_no,
            self.line_contents
        )
    }
}

#[derive(Debug)]
pub struct BindingsSearchResult<'a>(pub Vec<BindingDef<'a>>);

impl<'a> Display for BindingsSearchResult<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bindings_string = self
            .0
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{}", bindings_string)
    }
}

impl<'a> From<Vec<BindingDef<'a>>> for BindingsSearchResult<'a> {
    fn from(value: Vec<BindingDef<'a>>) -> Self {
        Self(value)
    }
}

pub fn normalize_binding<'a>(
    binding: &'a str,
    variables: &BTreeMap<String, String>,
) -> Cow<'a, str> {
    let mut normalized_binding = Cow::Borrowed(binding.trim());

    while normalized_binding.contains('$') {
        let updated_binding = normalized_binding
            .split('+')
            .map(|key| key.trim())
            .map(|key| {
                if !key.starts_with("$") || key.len() < 2 {
                    return key;
                }

                let var_name = &key[1..];

                variables
                    .get(var_name)
                    .map(|var_value| var_value.as_str())
                    .unwrap_or(key)
            })
            .collect::<Vec<_>>()
            .join("+");

        if updated_binding == normalized_binding {
            break;
        }

        normalized_binding = Cow::Owned(updated_binding);
    }
    normalized_binding
}

pub fn search_binding_result<'a>(
    binding: &str,
    config: &'a FullConfig,
    trawl_resources: &HashMap<String, String>,
) -> BindingsSearchResult<'a> {
    let variables = config.get_all_variables(trawl_resources);
    let matching_bindings: Vec<_> = config
        .get_all_bindings(&variables)
        .0
        .into_iter()
        .filter_map(|binding_def| {
            let does_normalized_binding_match = binding_def
                .normalized_binding
                .to_lowercase()
                .split('+')
                .zip(binding.to_lowercase().split('+'))
                .all(|(a, b)| a == b);

            let does_raw_binding_match = binding_def
                .orig_binding
                .to_lowercase()
                .contains(&binding.to_lowercase());

            if does_normalized_binding_match || does_raw_binding_match {
                Some(binding_def)
            } else {
                None
            }
        })
        .collect();

    BindingsSearchResult::from(matching_bindings)
}
