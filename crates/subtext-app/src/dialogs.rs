//! The About and Donation windows.
//!
//! CotEditor's About panel is a standard Cocoa one: a 200-point identity column
//! (icon, name, version, site link, copyright) beside a 350-point description
//! pane, in a 320-point-tall window. SubText shows the same two columns in its
//! own small window, because that is what the panel is for — saying which build
//! you are running, and which project it reimplements.
//!
//! Its Donation panel is a 560-point `NSPanel` with a coffee icon, one sentence
//! about the project being free, and the two links the non-App-Store build
//! offers. SubText has no donation channel of its own, so the panel says that
//! and points at the project it reimplements; the window, its title and its
//! shape are the same.

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, button::Button, h_flex, v_flex};
use gpui_kit::{
    Context, FontWeight, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};

use crate::localization;

/// The version SubText reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// CotEditor's Donation panel.
///
/// Upstream's App Store build offers an in-app purchase; its non-App Store
/// build (`#if SPARKLE`) shows the same panel with a "not available here"
/// message and the two external links. SubText is in the second situation for
/// both reasons — no StoreKit and no account — so this is that view.
pub struct DonationView;

impl DonationView {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DonationView {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for DonationView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(localization::t("Donation"));

        v_flex()
            .size_full()
            .items_start()
            .gap_4()
            .p_6()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().text_sm().child(localization::t(
                "SubText provides all features for free to everyone. You can support \
                     the original project this editor reimplements.",
            )))
            .child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        Icon::new(gpui_kit::assets::IconName::Coffee)
                            .with_size(gpui_kit::px(64.))
                            .text_color(cx.theme().primary),
                    )
                    .child(div().text_sm().text_center().child(localization::t(
                        "The In-App donation feature is available only in CotEditor \
                                 distributed in the App Store.",
                    )))
                    .child(
                        h_flex()
                            .gap_2()
                            .pt_2()
                            .child(
                                Button::new("donation-app-store")
                                    .label(localization::t("Open CotEditor in App Store"))
                                    .on_click(|_, _, cx| {
                                        cx.open_url("https://apps.apple.com/app/id1024640650")
                                    }),
                            )
                            .child(
                                Button::new("donation-sponsors")
                                    .label(localization::t("Open GitHub Sponsors"))
                                    .on_click(|_, _, cx| {
                                        cx.open_url("https://github.com/sponsors/1024jp")
                                    }),
                            ),
                    ),
            )
            .child(
                // The root column is `items_start`, so without `w_full` this
                // row shrinks to the button and `justify_end` does nothing.
                h_flex().w_full().justify_end().child(
                    Button::new("close-donation")
                        .label(localization::t("Close"))
                        .on_click(|_, window, _| window.remove_window()),
                ),
            )
    }
}

pub struct AboutView;

impl AboutView {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AboutView {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for AboutView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(localization::t("About SubText"));

        h_flex()
            .size_full()
            .items_stretch()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // Upstream's identity column: 200 points wide, the icon and the
            // build it names, over the window's quaternary fill.
            .child(
                v_flex()
                    .w(gpui_kit::px(200.))
                    .h_full()
                    .items_center()
                    .justify_center()
                    .gap_1p5()
                    .px_3()
                    .bg(cx.theme().muted)
                    .text_center()
                    .child(
                        Icon::new(gpui_kit::assets::IconName::FileText)
                            .with_size(gpui_kit::px(64.))
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(localization::t("SubText")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(localization::tf("Version {}", &[&VERSION])),
                    )
                    .child(
                        gpui_kit::base::Link::new("about-site")
                            .href("https://github.com/jincaiw/subtext")
                            // One step down from the version line: the full
                            // URL is 26 characters, and at `text_sm` it came
                            // within a few pixels of the 200-point column.
                            .text_xs()
                            .child("github.com/jincaiw/subtext"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("© 2026 SubText Authors"),
                    ),
            )
            .child(div().h_full().w(gpui_kit::px(1.)).bg(cx.theme().border))
            // The 350-point description pane. Upstream's holds its credits and
            // licenses; SubText has no such data, so it says what this build is.
            //
            // `min_w_0` is what keeps the pane at its 350 points: without it the
            // flex item's automatic minimum is the longest unbreakable word, and
            // an absolute resource path used to push the pane past the window
            // edge where the sentence beside it was clipped rather than wrapped.
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .gap_3()
                    .p_6()
                    .child(div().w_full().text_sm().child(localization::t(
                        "A cross-platform plain-text editor with 70 syntax \
                                 definitions, 14 themes and full keyboard control, built on \
                                 GPUI Kit.",
                    )))
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(localization::t(
                                "SubText is an open source program licensed under the \
                                 Apache License, Version 2.0.",
                            )),
                    )
                    .child(
                        div().flex_1().flex().items_end().justify_end().child(
                            Button::new("close-about")
                                .label(localization::t("Close"))
                                .on_click(|_, window, _| window.remove_window()),
                        ),
                    ),
            )
    }
}
