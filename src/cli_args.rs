use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "rust-config")]
#[command(version = "0.1.0")]
#[command(about = "Help users edit, manage and search for configiurations", long_about = None)]
pub struct CLIArguments {
    /// Optional if $XDG_DESKTOP_PORTAL is defined
    #[arg(short, long, value_enum)]
    session: Option<Session>,

    #[command(subcommand)]
    sub_command: OperationType,
}

impl CLIArguments {
    pub fn session(&self) -> Option<Session> {
        self.session
    }

    pub fn sub_command(&self) -> &OperationType {
        &self.sub_command
    }
}

#[derive(Subcommand, Debug)]
pub enum OperationType {
    /// Get the deatils for a resource, keybinding, package or config file
    Search(SearchArgs),

    /// Create a copy of a config partial and disable it's system instance
    Eject(EjectArgs),

    /// Help the user diff and reconcile upstream configs with their local versions
    Reconcile { name: String },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum Session {
    Wayland,
    X11,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum FilterType {
    Package,
    Bindings,
    Keyword,
    Resource,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Define filtering stratergy
    #[arg(short, long, value_enum)]
    filter: FilterType,
    pattern: String,
}

impl SearchArgs {
    pub fn filter(&self) -> FilterType {
        self.filter
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

#[derive(Args, Debug)]
pub struct EjectArgs {
    /// Define filtering stratergy
    #[arg(short, long, value_enum)]
    filter: FilterType,
    /// File to write to
    #[arg(short, long, value_enum)]
    output: Option<String>,
    pattern: String,
}

impl EjectArgs {
    pub fn filter(&self) -> FilterType {
        self.filter
    }

    pub fn output(&self) -> Option<&String> {
        self.output.as_ref()
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

#[derive(Args, Debug)]
pub struct ReconcileArgs {}
