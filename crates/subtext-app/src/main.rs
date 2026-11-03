//! The SubText binary.
//!
//! Everything lives in the library so the same code can be reached by tests
//! and by the `subtext` command.

// A release build is a GUI application on Windows: without this the binary is
// linked as a console program and every launch from Explorer opens a terminal
// window beside the editor. Debug builds keep the console, which is where
// `env_logger` and panics go on that platform.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    subtext_app::run();
}
