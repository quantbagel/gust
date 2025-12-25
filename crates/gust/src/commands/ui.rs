//! Console output helpers for consistent CLI formatting.

#![allow(dead_code)]

use console::{style, StyledObject};

/// Print a success message with green checkmark.
pub fn success(msg: impl std::fmt::Display) {
    println!("{} {}", style("✓").green().bold(), msg);
}

/// Print an info/action message with blue arrow.
pub fn info(msg: impl std::fmt::Display) {
    println!("{} {}", style("→").blue().bold(), msg);
}

/// Print a warning message with yellow exclamation.
pub fn warn(msg: impl std::fmt::Display) {
    println!("{} {}", style("!").yellow().bold(), msg);
}

/// Print a dim hint message.
pub fn hint(msg: impl std::fmt::Display) {
    println!("{} {}", style("→").dim(), msg);
}

/// Style text as a package/target name (cyan).
pub fn pkg(name: impl std::fmt::Display) -> StyledObject<String> {
    style(name.to_string()).cyan()
}

/// Style text as a version or count (cyan).
pub fn num<T: std::fmt::Display>(n: T) -> StyledObject<String> {
    style(n.to_string()).cyan()
}

/// Style text as dimmed/secondary.
pub fn dim(text: impl std::fmt::Display) -> StyledObject<String> {
    style(text.to_string()).dim()
}

/// Style text as success (green).
pub fn green(text: impl std::fmt::Display) -> StyledObject<String> {
    style(text.to_string()).green()
}

/// Print a section header.
pub fn header(title: impl std::fmt::Display) {
    println!("{}", style(title.to_string()).bold().underlined());
    println!();
}

/// Print a table separator line.
pub fn separator(width: usize) {
    println!("{}", style("─".repeat(width)).dim());
}
