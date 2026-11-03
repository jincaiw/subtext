//! The installed font families, and the questions the Appearance pane asks
//! about them.
//!
//! CotEditor hands the whole font problem to AppKit: `NSFontManager` lists the
//! families, `NSFont` answers `isFixedPitch`, and a saved font is archived as
//! `NSFont` data. SubText stores the family name and a point size instead, so
//! the two things AppKit did for free have to be done here.
//!
//! Two rules shape this module:
//!
//! * **GPUI panics on a family it cannot find.** It does so on the first line
//!   it lays out, not when the font is constructed, which means an unknown
//!   family saved in `settings.json` would take the document window down on
//!   its first repaint. Every family that reaches `.font_family(...)` is
//!   therefore checked against the installed list first, and anything unknown
//!   falls back to the theme's family.
//! * **The list is not cheap.** `all_font_names` walks the platform's font
//!   collection every time it is called (about a tenth of a second), and the
//!   Appearance pane asks on every frame, so the list is read once per process.

use std::sync::OnceLock;

use gpui_kit::{App, Font, ParentElement as _, SharedString, Styled as _, div, px};

/// Every family the platform reports, sorted for the picker.
///
/// Families whose name starts with a dot are AppKit's private fonts — `.SF NS`,
/// `.Apple Symbols Fallback`, and the rest of the system's furniture. AppKit's
/// own font panel hides them, and listing them would let a user choose a family
/// that is not meant to be chosen.
pub fn installed_families(cx: &App) -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = cx.text_system().all_font_names();
        names.retain(|name| !name.is_empty() && !name.starts_with('.'));
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    })
}

/// The family to actually ask GPUI for.
///
/// A saved family is returned in the platform's own spelling — a settings file
/// written on one machine may spell `SF Mono` as `sf mono` — and anything that
/// is not installed comes back as `fallback`. The caller should hand the result
/// straight to `.font_family(...)`; that is the point of the check.
pub fn resolve(cx: &App, family: Option<&str>, fallback: &str) -> String {
    let Some(family) = family.filter(|family| !family.is_empty()) else {
        return fallback.to_string();
    };
    let installed = installed_families(cx);
    if let Some(name) = installed.iter().find(|name| name.as_str() == family) {
        return name.clone();
    }
    // A hand-edited settings file may differ only in case. GPUI's lookup is
    // case-sensitive, so the saved spelling cannot be handed on as it stands.
    if let Some(name) = installed
        .iter()
        .find(|name| name.eq_ignore_ascii_case(family))
    {
        return name.clone();
    }
    fallback.to_string()
}

/// Whether `family` is installed. Used before offering a family as a choice.
pub fn is_installed(cx: &App, family: &str) -> bool {
    installed_families(cx)
        .iter()
        .any(|name| name.eq_ignore_ascii_case(family))
}

/// What the Appearance pane writes beside a font: the family's display name
/// and its size, separated by two spaces.
///
/// Upstream's `displayNameAndSize` is `"\(displayName ?? fontName)  \(pointSize)"`,
/// with the double space there on purpose — the sample is one string so that
/// the size is drawn in the same font as the name.
pub fn display_name_and_size(family: &str, size: f32) -> String {
    format!("{}  {size:.0}", display_name(family))
}

/// The name to *show* for a family, which is not always the name to ask GPUI
/// for.
///
/// The theme's sans face is `.SystemUIFont`, GPUI's private name for the
/// platform UI font. AppKit reports the same face as "System Font Regular"
/// through `NSFont.displayName`, and upstream's pane shows that — a leading
/// dot in a settings pane reads as a bug, not as a font. Every other family
/// is its own display name.
pub fn display_name(family: &str) -> &str {
    match family {
        ".SystemUIFont" => "System Font Regular",
        family => family,
    }
}

/// Whether `family` is fixed-pitch, the question behind upstream's
/// "The selected font doesn't seem to be monospaced." alert.
///
/// AppKit answers this with `NSFont.isFixedPitch`; GPUI does not expose the
/// trait, so the glyphs are measured instead. Every glyph in a fixed-pitch
/// face advances the same distance; in a proportional face `i` and `W` do not.
/// The characters below all exist in a Latin text face, so the measurement does
/// not fall through to a fallback font.
///
/// A family that is not installed is *not* reported as non-monospaced — there
/// is nothing to measure, and the alert would be a lie. Callers resolve the
/// family first.
pub fn is_monospaced(cx: &App, family: &str) -> bool {
    const SAMPLE: [char; 6] = ['i', 'l', 'W', 'M', 'm', '0'];

    // An uninstalled family resolves to the platform fallback below, so
    // measuring it would answer a question about a font the user did not
    // choose. Keep the promise the doc comment makes.
    if !is_installed(cx, family) {
        return true;
    }

    let font = Font {
        family: SharedString::from(family),
        ..Font::default()
    };
    let text_system = cx.text_system();
    let font_id = text_system.resolve_font(&font);
    let size = px(13.);

    let mut narrowest = f32::MAX;
    let mut widest = f32::MIN;
    for ch in SAMPLE {
        let width = f32::from(text_system.layout_width(font_id, size, ch));
        narrowest = narrowest.min(width);
        widest = widest.max(width);
    }
    // Sub-pixel hinting can leave a hundredth of a pixel between two glyphs
    // that are meant to be identical; a real proportional face differs by
    // whole pixels.
    widest - narrowest <= 0.01
}

/// Draw the font-in-use sample: the family's name and its size, in the family
/// itself, which is how the Appearance pane shows what is selected.
///
/// The family has to have been resolved already; `fonts::resolve` is what makes
/// that true.
pub fn sample(text: impl Into<SharedString>, family: &str, size: f32) -> gpui_kit::Div {
    div()
        .child(text.into())
        .font_family(family.to_string())
        .text_size(px(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_joins_name_and_size_with_two_spaces() {
        assert_eq!(display_name_and_size("Menlo", 13.0), "Menlo  13");
    }

    /// The theme's private system-face name is not a name to put in a pane.
    #[test]
    fn the_system_face_shows_its_appkit_display_name() {
        assert_eq!(
            display_name_and_size(".SystemUIFont", 13.0),
            "System Font Regular  13"
        );
    }
}
