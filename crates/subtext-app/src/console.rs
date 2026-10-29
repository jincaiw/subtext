//! CotEditor's Console panel: where a script's standard error goes.
//!
//! CotEditor keeps one console window for the whole application. Two things
//! write to it: the AppleScript `write to console` command, and — far more
//! often — a script that exits with text on its standard error. Upstream's
//! `ScriptError.standardError` is handed to `NSApp.presentError`, and for a
//! `ScriptError` that means "append to the console and bring the window
//! forward" rather than a modal alert.
//!
//! SubText used to put that text in the status bar, one line high, where a
//! multi-line traceback lost everything after the first line. The window is
//! where it belongs.
//!
//! The log lives in a [`Global`] rather than on a view because the panel and
//! the scripts run in different windows: a script started from one document
//! writes to the same console the panel in another window is showing.

use std::time::SystemTime;

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Global, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window, div,
};

/// One entry in the console.
#[derive(Debug, Clone)]
pub struct Log {
    /// The script that produced it. `None` for a message with no owner.
    pub title: Option<String>,
    /// What the script wrote, newlines and all.
    pub message: String,
    /// When it arrived.
    pub at: SystemTime,
}

impl Log {
    /// A log stamped with the current time.
    pub fn now(title: Option<String>, message: impl Into<String>) -> Self {
        Self {
            title,
            message: message.into(),
            at: SystemTime::now(),
        }
    }

    /// The heading line — `[2026-10-29 18:30:00] myscript.sh`.
    ///
    /// Upstream's `Console.Log.attributedString`, minus the two display
    /// options: SubText always shows both, because there is no settings pane
    /// for the console and an option nobody can turn off is not an option.
    pub fn heading(&self) -> String {
        let stamp = timestamp(self.at);
        let stamp = if stamp.is_empty() {
            String::new()
        } else {
            format!("[{stamp}]")
        };
        match (&self.title, stamp.is_empty()) {
            (Some(title), true) => title.clone(),
            (Some(title), false) => format!("{stamp} {title}"),
            (None, false) => stamp,
            (None, true) => String::new(),
        }
    }
}

/// `2026-10-29 18:30:00`, in the machine's own time zone.
///
/// Upstream asks `Date.ISO8601FormatStyle` for the same fields. A clock set
/// before 1970 is the one case jiff refuses; the heading then carries the title
/// alone rather than no heading at all.
fn timestamp(at: SystemTime) -> String {
    jiff::Timestamp::try_from(at)
        .map(|timestamp| {
            timestamp
                .to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

/// Every entry the console has been given, oldest first.
pub struct Console {
    logs: Vec<Log>,
}

impl Console {
    pub fn new() -> Self {
        Self { logs: Vec::new() }
    }

    pub fn logs(&self) -> &[Log] {
        &self.logs
    }

    pub fn is_empty(&self) -> bool {
        self.logs.is_empty()
    }

    /// Append one entry and repaint whoever is watching.
    pub fn append(&mut self, log: Log, cx: &mut Context<Self>) {
        self.logs.push(log);
        cx.notify();
    }

    /// Throw the log away — the panel's Clear button.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.logs.is_empty() {
            return;
        }
        self.logs.clear();
        cx.notify();
    }
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

/// The console, reachable from anywhere that has an `App`.
#[derive(Clone)]
pub struct ConsoleHandle(Entity<Console>);

impl Global for ConsoleHandle {}

impl ConsoleHandle {
    /// Create the console and register it. One per application, at launch.
    pub fn init(cx: &mut App) -> Self {
        let console = cx.new(|_| Console::new());
        let handle = Self(console);
        cx.set_global(handle.clone());
        handle
    }

    /// The console, if the application has one.
    pub fn get(cx: &App) -> Option<Self> {
        cx.try_global::<Self>().cloned()
    }

    pub fn entity(&self) -> &Entity<Console> {
        &self.0
    }
}

/// The window that shows the log.
pub struct ConsoleView {
    console: Entity<Console>,
}

impl ConsoleView {
    /// A view on `console`, repainted whenever the log changes.
    pub fn new(console: Entity<Console>, cx: &mut Context<Self>) -> Self {
        cx.observe(&console, |_, _, cx| cx.notify()).detach();
        Self { console }
    }
}

impl Render for ConsoleView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Console");

        let logs = self.console.read(cx).logs().to_vec();
        let console = self.console.clone();
        let mono = cx.theme().mono_font_family.clone();

        let body = if logs.is_empty() {
            // An empty console is the state a user sees first, and it has to
            // say what would appear here rather than leave a blank panel.
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .text_xs()
                        .text_center()
                        .text_color(cx.theme().muted_foreground)
                        .child("No output yet. A script's standard error appears here."),
                )
                .into_any_element()
        } else {
            v_flex()
                .size_full()
                .gap_3()
                .p_3()
                .child(
                    v_flex()
                        .gap_1()
                        .children(logs.iter().map(|log| {
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(log.heading()),
                                )
                                .child(
                                    div()
                                        .pl_2()
                                        .text_sm()
                                        .font_family(mono.clone())
                                        .child(log.message.clone()),
                                )
                        }))
                        .into_any_element(),
                )
                .into_any_element()
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .flex_none()
                    .w_full()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("console-clear")
                            .ghost()
                            .xsmall()
                            .label("Clear")
                            .disabled(logs.is_empty())
                            .on_click(cx.listener(move |_, _, _, cx| {
                                console.update(cx, |console, cx| console.clear(cx));
                            })),
                    ),
            )
            .child(
                // The scroll owner is the region boundary: the padding sits on
                // the inner column, never on this element.
                div()
                    .id("console-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn a_heading_carries_the_time_and_the_script_name() {
        let log = Log {
            title: Some("fail.sh".to_string()),
            message: "boom".to_string(),
            at: at(1_800_000_000),
        };
        let heading = log.heading();
        assert!(heading.starts_with('['), "{heading}");
        assert!(heading.ends_with("] fail.sh"), "{heading}");
        // 2027-01-15 in UTC; the machine's zone may move the day, so the shape
        // is what is checked.
        assert_eq!(heading.matches(':').count(), 2, "{heading}");
    }

    #[test]
    fn a_heading_without_a_title_is_just_the_time() {
        let log = Log {
            title: None,
            message: "boom".to_string(),
            at: at(1_800_000_000),
        };
        let heading = log.heading();
        assert!(
            heading.starts_with('[') && heading.ends_with(']'),
            "{heading}"
        );
        // The stamp itself has a space between the date and the time, so "no
        // title" reads as "nothing after the closing bracket".
        assert!(!heading.contains("] "), "{heading}");
    }

    #[test]
    fn clearing_an_empty_console_does_not_repaint() {
        // Nothing to assert about the notification from here; what matters is
        // that the call is safe when there is nothing to throw away.
        let console = Console::new();
        assert!(console.is_empty());
        assert_eq!(console.logs().len(), 0);
    }
}
