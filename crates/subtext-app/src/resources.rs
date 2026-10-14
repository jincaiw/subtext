//! Locating the bundled syntax definitions, themes and menu table.
//!
//! SubText keeps CotEditor's data files on disk rather than baking them into
//! the binary, so a user can drop a new `.cotsyntax` or `.cottheme` into their
//! own support directory and have it picked up — the same contract CotEditor
//! offers.

use std::path::{Path, PathBuf};

/// Where a resource was found, so the UI can tell the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceOrigin {
    /// Shipped with the build, next to the executable.
    Bundled,
    /// The development tree, resolved from `CARGO_MANIFEST_DIR`.
    Development,
    /// The user's own support directory.
    User,
}

#[derive(Debug, Clone)]
pub struct Resources {
    pub root: PathBuf,
    pub origin: ResourceOrigin,
}

impl Resources {
    /// Find the resource directory.
    ///
    /// The search order is deliberate: a user's own files win, so they can
    /// override a shipped syntax without rebuilding.
    pub fn discover() -> Option<Self> {
        for (path, origin) in Self::candidates() {
            if path.join("syntaxes").is_dir() {
                return Some(Self { root: path, origin });
            }
        }
        None
    }

    fn candidates() -> Vec<(PathBuf, ResourceOrigin)> {
        let mut candidates = Vec::new();

        // 1. Beside the executable, and inside a macOS bundle's Resources.
        if let Ok(executable) = std::env::current_exe() {
            let directory = executable.parent().unwrap_or(Path::new("."));
            candidates.push((directory.join("resources"), ResourceOrigin::Bundled));
            if let Some(contents) = directory.parent() {
                candidates.push((contents.join("Resources"), ResourceOrigin::Bundled));
            }
        }

        // 2. The development tree, so `cargo run` works without a copy step.
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        candidates.push((manifest.join("../../resources"), ResourceOrigin::Development));

        // 3. The user's support directory.
        if let Some(config) = dirs::config_dir() {
            candidates.push((config.join("subtext/resources"), ResourceOrigin::User));
        }

        candidates
    }

    pub fn syntaxes(&self) -> PathBuf {
        self.root.join("syntaxes")
    }

    pub fn themes(&self) -> PathBuf {
        self.root.join("themes")
    }

    /// CotEditor's tree-sitter highlight queries, kept for the languages it
    /// highlights with tree-sitter rather than regex rules.
    pub fn queries(&self) -> PathBuf {
        self.root.join("queries")
    }

    pub fn menu_table(&self) -> PathBuf {
        self.root.join("menu.json")
    }
}

/// The user's settings and key-binding files.
pub fn config_directory() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("subtext")
}

/// Where the user may drop their own syntax definitions and themes.
pub fn user_syntax_directory() -> PathBuf {
    config_directory().join("Syntaxes")
}

pub fn user_theme_directory() -> PathBuf {
    config_directory().join("Themes")
}

/// Where the user's scripts live.
///
/// macOS has a directory for exactly this — the application scripts folder,
/// which is where CotEditor reads from — and it is used there, so a folder of
/// scripts written for CotEditor keeps working once it is copied across. The
/// other two platforms have no such directory, so the scripts live beside the
/// settings, which is the contract the syntax and theme folders already offer.
pub fn script_directory() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        /// CotEditor's folder is keyed by its bundle identifier, which is what
        /// keeps one application's scripts out of another's. SubText is a plain
        /// executable rather than a bundle, so it names itself.
        const BUNDLE_ID: &str = "com.subtext.SubText";

        if let Some(home) = dirs::home_dir() {
            return home
                .join("Library")
                .join("Application Scripts")
                .join(BUNDLE_ID);
        }
    }
    config_directory().join("Scripts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_tree_resolves() {
        let resources = Resources::discover().expect("resources/ must be findable from the source tree");
        assert!(resources.syntaxes().is_dir());
        assert!(resources.themes().is_dir());
    }
}
