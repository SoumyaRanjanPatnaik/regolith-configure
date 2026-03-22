use std::{collections::BTreeMap, collections::HashMap, io::Write, sync::OnceLock};

use crate::cli_args::Session;
use crate::test_utils::{
    create_config_file, create_config_partial, create_config_partial_in_dir, create_mock_resources,
    create_mock_variables, create_temp_config_dir, create_test_full_config, MockResourceProvider,
    TestFixture, SAMPLE_BINDINGS_CONFIG, SAMPLE_MORE_BINDINGS_CONFIG, SAMPLE_ROOT_CONFIG,
};
use crate::{FullConfig, SessionMappings};

use std::fs::File;
use std::path::{Path, PathBuf};

// ========================================================================
// ConfigPartial::new
// ========================================================================

#[test]
fn test_config_partial_new_is_functional_for_imports() {
    let dir = create_temp_config_dir();
    create_config_file(dir.path(), "included.conf", "some content");
    let partial = create_config_partial_in_dir(dir.path(), "main.conf", "include included.conf");
    let paths = partial.get_imported_paths().unwrap();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("included.conf"));
}

// ========================================================================
// ConfigPartial::get_imported_paths
// ========================================================================

#[test]
fn test_get_imported_paths_single_relative_include() {
    let dir = create_temp_config_dir();
    create_config_file(dir.path(), "bindings.conf", "bindsym $mod+X kill");
    let partial = create_config_partial_in_dir(dir.path(), "config.conf", "include bindings.conf");
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

// ========================================================================
// ConfigPartial::config_variables
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
fn test_config_variables_set_from_resource() {
    let cases = [
        ("set_from_resource $mod mod mod_default", "Super"),
        ("set_from_resource $border wm.border.width 3", "3"),
    ];
    for (config_line, expected_value) in cases {
        let partial = create_config_partial("/tmp/config.conf", config_line);
        let resources = if expected_value == "Super" {
            create_mock_resources()
        } else {
            HashMap::new()
        };
        let vars: Vec<_> = partial.config_variables(&resources).collect();
        assert_eq!(vars.len(), 1, "Failed for: {}", config_line);
        assert_eq!(vars[0].1, expected_value, "Failed for: {}", config_line);
    }
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
fn test_config_variables_skips_irrelevant_lines() {
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
fn test_config_variables_whitespace_and_dollar_prefix() {
    let partial = create_config_partial(
        "/tmp/config.conf",
        "  set   $mod   Super\nset mod Super\nset_from_resource\t$alt\twm.alt\tAlt_default",
    );
    let resources = HashMap::new();
    let vars: Vec<_> = partial.config_variables(&resources).collect();
    assert_eq!(vars.len(), 3);
    assert_eq!(vars[0].0, "mod");
    assert_eq!(vars[1].0, "mod");
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
// ConfigPartial::config_bindings
// ========================================================================

#[test]
fn test_config_bindings_bindsym_and_bindcode() {
    let cases = [
        ("bindsym Mod4+Return exec terminal", "Mod4+Return", 1),
        ("bindcode 36+Return exec terminal", "36+Return", 1),
    ];
    for (config_line, expected_orig, expected_line_no) in cases {
        let partial = create_config_partial("/tmp/config.conf", config_line);
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1, "Failed for: {}", config_line);
        assert_eq!(bindings[0].orig_binding, expected_orig);
        assert_eq!(bindings[0].line_no, expected_line_no);
    }
}

#[test]
fn test_config_bindings_variable_resolution() {
    let cases = [
        ("$mod+Return", "Super+Return"),
        ("Shift+Return", "Shift+Return"),
        ("$mod+$alt+Return", "Super+Alt+Return"),
        ("$unknown+Return", "$unknown+Return"),
    ];
    for (binding, expected_normalized) in cases {
        let content = format!("bindsym {} exec terminal", binding);
        let partial = create_config_partial("/tmp/config.conf", &content);
        let variables = create_mock_variables();
        let bindings: Vec<_> = partial.config_bindings(&variables).collect();
        assert_eq!(bindings.len(), 1, "Failed for: {}", binding);
        assert_eq!(bindings[0].orig_binding, binding);
        assert_eq!(bindings[0].normalized_binding.as_ref(), expected_normalized);
    }
}

#[test]
fn test_config_bindings_with_options() {
    let partial = create_config_partial(
        "/tmp/config.conf",
        "bindsym --release --whole-window $mod+Button1 floating resize",
    );
    let variables = create_mock_variables();
    let bindings: Vec<_> = partial.config_bindings(&variables).collect();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].orig_binding, "$mod+Button1");
    assert_eq!(bindings[0].normalized_binding.as_ref(), "Super+Button1");
}

#[test]
fn test_config_bindings_multiple_bindings_and_line_numbers() {
    let partial = create_config_partial(
        "/tmp/config.conf",
        "set $mod Super\nbindsym $mod+Return exec terminal\nbindsym $mod+Q kill\nbindsym $mod+D exec dmenu_run",
    );
    let variables = create_mock_variables();
    let bindings: Vec<_> = partial.config_bindings(&variables).collect();
    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[0].orig_binding, "$mod+Return");
    assert_eq!(bindings[0].line_no, 2);
    assert_eq!(bindings[1].orig_binding, "$mod+Q");
    assert_eq!(bindings[1].line_no, 3);
    assert_eq!(bindings[2].orig_binding, "$mod+D");
    assert_eq!(bindings[2].line_no, 4);
}

#[test]
fn test_config_bindings_line_contents_preserved() {
    let partial = create_config_partial("/tmp/config.conf", "bindsym $mod+Return exec terminal");
    let variables = create_mock_variables();
    let bindings: Vec<_> = partial.config_bindings(&variables).collect();
    assert_eq!(
        bindings[0].line_contents,
        "bindsym $mod+Return exec terminal"
    );
}

// ========================================================================
// normalize_binding
// ========================================================================

#[test]
fn test_normalize_binding_cases() {
    let variables = create_mock_variables();
    let cases = [
        ("$mod+Return", "Super+Return"),
        ("$mod+$alt+Return", "Super+Alt+Return"),
        ("Shift+Return", "Shift+Return"),
        ("$unknown+Return", "$unknown+Return"),
        ("$mod", "Super"),
        ("Shift+$mod+Return", "Shift+Super+Return"),
        ("  $mod+Return  ", "Super+Return"),
        ("", ""),
        ("$", "$"),
        ("$mod+$alt+$mod", "Super+Alt+Super"),
    ];
    for (input, expected) in cases {
        let result = crate::search::bindings::normalize_binding(input, &variables);
        assert_eq!(result.as_ref(), expected, "Failed for input: {:?}", input);
    }
}

#[test]
fn test_normalize_binding_empty_variables_map() {
    let variables = BTreeMap::new();
    let result = crate::search::bindings::normalize_binding("$mod+Return", &variables);
    assert_eq!(result.as_ref(), "$mod+Return");
}

#[test]
fn test_normalize_binding_chained_variables() {
    let mut variables = BTreeMap::new();
    variables.insert("mod".to_string(), "$wm_mod".to_string());
    variables.insert("wm_mod".to_string(), "Super".to_string());
    let result = crate::search::bindings::normalize_binding("$mod+Return", &variables);
    assert_eq!(result.as_ref(), "Super+Return");
}

// ========================================================================
// search_binding_result
// ========================================================================

#[test]
fn test_search_binding_result_exact_match() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set $mod Super\ninclude bindings.conf",
        &[("bindings.conf", "bindsym $mod+Enter exec terminal")],
    );
    let resources = create_mock_resources();
    let result =
        crate::search::bindings::search_binding_result("Super+Enter", &full_config, &resources);
    assert!(!result.0.is_empty());
    assert!(result
        .0
        .iter()
        .any(|b| b.line_contents.contains("exec terminal")));
}

#[test]
fn test_search_binding_result_substring_and_case_insensitive() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set $mod Super\ninclude bindings.conf",
        &[("bindings.conf", "bindsym $mod+Shift+Q kill")],
    );
    let resources = create_mock_resources();
    let result_sub =
        crate::search::bindings::search_binding_result("Shift", &full_config, &resources);
    assert!(!result_sub.0.is_empty());

    let full_config_ci = create_test_full_config(
        Session::Wayland,
        "set $mod Super\ninclude bindings.conf",
        &[("bindings.conf", "bindsym $mod+return exec terminal")],
    );
    let result_ci =
        crate::search::bindings::search_binding_result("super+return", &full_config_ci, &resources);
    assert!(!result_ci.0.is_empty());
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
    let result =
        crate::search::bindings::search_binding_result("Super+D", &full_config, &resources);
    assert!(!result.0.is_empty());
    assert!(result
        .0
        .iter()
        .any(|b| b.line_contents.contains("dmenu_run")));
}

#[test]
fn test_search_binding_result_variable_resolution() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set $mod Super\nset $alt Alt\ninclude bindings.conf",
        &[("bindings.conf", "bindsym $mod+$alt+F fullscreen toggle")],
    );
    let resources = create_mock_resources();
    let result =
        crate::search::bindings::search_binding_result("Super+Alt+F", &full_config, &resources);
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
        crate::search::bindings::search_binding_result("Control+Delete", &full_config, &resources);
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
    let result = crate::search::bindings::search_binding_result("Super", &full_config, &resources);
    assert!(result.0.len() >= 2);
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
        crate::search::bindings::search_binding_result("Super+Enter", &full_config, &resources);
    assert!(!result.0.is_empty());
}

// ========================================================================
// FullConfig::new_from_session
// ========================================================================

#[test]
fn test_new_from_session_valid_configs() {
    for session in [Session::Wayland, Session::X11] {
        let fixture = TestFixture::new(session);
        let root_path: &'static Path =
            Box::leak(fixture.root_config_path.clone().into_boxed_path());
        let mappings: &SessionMappings =
            &[(Session::Wayland, root_path), (Session::X11, root_path)];

        fixture.add_config("bindings.conf", "bindsym $mod+Enter exec terminal");

        let mut file = File::create(&fixture.root_config_path).unwrap();
        file.write_all(b"set $mod Super\ninclude bindings.conf")
            .unwrap();

        let full_config = FullConfig::new_from_session(session, mappings).expect("Should succeed");
        assert_eq!(full_config.partials.len(), 2);
    }
}

#[test]
fn test_new_from_session_discovers_all_includes() {
    let fixture = TestFixture::new(Session::Wayland);
    let root_path: &'static Path = Box::leak(fixture.root_config_path.clone().into_boxed_path());
    let mappings: &SessionMappings = &[(Session::Wayland, root_path), (Session::X11, root_path)];

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
    let root_path: &'static Path = Box::leak(fixture.root_config_path.clone().into_boxed_path());
    let mappings: &SessionMappings = &[(Session::Wayland, root_path), (Session::X11, root_path)];

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
    let root_path: &'static Path = Box::leak(fixture.root_config_path.clone().into_boxed_path());
    let mappings: &SessionMappings = &[(Session::Wayland, root_path), (Session::X11, root_path)];

    let mut file = File::create(&fixture.root_config_path).unwrap();
    file.write_all(b"include nonexistent.conf").unwrap();

    let full_config =
        FullConfig::new_from_session(Session::Wayland, mappings).expect("Should succeed");
    assert_eq!(full_config.partials.len(), 1);
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
// search_keyword_result
// ========================================================================

#[test]
fn test_search_keyword_single_line_match() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set $mod Super\nbindsym $mod+Enter exec terminal",
        &[],
    );
    let result = crate::search::keyword::search_keyword_result("terminal", &full_config);
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
    let result = crate::search::keyword::search_keyword_result("terminal", &full_config);
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
    let result = crate::search::keyword::search_keyword_result("exec", &full_config);
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
    let result_upper = crate::search::keyword::search_keyword_result("TERMINAL", &full_config);
    let result_lower = crate::search::keyword::search_keyword_result("terminal", &full_config);
    let result_mixed = crate::search::keyword::search_keyword_result("TeRmInAl", &full_config);
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
    let result = crate::search::keyword::search_keyword_result("nonexistent", &full_config);
    assert!(result.0.is_empty());
}

#[test]
fn test_search_keyword_line_number_and_file_path() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set $mod Super\nbindsym $mod+Enter exec terminal\nbindsym $mod+D exec dmenu_run",
        &[],
    );
    let result = crate::search::keyword::search_keyword_result("dmenu_run", &full_config);
    assert_eq!(result.0.len(), 1);
    assert_eq!(result.0[0].line_number, 3);

    let full_config2 = create_test_full_config(
        Session::Wayland,
        "include bindings.conf",
        &[("bindings.conf", "bindsym $mod+Enter exec terminal")],
    );
    let result2 = crate::search::keyword::search_keyword_result("terminal", &full_config2);
    assert_eq!(result2.0.len(), 1);
    assert!(result2.0[0].file_path.ends_with("bindings.conf"));
}

// ========================================================================
// get_session_type
// ========================================================================

#[test]
fn test_get_session_type_valid_values() {
    for (env_val, expected) in [
        ("wayland", Some(Session::Wayland)),
        ("x11", Some(Session::X11)),
    ] {
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
            std::env::set_var("XDG_SESSION_TYPE", env_val);
        }
        assert_eq!(
            crate::get_session_type(),
            expected,
            "Failed for: {}",
            env_val
        );
    }
}

#[test]
fn test_get_session_type_invalid_values() {
    for env_val in ["", "mir", "Wayland", "X11"] {
        unsafe {
            std::env::remove_var("XDG_SESSION_TYPE");
            if !env_val.is_empty() {
                std::env::set_var("XDG_SESSION_TYPE", env_val);
            }
        }
        assert_eq!(
            crate::get_session_type(),
            None,
            "Expected None for: {:?}",
            env_val
        );
    }
}

// ========================================================================
// search_resource_result
// ========================================================================

#[test]
fn test_search_resource_exact_match() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set_from_resource $border regolithwm.border.width 3",
        &[],
    );
    let provider = MockResourceProvider::default();
    let result = crate::search::resource::search_resource_result(
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
    let result = crate::search::resource::search_resource_result(
        "regolithwm.border.width",
        &full_config,
        &provider,
    );
    assert!(result.has_exact_match);
    assert_eq!(result.runtime_value, Some("5".to_string()));
    assert_eq!(result.default_value, Some("3".to_string()));
}

#[test]
fn test_search_resource_substring_and_fuzzy_match() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set_from_resource $border regolithwm.border.width 3\nset_from_resource $font regolithwm.font.size 12",
        &[],
    );
    let provider = MockResourceProvider::default();

    // Substring match
    let result_sub = crate::search::resource::search_resource_result(
        "regolithwm.border",
        &full_config,
        &provider,
    );
    assert!(!result_sub.has_exact_match);
    assert!(result_sub
        .matched_resources
        .iter()
        .any(|r| r == "regolithwm.border.width"));

    // Fuzzy match (typo)
    let result_fuzzy = crate::search::resource::search_resource_result(
        "regolithwm.border.heigth",
        &full_config,
        &provider,
    );
    assert!(!result_fuzzy.has_exact_match);
    assert!(result_fuzzy
        .similar_resources
        .iter()
        .any(|r| r == "regolithwm.border.width"));
}

#[test]
fn test_search_resource_no_matches() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set_from_resource $border regolithwm.border.width 3",
        &[],
    );
    let provider = MockResourceProvider::default();
    let result = crate::search::resource::search_resource_result(
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
fn test_search_resource_case_insensitive() {
    let full_config = create_test_full_config(
        Session::Wayland,
        "set_from_resource $border regolithwm.border.width 3",
        &[],
    );
    let provider = MockResourceProvider::default();
    let result = crate::search::resource::search_resource_result(
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
    let result = crate::search::resource::search_resource_result(
        "regolithwm.border.width",
        &full_config,
        &provider,
    );
    assert!(result.has_exact_match);
    assert!(result.usages.len() >= 2);
}

#[test]
fn test_search_resource_special_characters_and_runtime_only() {
    // Special characters in name
    let full_config = create_test_full_config(
        Session::Wayland,
        "set_from_resource $font regolithwm.font.name \"Source Code Pro\"",
        &[],
    );
    let provider = MockResourceProvider::default();
    let result = crate::search::resource::search_resource_result(
        "regolithwm.font.name",
        &full_config,
        &provider,
    );
    assert!(result.has_exact_match);
    assert_eq!(
        result.default_value,
        Some("\"Source Code Pro\"".to_string())
    );

    // Runtime only (no config set_from_resource)
    let full_config2 = create_test_full_config(Session::Wayland, "set $mod Super", &[]);
    let mut resources = HashMap::new();
    resources.insert("regolithwm.border.width".to_string(), "2".to_string());
    let provider2 = MockResourceProvider::new(resources);
    let result2 = crate::search::resource::search_resource_result(
        "regolithwm.border.width",
        &full_config2,
        &provider2,
    );
    assert!(result2.has_exact_match);
    assert_eq!(result2.runtime_value, Some("2".to_string()));
    assert!(result2.default_value.is_none());
}

// ========================================================================
// Integration: FullConfig session setup
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
fn get_mock_configurations(root_config_path: &Path, config_dir: &Path) -> Vec<(PathBuf, String)> {
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
    let config_partials = setup_config_partials(get_mock_configurations, &get_session_mappings());

    let full_config = FullConfig::new_from_session(Session::Wayland, &get_session_mappings())
        .expect("Failed to create FullConfig from session");

    for (path, content) in config_partials {
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
