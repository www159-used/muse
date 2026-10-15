// SPDX-License-Identifier: AGPL-3.0-only

//! Scripting support for the terminal interface.
//!
//! The TUI colon prompt accepts standard Rhai. This module owns everything that
//! belongs to that language: the shared API descriptors that drive both engine
//! registration and completion, the serialized session that keeps submitted
//! definitions alive, and the worker thread that evaluates input off the render
//! loop. The music library and the native playback backend stay independent of
//! scripting; only this application crate depends on Rhai.
//!
//! Classification of submitted input between shipped commands and Rhai
//! scripts lives with the command line it belongs to, in [`crate::command`].

pub mod api;
pub mod completion;
pub mod session;
pub mod worker;

pub use worker::ScriptWorker;

#[cfg(test)]
mod tests {
    /// The scripting boundary: Rhai belongs to this application crate only.
    #[test]
    fn no_other_crate_depends_on_rhai() {
        for manifest in [
            include_str!("../../../muse-library/Cargo.toml"),
            include_str!("../../../muse-playback/Cargo.toml"),
            include_str!("../../../muse-e2e/Cargo.toml"),
        ] {
            assert!(
                !manifest.contains("rhai"),
                "only the application crate may depend on Rhai"
            );
        }
    }
}
