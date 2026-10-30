//! Escape, claimed before a text field inside a dialog can take it.
//!
//! `Dialog` closes on Escape by itself — `close_on_escape` is on by default —
//! but it only sees the key if nothing nearer the focus has already taken it.
//! A focused `Input` does exactly that: with the caret in `Begin:` of the
//! Custom Surround sheet, Escape did nothing at all, which a UI test caught
//! (§22.29). Capture runs on the way *down*, before the field, which is where
//! the surface can claim the key first.
//!
//! The command palette has the same handler written by hand; it predates this
//! module and is due to move here the next time it is touched.

use gpui_kit::component::WindowExt as _;
use gpui_kit::{InteractiveElement, KeyDownEvent};

/// Closes the dialog this element belongs to when Escape is pressed.
///
/// Applied to the root of a sheet's view — the element that wraps every field,
/// so the capture phase reaches it before any of them. A method rather than a
/// free function so it reads as part of the builder chain it joins.
pub trait DismissOnEscape: InteractiveElement + Sized {
    fn dismiss_on_escape(self) -> Self {
        self.capture_key_down(|event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                window.close_dialog(cx);
            }
        })
    }
}

impl<E: InteractiveElement> DismissOnEscape for E {}
