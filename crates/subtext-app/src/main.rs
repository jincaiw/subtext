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
    // `--version` separates "the binary cannot start" from "the window cannot
    // open": a desktop launcher shows neither, a terminal shows both. It is
    // also what the Linux troubleshooting section asks a user to run first,
    // because a missing glibc stops the loader before a single line of this
    // program runs. Documents arrive through the operating system's open-file
    // event, so nothing else reads the argument list.
    if std::env::args().any(|argument| argument == "--version" || argument == "-V") {
        println!("SubText {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    subtext_app::run();
}
