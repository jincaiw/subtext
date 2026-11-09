//! AppleScript string literals.
//!
//! Two places hand text to `osascript`: the startup failure alert
//! ([`crate::startup`]) and the pasteboard writer in [`crate::workspace`].
//! Both need the same escaping, and the rule is AppleScript's, so it lives
//! here rather than beside either caller.
//!
//! The module is compiled for macOS and for tests only: those are the only
//! configurations that have an `osascript` to talk to.

/// `text` escaped for the inside of an AppleScript string literal.
///
/// Only the backslash and the double quote end a literal early. A real line
/// break is a legal character inside one, which is why there is no `\n` case.
pub(crate) fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_backslashes_are_escaped() {
        assert_eq!(
            escape("a \"quoted\" path C:\\tmp"),
            "a \\\"quoted\\\" path C:\\\\tmp"
        );
    }

    #[test]
    fn a_line_break_stays_a_line_break() {
        assert_eq!(escape("one\ntwo"), "one\ntwo");
    }
}
