//! The About window.
//!
//! CotEditor's About panel is a standard Cocoa one: the application icon, the
//! name, the version, and a credit line. SubText shows the same information in
//! its own small window, because that is what the panel is for — saying which
//! build you are running, and which resources it found.

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, button::Button, h_flex, v_flex};
use gpui_kit::{
    Context, FontWeight, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};

use crate::resources::Resources;

/// The version SubText reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
        window.set_window_title("About SubText");

        let resources = Resources::discover()
            .map(|resources| {
                format!("Resources: {:?} — {}", resources.origin, resources.root.display())
            })
            .unwrap_or_else(|| {
                "Resources: not found; using the built-in fallbacks".to_string()
            });

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                Icon::new(gpui_kit::assets::IconName::FileText)
                    .large()
                    .text_color(cx.theme().primary),
            )
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("SubText"))
            .child(div().text_sm().child(format!("Version {VERSION}")))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .text_center()
                    .child(
                        "A cross-platform plain-text editor, reimplementing CotEditor's \
                         menus, key bindings, syntax rules and themes on GPUI Kit.",
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .text_center()
                    .child(resources),
            )
            .child(
                h_flex()
                    .gap_2()
                    .pt_2()
                    .child(
                        Button::new("close-about")
                            .label("Close")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
    }
}
