//! CLI command implementations.
//!
//! This module contains all the command handlers for the Gust CLI.
//! Commands are organized into submodules by functionality.

pub mod ui;
pub mod version;
mod core;

// Re-export command functions from core
pub use core::{
    new_package, init, build, run, test, clean,
    add, remove, install, update, tree, outdated,
    cache_list, cache_stats, cache_clean, cache_path,
    migrate, info, search,
    swift_list, swift_current, swift_install, swift_use,
    xcode_generate, doctor,
};
