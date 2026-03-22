//! Terminal output formatting utilities.
//!
//! Provides consistent colorization for CLI output with automatic terminal
//! detection. Colors are only applied when outputting to an interactive
//! terminal (respects `NO_COLOR` environment variable and piped output).

use colored::Colorize;

pub fn file_path(s: &str) -> colored::ColoredString {
    s.cyan().bold()
}

pub fn line_number(n: usize) -> colored::ColoredString {
    n.to_string().yellow()
}

pub fn section_header(s: &str) -> colored::ColoredString {
    s.bold()
}

pub fn resource_name(s: &str) -> colored::ColoredString {
    s.cyan()
}

pub fn value_found(s: &str) -> colored::ColoredString {
    s.green()
}

pub fn value_not_found(s: &str) -> colored::ColoredString {
    s.red()
}

pub fn default_value(s: &str) -> colored::ColoredString {
    s.blue()
}

pub fn override_value(s: &str) -> colored::ColoredString {
    s.magenta()
}

pub fn command(s: &str) -> colored::ColoredString {
    s.green()
}

pub fn similar_item(s: &str) -> colored::ColoredString {
    s.yellow()
}

pub fn in_use(s: &str) -> colored::ColoredString {
    s.green().italic()
}

pub fn hint(s: &str) -> colored::ColoredString {
    s.dimmed().italic()
}
