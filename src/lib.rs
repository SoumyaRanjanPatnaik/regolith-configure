pub mod cli_args;
use anyhow::{Context, Result, anyhow};
use cli_args::{EjectArgs, SearchArgs, Session};
use glob::glob;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, LinkedList},
    env,
    fmt::Display,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use zbus::{
    blocking::{Connection, fdo::PropertiesProxy},
    names::InterfaceName,
};

/// Represents a config file that is imported when loading
/// the main configration
#[derive(Debug)]
struct ConfigPartial {
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
    ) -> impl Iterator<Item = BindingDef<'a>> {
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
            let resolved_binding = normalize_binding(binding, variables);

            Some(BindingDef {
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
    ) -> BindingsSearchResult<'_> {
        let bindings: Vec<_> = self
            .partials
            .iter()
            .flat_map(|partial| partial.config_bindings(variables))
            .collect();

        BindingsSearchResult::from(bindings)
    }
}

#[derive(Debug)]
struct BindingDef<'a> {
    #[allow(dead_code)]
    orig_binding: &'a str,
    normalized_binding: Cow<'a, str>,
    src_config: &'a ConfigPartial,
    line_no: usize,
    line_contents: String,
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
struct BindingsSearchResult<'a>(Vec<BindingDef<'a>>);

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

/// Normalizes a binding by substituting variables with their corresponding values
///
/// * `binding`: The search pattern representing the binding
/// * `variables`: Configuration variables for resolving bindings
fn normalize_binding<'a>(binding: &'a str, variables: &BTreeMap<String, String>) -> Cow<'a, str> {
    let mut normalized_binding = Cow::Borrowed(binding.trim());

    while normalized_binding.contains('$') {
        let updated_binding = normalized_binding
            .split('+')
            .map(|key| key.trim())
            .map(|key| {
                // Only consider keys that are variables
                if !key.starts_with("$") || key.len() < 2 {
                    return key;
                }

                // Only consider the part after the '$' sign
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

/// Searches for keybinding definitions matching the user's search query
/// * `binding`: The search pattern representing the binding
/// * `config`: The full Regolith configuration
/// * `trawl_resources`: Resource values set in trawlcat that represent the bindings
fn search_binding_result<'a>(
    binding: &str,
    config: &'a FullConfig,
    trawl_resources: &HashMap<String, String>,
) -> impl Display + 'a {
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

/// Filters packages and files related to user's search query
pub fn search_config<'a>(
    args: &SearchArgs,
    config: &'a FullConfig,
    trawl_resources: &HashMap<String, String>,
) -> Option<Box<dyn Display + 'a>> {
    match args.filter() {
        cli_args::FilterType::Bindings => Some(Box::new(search_binding_result(
            args.pattern(),
            config,
            trawl_resources,
        ))),
        cli_args::FilterType::Keyword => todo!(),
        cli_args::FilterType::Resource => todo!(),
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

pub fn get_trawl_resources() -> Result<HashMap<String, String>> {
    let connection = Connection::session().context("Failed to connect to D-Bus session bus")?;
    let props = PropertiesProxy::builder(&connection)
        .destination("org.regolith.Trawl")?
        .path("/org/regolith/Trawl")?
        .build()
        .context("Failed to connect to Trawl D-Bus service")?;

    let resources_map: HashMap<String, String> = props
        .get(InterfaceName::try_from("org.regolith.trawl1")?, "Resources")
        .context("Failed to read property 'Resources' from Trawl D-Bus service")?
        .try_into()?;

    Ok(resources_map)
}

#[cfg(test)]
mod tests {
    use std::{io::Write, sync::OnceLock};

    use super::*;

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

    #[test]
    fn test_normalize_binding() {
        let mut variables = BTreeMap::new();
        variables.insert("mod".to_string(), "Super".to_string());
        variables.insert("alt".to_string(), "Alt".to_string());
        let binding = "$mod+Shift+X";
        let normalized = normalize_binding(binding, &variables);
        assert_eq!(normalized, "Super+Shift+X");

        let binding2 = "$alt+F4";
        let normalized2 = normalize_binding(binding2, &variables);
        assert_eq!(normalized2, "Alt+F4");
    }

    #[test]
    fn test_search_binding_result() {
        setup_config_partials(get_mock_configurations, &get_session_mappings());
        let full_config = FullConfig::new_from_session(Session::Wayland, &get_session_mappings())
            .expect("Failed to create FullConfig from session");
        let mut trawl_resources = HashMap::new();
        trawl_resources.insert("mod".to_string(), "Super".to_string());
        trawl_resources.insert("wm.launch.terminall".to_string(), "Enter".to_string());
        let search_result = search_binding_result("Super+Enter", &full_config, &trawl_resources);
        let result_string = format!("{}", search_result);
        assert!(result_string.contains("bindings.conf"));
    }
}
