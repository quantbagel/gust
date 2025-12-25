//! CLI command implementations.
//!
//! This module contains all the command handlers for the Gust CLI.
//! Commands are organized into submodules by functionality.

mod core;
pub mod ui;
pub mod version;

// Re-export command functions from core
pub use core::{
    add, build, cache_clean, cache_list, cache_path, cache_stats, clean, doctor, generate, info,
    init, install, migrate, new_package, outdated, remove, run, search, swift_current,
    swift_install, swift_list, swift_use, test, tree, update, xcode_generate,
};
