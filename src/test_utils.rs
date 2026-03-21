use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use tempfile::TempDir;

use crate::cli_args::Session;
use crate::resources::ResourceProvider;
use crate::{ConfigPartial, FullConfig, SessionMappings};

/// Mock ResourceProvider that returns a configurable HashMap
pub struct MockResourceProvider {
    resources: HashMap<String, String>,
}

impl MockResourceProvider {
    pub fn new(resources: HashMap<String, String>) -> Self {
        Self { resources }
    }
}

impl Default for MockResourceProvider {
    fn default() -> Self {
        Self::new(HashMap::new())
    }
}

impl ResourceProvider for MockResourceProvider {
    fn get_all_resources(&self) -> Result<HashMap<String, String>> {
        Ok(self.resources.clone())
    }
}

/// Creates a temporary directory with config files for testing
pub fn create_temp_config_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

/// Creates a config file in the given directory
pub fn create_config_file(dir: &Path, filename: &str, content: &str) -> PathBuf {
    let path = dir.join(filename);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent dirs");
    }
    let mut file = File::create(&path).expect("Failed to create config file");
    file.write_all(content.as_bytes())
        .expect("Failed to write config file");
    path
}

/// Creates an Xresource file in the given directory
pub fn create_xresources_file(dir: &Path, filename: &str, content: &str) -> PathBuf {
    create_config_file(dir, filename, content)
}

/// Creates a ConfigPartial from path and content strings
pub fn create_config_partial(path: &str, content: &str) -> ConfigPartial {
    ConfigPartial::new(Path::new(path), content)
}

/// Creates a ConfigPartial from path and content using a temp dir
pub fn create_config_partial_in_dir(dir: &Path, filename: &str, content: &str) -> ConfigPartial {
    let path = create_config_file(dir, filename, content);
    let content = fs::read_to_string(&path).expect("Failed to read config file");
    ConfigPartial::new(&path, &content)
}

/// Creates a FullConfig with multiple partials for testing using session mappings
pub fn create_test_full_config(
    session: Session,
    root_config_content: &str,
    included_configs: &[(&str, &str)],
) -> FullConfig {
    let fixture = TestFixture::new(session);

    // Add all included configs
    for (filename, content) in included_configs {
        fixture.add_config(filename, content);
    }

    // Create root config
    fixture.create_full_config(root_config_content)
}

/// Helper to get session mappings for testing
pub fn get_test_session_mappings() -> Box<SessionMappings> {
    Box::new([
        (
            Session::Wayland,
            Path::new("/tmp/regolith-test/sway/config"),
        ),
        (Session::X11, Path::new("/tmp/regolith-test/i3/config")),
    ])
}

/// Creates a temporary directory with pre-configured test files
pub struct TestFixture {
    pub temp_dir: TempDir,
    pub config_dir: PathBuf,
    pub root_config_path: PathBuf,
}

impl TestFixture {
    pub fn new(session: Session) -> Self {
        let temp_dir = create_temp_config_dir();
        let config_dir = temp_dir.path().join(match session {
            Session::Wayland => "sway",
            Session::X11 => "i3",
        });
        fs::create_dir_all(&config_dir).expect("Failed to create config dir");

        let root_config_path = config_dir.join("config");

        Self {
            temp_dir,
            config_dir,
            root_config_path,
        }
    }

    pub fn add_config(&self, filename: &str, content: &str) -> PathBuf {
        create_config_file(&self.config_dir, filename, content)
    }

    pub fn add_xresources(&self, filename: &str, content: &str) -> PathBuf {
        create_xresources_file(&self.config_dir, filename, content)
    }

    pub fn create_full_config(&self, root_content: &str) -> FullConfig {
        let mut file = File::create(&self.root_config_path).expect("Failed to create root config");
        file.write_all(root_content.as_bytes())
            .expect("Failed to write root config");

        // Use new_from_session with a custom session mapping pointing to our temp config
        // Note: We need to leak the path to get 'static lifetime for SessionMappings
        let leaked_path: &'static Path = Box::leak(self.root_config_path.clone().into_boxed_path());
        let session_mappings: &SessionMappings =
            &[(Session::Wayland, leaked_path), (Session::X11, leaked_path)];

        FullConfig::new_from_session(Session::Wayland, session_mappings)
            .expect("Failed to create FullConfig")
    }
}

/// Thread-safe static storage for test fixtures
static TEST_FIXTURE_WAYLAND: OnceLock<TestFixture> = OnceLock::new();
static TEST_FIXTURE_X11: OnceLock<TestFixture> = OnceLock::new();

/// Get or initialize a test fixture for the given session
pub fn get_or_init_fixture(session: Session) -> &'static TestFixture {
    let fixture = match session {
        Session::Wayland => &TEST_FIXTURE_WAYLAND,
        Session::X11 => &TEST_FIXTURE_X11,
    };
    fixture.get_or_init(|| TestFixture::new(session))
}

/// Sample config content for basic testing
pub const SAMPLE_ROOT_CONFIG: &str = r#"
set $mod Super
set $alt Alt
include bindings.conf
include more_bindings.conf
"#;

pub const SAMPLE_BINDINGS_CONFIG: &str = r#"
bindsym $mod+Enter exec terminal
bindsym $mod+Shift+Q kill
bindsym $mod+D exec dmenu_run
"#;

pub const SAMPLE_MORE_BINDINGS_CONFIG: &str = r#"
bindsym $mod+H split h
bindsym $mod+V split v
bindsym $mod+F fullscreen toggle
"#;

pub const SAMPLE_XRESOURCES: &str = r#"
! This is a comment
regolithwm.border.width: 2
regolithwm.window.titlebar: false
regolithwm.font.size: 12
"#;

/// Creates a mock HashMap of resources for testing
pub fn create_mock_resources() -> HashMap<String, String> {
    let mut resources = HashMap::new();
    resources.insert("mod".to_string(), "Super".to_string());
    resources.insert("alt".to_string(), "Alt".to_string());
    resources.insert("regolithwm.border.width".to_string(), "2".to_string());
    resources.insert("regolithwm.font.size".to_string(), "12".to_string());
    resources
}

/// Creates variables map from resources for testing
pub fn create_mock_variables() -> std::collections::BTreeMap<String, String> {
    let mut variables = std::collections::BTreeMap::new();
    variables.insert("mod".to_string(), "Super".to_string());
    variables.insert("alt".to_string(), "Alt".to_string());
    variables
}
