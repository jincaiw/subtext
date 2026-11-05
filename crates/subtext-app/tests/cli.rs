//! The command line, such as it is.
//!
//! `--version` is the first step of the Linux troubleshooting section: a
//! desktop launcher shows neither the program's output nor an early loader
//! failure, so this flag is how a user tells "the binary will not start" from
//! "the window will not open". It must therefore print and exit without ever
//! touching the interface — a regression here would open a window on a machine
//! that is already the subject of a bug report.

use std::process::Command;

fn version_flag(flag: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_subtext"))
        .arg(flag)
        .output()
        .expect("the binary under test runs");
    assert!(
        output.status.success(),
        "{flag} exited with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn version_is_printed_without_starting_the_interface() {
    let expected = format!("SubText {}", env!("CARGO_PKG_VERSION"));
    assert_eq!(version_flag("--version"), expected);
    assert_eq!(version_flag("-V"), expected);
}
