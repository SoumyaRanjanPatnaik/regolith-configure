//! Test utilities for configuration testing.
//!
//! This module provides helpers for creating test fixtures, mock providers,
//! and sample configurations. Only available when compiled with `#[cfg(test)]`.

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

/// A mock resource provider with configurable resources.
pub struct MockResourceProvider {
    resources: HashMap<String, String>,
}

impl MockResourceProvider {
    /// Creates a new mock provider with the given resources.
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

/// Creates a temporary directory for test files.
pub fn create_temp_config_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

/// Creates a config file in the given directory.
///
/// Parent directories are created if they don't exist.
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

/// Creates an Xresource file in the given directory.
///
/// Alias for [`create_config_file`].
pub fn create_xresources_file(dir: &Path, filename: &str, content: &str) -> PathBuf {
    create_config_file(dir, filename, content)
}

/// Creates a `ConfigPartial` from path and content strings.
pub fn create_config_partial(path: &str, content: &str) -> ConfigPartial {
    ConfigPartial::new(Path::new(path), content)
}

/// Creates a `ConfigPartial` from a file in a temp directory.
///
/// The file is created and its contents are read back.
pub fn create_config_partial_in_dir(dir: &Path, filename: &str, content: &str) -> ConfigPartial {
    let path = create_config_file(dir, filename, content);
    let content = fs::read_to_string(&path).expect("Failed to read config file");
    ConfigPartial::new(&path, &content)
}

/// Creates a `FullConfig` with multiple partials for testing.
///
/// # Arguments
///
/// * `session` - The session type for the config
/// * `root_config_content` - Content for the root config file
/// * `included_configs` - List of (filename, content) pairs for included configs
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

/// Returns session mappings for testing.
///
/// Maps to temporary paths under `/tmp/regolith-test/`.
pub fn get_test_session_mappings() -> Box<SessionMappings> {
    Box::new([
        (
            Session::Wayland,
            Path::new("/tmp/regolith-test/sway/config"),
        ),
        (Session::X11, Path::new("/tmp/regolith-test/i3/config")),
    ])
}

/// A test fixture with a temporary directory and config paths.
pub struct TestFixture {
    /// The temporary directory.
    pub temp_dir: TempDir,
    /// The config directory (e.g., `sway/` or `i3/`).
    pub config_dir: PathBuf,
    /// Path to the root config file.
    pub root_config_path: PathBuf,
}

impl TestFixture {
    /// Creates a new test fixture for the given session.
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

    /// Adds a config file to the fixture's config directory.
    pub fn add_config(&self, filename: &str, content: &str) -> PathBuf {
        create_config_file(&self.config_dir, filename, content)
    }

    /// Adds an Xresources file to the fixture's config directory.
    pub fn add_xresources(&self, filename: &str, content: &str) -> PathBuf {
        create_xresources_file(&self.config_dir, filename, content)
    }

    /// Creates a `FullConfig` from the root config content.
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

/// Gets or initializes a shared test fixture for the given session.
///
/// Fixtures are cached statically for reuse across tests.
pub fn get_or_init_fixture(session: Session) -> &'static TestFixture {
    let fixture = match session {
        Session::Wayland => &TEST_FIXTURE_WAYLAND,
        Session::X11 => &TEST_FIXTURE_X11,
    };
    fixture.get_or_init(|| TestFixture::new(session))
}

/// Sample root config content for basic testing.
pub const SAMPLE_ROOT_CONFIG: &str = r#"
set $mod Super
set $alt Alt
include bindings.conf
include more_bindings.conf
"#;

/// Sample bindings config content for testing.
pub const SAMPLE_BINDINGS_CONFIG: &str = r#"
bindsym $mod+Enter exec terminal
bindsym $mod+Shift+Q kill
bindsym $mod+D exec dmenu_run
"#;

/// Sample additional bindings config content for testing.
pub const SAMPLE_MORE_BINDINGS_CONFIG: &str = r#"
bindsym $mod+H split h
bindsym $mod+V split v
bindsym $mod+F fullscreen toggle
"#;

/// Sample Xresources content for testing.
pub const SAMPLE_XRESOURCES: &str = r#"
! This is a comment
regolithwm.border.width: 2
regolithwm.window.titlebar: false
regolithwm.font.size: 12
"#;

/// Creates a mock resources map for testing.
pub fn create_mock_resources() -> HashMap<String, String> {
    let mut resources = HashMap::new();
    resources.insert("mod".to_string(), "Super".to_string());
    resources.insert("alt".to_string(), "Alt".to_string());
    resources.insert("regolithwm.border.width".to_string(), "2".to_string());
    resources.insert("regolithwm.font.size".to_string(), "12".to_string());
    resources
}

/// Creates a mock variables map for testing.
pub fn create_mock_variables() -> std::collections::BTreeMap<String, String> {
    let mut variables = std::collections::BTreeMap::new();
    variables.insert("mod".to_string(), "Super".to_string());
    variables.insert("alt".to_string(), "Alt".to_string());
    variables
}
