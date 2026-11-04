//! Looking for a newer release.
//!
//! CotEditor ships [Sparkle]: it installs a *Check for Updates…* item at index 1
//! of the application menu at runtime (`UpdaterManager.swift`), asks its appcast
//! daily unless the user says otherwise, and reads two switches out of the
//! General pane (`SUEnableAutomaticChecks`, `SUAllowsAutomaticUpdates`). SubText
//! has the same two switches in `GeneralOptions` — `enable_automatic_update_checks`
//! and `checks_updates_for_beta` — and this module is the rest of that behaviour,
//! reading GitHub Releases instead of an appcast because that is where this
//! project's releases live.
//!
//! What it does *not* do is install the download. Sparkle replaces the
//! application bundle in place; SubText is unsigned and ships as a plain
//! `.app`/archive, so a half-written bundle is a bricked install with no way to
//! tell the user what happened. The dialog therefore hands the correct archive
//! for this platform to the browser — one click from the update — rather than
//! performing an update it cannot promise to finish. This paragraph is the
//! honest boundary; do not turn it into a silent download.
//!
//! Everything that decides an answer — the version comparison, which asset a
//! platform wants, when a check is due — is a pure function here and has a unit
//! test. Only the request itself is I/O, and it is reached through the
//! application's own HTTP client so tests and the platform agree.
//!
//! [Sparkle]: https://sparkle-project.org

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::http_client::HttpClient;
use gpui_kit::http_client::github::{GithubRelease, latest_github_release};
use gpui_kit::{App, BorrowAppContext as _, Global};

/// Where the releases are, as `owner/repository` for the API and as a page for
/// a reader. `dispatch.rs` opens the same page for *What's New*.
pub const REPOSITORY: &str = "jincaiw/subtext";
pub const RELEASES_URL: &str = "https://github.com/jincaiw/subtext/releases";

/// How long an automatic check waits after the last one.
///
/// Sparkle's `SUScheduledCheckInterval` default is one day, and the General
/// pane offers no way to change it here either. A manual check ignores this.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// The version this build reports.
///
/// The workspace version from `Cargo.toml`, which is what the release tag is
/// built from — the two cannot drift without failing the release.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version {
        major: 0,
        minor: 0,
        patch: 0,
    })
}

/// A dotted version, as a release tag or a crate version writes it.
///
/// Prerelease suffixes (`0.5.0-beta.1`) are not part of the comparison: the
/// channel is [`GithubRelease::pre_release`], which is what the beta switch
/// turns on, and `0.5.0-beta.1` is a newer release than `0.4.1` for the same
/// reason `0.5.0` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Read `v1.2.3`, `1.2.3` or `1.2.3-beta.1`; `None` for anything else.
    ///
    /// A tag that does not parse is not an update. Reporting it as one would
    /// put a dialog in front of the user for a release this build cannot reason
    /// about, so the check skips it instead.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text
            .strip_prefix('v')
            .or_else(|| text.strip_prefix('V'))
            .unwrap_or(text);
        let core = text.split(['-', '+']).next().unwrap_or_default();
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        // A two-component version (`1.2`) is not what this project tags, and
        // guessing the patch would compare it against the wrong release.
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    pub fn is_newer_than(self, other: Self) -> bool {
        self > other
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A release worth telling the user about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    /// The version, without the tag's `v`.
    pub version: Version,
    /// The tag's own spelling, for messages that quote it.
    pub tag: String,
    /// The release page, which is where the notes are.
    pub page_url: String,
    /// The archive for this platform, or the release page when the release has
    /// no asset this platform can use.
    pub download_url: String,
    /// Whether GitHub marks the release as a prerelease.
    pub prerelease: bool,
}

impl Update {
    /// Turn a release into an update, unless this build is already at least as
    /// new as it is.
    fn new(release: GithubRelease) -> Option<Self> {
        let version = Version::parse(&release.tag_name)?;
        if !version.is_newer_than(current_version()) {
            return None;
        }
        let page_url = format!("{RELEASES_URL}/tag/{}", release.tag_name);
        let download_url = release
            .assets
            .iter()
            .find(|asset| asset.name.ends_with(platform_asset_suffix()))
            .map(|asset| asset.browser_download_url.clone())
            .unwrap_or_else(|| page_url.clone());
        Some(Self {
            version,
            tag: release.tag_name,
            page_url,
            download_url,
            prerelease: release.pre_release,
        })
    }
}

/// The suffix of the archive this build knows how to install.
///
/// `tools/package-macos.sh` names the macOS zip after `uname -m`, and the two
/// Linux and Windows jobs name theirs after the runner. Keep this in step with
/// `.github/workflows/release-other-platforms.yml` and the packaging scripts;
/// a suffix nothing matches is not fatal — the dialog falls back to the release
/// page — but it is a worse update.
fn platform_asset_suffix() -> &'static str {
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "-arm64.zip"
        } else {
            "-x86_64.zip"
        }
    } else if cfg!(target_os = "windows") {
        "-windows-x86_64.zip"
    } else if cfg!(target_arch = "aarch64") {
        "-linux-aarch64.tar.gz"
    } else {
        "-linux-x86_64.tar.gz"
    }
}

/// Ask GitHub for the newest release this build's channel allows.
///
/// The stable channel asks for the newest non-prerelease. The beta channel asks
/// for both and keeps the newer, because a prerelease older than the current
/// stable must not hide a newer stable, and vice versa. `Ok(None)` means the
/// request worked and this build is current; `Err` is a network or API failure
/// and is only reported when the user asked for the check.
pub async fn check(
    http: Arc<dyn HttpClient>,
    allow_prerelease: bool,
) -> anyhow::Result<Option<Update>> {
    // `require_assets = false`: a release whose assets are still uploading is
    // still a release, and the page is a usable fallback target.
    let stable = latest_github_release(REPOSITORY, false, false, http.clone()).await;
    let beta = if allow_prerelease {
        latest_github_release(REPOSITORY, false, true, http)
            .await
            .ok()
    } else {
        None
    };

    // Both channels empty is only an error when neither request answered: a
    // beta channel with no prerelease is normal, and `latest_github_release`
    // reports it as an error.
    if stable.is_err() && beta.is_none() {
        return Err(stable.expect_err("checked above"));
    }

    let update = [stable.ok(), beta]
        .into_iter()
        .flatten()
        .filter_map(Update::new)
        .max_by_key(|update| update.version);

    Ok(update)
}

/// When the last check finished, in seconds since the Unix epoch.
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Whether an automatic check is due, given the recorded time of the last one.
///
/// A clock that has gone backwards — an NTP correction, a hand-set date —
/// cannot postpone the next check forever: a recorded time in the future is
/// treated as "not due" only for the interval itself, never longer. `None`
/// (never checked) is always due.
pub fn check_is_due(last_check: Option<u64>, now: u64) -> bool {
    let Some(last_check) = last_check else {
        return true;
    };
    match now.checked_sub(last_check) {
        Some(elapsed) => elapsed >= CHECK_INTERVAL.as_secs(),
        // The recorded time is in the future. Treat the interval as starting
        // now rather than as never having checked.
        None => false,
    }
}

/// The application-wide "a check is running" flag.
///
/// A `Global` rather than a field on the workspace because a check belongs to
/// the application: two document windows at launch must not both pop an update
/// dialog. The start time is kept so a workspace that goes away mid-check
/// cannot wedge the flag — after [`IN_FLIGHT_STALE`] the next check starts
/// anyway.
#[derive(Default)]
struct UpdateChecks {
    started: Option<Instant>,
}

impl Global for UpdateChecks {}

/// How long a check may run before the next one is allowed to start.
///
/// The request itself times out after ten seconds in `gpui-pre-http-client`;
/// this is the generous bound that also covers a workspace being closed while
/// its callback is queued.
const IN_FLIGHT_STALE: Duration = Duration::from_secs(120);

/// Whether a check is already running somewhere in this application.
pub fn is_checking(cx: &mut App) -> bool {
    if !cx.has_global::<UpdateChecks>() {
        cx.set_global(UpdateChecks::default());
    }
    cx.update_global::<UpdateChecks, _>(|state, _| {
        state
            .started
            .is_some_and(|started| started.elapsed() < IN_FLIGHT_STALE)
    })
}

/// Mark a check as running. Call [`finish`] when it answers.
pub fn begin(cx: &mut App) {
    if !cx.has_global::<UpdateChecks>() {
        cx.set_global(UpdateChecks::default());
    }
    cx.update_global::<UpdateChecks, _>(|state, _| state.started = Some(Instant::now()));
}

/// Clear the running flag.
pub fn finish(cx: &mut App) {
    if !cx.has_global::<UpdateChecks>() {
        return;
    }
    cx.update_global::<UpdateChecks, _>(|state, _| state.started = None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::http_client::github::GithubReleaseAsset;

    fn release(tag: &str, prerelease: bool) -> GithubRelease {
        GithubRelease {
            tag_name: tag.to_owned(),
            pre_release: prerelease,
            assets: Vec::new(),
            tarball_url: String::new(),
            zipball_url: String::new(),
        }
    }

    fn asset(name: &str) -> GithubReleaseAsset {
        GithubReleaseAsset {
            name: name.to_owned(),
            browser_download_url: format!("https://example.test/{name}"),
            digest: None,
        }
    }

    /// A tag's own spelling, a missing `v`, and a prerelease suffix all parse;
    /// a version with a component missing does not.
    #[test]
    fn versions_parse_from_release_tags() {
        assert_eq!(
            Version::parse("v0.4.1"),
            Some(Version {
                major: 0,
                minor: 4,
                patch: 1
            })
        );
        assert_eq!(Version::parse("12.30.4"), Version::parse("v12.30.4"));
        assert_eq!(Version::parse("v0.5.0-beta.1"), Version::parse("0.5.0"));
        // A two-part version is not this project's scheme, and guessing the
        // patch would compare it against the wrong release.
        assert_eq!(Version::parse("v1.2"), None);
        assert_eq!(Version::parse("v1.2.3.4"), None);
        assert_eq!(Version::parse("latest"), None);
        assert_eq!(Version::parse(""), None);
    }

    /// Versions compare component by component, not as strings: `0.10.0` is
    /// newer than `0.9.9`, which a string compare would get backwards.
    #[test]
    fn versions_compare_numerically() {
        let older = Version::parse("0.9.9").expect("a version");
        let newer = Version::parse("0.10.0").expect("a version");
        assert!(newer.is_newer_than(older));
        assert!(!older.is_newer_than(newer));
        assert!(!newer.is_newer_than(newer));
    }

    /// The dialog must not open for the version already running.
    #[test]
    fn the_current_version_is_not_an_update() {
        let current = current_version();
        assert_eq!(Update::new(release(&format!("v{current}"), false)), None);
        assert_eq!(Update::new(release("v0.0.1", false)), None);
        assert!(Update::new(release("v9999.0.0", false)).is_some());
    }

    /// The update names the archive this platform can open, and falls back to
    /// the release page when the release has nothing that fits.
    #[test]
    fn the_update_picks_this_platforms_archive() {
        let mut github = release("v9.9.9", false);
        github.assets = vec![
            asset("SubText-9.9.9-arm64.zip"),
            asset("SubText-9.9.9-linux-aarch64.tar.gz"),
            asset("SubText-9.9.9-linux-x86_64.tar.gz"),
            asset("SubText-9.9.9-windows-x86_64.zip"),
        ];
        let update = Update::new(github).expect("a newer version");

        let wanted = format!("SubText-9.9.9{}", platform_asset_suffix());
        assert_eq!(
            update.download_url,
            format!("https://example.test/{wanted}")
        );
        assert_eq!(
            update.page_url,
            "https://github.com/jincaiw/subtext/releases/tag/v9.9.9"
        );
        assert!(!update.prerelease);

        let mut bare = release("v9.9.9", true);
        bare.assets = vec![asset("notes.txt")];
        let update = Update::new(bare).expect("a newer version");
        assert_eq!(update.download_url, update.page_url);
        assert!(update.prerelease);
    }

    /// GitHub's own payload shape is what this module reads, so the test parses
    /// one rather than only building the structs by hand.
    #[test]
    fn githubs_release_json_is_what_we_read() {
        let body = br#"[
            {
                "tag_name": "v0.9.0-beta.2",
                "prerelease": true,
                "assets": [
                    {
                        "name": "SubText-0.9.0-beta.2-arm64.zip",
                        "browser_download_url": "https://example.test/beta.zip"
                    }
                ],
                "tarball_url": "https://api.github.test/tarball",
                "zipball_url": "https://api.github.test/zipball"
            }
        ]"#;
        let releases: Vec<GithubRelease> =
            serde_json::from_slice(body).expect("GitHub's shape should parse");
        assert_eq!(releases.len(), 1);
        let update = Update::new(releases.into_iter().next().expect("one release"))
            .expect("a newer version");
        assert!(update.prerelease);
        assert_eq!(update.download_url, "https://example.test/beta.zip");
    }

    /// A check is due a day after the last one, and after a clock that jumped
    /// backwards, but not before either.
    #[test]
    fn automatic_checks_are_daily() {
        let day = CHECK_INTERVAL.as_secs();
        assert!(check_is_due(None, 1_000));
        assert!(!check_is_due(Some(1_000), 1_000 + day - 1));
        assert!(check_is_due(Some(1_000), 1_000 + day));
        // A recorded time in the future cannot postpone the check forever.
        assert!(!check_is_due(Some(10_000), 1_000));
        assert!(check_is_due(Some(10_000), 10_000 + day));
    }
}
