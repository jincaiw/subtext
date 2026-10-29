//! The bug report template — CotEditor's `Help ▸ Create Bug Report`.
//!
//! Upstream's `IssueReport` is not a web form: it opens a **new untitled
//! document** holding a Markdown template with the environment already filled
//! in, sets the syntax to Markdown, and leaves the user to write the parts only
//! they know. That is the whole feature, and it is a better answer than a link
//! because the version and system are the two things a report usually gets
//! wrong.
//!
//! The text is built here, with the environment passed in, so the layout can be
//! checked without reading the machine it runs on.

use std::fmt::Write as _;

/// What the report says about the machine it was written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    /// The application version, e.g. `0.3.13`.
    pub version: String,
    /// The operating system and architecture, e.g. `macOS 15.3.1 (aarch64)`.
    pub system: String,
    /// The interface language, or `None` when the process has no locale set.
    pub language: Option<String>,
}

impl Environment {
    /// The environment this process is running in.
    pub fn detect() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            system: system_description(),
            language: language(),
        }
    }

    /// The three lines the template puts under `## Environment`.
    fn lines(&self) -> String {
        let mut text = format!("- SubText: {}\n- System: {}\n", self.version, self.system);
        match &self.language {
            Some(language) => {
                let _ = writeln!(text, "- Language: {language}");
            }
            // Upstream prints an en dash, because a missing locale is a fact
            // about the report and not something to leave out.
            None => text.push_str("- Language: –\n"),
        }
        text
    }
}

/// The bug report, as the document's title and its body.
pub struct IssueReport;

impl IssueReport {
    /// The document title — upstream's `IssueReport.title`.
    pub const TITLE: &'static str = "Issue Report";

    /// The Markdown body, with `environment` filled in.
    ///
    /// Upstream builds this by joining blocks with two and three newlines: the
    /// description and the environment are one block of four paragraphs, and
    /// the headings after it are separated by blank *double* lines so there is
    /// room to type under each.
    pub fn template(environment: &Environment) -> String {
        let description = format!(
            "Fill in the following template, then post it on {link}. \
             Write your message in either English or Chinese.",
            link = Self::ISSUE_LINK
        );
        let first = [
            description,
            "-".repeat(25),
            "## Environment".to_string(),
            environment.lines().trim_end().to_string(),
        ]
        .join("\n\n");

        let rest = [
            "## Short Description",
            "## Steps to Reproduce the Issue",
            "## Expected Result",
        ]
        .join("\n\n\n");

        format!("{first}\n\n\n{rest}\n")
    }

    /// The link the description points at.
    const ISSUE_LINK: &'static str = "[GitHub Issues](https://github.com/jincaiw/subtext/issues)";
}

/// `macOS 15.3.1 (aarch64)`, `Linux (x86_64)`, `Windows (x86_64)`.
///
/// The version comes from the OS on macOS (`sw_vers -productVersion`, which is
/// the same string `ProcessInfo.operatingSystemVersionString` reports); the
/// other platforms only get the architecture, because there is no equivalent
/// call and a wrong guess is worse than a missing number in a bug report.
fn system_description() -> String {
    let arch = std::env::consts::ARCH;
    #[cfg(target_os = "macos")]
    {
        match macos_version() {
            Some(version) => format!("macOS {version} ({arch})"),
            // No `sw_vers` is the case in a sandbox that strips the PATH; the
            // report is still worth opening without the number.
            None => format!("macOS ({arch})"),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let os = match std::env::consts::OS {
            "linux" => "Linux",
            "windows" => "Windows",
            other => other,
        };
        format!("{os} ({arch})")
    }
}

#[cfg(target_os = "macos")]
fn macos_version() -> Option<String> {
    let output = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

/// The interface language.
///
/// Upstream asks the system (`Locale.current`) and prints the language's English
/// name — `Chinese`. Two things that gets wrong here: a GUI process on macOS
/// inherits no `LANG` (so the environment alone answers `–` for almost
/// everyone), and turning `zh-Hans_US` into `Chinese` needs Foundation's
/// `localizedString(forLanguageCode:)`, which a table in this repository would
/// only approximate.
///
/// So: the system locale first, the environment second, and the identifier as
/// the system spells it. A maintainer reads `zh-Hans_US` without the table.
fn language() -> Option<String> {
    #[cfg(target_os = "macos")]
    if let Some(locale) = macos_locale() {
        return Some(locale);
    }
    let raw = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .ok()?;
    let language = raw.split('.').next()?.trim();
    if language.is_empty() || language == "C" || language == "POSIX" {
        return None;
    }
    Some(language.to_string())
}

/// `defaults read -g AppleLocale` — `zh-Hans_US`, or `en_US@rg=gbzzzz`, whose
/// region-part suffix is not part of the locale a report wants.
#[cfg(target_os = "macos")]
fn macos_locale() -> Option<String> {
    let output = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let locale = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let locale = locale.split('@').next()?.trim().to_string();
    (!locale.is_empty()).then_some(locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment() -> Environment {
        Environment {
            version: "0.3.13".to_string(),
            system: "macOS 15.3.1 (aarch64)".to_string(),
            language: Some("en_US".to_string()),
        }
    }

    #[test]
    fn the_template_opens_with_the_description_and_the_environment() {
        let text = IssueReport::template(&environment());
        let head: Vec<&str> = text.lines().take(11).collect();
        assert!(
            head[0].starts_with("Fill in the following template"),
            "{head:?}"
        );
        assert!(
            head[0].contains("github.com/jincaiw/subtext/issues"),
            "{head:?}"
        );
        // Paragraph, blank, the rule.
        assert_eq!(head[2], "-".repeat(25), "{head:?}");
        assert_eq!(head[4], "## Environment", "{head:?}");
        assert_eq!(head[6], "- SubText: 0.3.13", "{head:?}");
        assert_eq!(head[7], "- System: macOS 15.3.1 (aarch64)", "{head:?}");
        assert_eq!(head[8], "- Language: en_US", "{head:?}");
    }

    #[test]
    fn every_heading_after_the_environment_has_room_to_type_under_it() {
        let text = IssueReport::template(&environment());
        for heading in [
            "## Short Description",
            "## Steps to Reproduce the Issue",
            "## Expected Result",
        ] {
            assert!(text.contains(heading), "{heading} is missing");
        }
        // Upstream joins those headings with *two* blank lines, so there are
        // three newlines between them.
        assert!(
            text.contains("## Short Description\n\n\n## Steps"),
            "{text}"
        );
        assert!(text.ends_with("## Expected Result\n"), "{text}");
    }

    #[test]
    fn a_missing_locale_prints_an_en_dash_rather_than_nothing() {
        let mut environment = environment();
        environment.language = None;
        let text = IssueReport::template(&environment);
        assert!(text.contains("- Language: –"), "{text}");
    }

    #[test]
    fn the_environment_sits_under_its_heading_with_no_trailing_blank_line() {
        let text = IssueReport::template(&environment());
        // Two newlines after the last environment line, then three before the
        // next heading: a blank line and a gap.
        assert!(
            text.contains("- Language: en_US\n\n\n## Short Description"),
            "{text}"
        );
    }

    #[test]
    fn the_system_line_names_the_platform_and_the_architecture() {
        let system = system_description();
        assert!(system.contains(std::env::consts::ARCH), "{system}");
        assert!(
            system.starts_with("macOS")
                || system.starts_with("Linux")
                || system.starts_with("Windows"),
            "{system}"
        );
    }
}
