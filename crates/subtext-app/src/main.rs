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
    // program runs. The flags are checked here, ahead of the file paths the
    // library opens for the same argument list.
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument == "--version" || argument == "-V")
    {
        println!("SubText {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // `--diagnose` starts the editor exactly as a desktop launch would, but
    // names the display environment, the system and the log file first, and
    // keeps the window-open failure in the terminal instead of a dialog. It
    // is the command to ask a user for when the icon produces no window.
    if arguments.iter().any(|argument| argument == "--diagnose") {
        subtext_app::diagnose();
        return;
    }
    subtext_app::run();
}
