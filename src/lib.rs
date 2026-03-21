pub mod cli_args;
pub mod config;
pub mod resources;
pub mod search;
#[cfg(test)]
pub mod test_utils;
use anyhow::{anyhow, Context, Result};
use cli_args::{EjectArgs, Session};
use glob::glob;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, LinkedList},
    env,
    fmt::Display,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// Represents a config file that is imported when loading
/// the main configration
#[derive(Debug)]
pub struct ConfigPartial {
    /// Location of the config file
    file_name: PathBuf,
    /// Contents of the config file
    config: String,
}

impl ConfigPartial {
    /// Create a new ConfigPartial
    pub fn new(file_name: &Path, config: &str) -> Self {
        Self {
            file_name: file_name.to_path_buf(),
            config: config.to_string(),
        }
    }

    /// Get all imported config file paths from this config partial
    pub fn get_imported_paths(&self) -> Result<Vec<PathBuf>> {
        let mut imports = Vec::new();
        for line in self.config.lines() {
            // Only consider lines that start with "include"
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

            // Obtain the absolute path for relatively imported paths
            if !import_path.starts_with('/') {
                import_path = self
                    .file_name
                    .parent()
                    .ok_or_else(|| anyhow!("Config file has no parent directory"))?
                    .join(import_path)
                    .to_string_lossy()
                    .to_string();
            }

            // Expand glob patterns in import paths
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

    /// Parse configuration variables from this config partial
    /// * `trawl_resources`: Resource values set in trawlcat that represent the variables
    ///
    /// Returns an iterator over the parsed configuration variables
    pub fn config_variables(
        &self,
        trawl_resources: &HashMap<String, String>,
    ) -> impl Iterator<Item = (String, String)> {
        self.config
            .lines()
            // For each line, attempt to parse variable definitions
            // Supported commands are:
            //   arg0               arg1        arg2            arg3
            //   ---                ----        ---             ---
            // - set                <var_name>  <var_value>
            // - set_from_resource  <var_name>  <resource_name> <default_resource_value>
            .filter_map(|line: &str| {
                let mut args = line.trim().split_whitespace();
                let (command, var_declaration) = (args.next()?, args.next()?);
                let var_name = var_declaration.strip_prefix('$').unwrap_or(var_declaration);

                let var_value = match command {
                    "set" => {
                        // arg2 corresponds to <var_value>
                        args.next()?.to_string()
                    }
                    "set_from_resource" => {
                        // <resource_name> and <default_resource_value> are arg2 and arg3 respectively
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

    /// Parse keybinding definitions from this config partial
    /// * `variables`: Configuration variables for resolving bindings
    ///
    /// Returns an iterator over the parsed keybinding definitions
    pub fn config_bindings<'a>(
        &'a self,
        variables: &BTreeMap<String, String>,
    ) -> impl Iterator<Item = search::bindings::BindingDef<'a>> {
        // Enumerate lines with 1-indexed line numbers
        self.config.lines().enumerate().filter_map(|(index, line)| {
            let mut args = line
                .trim()
                .split_whitespace()
                // Skip options starting with '--'
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

type SessionMappings = [(Session, &'static Path)];
/// Represents the full Regolith configuration, including all imported partials
#[derive(Debug)]
pub struct FullConfig {
    /// Location of the root config file
    _config_root: PathBuf,
    /// All imported config partials
    partials: Vec<ConfigPartial>,
}

impl FullConfig {
    /// Create a new FullConfig from the given session type
    ///
    /// * `session`: The session type (X11 or Wayland)
    /// * `regolith_base_config_dir`: Optional base config directory for Regolith
    ///
    /// Panics if an invalid session type is provided
    pub fn new_from_session<'a>(
        session: Session,
        session_mappings: &'a SessionMappings,
    ) -> Result<Self> {
        // Determine the config root based on the session type
        let root_config_path = session_mappings
            .iter()
            .find_map(|&(sess, ref path)| {
                if sess == session {
                    Some(Path::new(path))
                } else {
                    None
                }
            })
            .expect("Invalid session type provided");

        // Build config partial for root config
        let root_config = {
            let mut config_str = String::new();
            let mut root_config_file_handle = File::open(&root_config_path)?;
            root_config_file_handle.read_to_string(&mut config_str)?;
            ConfigPartial::new(&root_config_path, &config_str)
        };

        Ok(Self {
            _config_root: root_config_path.to_path_buf(),
            partials: Self::discover_config_partials(root_config)?,
        })
    }

    /// Walk the config import tree and discover all config partials
    /// * `root_config`: The root config partial to start the discovery from
    ///
    /// Returns a result containing the vector of all discovered config partials
    fn discover_config_partials(root_config: ConfigPartial) -> Result<Vec<ConfigPartial>> {
        let mut dicovered_config_partials = Vec::new();

        let mut bfs_queue = LinkedList::from([root_config]);
        let mut seen_paths = BTreeSet::new();
        while bfs_queue.len() > 0 {
            let Some(current_partial) = bfs_queue.pop_front() else {
                break;
            };

            for import_path in current_partial.get_imported_paths()? {
                if seen_paths.contains(&import_path) {
                    continue;
                }

                seen_paths.insert(import_path.clone());

                let mut import_config = String::new();
                let mut import_file_handle = File::open(&import_path)?;
                import_file_handle.read_to_string(&mut import_config)?;

                let import_partial = ConfigPartial::new(&import_path, &import_config);
                bfs_queue.push_back(import_partial);
            }

            dicovered_config_partials.push(current_partial);
        }

        Ok(dicovered_config_partials)
    }

    /// Parse configuration variables from all config partials
    /// * `trawl_resources`: Resource values set in trawlcat that represent the variables
    ///
    /// Returns a BTreeMap of all parsed configuration variables
    fn get_all_variables(
        &self,
        trawl_resources: &HashMap<String, String>,
    ) -> BTreeMap<String, String> {
        self.partials
            .iter()
            .flat_map(|partial| partial.config_variables(trawl_resources))
            .collect()
    }

    /// Parse keybinding definitions from all config partials
    fn get_all_bindings(
        &'_ self,
        variables: &BTreeMap<String, String>,
    ) -> search::bindings::BindingsSearchResult<'_> {
        let bindings: Vec<_> = self
            .partials
            .iter()
            .flat_map(|partial| partial.config_bindings(variables))
            .collect();

        search::bindings::BindingsSearchResult::from(bindings)
    }
}

/// Eject the system config and copy it to the user's local config directory
pub fn eject_config(_args: &EjectArgs, _session: Session) -> Box<dyn Display> {
    todo!()
}

pub fn get_session_type() -> Option<Session> {
    return env::vars().find_map(|(name, value)| {
        return match name.as_str() {
            "XDG_SESSION_TYPE" => match value.as_str() {
                "wayland" => Some(Session::Wayland),
                "x11" => Some(Session::X11),
                _ => None,
            },
            _ => None,
        };
    });
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, collections::HashMap, io::Write, sync::OnceLock};

    use super::*;
    use test_utils::{
        create_config_file, create_config_partial, create_config_partial_in_dir,
        create_mock_resources, create_mock_variables, create_temp_config_dir,
        create_test_full_config, MockResourceProvider, TestFixture, SAMPLE_BINDINGS_CONFIG,
        SAMPLE_MORE_BINDINGS_CONFIG, SAMPLE_ROOT_CONFIG,
    };

    // ========================================================================
    // Tests for ConfigPartial::new
    // ========================================================================

    #[test]
    fn test_config_partial_new_with_valid_path_and_content() {
        let partial = create_config_partial("/some/path/config.conf", "set $mod Super");
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_config_partial_new_with_empty_content() {
        let partial = create_config_partial("/some/path/config.conf", "");
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_config_partial_new_with_path_containing_spaces() {
        let partial = create_config_partial("/path with spaces/my config.conf", "some content");
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_config_partial_new_with_special_chars_in_path() {
        let partial = create_config_partial("/tmp/config-dir_1/file@v2.conf", "content");
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_config_partial_new_is_functional_for_imports() {
        let dir = create_temp_config_dir();
        create_config_file(dir.path(), "included.conf", "some content");
        let partial =
            create_config_partial_in_dir(dir.path(), "main.conf", "include included.conf");
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("included.conf"));
    }

    // ========================================================================
    // Tests for ConfigPartial::get_imported_paths
    // ========================================================================

    #[test]
    fn test_get_imported_paths_no_includes() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "set $mod Super\nbindsym $mod+Enter exec terminal\n",
        );
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_get_imported_paths_single_relative_include() {
        let dir = create_temp_config_dir();
        create_config_file(dir.path(), "bindings.conf", "bindsym $mod+X kill");
        let partial =
            create_config_partial_in_dir(dir.path(), "config.conf", "include bindings.conf");
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("bindings.conf"));
        assert!(paths[0].is_file());
    }

    #[test]
    fn test_get_imported_paths_single_absolute_include() {
        let dir = create_temp_config_dir();
        let abs_path = create_config_file(dir.path(), "absolute.conf", "bindsym $mod+X kill");
        let content = format!("include {}", abs_path.display());
        let partial = create_config_partial_in_dir(dir.path(), "config.conf", &content);
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], abs_path);
    }

    #[test]
    fn test_get_imported_paths_multiple_includes() {
        let dir = create_temp_config_dir();
        create_config_file(dir.path(), "bindings.conf", "bindsym $mod+X kill");
        create_config_file(dir.path(), "vars.conf", "set $mod Super");
        let partial = create_config_partial_in_dir(
            dir.path(),
            "config.conf",
            "include bindings.conf\ninclude vars.conf",
        );
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 2);
        let path_names: Vec<_> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(path_names.contains(&"bindings.conf".to_string()));
        assert!(path_names.contains(&"vars.conf".to_string()));
    }

    #[test]
    fn test_get_imported_paths_glob_pattern() {
        let dir = create_temp_config_dir();
        create_config_file(dir.path(), "bind1.conf", "bindsym $mod+A exec a");
        create_config_file(dir.path(), "bind2.conf", "bindsym $mod+B exec b");
        create_config_file(dir.path(), "other.txt", "not a config");
        let partial = create_config_partial_in_dir(dir.path(), "config.conf", "include bind*.conf");
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 2);
        let path_names: Vec<_> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(path_names.contains(&"bind1.conf".to_string()));
        assert!(path_names.contains(&"bind2.conf".to_string()));
    }

    #[test]
    fn test_get_imported_paths_quoted_include_path() {
        let dir = create_temp_config_dir();
        create_config_file(dir.path(), "quoted.conf", "content");
        let partial =
            create_config_partial_in_dir(dir.path(), "config.conf", "include \"quoted.conf\"");
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("quoted.conf"));
    }

    #[test]
    fn test_get_imported_paths_mixed_include_types() {
        let dir = create_temp_config_dir();
        create_config_file(dir.path(), "relative.conf", "content1");
        let abs_path = create_config_file(dir.path(), "absolute.conf", "content2");
        let content = format!("include relative.conf\ninclude {}", abs_path.display());
        let partial = create_config_partial_in_dir(dir.path(), "config.conf", &content);
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("relative.conf")));
        assert!(paths.iter().any(|p| p == &abs_path));
    }

    #[test]
    fn test_get_imported_paths_nonexistent_file_returns_empty() {
        let dir = create_temp_config_dir();
        let partial =
            create_config_partial_in_dir(dir.path(), "config.conf", "include nonexistent.conf");
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_get_imported_paths_skips_non_include_lines() {
        let dir = create_temp_config_dir();
        create_config_file(dir.path(), "bindings.conf", "content");
        let partial = create_config_partial_in_dir(
            dir.path(),
            "config.conf",
            "# include comment.conf\nset $mod Super\ninclude bindings.conf\nbindsym $mod+X kill",
        );
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("bindings.conf"));
    }

    #[test]
    fn test_get_imported_paths_nested_directory_include() {
        let dir = create_temp_config_dir();
        let sub_dir = dir.path().join("subdir");
        std::fs::create_dir_all(&sub_dir).unwrap();
        create_config_file(&sub_dir, "nested.conf", "content");
        let partial =
            create_config_partial_in_dir(dir.path(), "config.conf", "include subdir/nested.conf");
        let paths = partial.get_imported_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("nested.conf"));
    }

    #[test]
    fn test_get_imported_paths_empty_config_returns_empty() {
        let partial = create_config_partial("/tmp/config.conf", "");
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_get_imported_paths_whitespace_only_config_returns_empty() {
        let partial = create_config_partial("/tmp/config.conf", "   \n  \n   ");
        let paths = partial.get_imported_paths().unwrap();
        assert!(paths.is_empty());
    }

    // ========================================================================
    // Tests for ConfigPartial::config_variables
    // ========================================================================

    #[test]
    fn test_config_variables_set_commands() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "set $mod Super\nset $alt Alt\nset $term alacritty",
        );
        let resources = create_mock_resources();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&("mod".to_string(), "Super".to_string())));
        assert!(vars.contains(&("alt".to_string(), "Alt".to_string())));
        assert!(vars.contains(&("term".to_string(), "alacritty".to_string())));
    }

    #[test]
    fn test_config_variables_set_from_resource_with_runtime_value() {
        let partial =
            create_config_partial("/tmp/config.conf", "set_from_resource $mod mod mod_default");
        let resources = create_mock_resources();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0], ("mod".to_string(), "Super".to_string()));
    }

    #[test]
    fn test_config_variables_set_from_resource_with_default_value() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "set_from_resource $border wm.border.width 3",
        );
        let resources = HashMap::new();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0], ("border".to_string(), "3".to_string()));
    }

    #[test]
    fn test_config_variables_mixed_set_and_set_from_resource() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "set $term alacritty\nset_from_resource $mod mod mod_default\nset $launcher dmenu_run",
        );
        let resources = create_mock_resources();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&("term".to_string(), "alacritty".to_string())));
        assert!(vars.contains(&("mod".to_string(), "Super".to_string())));
        assert!(vars.contains(&("launcher".to_string(), "dmenu_run".to_string())));
    }

    #[test]
    fn test_config_variables_empty_config_returns_empty() {
        let partial = create_config_partial("/tmp/config.conf", "");
        let resources = create_mock_resources();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert!(vars.is_empty());
    }

    #[test]
    fn test_config_variables_no_variable_definitions() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "bindsym $mod+Enter exec terminal\n# some comment\ninclude other.conf",
        );
        let resources = create_mock_resources();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert!(vars.is_empty());
    }

    #[test]
    fn test_config_variables_strips_dollar_prefix() {
        let partial = create_config_partial("/tmp/config.conf", "set $mod Super");
        let resources = HashMap::new();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "mod");
    }

    #[test]
    fn test_config_variables_without_dollar_prefix() {
        let partial = create_config_partial("/tmp/config.conf", "set mod Super");
        let resources = HashMap::new();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0], ("mod".to_string(), "Super".to_string()));
    }

    #[test]
    fn test_config_variables_skips_comment_and_binding_lines() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "# comment\nset $mod Super\nbindsym $mod+X kill\nset $alt Alt\n# another comment",
        );
        let resources = HashMap::new();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&("mod".to_string(), "Super".to_string())));
        assert!(vars.contains(&("alt".to_string(), "Alt".to_string())));
    }

    #[test]
    fn test_config_variables_whitespace_around_commands() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "  set   $mod   Super\n\tset_from_resource\t$alt\twm.alt\tAlt_default",
        );
        let resources = HashMap::new();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&("mod".to_string(), "Super".to_string())));
        assert!(vars.contains(&("alt".to_string(), "Alt_default".to_string())));
    }

    #[test]
    fn test_config_variables_last_value_wins_for_duplicates() {
        let partial = create_config_partial("/tmp/config.conf", "set $mod Super\nset $mod Hyper");
        let resources = HashMap::new();
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[1], ("mod".to_string(), "Hyper".to_string()));
    }

    // ========================================================================
    // Tests for ConfigPartial::config_bindings
    // ========================================================================

    #[test]
    fn test_config_bindings_bindsym() {
        let partial =
            create_config_partial("/tmp/config.conf", "bindsym Mod4+Return exec terminal");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].orig_binding, "Mod4+Return");
        assert_eq!(bindings[0].normalized_binding.as_ref(), "Mod4+Return");
        assert_eq!(bindings[0].line_no, 1);
    }

    #[test]
    fn test_config_bindings_bindcode() {
        let partial = create_config_partial("/tmp/config.conf", "bindcode 36+Return exec terminal");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].orig_binding, "36+Return");
        assert_eq!(bindings[0].line_no, 1);
    }

    #[test]
    fn test_config_bindings_with_variable_reference() {
        let partial =
            create_config_partial("/tmp/config.conf", "bindsym $mod+Return exec terminal");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].orig_binding, "$mod+Return");
        assert_eq!(bindings[0].normalized_binding.as_ref(), "Super+Return");
    }

    #[test]
    fn test_config_bindings_without_variable_reference() {
        let partial =
            create_config_partial("/tmp/config.conf", "bindsym Shift+Return exec terminal");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].orig_binding, "Shift+Return");
        assert_eq!(bindings[0].normalized_binding.as_ref(), "Shift+Return");
    }

    #[test]
    fn test_config_bindings_with_multiple_variables() {
        let partial =
            create_config_partial("/tmp/config.conf", "bindsym $mod+$alt+Return exec terminal");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].orig_binding, "$mod+$alt+Return");
        assert_eq!(bindings[0].normalized_binding.as_ref(), "Super+Alt+Return");
    }

    #[test]
    fn test_config_bindings_empty_config_returns_empty() {
        let partial = create_config_partial("/tmp/config.conf", "");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert!(bindings.is_empty());
    }

    #[test]
    fn test_config_bindings_no_bindings() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "set $mod Super\ninclude other.conf\n# comment",
        );
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert!(bindings.is_empty());
    }

    #[test]
    fn test_config_bindings_with_options() {
        let partial = create_config_partial("/tmp/config.conf", "bindsym --release $mod+X kill");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].orig_binding, "$mod+X");
        assert_eq!(bindings[0].normalized_binding.as_ref(), "Super+X");
    }

    #[test]
    fn test_config_bindings_with_multiple_options() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "bindsym --release --whole-window $mod+Button1 floating resize",
        );
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].orig_binding, "$mod+Button1");
    }

    #[test]
    fn test_config_bindings_line_no_is_one_indexed() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "set $mod Super\nbindsym $mod+Return exec terminal\nbindsym $mod+Q kill",
        );
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].line_no, 2);
        assert_eq!(bindings[1].line_no, 3);
    }

    #[test]
    fn test_config_bindings_multiple_bindings() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "bindsym $mod+Return exec terminal\nbindsym $mod+Q kill\nbindsym $mod+D exec dmenu_run",
        );
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].orig_binding, "$mod+Return");
        assert_eq!(bindings[1].orig_binding, "$mod+Q");
        assert_eq!(bindings[2].orig_binding, "$mod+D");
    }

    #[test]
    fn test_config_bindings_line_contents_preserved() {
        let partial =
            create_config_partial("/tmp/config.conf", "bindsym $mod+Return exec terminal");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(
            bindings[0].line_contents,
            "bindsym $mod+Return exec terminal"
        );
    }

    #[test]
    fn test_config_bindings_skips_comments_and_set_lines() {
        let partial = create_config_partial(
            "/tmp/config.conf",
            "# comment\nset $mod Super\nbindsym $mod+X kill\n# another comment",
        );
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].line_no, 3);
    }

    #[test]
    fn test_config_bindings_unresolved_variable_keeps_prefix() {
        let partial =
            create_config_partial("/tmp/config.conf", "bindsym $unknown+Return exec terminal");
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].normalized_binding.as_ref(), "$unknown+Return");
    }

    // ========================================================================
    // Original tests
    // ========================================================================

    /// Get session mappings for testing
    fn get_session_mappings() -> Box<SessionMappings> {
        Box::new([
            (
                Session::Wayland,
                Path::new("/tmp/regolith-configuration/sway/config"),
            ),
            (
                Session::X11,
                Path::new("/tmp/regolith-configuration/i3/config"),
            ),
        ])
    }

    /// Mock configurations for testing
    fn get_mock_configurations(
        root_config_path: &Path,
        config_dir: &Path,
    ) -> Vec<(PathBuf, String)> {
        let root_config_path = Path::new(root_config_path);
        vec![
            (
                PathBuf::from(root_config_path),
                "
                set $mod Super
                include bindings.conf
                include more_bindings.conf
                "
                .to_string(),
            ),
            (
                config_dir.join("bindings.conf"),
                "
                bindsym $mod+Enter exec terminal
                bindsym $mod+Shift+Q kill
                "
                .to_string(),
            ),
            (
                config_dir.join("more_bindings.conf"),
                "bindsym $mod+D exec dmenu_run".to_string(),
            ),
        ]
    }

    /// Sets up a temporary Regolith configuration directory with partial configs for testing
    /// Returns the config partials created
    fn setup_config_partials<'a, 'b, MockConfigsFn>(
        get_mock_configurations: MockConfigsFn,
        session_mappings: &'a SessionMappings,
    ) -> &'b [(PathBuf, String)]
    where
        MockConfigsFn: for<'c, 'd> Fn(&'c Path, &'d Path) -> Vec<(PathBuf, String)>,
    {
        static INITIALIZER: OnceLock<Box<[(PathBuf, String)]>> = OnceLock::new();

        let config_partials = INITIALIZER.get_or_init(|| {
            let mut all_partials = Vec::new();
            for session_mapping in session_mappings {
                let root_config_path = Path::new(session_mapping.1);
                if root_config_path.exists() {
                    std::fs::remove_file(root_config_path).unwrap();
                }

                // setup regolith base config dir for the session
                let config_dir = root_config_path.parent().unwrap();
                if !config_dir.exists() {
                    std::fs::create_dir_all(config_dir).unwrap();
                }

                let config_partials = get_mock_configurations(root_config_path, config_dir);

                for (path, content) in config_partials.iter() {
                    let mut file = File::create(&path).unwrap();
                    file.write_all(content.as_bytes()).unwrap();
                }
                all_partials.extend(config_partials);
            }
            all_partials.into_boxed_slice()
        });
        &config_partials
    }

    #[test]
    fn test_new_full_config_from_session() {
        unsafe {
            std::env::set_var("XDG_SESSION_TYPE", "wayland");
        }
        let config_partials =
            setup_config_partials(get_mock_configurations, &get_session_mappings());

        let full_config = FullConfig::new_from_session(Session::Wayland, &get_session_mappings())
            .expect("Failed to create FullConfig from session");

        for (path, content) in config_partials {
            // Skip i3 config partials since we're testing Wayland session
            if path.parent().unwrap().ends_with("i3") {
                continue;
            }
            let matching_partial = full_config
                .partials
                .iter()
                .find(|partial| partial.file_name == *path)
                .expect("Config partial not found in FullConfig");

            assert_eq!(matching_partial.config, *content);
        }
    }

    // ========================================================================
    // Tests for normalize_binding
    // ========================================================================

    #[test]
    fn test_normalize_binding_single_variable() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("$mod+Return", &variables);
        assert_eq!(result.as_ref(), "Super+Return");
    }

    #[test]
    fn test_normalize_binding_multiple_variables() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("$mod+$alt+Return", &variables);
        assert_eq!(result.as_ref(), "Super+Alt+Return");
    }

    #[test]
    fn test_normalize_binding_no_variables_in_binding() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("Shift+Return", &variables);
        assert_eq!(result.as_ref(), "Shift+Return");
    }

    #[test]
    fn test_normalize_binding_unknown_variable_unchanged() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("$unknown+Return", &variables);
        assert_eq!(result.as_ref(), "$unknown+Return");
    }

    #[test]
    fn test_normalize_binding_empty_variables_map() {
        let variables = BTreeMap::new();
        let result = search::bindings::normalize_binding("$mod+Return", &variables);
        assert_eq!(result.as_ref(), "$mod+Return");
    }

    #[test]
    fn test_normalize_binding_variable_at_end() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("$mod", &variables);
        assert_eq!(result.as_ref(), "Super");
    }

    #[test]
    fn test_normalize_binding_variable_in_middle() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("Shift+$mod+Return", &variables);
        assert_eq!(result.as_ref(), "Shift+Super+Return");
    }

    #[test]
    fn test_normalize_binding_chained_variables() {
        let mut variables = BTreeMap::new();
        variables.insert("mod".to_string(), "$wm_mod".to_string());
        variables.insert("wm_mod".to_string(), "Super".to_string());
        let result = search::bindings::normalize_binding("$mod+Return", &variables);
        assert_eq!(result.as_ref(), "Super+Return");
    }

    #[test]
    fn test_normalize_binding_trims_whitespace() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("  $mod+Return  ", &variables);
        assert_eq!(result.as_ref(), "Super+Return");
    }

    #[test]
    fn test_normalize_binding_empty_string() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("", &variables);
        assert_eq!(result.as_ref(), "");
    }

    #[test]
    fn test_normalize_binding_dollar_sign_only() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("$", &variables);
        assert_eq!(result.as_ref(), "$");
    }

    #[test]
    fn test_normalize_binding_all_variables_resolved() {
        let variables = create_mock_variables();
        let result = search::bindings::normalize_binding("$mod+$alt+$mod", &variables);
        assert_eq!(result.as_ref(), "Super+Alt+Super");
    }

    // ========================================================================
    // Tests for search_binding_result
    // ========================================================================

    #[test]
    fn test_search_binding_result_exact_normalized_match() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\ninclude bindings.conf",
            &[("bindings.conf", "bindsym $mod+Enter exec terminal")],
        );
        let resources = create_mock_resources();
        let result =
            search::bindings::search_binding_result("Super+Enter", &full_config, &resources);
        assert!(!result.0.is_empty());
        assert!(result
            .0
            .iter()
            .any(|b| b.line_contents.contains("exec terminal")));
    }

    #[test]
    fn test_search_binding_result_substring_raw_match() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "include bindings.conf",
            &[("bindings.conf", "bindsym $mod+Shift+Q kill")],
        );
        let resources = create_mock_resources();
        let result = search::bindings::search_binding_result("Shift", &full_config, &resources);
        assert!(!result.0.is_empty());
    }

    #[test]
    fn test_search_binding_result_case_insensitive() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\ninclude bindings.conf",
            &[("bindings.conf", "bindsym $mod+return exec terminal")],
        );
        let resources = create_mock_resources();
        let result =
            search::bindings::search_binding_result("super+return", &full_config, &resources);
        assert!(!result.0.is_empty());
    }

    #[test]
    fn test_search_binding_result_multiple_config_files() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\ninclude bindings.conf\ninclude more_bindings.conf",
            &[
                ("bindings.conf", "bindsym $mod+Enter exec terminal"),
                ("more_bindings.conf", "bindsym $mod+D exec dmenu_run"),
            ],
        );
        let resources = create_mock_resources();
        let result = search::bindings::search_binding_result("Super+D", &full_config, &resources);
        assert!(!result.0.is_empty());
        assert!(result
            .0
            .iter()
            .any(|b| b.line_contents.contains("dmenu_run")));
    }

    #[test]
    fn test_search_binding_result_with_variable_resolution() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\nset $alt Alt\ninclude bindings.conf",
            &[("bindings.conf", "bindsym $mod+$alt+F fullscreen toggle")],
        );
        let resources = create_mock_resources();
        let result =
            search::bindings::search_binding_result("Super+Alt+F", &full_config, &resources);
        assert!(!result.0.is_empty());
    }

    #[test]
    fn test_search_binding_result_no_matches() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\ninclude bindings.conf",
            &[("bindings.conf", "bindsym $mod+Enter exec terminal")],
        );
        let resources = create_mock_resources();
        let result =
            search::bindings::search_binding_result("Control+Delete", &full_config, &resources);
        assert!(result.0.is_empty());
    }

    #[test]
    fn test_search_binding_result_multiple_matches() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\ninclude bindings.conf",
            &[(
                "bindings.conf",
                "bindsym $mod+Enter exec terminal\nbindsym $mod+Shift+Enter exec alt-terminal",
            )],
        );
        let resources = create_mock_resources();
        let result = search::bindings::search_binding_result("Super", &full_config, &resources);
        assert!(result.0.len() >= 2);
    }

    #[test]
    fn test_search_binding_result_special_characters() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\ninclude bindings.conf",
            &[("bindings.conf", "bindsym $mod+minus exec test")],
        );
        let resources = create_mock_resources();
        let result =
            search::bindings::search_binding_result("Super+minus", &full_config, &resources);
        assert!(!result.0.is_empty());
    }

    #[test]
    fn test_search_binding_result_from_trawl_resources() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $mod mod Super\ninclude bindings.conf",
            &[("bindings.conf", "bindsym $mod+Enter exec terminal")],
        );
        let resources = create_mock_resources();
        let result =
            search::bindings::search_binding_result("Super+Enter", &full_config, &resources);
        assert!(!result.0.is_empty());
    }

    // ========================================================================
    // Tests for FullConfig::new_from_session
    // ========================================================================

    #[test]
    fn test_new_from_session_wayland_valid_config() {
        let fixture = TestFixture::new(Session::Wayland);
        let root_path: &'static Path =
            Box::leak(fixture.root_config_path.clone().into_boxed_path());
        let mappings: &SessionMappings =
            &[(Session::Wayland, root_path), (Session::X11, root_path)];

        fixture.add_config("bindings.conf", "bindsym $mod+Enter exec terminal");

        let mut file = File::create(&fixture.root_config_path).unwrap();
        file.write_all(b"set $mod Super\ninclude bindings.conf")
            .unwrap();

        let full_config =
            FullConfig::new_from_session(Session::Wayland, mappings).expect("Should succeed");
        assert_eq!(full_config.partials.len(), 2);
        assert!(full_config
            .partials
            .iter()
            .any(|p| p.file_name == fixture.root_config_path));
    }

    #[test]
    fn test_new_from_session_x11_valid_config() {
        let fixture = TestFixture::new(Session::X11);
        let root_path: &'static Path =
            Box::leak(fixture.root_config_path.clone().into_boxed_path());
        let mappings: &SessionMappings =
            &[(Session::Wayland, root_path), (Session::X11, root_path)];

        fixture.add_config("bindings.conf", "bindsym $mod+Q kill");

        let mut file = File::create(&fixture.root_config_path).unwrap();
        file.write_all(b"set $mod Mod1\ninclude bindings.conf")
            .unwrap();

        let full_config =
            FullConfig::new_from_session(Session::X11, mappings).expect("Should succeed");
        assert_eq!(full_config.partials.len(), 2);
    }

    #[test]
    fn test_new_from_session_discovers_all_includes() {
        let fixture = TestFixture::new(Session::Wayland);
        let root_path: &'static Path =
            Box::leak(fixture.root_config_path.clone().into_boxed_path());
        let mappings: &SessionMappings =
            &[(Session::Wayland, root_path), (Session::X11, root_path)];

        fixture.add_config("bindings.conf", "bindsym $mod+Enter exec terminal");
        fixture.add_config("more_bindings.conf", "bindsym $mod+D exec dmenu_run");

        let mut file = File::create(&fixture.root_config_path).unwrap();
        file.write_all(b"include bindings.conf\ninclude more_bindings.conf")
            .unwrap();

        let full_config =
            FullConfig::new_from_session(Session::Wayland, mappings).expect("Should succeed");
        assert_eq!(full_config.partials.len(), 3);

        let filenames: Vec<_> = full_config
            .partials
            .iter()
            .filter_map(|p| {
                p.file_name
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
            })
            .collect();
        assert!(filenames.contains(&"config".to_string()));
        assert!(filenames.contains(&"bindings.conf".to_string()));
        assert!(filenames.contains(&"more_bindings.conf".to_string()));
    }

    #[test]
    fn test_new_from_session_nested_includes() {
        let fixture = TestFixture::new(Session::Wayland);
        let root_path: &'static Path =
            Box::leak(fixture.root_config_path.clone().into_boxed_path());
        let mappings: &SessionMappings =
            &[(Session::Wayland, root_path), (Session::X11, root_path)];

        fixture.add_config("level1.conf", "include level2.conf\nset $level1 true");
        fixture.add_config("level2.conf", "set $level2 true");

        let mut file = File::create(&fixture.root_config_path).unwrap();
        file.write_all(b"include level1.conf").unwrap();

        let full_config =
            FullConfig::new_from_session(Session::Wayland, mappings).expect("Should succeed");
        assert_eq!(full_config.partials.len(), 3);
    }

    #[test]
    fn test_new_from_session_nonexistent_root_config_returns_error() {
        let mappings: &SessionMappings = &[(
            Session::Wayland,
            Path::new("/tmp/regolith-nonexistent-test-root/config"),
        )];

        let result = FullConfig::new_from_session(Session::Wayland, mappings);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_from_session_nonexistent_included_config_skipped() {
        let fixture = TestFixture::new(Session::Wayland);
        let root_path: &'static Path =
            Box::leak(fixture.root_config_path.clone().into_boxed_path());
        let mappings: &SessionMappings =
            &[(Session::Wayland, root_path), (Session::X11, root_path)];

        let mut file = File::create(&fixture.root_config_path).unwrap();
        file.write_all(b"include nonexistent.conf").unwrap();

        let full_config =
            FullConfig::new_from_session(Session::Wayland, mappings).expect("Should succeed");
        // Only the root config partial is present; nonexistent include is silently skipped
        assert_eq!(full_config.partials.len(), 1);
    }

    #[test]
    fn test_new_from_session_root_config_only_no_includes() {
        let fixture = TestFixture::new(Session::Wayland);
        let root_path: &'static Path =
            Box::leak(fixture.root_config_path.clone().into_boxed_path());
        let mappings: &SessionMappings =
            &[(Session::Wayland, root_path), (Session::X11, root_path)];

        let mut file = File::create(&fixture.root_config_path).unwrap();
        file.write_all(b"set $mod Super\nbindsym $mod+Enter exec terminal")
            .unwrap();

        let full_config =
            FullConfig::new_from_session(Session::Wayland, mappings).expect("Should succeed");
        assert_eq!(full_config.partials.len(), 1);
        assert!(full_config.partials[0].file_name == fixture.root_config_path);
    }

    #[test]
    fn test_new_from_session_with_sample_configs() {
        let full_config = create_test_full_config(
            Session::Wayland,
            SAMPLE_ROOT_CONFIG,
            &[
                ("bindings.conf", SAMPLE_BINDINGS_CONFIG),
                ("more_bindings.conf", SAMPLE_MORE_BINDINGS_CONFIG),
            ],
        );
        assert!(full_config.partials.len() >= 3);
    }

    // ========================================================================
    // Tests for search_keyword_result
    // ========================================================================

    #[test]
    fn test_search_keyword_single_line_match() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\nbindsym $mod+Enter exec terminal",
            &[],
        );
        let result = search::keyword::search_keyword_result("terminal", &full_config);
        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0].line_number, 2);
        assert_eq!(
            result.0[0].line_contents,
            "bindsym $mod+Enter exec terminal"
        );
    }

    #[test]
    fn test_search_keyword_multiple_lines_same_file() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "include bindings.conf",
            &[(
                "bindings.conf",
                "bindsym $mod+Enter exec terminal\nbindsym $mod+D exec dmenu_run\nbindsym $mod+Shift+Enter exec alt-terminal",
            )],
        );
        let result = search::keyword::search_keyword_result("terminal", &full_config);
        assert_eq!(result.0.len(), 2);
        assert_eq!(result.0[0].line_number, 1);
        assert_eq!(result.0[1].line_number, 3);
    }

    #[test]
    fn test_search_keyword_across_multiple_config_files() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "include bindings.conf\ninclude more_bindings.conf",
            &[
                ("bindings.conf", "bindsym $mod+Enter exec terminal"),
                ("more_bindings.conf", "bindsym $mod+D exec dmenu_run"),
            ],
        );
        let result = search::keyword::search_keyword_result("exec", &full_config);
        assert_eq!(result.0.len(), 2);
        let paths: Vec<_> = result
            .0
            .iter()
            .map(|d| {
                d.file_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert!(paths.contains(&"bindings.conf".to_string()));
        assert!(paths.contains(&"more_bindings.conf".to_string()));
    }

    #[test]
    fn test_search_keyword_case_insensitive() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "bindsym $mod+Enter exec TERMINAL\nbindsym $mod+D exec dmenu_run",
            &[],
        );
        let result_upper = search::keyword::search_keyword_result("TERMINAL", &full_config);
        let result_lower = search::keyword::search_keyword_result("terminal", &full_config);
        let result_mixed = search::keyword::search_keyword_result("TeRmInAl", &full_config);
        assert_eq!(result_upper.0.len(), 1);
        assert_eq!(result_lower.0.len(), 1);
        assert_eq!(result_mixed.0.len(), 1);
    }

    #[test]
    fn test_search_keyword_no_matches() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\nbindsym $mod+Enter exec terminal",
            &[],
        );
        let result = search::keyword::search_keyword_result("nonexistent", &full_config);
        assert!(result.0.is_empty());
    }

    #[test]
    fn test_search_keyword_empty_keyword() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\nbindsym $mod+Enter exec terminal",
            &[],
        );
        let result = search::keyword::search_keyword_result("", &full_config);
        assert_eq!(result.0.len(), 2);
    }

    #[test]
    fn test_search_keyword_special_characters() {
        let full_config =
            create_test_full_config(Session::Wayland, "bindsym $mod+Enter exec terminal", &[]);
        let result = search::keyword::search_keyword_result("$mod", &full_config);
        assert_eq!(result.0.len(), 1);
        assert!(result.0[0].line_contents.contains("$mod"));
    }

    #[test]
    fn test_search_keyword_substring_match() {
        let full_config =
            create_test_full_config(Session::Wayland, "bindsym $mod+Enter exec terminal", &[]);
        let result = search::keyword::search_keyword_result("term", &full_config);
        assert_eq!(result.0.len(), 1);
        assert!(result.0[0].line_contents.contains("terminal"));
    }

    #[test]
    fn test_search_keyword_with_whitespace() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "bindsym $mod+Enter exec terminal\nset $mod Super",
            &[],
        );
        let result = search::keyword::search_keyword_result("exec terminal", &full_config);
        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0].line_contents,
            "bindsym $mod+Enter exec terminal"
        );
    }

    #[test]
    fn test_search_keyword_empty_config() {
        let full_config = create_test_full_config(Session::Wayland, "", &[]);
        let result = search::keyword::search_keyword_result("anything", &full_config);
        assert!(result.0.is_empty());
    }

    #[test]
    fn test_search_keyword_line_number_one_indexed() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set $mod Super\nbindsym $mod+Enter exec terminal\nbindsym $mod+D exec dmenu_run",
            &[],
        );
        let result = search::keyword::search_keyword_result("dmenu_run", &full_config);
        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0].line_number, 3);
    }

    #[test]
    fn test_search_keyword_preserves_file_path() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "include bindings.conf",
            &[("bindings.conf", "bindsym $mod+Enter exec terminal")],
        );
        let result = search::keyword::search_keyword_result("terminal", &full_config);
        assert_eq!(result.0.len(), 1);
        assert!(result.0[0].file_path.ends_with("bindings.conf"));
    }

    // ========================================================================
    // Tests for get_session_type
    // ========================================================================

    #[test]
    fn test_get_session_type_wayland() {
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
            std::env::set_var("XDG_SESSION_TYPE", "wayland");
        }
        let session = get_session_type();
        assert_eq!(session, Some(Session::Wayland));
    }

    #[test]
    fn test_get_session_type_x11() {
        // Remove first to avoid stale value from other test runs
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
            std::env::set_var("XDG_SESSION_TYPE", "x11");
        }
        let session = get_session_type();
        assert_eq!(session, Some(Session::X11));
    }

    #[test]
    fn test_get_session_type_not_set() {
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
        }
        let session = get_session_type();
        assert_eq!(session, None);
    }

    #[test]
    fn test_get_session_type_invalid_value() {
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
            std::env::set_var("XDG_SESSION_TYPE", "mir");
        }
        let session = get_session_type();
        assert_eq!(session, None);
    }

    #[test]
    fn test_get_session_type_empty_string() {
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
            std::env::set_var("XDG_SESSION_TYPE", "");
        }
        let session = get_session_type();
        assert_eq!(session, None);
    }

    #[test]
    fn test_get_session_type_case_sensitive() {
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
            std::env::set_var("XDG_SESSION_TYPE", "Wayland");
        }
        let session = get_session_type();
        assert_eq!(session, None);
    }

    // ========================================================================
    // Tests for search_resource_result
    // ========================================================================

    #[test]
    fn test_search_resource_exact_match_in_config() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $border regolithwm.border.width 3",
            &[],
        );
        let provider = MockResourceProvider::default();
        let result = search::resource::search_resource_result(
            "regolithwm.border.width",
            &full_config,
            &provider,
        );
        assert!(result.has_exact_match);
        assert_eq!(result.resource_name, "regolithwm.border.width");
        assert_eq!(result.default_value, Some("3".to_string()));
        assert!(result.usages.len() >= 1);
    }

    #[test]
    fn test_search_resource_runtime_value() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $border regolithwm.border.width 3",
            &[],
        );
        let mut resources = HashMap::new();
        resources.insert("regolithwm.border.width".to_string(), "5".to_string());
        let provider = MockResourceProvider::new(resources);
        let result = search::resource::search_resource_result(
            "regolithwm.border.width",
            &full_config,
            &provider,
        );
        assert!(result.has_exact_match);
        assert_eq!(result.runtime_value, Some("5".to_string()));
        assert_eq!(result.default_value, Some("3".to_string()));
    }

    #[test]
    fn test_search_resource_substring_match() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $border regolithwm.border.width 3\nset_from_resource $font regolithwm.font.size 12",
            &[],
        );
        let provider = MockResourceProvider::default();
        let result =
            search::resource::search_resource_result("regolithwm.border", &full_config, &provider);
        assert!(!result.has_exact_match);
        assert!(result
            .matched_resources
            .iter()
            .any(|r| r == "regolithwm.border.width"));
        assert!(!result
            .matched_resources
            .iter()
            .any(|r| r == "regolithwm.font.size"));
    }

    #[test]
    fn test_search_resource_no_matches() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $border regolithwm.border.width 3",
            &[],
        );
        let provider = MockResourceProvider::default();
        let result = search::resource::search_resource_result(
            "nonexistent.resource",
            &full_config,
            &provider,
        );
        assert!(!result.has_exact_match);
        assert!(result.runtime_value.is_none());
        assert!(result.default_value.is_none());
        assert!(result.matched_resources.is_empty());
        assert!(result.usages.is_empty());
    }

    #[test]
    fn test_search_resource_similar_fuzzy_match() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $border regolithwm.border.width 3",
            &[],
        );
        let provider = MockResourceProvider::default();
        let result = search::resource::search_resource_result(
            "regolithwm.border.heigth",
            &full_config,
            &provider,
        );
        assert!(!result.has_exact_match);
        assert!(result
            .similar_resources
            .iter()
            .any(|r| r == "regolithwm.border.width"));
    }

    #[test]
    fn test_search_resource_case_insensitive() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $border regolithwm.border.width 3",
            &[],
        );
        let provider = MockResourceProvider::default();
        let result = search::resource::search_resource_result(
            "RegolithWM.Border.Width",
            &full_config,
            &provider,
        );
        assert!(result.has_exact_match);
        assert_eq!(result.default_value, Some("3".to_string()));
    }

    #[test]
    fn test_search_resource_multiple_usages() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $border regolithwm.border.width 3\ninclude more.conf",
            &[(
                "more.conf",
                "set_from_resource $border_alt regolithwm.border.width 2",
            )],
        );
        let provider = MockResourceProvider::default();
        let result = search::resource::search_resource_result(
            "regolithwm.border.width",
            &full_config,
            &provider,
        );
        assert!(result.has_exact_match);
        assert!(result.usages.len() >= 2);
    }

    #[test]
    fn test_search_resource_special_characters_in_name() {
        let full_config = create_test_full_config(
            Session::Wayland,
            "set_from_resource $font regolithwm.font.name \"Source Code Pro\"",
            &[],
        );
        let provider = MockResourceProvider::default();
        let result = search::resource::search_resource_result(
            "regolithwm.font.name",
            &full_config,
            &provider,
        );
        assert!(result.has_exact_match);
        assert_eq!(
            result.default_value,
            Some("\"Source Code Pro\"".to_string())
        );
    }

    #[test]
    fn test_search_resource_from_runtime_only() {
        let full_config = create_test_full_config(Session::Wayland, "set $mod Super", &[]);
        let mut resources = HashMap::new();
        resources.insert("regolithwm.border.width".to_string(), "2".to_string());
        let provider = MockResourceProvider::new(resources);
        let result = search::resource::search_resource_result(
            "regolithwm.border.width",
            &full_config,
            &provider,
        );
        assert!(result.has_exact_match);
        assert_eq!(result.runtime_value, Some("2".to_string()));
        assert!(result.default_value.is_none());
    }

    #[test]
    fn test_search_resource_empty_config() {
        let full_config = create_test_full_config(Session::Wayland, "", &[]);
        let provider = MockResourceProvider::default();
        let result = search::resource::search_resource_result("anything", &full_config, &provider);
        assert!(!result.has_exact_match);
        assert!(result.runtime_value.is_none());
        assert!(result.default_value.is_none());
        assert!(result.matched_resources.is_empty());
        assert!(result.usages.is_empty());
        assert!(result.overrides.is_empty());
    }
}
