//! CLI argument types and session detection.
//!
//! This module provides the command-line interface types for the
//! regolith-configure tool, including session types, filter options,
//! and operation-specific arguments.

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Output format for search results.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum OutputMode {
    /// Show only values (runtime, default, override)
    Minimal,
    /// Show summary with related resources
    #[default]
    Summary,
    /// Show all details including usage lines
    Full,
}

/// Command-line arguments for regolith-configure.
#[derive(Parser, Debug)]
#[command(name = "rust-config")]
#[command(version = "0.1.0")]
#[command(about = "Help users edit, manage and search for configiurations", long_about = None)]
pub struct CLIArguments {
    /// Optional if $XDG_DESKTOP_PORTAL is defined
    #[arg(short, long, global = true, value_enum)]
    session: Option<Session>,

    /// Show only values (runtime, default, override)
    #[arg(long, global = true, group = "output-mode")]
    minimal: bool,

    /// Show summary with related resources (default)
    #[arg(long, global = true, group = "output-mode")]
    summary: bool,

    /// Show all details including usage lines
    #[arg(long, global = true, group = "output-mode")]
    full: bool,

    #[command(subcommand)]
    sub_command: OperationType,
}

impl CLIArguments {
    /// Returns the explicitly specified session, if any.
    pub fn session(&self) -> Option<Session> {
        self.session
    }

    /// Returns the operation to perform.
    pub fn sub_command(&self) -> &OperationType {
        &self.sub_command
    }

    /// Returns the output mode for search results.
    pub fn output_mode(&self) -> OutputMode {
        if self.minimal {
            OutputMode::Minimal
        } else if self.full {
            OutputMode::Full
        } else {
            OutputMode::Summary
        }
    }
}

/// Available operations for regolith-configure.
#[derive(Subcommand, Debug)]
pub enum OperationType {
    /// Get the deatils for a resource, keybinding, package or config file
    Search(SearchArgs),

    /// Create a copy of a config partial and disable it's system instance
    Eject(EjectArgs),

    /// Help the user diff and reconcile upstream configs with their local versions
    Reconcile { name: String },

    /// Set the value of a resource in ~/.config/regolith3/Xresources
    SetResource(SetResourceArgs),
}

/// Display server session type.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum Session {
    Wayland,
    X11,
}

/// Search filter type for narrowing results.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum FilterType {
    #[value(alias = "bindings")]
    Binding,
    #[value(alias = "keywords")]
    Keyword,
    #[value(alias = "resources")]
    Resource,
}

/// Arguments for the search operation.
#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Define filtering stratergy
    #[arg(value_enum)]
    filter: FilterType,
    pattern: String,
}

impl SearchArgs {
    /// Creates new search arguments with the given pattern and filter.
    pub fn new(pattern: &str, filter: FilterType) -> Self {
        Self {
            filter,
            pattern: pattern.into(),
        }
    }

    /// Returns the filter type for this search.
    pub fn filter(&self) -> FilterType {
        self.filter
    }

    /// Returns the search pattern.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

/// Arguments for the eject operation.
#[derive(Args, Debug)]
pub struct EjectArgs {
    /// Define filtering stratergy
    #[arg(value_enum)]
    filter: FilterType,
    /// File to write to
    #[arg(short, long)]
    output: Option<String>,
    pattern: String,
}

impl EjectArgs {
    /// Returns the filter type for this eject operation.
    pub fn filter(&self) -> FilterType {
        self.filter
    }

    /// Returns the output file path, if specified.
    pub fn output(&self) -> Option<&String> {
        self.output.as_ref()
    }

    /// Returns the pattern to match.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

/// Arguments for the reconcile operation.
#[derive(Args, Debug)]
pub struct ReconcileArgs {}

/// Arguments for the set-resource operation.
#[derive(Args, Debug)]
pub struct SetResourceArgs {
    /// The resource name to set
    resource: String,

    /// The value to assign to the resource
    value: String,
}

impl SetResourceArgs {
    /// Returns the resource name to set.
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Returns the value to assign.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Detects the current session type from the environment.
///
/// Returns `Some(Session)` if `$XDG_SESSION_TYPE` is set to exactly
/// `"wayland"` or `"x11"`. Returns `None` for any other value or if
/// the variable is not set.
pub fn get_session_type() -> Option<Session> {
    std::env::vars().find_map(|(name, value)| match name.as_str() {
        "XDG_SESSION_TYPE" => match value.as_str() {
            "wayland" => Some(Session::Wayland),
            "x11" => Some(Session::X11),
            _ => None,
        },
        _ => None,
    })
}
