//! The document window's toolbar, and the palette that customizes it.
//!
//! CotEditor's toolbar is an `NSToolbar` driven by
//! `DocumentWindowController`'s `NSToolbarDelegate`. Its default set for a text
//! document is `syntax`, a flexible space and `toggleInspector`; the delegate
//! also offers a fixed list of allowed items, and *View ▸ Customize Toolbar…*
//! opens AppKit's palette, where the user drags items in and out and back into
//! order. The choice is autosaved per toolbar, so it survives the next launch.
//!
//! SubText has no `NSToolbar`, so this module holds the same three things as
//! data: the allowed identifiers in upstream's order, the default set, and the
//! labels / symbols each item shows. [`ToolbarSheet`] is the palette, and
//! [`ToolbarItemId`] is what the settings file stores — the identifiers are
//! upstream's own strings (`syntaxStyle`, `toggleInspector`, …), so a saved
//! toolbar can be compared with CotEditor's item by item.
//!
//! # The palette floats over the window, it is not a dialog
//!
//! AppKit's palette is a sheet, and the *real* toolbar stays visible and live
//! above it: items are dragged from the grid straight into the toolbar, out of
//! the toolbar back into the palette, and along the toolbar to be reordered.
//! `gpui_component`'s `Dialog` cannot do that — its layer occludes the whole
//! window, so nothing underneath it can be a drop target (see
//! `docs/PLAN.md` §22.52). The palette is therefore drawn by the workspace as
//! an in-tree, absolutely positioned panel below the title bar, with the
//! occlusion switched off around it. The toolbar behind stays the toolbar, so
//! the drag targets are the same surfaces AppKit uses.
//!
//! A click on a grid tile still toggles membership, which AppKit does not
//! offer; it stays because it is the only gesture available to a hand that
//! cannot drag. Escape and *Done* close the palette.
//!
//! # What is shown but not honoured
//!
//! Three groups in upstream's allowed list control features SubText does not
//! have: writing direction, text orientation, and the macOS system panels
//! (spelling, emoji & symbols, print, share). They stay in the list because the
//! menu bar keeps their commands for the same reason — an item that disappears
//! is harder to account for than one that says what it cannot do when pressed.

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Colorize as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex,
};
use gpui_kit::{
    Anchor, AnyElement, App, AppContext as _, Context, Entity, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, WeakEntity, Window, div, px, rems,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::localization;
use crate::workspace::Workspace;

/// How many tiles the customization palette has across.
///
/// AppKit lays its palette out four wide; the nineteen items that are not
/// immovable therefore fill four rows of four and a last row of three.
const GRID_COLUMNS: usize = 4;
/// One tile, in rem — 125 points.
///
/// Measured off a 2× capture of AppKit's palette: the tiles' centres sit 133
/// points apart and the gaps between them are 8, which leaves 125 for the tile
/// itself. The first tile is much narrower than the whole sheet, which is why
/// the grid below is a fixed-width block rather than four equal columns of the
/// sheet's width.
const TILE_WIDTH: f32 = 7.8125;
/// The gap between tiles, in rem — 8 points, the same as `gap_2`.
const TILE_GAP: f32 = 0.5;
/// How tall one tile is, in rem — 70 points: the control, a gap, and the
/// caption under it. Rows in the capture repeat every 78 points, so 70 plus the
/// 8-point gap.
const TILE_HEIGHT: f32 = 4.375;
/// The width of the tile block itself, in rem — 524 points.
const GRID_WIDTH: f32 = GRID_COLUMNS as f32 * TILE_WIDTH + (GRID_COLUMNS as f32 - 1.) * TILE_GAP;
/// How wide the whole palette is, in rem — 616 points.
///
/// AppKit's sheet is 616 points wide and the 524-point tile block sits centred
/// in it, so each side carries 46 points. The block is centred inside the
/// sheet's *content* area, not inside the sheet: the captions, the preview
/// strip and the footer start 29 points in from the card edge (measured: the
/// caption ink begins at 403 pixels and the strip at 402, on a card that
/// starts at 345), so the content runs 558 points and the remaining 17 sit
/// between the block and the content edge on each side.
pub const PALETTE_WIDTH: f32 = 38.5;

/// The height of one item's control in the palette, in rem — 37 points.
///
/// A single-button item is a circle that wide; a pair or a pop-up is a capsule
/// that tall. The two read as the same weight, which is why they share a number.
const TILE_CONTROL: f32 = 2.3125;

/// The width of a two-glyph capsule in the grid, in rem — 75 points.
///
/// The grid's cells are 125 points wide, and the pairs do not fill them: the
/// capture measures 75, and the extra space inside the cell is what separates
/// one tile from the next. A content-sized capsule would shrink to the glyphs
/// and lose that air.
const PAIR_WIDTH: f32 = 4.6875;

/// The width of the syntax pop-up, in rem — 151 points, in both places it
/// appears.
///
/// It is the one control whose width does not come from a glyph: AppKit gives
/// the `NSPopUpButton` the room a syntax name needs, and the same width in the
/// palette's grid as in the live toolbar — the capture measures 302 pixels
/// between the outer edges of the hairline in each. That is 26 points wider
/// than a grid tile, so the tile it sits in grows with it (`tile_width`) and
/// the first row runs that much past the block.
const SYNTAX_POPUP_WIDTH: f32 = 9.4375;

/// The width of the indent pair, in rem — 82 points.
///
/// Wider than `PAIR_WIDTH` because the picture is: two blocks of four lines
/// with an arrow pushing into each, all four clusters inside one capsule. AppKit
/// measures 164 pixels across at 2x, and at the shared 75 the arrows end up
/// touching the outline.
const INDENT_WIDTH: f32 = 5.125;

/// The width of the fonts picture when the localized word fills it, in rem —
/// 46 points.
///
/// AppKit draws this one picture as the localized word for "format" — `格式`
/// under a Chinese system — inside an oval, and sizes the oval to the word:
/// the reference measures 92 pixels across at 2x, against 74 for the circular
/// pictures beside it. English keeps `Aa`, which fits the 37-point circle every
/// other single-button item uses.
const FONTS_WORD_WIDTH: f32 = 2.875;

/// How far the preview strip's band sits below the sheet's fill, and how far a
/// control standing on that band sits above it, as fractions of the fill's
/// lightness.
///
/// The reference's sheet is a translucent material, so its absolute value
/// follows whatever is behind the window: one capture measured the sheet at 253
/// over a page of 250, a fresh one 240-242 over 251. What holds in both is the
/// order — the sheet a step below the page, the band below the sheet, and a
/// strip control above the band, back at about the page's tone. In the fresh
/// capture the sheet is 240-242, the band 234 and a control 251; against
/// `sheet_fill`'s 245 these two fractions land the band at 237 and the control
/// at 251.
const BAND_STEP: f32 = 0.033;
const STRIP_CONTROL_STEP: f32 = 0.024;

/// The fill of the customization sheet the grid and the preview strip sit on.
///
/// AppKit's sheet is a material that reads a step *darker* than the editor
/// behind it. `popover` is white in light mode — the same white as the window
/// chrome — so the sheet came out five levels *above* the page instead of below
/// it. `muted` keeps the ordering in both appearances: neutral-100 between the
/// chrome and the paper in light, neutral-800 above neutral-950 in dark, where
/// a floating surface has to be lighter. It also has steps to derive from in
/// both directions, which the strip below needs.
pub(crate) fn sheet_fill(cx: &App) -> Hsla {
    cx.theme().muted
}

// The pictures AppKit draws with paths rather than type: solid arrows for the
// indent buttons, the slashes of the comment mark, the tab glyph, the pop-up
// chevrons, and the four system symbols. `Icon::data` copies the bytes and the
// renderer tints the rasterised mask with `text_color`, so these follow the
// theme like every system glyph does.
//
// They live at module scope because two surfaces draw the same items — the
// grid in this sheet and the strip under it — and a picture that appeared twice
// with two definitions would drift.
const ARROW_LEFT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M10 0 10 10 0 5Z"/></svg>"#;
const ARROW_RIGHT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0 0 0 10 10 5Z"/></svg>"#;
// The pop-up indicator: two thin chevrons, one above the other. The Lucide
// glyph is drawn on the icon grid and comes out half again too heavy at this
// size, where it sits inside a title rather than beside it.
const CHEVRONS_UP_DOWN: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 12"><g fill="none" stroke="#000" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"><path d="M2 4.2 5 1.2 8 4.2"/><path d="M2 7.8 5 10.8 8 7.8"/></g></svg>"##;
// Comment: two slanted marks and four lines of code, right-aligned like a
// paragraph. One picture rather than two elements because the slashes and the
// rules share a baseline and a 9-pixel line pitch that a flex row cannot keep.
const COMMENT_MARK: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 41 34"><g fill="#000"><path d="M8 0h4l-8 34H0z"/><path d="M18 0h4l-8 34h-4z"/><rect x="26" y="2" width="14" height="3" rx="1.5"/><rect x="24" y="11" width="16" height="3" rx="1.5"/><rect x="22" y="19" width="18" height="3" rx="1.5"/><rect x="20" y="28" width="11" height="3" rx="1.5"/></g></svg>"##;
// A tab stop: a rounded cell with its inner bar, a solid corner that points the
// way the caret moves, and the stop line after it.
const TAB_GLYPH: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 20"><g fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="3" width="12" height="14" rx="2.5"/><path d="M4.6 3v14"/><path d="M13 3l7 7-7 7"/><path d="M23 2v16"/></g></svg>"##;
// The pictures the system supplies as symbols rather than as type. AppKit's
// palette shows every item in its *off* state, which is why these are the
// plain forms: a solid pilcrow, a pencil, an outline thumbtack, and the
// four-line wrap mark. Lucide's nearest glyphs are all different pictures from
// these — an outlined pilcrow, a plain pencil, a pin with a filled head — and
// in a palette of pictures the picture is the label.
//
// The `.slash` variants are the same drawing with a diagonal struck across it.
// Rather than trace each picture twice, `symbol_art` appends one of the slash
// paths below to the base picture; a second copy of the pencil is how the
// palette and the strip would end up showing two different pencils.
const PARAGRAPH_SIGN: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 31"><g fill="#000"><path d="M4.5 0H26V3H15V17H11A11 5.5 0 0 1 0 11.5V4.5Z"/><path d="M11 17H15V31H11Z"/><rect x="18" width="4" height="31"/></g></svg>"##;
const PENCIL: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 27 27"><path d="M10.75 12.25 14 15.5 2.5 27H0v-4Z" fill="#000"/><path d="M19.5 3.5 22.75 6.75 17.75 11.75 14.5 8.5Z" fill="#000"/><circle cx="24.3" cy="2.4" r="2.3" fill="#000"/></svg>"##;
// The slashes, in each picture's own coordinates and at the weight that
// picture's strokes are drawn with. Each runs corner to corner the way the
// pencil already did, so the four struck variants read as one family.
const SLASH_PARAGRAPH: &str = r##"<path d="M1.8 2.6 24.2 28.4" fill="none" stroke="#000" stroke-width="3" stroke-linecap="round"/>"##;
const SLASH_PENCIL: &str = r##"<path d="M4.7 2.7 24.9 22.9" fill="none" stroke="#000" stroke-width="3.5" stroke-linecap="round"/>"##;
const SLASH_PIN: &str = r##"<path d="M2.2 2.8 22.8 34.8" fill="none" stroke="#000" stroke-width="3" stroke-linecap="round"/>"##;
const SLASH_WRAP: &str = r##"<path d="M13.5-64.5 100-1.5" fill="none" stroke="#000" stroke-width="6.5" stroke-linecap="round"/>"##;
// The thumbtack is a stroke drawing: a thin bar, two shoulders that pinch in to
// the neck, a bell, and a needle. The bell is a flat top that flares out almost
// horizontally and only then turns down — a bezier with its control point at the
// far corner flattens the dome and reads as a box at 13 points, which is what
// the first attempt did.
const PIN: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 38"><rect x="2" width="21" height="2.2" rx="1.1" fill="#000"/><g fill="none" stroke="#000" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M8 14.4V9.4C8 6.6 6.1 5.1 3.4 3.4"/><path d="M16 14.4V9.4C16 6.6 17.9 5.1 20.6 3.4"/><path d="M7.4 14.6C4.4 15.4 1.6 17.6 1.6 20.4V22.6H23.4V20.4C23.4 17.6 20.6 15.4 17.6 14.6Z"/></g><path d="M12.5 24V36.6" fill="none" stroke="#000" stroke-width="2.8" stroke-linecap="round"/></svg>"##;
// The double-headed arrow AppKit puts inside the flexible space: two solid
// heads with a dotted shaft between them. The dots are a dashed stroke rather
// than five little rectangles so the pitch survives resampling — the reference
// shows five 1-point squares with a point of gap either side, and a dash
// pattern of "2 2" over a 20-unit shaft lands on exactly that.
const STRETCH_ARROW: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 56 10"><g fill="#000"><path d="M16 0 0 5 16 10Z"/><path d="M40 0 56 5 40 10Z"/></g><path d="M18 5H38" fill="none" stroke="#000" stroke-width="2" stroke-dasharray="2 2"/></svg>"##;
// The inspector's symbol: a rounded pane split by a divider, with three short
// bars in the right-hand column. Traced off the reference at 2× — the frame is
// 39×31 device pixels, the stroke 3 of them, and the bars 4.5×2 — which is why
// the viewBox counts device pixels and `symbol_art` asks for 19.5 points.
const INSPECTOR: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 39 31"><g fill="none" stroke="#000" stroke-width="3" stroke-linejoin="round"><rect x="1.5" y="1.5" width="36" height="28" rx="6"/><path d="M24 1.5V29.5"/></g><g fill="#000"><rect x="28.5" y="7" width="5" height="2"/><rect x="28.5" y="11.5" width="5" height="2"/><rect x="28.5" y="16" width="5" height="2"/></g></svg>"##;
// The wrap mark is SF `text.wrap` traced from the symbol sheet: four lines and
// a return arrow that curves back to the left. The path is broken across lines
// at spaces, which the SVG parser treats as the same separator.
const TEXT_WRAP: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="10.7656 -68.7012 92.2254 71.8848"><g fill="#000"><path d="M66.5458-53.6914C64.9052-53.6914 63.5771-53.0273
    61.9364-51.5039L37.7177-28.5742C36.6239-27.5586 36.2333-26.3086
    36.2333-25.2539C36.2333-24.1601 36.6239-22.9492 37.7177-21.8945L61.9364
    0.7617C63.7723 2.4805 64.827 3.1836 66.4677 3.1836C68.8115 3.1836 70.4911
    1.3086 70.4911-0.918L70.4911-12.0898L71.2723-12.0898C94.0457-12.0898
    102.991-27.0117 102.991-49.1601C102.991-52.0898 101.311-53.6913
    99.5536-53.6913C98.1864-53.6913 96.5849-53.4179 95.413-51.3085C89.8271-41.2304
    82.5615-38.4179 71.2724-38.4179L70.4912-38.4179L70.4912-49.8241C70.4912-52.0507
    68.8115-53.6913 66.5458-53.6913Z"/><path d="M14.0371-62.1582L85.2773-62.1582C87.1328-62.1582 88.6465-63.5742
    88.6465-65.4297C88.6465-67.2852 87.1328-68.7012
    85.2773-68.7012L14.0371-68.7012C12.2305-68.7012 10.7656-67.2852
    10.7656-65.4297C10.7656-63.5742 12.2305-62.1582
    14.0371-62.1582ZM14.0371-42.0898L38.2734-42.0898C40.0801-42.0898
    41.5938-43.5547 41.5938-45.4102C41.5938-47.2168 40.0801-48.6816
    38.2734-48.6816L14.0371-48.6816C12.2305-48.6816 10.7656-47.2168
    10.7656-45.4102C10.7656-43.5547 12.2305-42.0898
    14.0371-42.0898ZM14.0371-22.0703L26.2773-22.0703C28.1328-22.0703
    29.6465-23.5352 29.6465-25.3906C29.6465-27.1973 28.1328-28.6621
    26.2773-28.6621L14.0371-28.6621C12.2305-28.6621 10.7656-27.1973
    10.7656-25.3906C10.7656-23.5352 12.2305-22.0703
    14.0371-22.0703ZM14.0371-2.0508L38.2734-2.0508C40.0801-2.0508 41.5938-3.5156
    41.5938-5.3711C41.5938-7.1777 40.0801-8.6426
    38.2734-8.6426L14.0371-8.6426C12.2305-8.6426 10.7656-7.1778
    10.7656-5.3711C10.7656-3.5156 12.2305-2.0508 14.0371-2.0508Z"/></g></svg>"##;

// SF `face.smiling` traced off the palette capture: the ring's ink is 34.5
// device pixels across at a 1.5-point stroke, the eyes are 2×2.5 points, and
// the mouth is an *outlined lens* — a straight chord over a deep bottom arc,
// 21 points wide and 10 tall. Lucide's nearest face is a thinner ring with
// stubbier eyes, which at this size is a different face; the lens in particular
// is what makes the system picture read as a smile rather than a glyph.
const FACE_SMILING: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><g fill="none" stroke="#000" stroke-linecap="round" stroke-linejoin="round"><circle cx="50" cy="50" r="45.65" stroke-width="8.7"/><path d="M23.9 63H76.1Q50 97.8 23.9 63Z" stroke-width="8.7"/><path d="M35.5 36.95v2.9M64.5 36.95v2.9" stroke-width="11.6"/></g></svg>"##;

// The indent guide: a vertical rule with two hanging lines, a second guide
// below it, and two lines indented inside that guide. Drawn as one picture
// because the geometry matters — the bars sit on a 9-pixel pitch and the
// indented pair starts at the second guide's right edge, which a nested flex
// layout cannot hold at this size.
const INDENT_GUIDES: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 39 31"><g fill="#000"><rect width="5" height="31" rx="2.5"/><rect x="10" y="1" width="29" height="3" rx="1.5"/><rect x="10" y="10" width="29" height="3" rx="1.5"/><rect x="10" y="16" width="6" height="15" rx="3"/><rect x="19" y="18" width="20" height="3" rx="1.5"/><rect x="19" y="27" width="12" height="3" rx="1.5"/></g></svg>"##;

/// The base picture with `slash` appended inside its `<svg>` element.
fn slashed(base: &'static [u8], slash: &'static str) -> Cow<'static, [u8]> {
    let svg = std::str::from_utf8(base).expect("the art is UTF-8");
    let close = svg.rfind("</svg>").expect("the art is an SVG document");
    let mut struck = String::with_capacity(svg.len() + slash.len());
    struck.push_str(&svg[..close]);
    struck.push_str(slash);
    struck.push_str("</svg>");
    Cow::Owned(struck.into_bytes())
}

/// The picture for the items whose system art is a symbol rather than type,
/// and the size it is drawn at. `None` means the item has an icon in the
/// Lucide set.
///
/// `on` selects between AppKit's `stateImages[.on]` and `[.off]`, the way
/// upstream does at `DocumentWindowController.swift:918-967`. The wrap mark,
/// the pilcrow and the thumbtack are struck while the command is on; the
/// pencil is struck while editing is *off*. Indent guides and the inspector
/// draw one picture in both states. The palette always asks for `false`:
/// AppKit's palette shows every item in its off state.
fn symbol_art(item: ToolbarItemId, on: bool) -> Option<(Cow<'static, [u8]>, f32)> {
    // Editable is the one item whose slash belongs to the off state.
    if item == ToolbarItemId::Editable {
        let picture = if on {
            Cow::Borrowed(PENCIL)
        } else {
            slashed(PENCIL, SLASH_PENCIL)
        };
        return Some((picture, 13.5));
    }
    let (base, size, slash) = match item {
        ToolbarItemId::Invisibles => (PARAGRAPH_SIGN, 13., SLASH_PARAGRAPH),
        ToolbarItemId::WrapLines => (TEXT_WRAP, 20., SLASH_WRAP),
        ToolbarItemId::KeepOnTop => (PIN, 12.5, SLASH_PIN),
        ToolbarItemId::IndentGuides => return Some((Cow::Borrowed(INDENT_GUIDES), 19.5)),
        ToolbarItemId::ToggleInspector => return Some((Cow::Borrowed(INSPECTOR), 19.5)),
        _ => return None,
    };
    let picture = if on {
        slashed(base, slash)
    } else {
        Cow::Borrowed(base)
    };
    Some((picture, size))
}

/// The live toolbar's copy of a system picture.
///
/// Upstream assigns a single `NSImage` to the palette tile *and* to the
/// toolbar item, varying it only through `stateImages`, so the sheet and the
/// strip cannot show different drawings. Both surfaces call this module for
/// that reason. The tint is left to the caller: an `Icon` without one takes
/// the theme's foreground, which is the ink the palette names explicitly.
pub(crate) fn symbol_glyph(item: ToolbarItemId, on: bool) -> Option<AnyElement> {
    let (svg, size) = symbol_art(item, on)?;
    Some(Icon::default().data(&svg).size(px(size)).into_any_element())
}

/// The picture on the Fonts item.
///
/// SF Symbols' `textformat` is script-dependent: the Chinese system draws
/// 格式 and the Latin one "Aa". CotEditor puts the same symbol on the palette
/// tile and on the toolbar item, so both draw it through this function.
pub(crate) fn fonts_glyph() -> AnyElement {
    let chinese = localization::language() == localization::Language::Chinese;
    div()
        .text_size(px(if chinese { 14. } else { 16. }))
        .child(if chinese { "格式" } else { "Aa" })
        .into_any_element()
}

/// The picture on the spell-check item: "Abc" over a dotted rule.
///
/// The tick Lucide draws belongs to the command's state, not its picture — the
/// system item shows the same mark either way. `color` paints the dots, which
/// do not inherit the surrounding text colour the way the letters do.
pub(crate) fn spell_check_glyph(color: Hsla) -> AnyElement {
    // Measured off the 2× capture: "Abc" is 16-point type whose ink is 24
    // device pixels tall, and the dotted rule under it is seven 2-point dots on
    // a 4.3-point pitch, spanning 27.5 points — the same width as the word. The
    // ink gap between the two is 3 points. GPUI's 16-point line box already
    // leaves about 2.25 points below the baseline, so the dots' row is pushed
    // down by the remainder rather than padded from scratch.
    v_flex()
        .flex_none()
        .items_center()
        .child(div().text_size(px(16.)).line_height(rems(1.)).child("Abc"))
        .child(
            h_flex()
                .flex_none()
                .mt(px(1.))
                .gap(px(2.3))
                .children((0..7).map(|_| {
                    div()
                        .flex_none()
                        .w(px(2.))
                        .h(px(2.))
                        .rounded_full()
                        .bg(color)
                })),
        )
        .into_any_element()
}

/// The picture on the emoji item: SF `face.smiling`.
///
/// The palette draws it at 17.25 points and the live strip at 16, the same
/// enlargement AppKit gives every symbol in the customization sheet, so the
/// size is the caller's; the drawing keeps its proportions either way.
pub(crate) fn face_smiling_glyph(size: f32, ink: Hsla) -> AnyElement {
    art_shape(FACE_SMILING, size, ink)
}

// The pair pictures — writing direction, text orientation, indent — are traced
// by hand rather than asked of the system: three of the six halves are SF
// Symbols upstream (`text.alignleft`, `text.alignright`,
// `decrease.indent`/`increase.indent`) and the fourth is its own asset
// (`.textVertical`), none of which GPUI can render. They live here, not in the
// palette builder, because the live strip draws the same pictures; two
// drawings of one AppKit picture is how the palette and the toolbar drift
// apart.
//
// A line of text is a 1.5-point stroke, the rule thickness the reference's
// 2× capture shows in every pair picture; the rounded end keeps a bar that
// thin from reading as a smudge. CotEditor's own drawings put four rules on a
// 4.33-point pitch, leaving 2.8 points of paper between them; the indent
// symbol's rhythm is its own and is set in `indent_glyph`.
fn art_bar(width: f32, height: f32, color: Hsla) -> AnyElement {
    div()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .rounded(px(height / 2.))
        .bg(color)
        .into_any_element()
}

/// Four stacked bars, flush to one side: the writing-direction and
/// text-orientation pictures.
fn art_rules(widths: &[f32], right: bool, ink: Hsla) -> AnyElement {
    art_rules_on(widths, right, 2.8, ink)
}

/// The same bars at a caller-chosen pitch. The indent picture is an SF Symbol
/// rather than one of CotEditor's own drawings, and the symbol spaces its
/// rules a little wider (`indent_glyph`); everything else keeps the 2.8-point
/// paper the pair pictures share.
fn art_rules_on(widths: &[f32], right: bool, gap: f32, ink: Hsla) -> AnyElement {
    let mut column = v_flex().flex_none().gap(px(gap));
    if right {
        column = column.items_end();
    } else {
        column = column.items_start();
    }
    for &width in widths {
        column = column.child(art_bar(width, 1.5, ink));
    }
    column.into_any_element()
}

/// The same strokes stood on end, top-aligned: the vertical half of the
/// text-orientation picture.
fn art_strokes(heights: &[f32], ink: Hsla) -> AnyElement {
    let mut row = h_flex().flex_none().items_start().gap(px(2.8));
    for &height in heights {
        row = row.child(art_bar(1.5, height, ink));
    }
    row.into_any_element()
}

fn art_shape(svg: &[u8], size: f32, ink: Hsla) -> AnyElement {
    Icon::default()
        .data(svg)
        .size(px(size))
        .text_color(ink)
        .into_any_element()
}

/// One half of the writing-direction pair. The left block reads as text that
/// starts at the margin, the right one as text that ends at it.
pub(crate) fn writing_direction_glyph(right: bool, ink: Hsla) -> AnyElement {
    art_rules(&[16.5, 10.5, 16.5, 10.5], right, ink)
}

/// One half of the text-orientation pair: horizontal text as stacked bars,
/// vertical text as strokes that stand on end, both starting from the top so
/// the contrast reads as rotation.
pub(crate) fn text_orientation_glyph(vertical: bool, ink: Hsla) -> AnyElement {
    if vertical {
        art_strokes(&[10.5, 16.5, 10., 16.], ink)
    } else {
        art_rules(&[16.5, 10.5, 16.5, 10.5], false, ink)
    }
}

/// One half of the indent pair. A block of four lines with a solid triangle
/// pushing into it from the right (decrease) or from the left (increase).
/// AppKit's arrows are filled, not the outlined chevrons the Lucide set has.
///
/// This is the one pair picture AppKit draws with an SF Symbol
/// (`decrease.indent` / `increase.indent`), so its numbers come from the
/// symbol rather than from a CotEditor resource: rendered at the toolbar's
/// 16-point symbol size, the three long rules are 11.5 points, the last 7.5,
/// and the strokes sit on a 4.5-point pitch — 3 points of paper, where the
/// writing-direction picture leaves 2.8. The triangle is square, 4.4 points
/// each way, and stands 3.1 points clear of the rules. Together that is ink
/// 19 by 15 points, against the symbol's 18.9 by 14.9. The path fills its
/// whole viewBox, so the row gap is the gap the eye sees.
pub(crate) fn indent_glyph(decrease: bool, ink: Hsla) -> AnyElement {
    let arrow = if decrease { ARROW_LEFT } else { ARROW_RIGHT };
    let mut row = h_flex().flex_none().items_center().gap(px(3.1));
    if decrease {
        row = row.child(art_rules_on(&[11.5, 11.5, 11.5, 7.5], false, 3., ink));
        row = row.child(art_shape(arrow, 4.4, ink));
    } else {
        row = row.child(art_shape(arrow, 4.4, ink));
        row = row.child(art_rules_on(&[11.5, 11.5, 11.5, 7.5], false, 3., ink));
    }
    row.into_any_element()
}

/// A caption under a picture in the palette.
///
/// AppKit's palette labels are tertiary: measured off a 2× capture, the caption
/// under a grid tile renders 88 (#585858) on the 253 sheet, and it is neutral
/// grey — the three channels agree exactly. `muted_foreground` is not a neutral
/// grey here: `project_theme` maps it to the cottheme's *comment* colour, which
/// is warm, so mixing that token (the old implementation) left the captions
/// (122, 116, 102) — visibly brown next to AppKit's grey. The label is
/// therefore derived from `foreground`, which is neutral by construction, at
/// the alpha that lands on 88 over a light sheet and follows the theme into
/// dark mode instead of pinning a value.
///
/// The strip below uses a lighter veil of its own (`strip_caption`): its
/// labels measure 111 over the 245 band, not 88.
fn palette_caption(cx: &App) -> Hsla {
    cx.theme().foreground.opacity(0.75)
}

/// A caption under a picture in the toolbar preview strip.
///
/// The strip sits on a darker band than the sheet, and the reference draws its
/// captions lighter there: 111 against the band's 245, where the grid's
/// captions land on 88 against the sheet's 253. Over the same near-black
/// foreground that is about 0.64 rather than 0.75. Same neutral grey, just a
/// step further back — the strip is a picture of the toolbar, not a control.
fn strip_caption(cx: &App) -> Hsla {
    cx.theme().foreground.opacity(0.64)
}

/// The width of one grid cell, in rem.
///
/// Every tile is `TILE_WIDTH` except the syntax one, which grows to hold the
/// pop-up: a `Button` clips its content to its own box, so the only way to show
/// a 151-point control is to give it a 151-point cell. The row the wide cell
/// sits in is measured from its cells too (`render_grid`), so the extra 26
/// points land after the pair beside it rather than being clipped away.
fn tile_width(item: ToolbarItemId) -> f32 {
    if item == ToolbarItemId::Syntax {
        SYNTAX_POPUP_WIDTH
    } else {
        TILE_WIDTH
    }
}

/// How the toolbar draws its items.
///
/// AppKit owns this as an `NSToolbar` display mode and persists it per window.
/// CotEditor never sets one, so the palette's 显示 pop-up is AppKit's own and
/// the value lives in the user defaults rather than in CotEditor's settings —
/// which is why SubText keeps its own copy here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolbarDisplayMode {
    /// The glyph with the item's label under it, each inside a capsule. AppKit's
    /// default for an `NSToolbar`, and what CotEditor's document window shows:
    /// upstream never calls `setDisplayMode(_:)`, so the toolbar keeps the
    /// `.iconAndLabel` AppKit gives it.
    #[default]
    IconAndText,
    /// The glyph alone, in a shorter bar.
    IconOnly,
    /// AppKit's name for label-only. On macOS 26 a toolbar built like
    /// CotEditor's draws this mode exactly like [`Self::IconAndText`] — see
    /// [`Self::title_bar_height`] — so the mode exists and is listed, but does
    /// not change the bar.
    TextOnly,
}

impl ToolbarDisplayMode {
    /// The palette's own name for the mode.
    pub fn label(self) -> &'static str {
        match self {
            Self::IconOnly => "Icon Only",
            Self::IconAndText => "Icon and Text",
            Self::TextOnly => "Text Only",
        }
    }

    /// Every mode, in the order the pop-up lists them — AppKit's order, which
    /// puts the pair first and the single representations after it.
    pub const ALL: [Self; 3] = [Self::IconAndText, Self::IconOnly, Self::TextOnly];

    /// The title bar height this mode needs, in rem.
    ///
    /// The number is the strip CotEditor's window actually draws, measured
    /// from a 2× capture: an item's bezel is 37 points tall and sits 7.5
    /// points below the top of the window, so the icon-only strip is
    /// 37 + 7.5 × 2 = 52 points, and the icon-and-text strip adds the caption
    /// band below it for 66 points. At the 16-point rem SubText uses those are
    /// 3.25 and 4.125 rem. The bezel height itself lives in
    /// `workspace::TOOLBAR_ITEM_HEIGHT`, which is what the items are drawn at;
    /// the two numbers have to move together.
    ///
    /// Label-only keeps the caption strip. Measured against a probe window
    /// built from CotEditor's item shapes (`NSToolbarItemGroup`s with
    /// `controlRepresentation = .expanded`, a toolbar pop-up button, and a
    /// plain image item), `.labelOnly` and `.iconAndLabel` captured
    /// byte-identical windows — 66 points to the separator — while
    /// `.iconOnly` measured 52. AppKit therefore does not give this toolbar a
    /// third shape, and neither does SubText.
    pub fn title_bar_height(self) -> f32 {
        match self {
            Self::IconAndText | Self::TextOnly => 4.125,
            Self::IconOnly => 3.25,
        }
    }
}

/// The glyph inside the text-size pair.
///
/// Upstream's two items use the SF Symbols `textformat.size.smaller` and
/// `.larger`, and those symbols have script-specific variants: the Latin one is
/// an "A" with a down or up arrow, the Han one is 小 / 大. CotEditor's Chinese
/// interface therefore shows 小 / 大, and so does SubText's. The toolbar and
/// the palette both draw the pair through this function, so the two cannot
/// disagree about which language is on.
pub(crate) fn text_size_glyph(smaller: bool) -> AnyElement {
    // Both branches inherit the size and colour of whatever draws them, which
    // is what keeps the character and the symbol looking like one pair.
    if localization::language() == localization::Language::Chinese {
        div()
            .child(if smaller { "小" } else { "大" })
            .into_any_element()
    } else {
        div()
            .child(if smaller {
                IconName::AArrowDown
            } else {
                IconName::AArrowUp
            })
            .into_any_element()
    }
}

/// One identifier of the document toolbar.
///
/// The names in `serde` are upstream's `NSToolbarItem.Identifier` raw values.
/// `Unknown` exists so a settings file written by a newer build — or edited by
/// hand — does not make the whole `Settings` fail to deserialize and be
/// replaced with defaults; [`sanitize`](Self::sanitize) drops it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolbarItemId {
    #[serde(rename = "syntaxStyle")]
    Syntax,
    #[serde(rename = "toggleInspector")]
    ToggleInspector,
    #[serde(rename = "toggleSidebar")]
    ToggleSidebar,
    #[serde(rename = "textSize")]
    TextSize,
    #[serde(rename = "writingDirection")]
    WritingDirection,
    #[serde(rename = "textOrientation")]
    TextOrientation,
    #[serde(rename = "indent")]
    Indent,
    #[serde(rename = "comment")]
    Comment,
    #[serde(rename = "tabStyle")]
    TabStyle,
    #[serde(rename = "wrapLines")]
    WrapLines,
    #[serde(rename = "invisibles")]
    Invisibles,
    #[serde(rename = "indentGuides")]
    IndentGuides,
    #[serde(rename = "keepOnTop")]
    KeepOnTop,
    #[serde(rename = "editable")]
    Editable,
    #[serde(rename = "spellCheck")]
    SpellCheck,
    #[serde(rename = "emojiAndSymbols")]
    EmojiAndSymbols,
    #[serde(rename = "fonts")]
    Fonts,
    #[serde(rename = "find")]
    Find,
    #[serde(rename = "print")]
    Print,
    #[serde(rename = "share")]
    Share,
    #[serde(rename = "space")]
    Space,
    #[serde(rename = "flexibleSpace")]
    FlexibleSpace,
    #[serde(other)]
    Unknown,
}

impl ToolbarItemId {
    /// Everything the palette offers, in the order upstream's
    /// `toolbarAllowedItemIdentifiers(_:)` lists it.
    ///
    /// Upstream's text-document list has twenty-one identifiers, and AppKit's
    /// palette then hides two of them: the immovable `toggleInspector`, and
    /// `writingToolsItemIdentifier`, which upstream drops when
    /// `NSWritingToolsCoordinator.isWritingToolsAvailable` is false. That
    /// leaves the nineteen tiles a CotEditor user sees, in rows of four.
    ///
    /// `ToggleSidebar` is therefore not here. Upstream offers it only for
    /// directory documents, where it sits among the immovable items; a text
    /// window's sidebar toggle is part of the split view's own chrome. SubText
    /// has no directory documents, so offering the item would add a twenty-first
    /// tile upstream does not have. The command is still reachable — from the
    /// 显示 menu and its shortcut — and the variant is still declared, because a
    /// toolbar saved with it has to keep rendering.
    pub const ALLOWED: [ToolbarItemId; 21] = [
        Self::Syntax,
        Self::ToggleInspector,
        Self::TextSize,
        Self::WritingDirection,
        Self::TextOrientation,
        Self::Indent,
        Self::Comment,
        Self::TabStyle,
        Self::WrapLines,
        Self::Invisibles,
        Self::IndentGuides,
        Self::KeepOnTop,
        Self::Editable,
        Self::SpellCheck,
        Self::EmojiAndSymbols,
        Self::Fonts,
        Self::Find,
        Self::Print,
        Self::Share,
        Self::Space,
        Self::FlexibleSpace,
    ];

    /// What a fresh window shows: upstream's `toolbarDefaultItemIdentifiers`
    /// for a text document.
    pub const DEFAULT: [ToolbarItemId; 3] =
        [Self::Syntax, Self::FlexibleSpace, Self::ToggleInspector];

    /// The items AppKit calls immovable: they cannot be taken out.
    pub const IMMOVABLE: [ToolbarItemId; 2] = [Self::FlexibleSpace, Self::ToggleInspector];

    /// The item's label, as the palette and the toolbar's accessible name show
    /// it. These are `Toolbar.*.label` from upstream's `Document` table.
    pub fn label(self) -> &'static str {
        match self {
            Self::Syntax => "Syntax",
            Self::ToggleInspector => "Inspector",
            Self::ToggleSidebar => "Sidebar",
            Self::TextSize => "Text Size",
            Self::WritingDirection => "Writing Direction",
            Self::TextOrientation => "Text Orientation",
            Self::Indent => "Indent",
            Self::Comment => "Comment",
            Self::TabStyle => "Tab Style",
            Self::WrapLines => "Line Wrapping",
            Self::Invisibles => "Invisibles",
            Self::IndentGuides => "Indent Guides",
            Self::KeepOnTop => "Keep on Top",
            Self::Editable => "Edit",
            Self::SpellCheck => "Spell Check",
            Self::EmojiAndSymbols => "Emoji & Symbols",
            Self::Fonts => "Fonts",
            Self::Find => "Find",
            Self::Print => "Print",
            Self::Share => "Share",
            Self::Space => "Space",
            Self::FlexibleSpace => "Flexible Space",
            Self::Unknown => "Unknown",
        }
    }

    /// The name the palette captions an item with, which is not always the name
    /// the toolbar's accessibility label uses.
    ///
    /// The one divergence is the fixed space. `label()` has to keep saying
    /// "Space", because `settings_window` binds that key to a text-insertion
    /// command and the toolbar strip reads it too; AppKit's palette captions
    /// its spacer with its own "间距" though, and `Fixed Space` is the free
    /// upstream string that already carries that wording.
    pub fn palette_label(self) -> &'static str {
        match self {
            Self::Space => "Fixed Space",
            _ => self.label(),
        }
    }

    /// A stable slug for element ids, so a row keeps its identity across a
    /// reorder. The settings name is what the row stands for.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Syntax => "syntaxStyle",
            Self::ToggleInspector => "toggleInspector",
            Self::ToggleSidebar => "toggleSidebar",
            Self::TextSize => "textSize",
            Self::WritingDirection => "writingDirection",
            Self::TextOrientation => "textOrientation",
            Self::Indent => "indent",
            Self::Comment => "comment",
            Self::TabStyle => "tabStyle",
            Self::WrapLines => "wrapLines",
            Self::Invisibles => "invisibles",
            Self::IndentGuides => "indentGuides",
            Self::KeepOnTop => "keepOnTop",
            Self::Editable => "editable",
            Self::SpellCheck => "spellCheck",
            Self::EmojiAndSymbols => "emojiAndSymbols",
            Self::Fonts => "fonts",
            Self::Find => "find",
            Self::Print => "print",
            Self::Share => "share",
            Self::Space => "space",
            Self::FlexibleSpace => "flexibleSpace",
            Self::Unknown => "unknown",
        }
    }

    /// The closest symbol in the Lucide set for upstream's SF Symbol.
    pub fn icon(self) -> IconName {
        match self {
            Self::Syntax => IconName::Languages,
            Self::ToggleInspector => IconName::PanelRight,
            Self::ToggleSidebar => IconName::PanelLeft,
            Self::TextSize => IconName::Type,
            Self::WritingDirection => IconName::TextAlignStart,
            Self::TextOrientation => IconName::AlignHorizontalJustifyCenter,
            Self::Indent => IconName::ListIndentIncrease,
            Self::Comment => IconName::TextQuote,
            Self::TabStyle => IconName::ArrowRightToLine,
            Self::WrapLines => IconName::TextWrap,
            Self::Invisibles => IconName::Pilcrow,
            Self::IndentGuides => IconName::ListTree,
            Self::KeepOnTop => IconName::Pin,
            Self::Editable => IconName::Pencil,
            Self::SpellCheck => IconName::SpellCheck2,
            Self::EmojiAndSymbols => IconName::FaceSlightlySmiling,
            Self::Fonts => IconName::CaseSensitive,
            Self::Find => IconName::Search,
            Self::Print => IconName::Printer,
            Self::Share => IconName::Share,
            Self::Space | Self::FlexibleSpace => IconName::Ellipsis,
            Self::Unknown => IconName::Ellipsis,
        }
    }

    /// Whether the item can be removed from the toolbar.
    pub fn is_immovable(self) -> bool {
        Self::IMMOVABLE.contains(&self)
    }

    /// A settings-file list made safe to render: no unknown identifiers, no
    /// duplicates, and both of AppKit's immovable items present.
    ///
    /// The immovable ones are restored rather than rejected, because a toolbar
    /// with no flexible space and no inspector toggle is not a toolbar CotEditor
    /// would ever show — and a hand-edited settings file is the only way to make
    /// one. A missing flexible space goes immediately before the inspector
    /// toggle, which is where the default set puts it.
    pub fn sanitize(items: &[ToolbarItemId]) -> Vec<ToolbarItemId> {
        let mut sanitized: Vec<ToolbarItemId> = Vec::with_capacity(items.len() + 2);
        for &item in items {
            if item != Self::Unknown && !sanitized.contains(&item) {
                sanitized.push(item);
            }
        }
        for required in Self::IMMOVABLE {
            if sanitized.contains(&required) {
                continue;
            }
            let at = if required == Self::FlexibleSpace {
                sanitized
                    .iter()
                    .position(|item| *item == Self::ToggleInspector)
                    .unwrap_or(sanitized.len())
            } else {
                sanitized.len()
            };
            sanitized.insert(at, required);
        }
        sanitized
    }

    /// Move the item at `from` to `to`, where `to` is the index it should end
    /// up at after the move.
    ///
    /// The same "drop on a row means take that row's place" rule the encoding
    /// list uses. A drag whose indices do not fit the list is a no-op rather
    /// than a panic: the list can change under a drag that was started a frame
    /// earlier.
    pub fn move_item(items: &mut Vec<ToolbarItemId>, from: usize, to: usize) {
        if from >= items.len() || to >= items.len() || from == to {
            return;
        }
        let item = items.remove(from);
        items.insert(to, item);
    }
}

/// The toolbar customization palette.
///
/// AppKit's sheet, drawn the way AppKit draws it: an instruction, a grid of
/// every item the user can add — each tile a picture of the control it becomes
/// — a preview of the toolbar as it now stands, and a footer with the display
/// mode and *Done*.
pub struct ToolbarSheet {
    workspace: WeakEntity<Workspace>,
    /// The items in the toolbar, in order. The sheet owns a copy so a click can
    /// add or remove one and the preview can re-render without asking the
    /// workspace; every edit is pushed back through
    /// [`Workspace::set_toolbar_items`], which sanitizes and saves it.
    items: Vec<ToolbarItemId>,
    /// The document's name, drawn at the leading edge of the preview strip.
    /// AppKit shows the toolbar's own window there, which is what makes the
    /// strip read as a toolbar rather than as a row of buttons.
    document_name: SharedString,
    /// The active syntax's name, shown inside the syntax tile the way AppKit's
    /// live `NSPopUpButton` shows it.
    syntax_name: SharedString,
    /// What the 显示 pop-up shows and changes.
    display_mode: ToolbarDisplayMode,
}

impl ToolbarSheet {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        items: Vec<ToolbarItemId>,
        document_name: impl Into<SharedString>,
        syntax_name: impl Into<SharedString>,
        display_mode: ToolbarDisplayMode,
    ) -> Self {
        Self {
            workspace,
            items: ToolbarItemId::sanitize(&items),
            document_name: document_name.into(),
            syntax_name: syntax_name.into(),
            display_mode,
        }
    }

    /// The items currently in the toolbar, for tests and for the workspace.
    pub fn items(&self) -> &[ToolbarItemId] {
        &self.items
    }

    /// Add an item, if it is not already in the toolbar.
    ///
    /// It goes immediately left of the inspector toggle, which is the one item
    /// that is always at the trailing edge — that way a new item is visible
    /// rather than appended past the inspector. It can then be dragged anywhere
    /// else on the toolbar.
    pub fn add(&mut self, item: ToolbarItemId) {
        if item == ToolbarItemId::Unknown || self.items.contains(&item) {
            return;
        }
        let at = self
            .items
            .iter()
            .position(|existing| *existing == ToolbarItemId::ToggleInspector)
            .unwrap_or(self.items.len());
        self.items.insert(at, item);
    }

    /// Insert an item at a row of the toolbar, where `to` is the index the drop
    /// landed on.
    ///
    /// The real toolbar's slots and the preview strip both report drops here.
    /// Dropping a tile onto a row puts the item there: a new item is inserted,
    /// and one that is already in the toolbar is moved, so a drag out of the
    /// grid is also a way to reposition an item that is already showing. An
    /// index past the end appends.
    pub fn insert_at(&mut self, item: ToolbarItemId, to: usize) {
        if item == ToolbarItemId::Unknown {
            return;
        }
        let to = to.min(self.items.len());
        if let Some(from) = self.items.iter().position(|existing| *existing == item) {
            let last = self.items.len().saturating_sub(1);
            ToolbarItemId::move_item(&mut self.items, from, to.min(last));
        } else {
            self.items.insert(to, item);
        }
    }

    /// Take an item out, unless it is immovable.
    pub fn remove(&mut self, item: ToolbarItemId) {
        if item.is_immovable() {
            return;
        }
        self.items.retain(|existing| *existing != item);
    }

    /// Put the shipped set back.
    pub fn reset(&mut self) {
        self.items = ToolbarItemId::DEFAULT.to_vec();
    }

    /// Move an item, as a drop on a row of the real toolbar reports it.
    ///
    /// The preview strip is a picture, as AppKit's is: dragging a row around
    /// inside it does nothing. Only the toolbar itself reorders.
    pub fn move_row(&mut self, from: usize, to: usize) {
        ToolbarItemId::move_item(&mut self.items, from, to);
    }

    /// Push the current list to the workspace, which sanitizes and saves it.
    ///
    /// A sheet built without a workspace still re-renders; it just has nowhere
    /// to save to.
    pub(crate) fn write(&mut self, cx: &mut Context<Self>) {
        self.items = ToolbarItemId::sanitize(&self.items);
        let items = self.items.clone();
        self.workspace
            .update(cx, move |workspace, cx| {
                workspace.set_toolbar_items(items, cx)
            })
            .ok();
        cx.notify();
    }

    /// The grid of items the user can add.
    ///
    /// The two immovable items are left out: AppKit does not offer them in the
    /// palette either, because there is no state in which they are absent.
    fn render_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let tiles: Vec<ToolbarItemId> = ToolbarItemId::ALLOWED
            .into_iter()
            .filter(|item| !item.is_immovable())
            .collect();
        // The block is a fixed width, and its rows are left-aligned inside it,
        // because that is what AppKit does: the last row has three tiles and
        // they line up under the first three of the rows above, rather than
        // spreading out to the row's own centre. Centring each row separately
        // — which is what this did before — moves the short row half a tile to
        // the right and reads as a mistake.
        //
        // A row is as wide as its own cells, which is how the syntax pop-up's
        // 151-point cell fits: that row runs 26 points past the block instead
        // of squeezing its cells, and the cells after the pop-up slide along
        // with it. The block's left edge still lines the rows up.
        let mut grid = v_flex().w(rems(GRID_WIDTH)).gap_2();
        for row in tiles.chunks(GRID_COLUMNS) {
            let row_width = row.iter().copied().map(tile_width).sum::<f32>()
                + TILE_GAP * row.len().saturating_sub(1) as f32;
            let mut line = h_flex().w(rems(row_width)).gap_2();
            for item in row.iter().copied() {
                line = line.child(self.render_tile(item, cx));
            }
            grid = grid.child(line);
        }
        h_flex()
            .w_full()
            .justify_center()
            .child(grid)
            .into_any_element()
    }

    /// One tile: the item's control above its name.
    ///
    /// The tile carries no frame. AppKit's palette draws the control and its
    /// caption straight onto the sheet, with nothing around them, so a bordered
    /// card here reads as a different, heavier widget. `ghost` keeps the hover
    /// and pressed states — the tile still has to look clickable — without the
    /// resting border.
    ///
    /// The whole tile is one button. AppKit makes the tile's control live — the
    /// syntax pop-up opens from the palette — but a live control inside a
    /// button cannot be clicked without also toggling membership, so here the
    /// tile is a picture and the click is the one action the palette has:
    /// in or out.
    fn render_tile(&self, item: ToolbarItemId, cx: &mut Context<Self>) -> AnyElement {
        let in_use = self.items.contains(&item);
        let label = localization::t(item.palette_label());
        let button = Button::new(("toolbar-tile", item as usize))
            .ghost()
            .compact()
            // The compact button insets its content by 8 points a side. The
            // palette's pictures are sized to the tile, so that inset would
            // shrink the cell's content box and the syntax pop-up's name would
            // end up sitting on its outline. The tile supplies its own spacing
            // through the grid's gaps instead.
            .px_0()
            .w(rems(tile_width(item)))
            .h(rems(TILE_HEIGHT))
            // An item already in the toolbar is drawn at half opacity, the way
            // AppKit draws it. In the reference the seven tiles whose items are
            // in the toolbar measure ink 145 and caption 170, against 38 and 88
            // for the twelve that are not; each pair sits exactly halfway to the
            // 253 sheet, which is what a 50% group opacity does to picture and
            // caption alike. The tile still clicks — it removes the item — and
            // its tooltip says so, so the dimming reads as "already in", not as
            // "unavailable".
            .accessibility_label(label)
            .tooltip(if in_use {
                localization::t("Remove from the toolbar")
            } else {
                localization::t("Add")
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.items.contains(&item) {
                    this.remove(item);
                } else {
                    this.add(item);
                }
                this.write(cx);
            }))
            .child(
                v_flex()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    // The sheet's own 50% marker for "already in the toolbar",
                    // measured off the reference in the comment above.
                    .opacity(if in_use { 0.5 } else { 1.0 })
                    .child(self.render_representation(item, cx))
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_center()
                            // The same 11-point caption the strip uses; the
                            // reference's tile labels measure 41 pixels for two
                            // characters at 2x.
                            .text_size(px(11.))
                            .line_height(rems(0.8125))
                            .text_color(palette_caption(cx))
                            .child(label),
                    ),
            )
            .into_any_element();

        // The tile is also a drag source, so a hand that reaches for the grid
        // can drag the item down to the strip. The click above is the shortcut
        // for the same thing; both end in `add`.
        div()
            .id(("toolbar-tile-source", item as usize))
            .flex_none()
            .on_drag(
                ToolbarDrag::Palette {
                    item,
                    label: label.into(),
                },
                |drag: &ToolbarDrag, _, _, cx| {
                    let label = drag.label();
                    cx.new(|_| DragPreview { label })
                },
            )
            .child(button)
            .test_support()
            .into_any_element()
    }

    /// The picture of an item's control.
    ///
    /// AppKit distinguishes the items that are a *pair* or a *pop-up* — text
    /// size, writing direction, indent and the like — by drawing them as one
    /// capsule around two or more glyphs, with a hairline between the halves; a
    /// single-button item is a circle. The two spacers are drawn as empty
    /// boxes, which is how the palette shows a gap.
    fn render_representation(&self, item: ToolbarItemId, cx: &mut Context<Self>) -> AnyElement {
        // AppKit's palette splits its greys: the *pictures* of the controls are
        // drawn in near-black ink (measured 40–43 in a 2× capture) while the
        // captions under them are tertiary grey. `muted_foreground` is neither
        // — it is the secondary label — and it never actually reached the
        // glyphs anyway: an `Icon` renders its SVG with `cx.theme().foreground`
        // or the window's text colour and does not inherit a parent div's
        // `text_color`. So the ink is named here, on the icon, and the intent
        // and the pixels agree.
        let ink = cx.theme().foreground;
        // The outlines around the pictures are one *device* pixel of AppKit's
        // quaternary separator — measured at 227 on the 253 sheet, and one
        // pixel wide at 2×, i.e. half a point. `cx.theme().border` is not that:
        // `project_theme` maps it to the cottheme's *invisibles* colour (191),
        // and `border_1()` draws it two device pixels wide, which is why the
        // palette's controls read as drawn with a felt pen beside the
        // reference. A tenth of the ink over the sheet lands on 227 and follows
        // the theme into dark mode the way AppKit's quaternary does.
        let hairline = ink.opacity(0.12);

        // Whether a tile is dimmed is not this function's business: the sheet
        // dims every item that is already in the toolbar, and `render_tile`
        // applies the measured 50% group opacity to the whole tile. The dim is
        // a uniform veil over picture and caption, never a change to the art,
        // so the pictures here stay in full ink.

        // The controls here are drawn *on* the sheet, not inside a filled chip:
        // the reference's grid measures 252–253 inside the capsule and circle
        // outlines against a sheet of 253, so it carries no fill of its own and
        // the hairline above is the only thing that makes a picture read as a
        // control. The empty boxes are left open too, and those are gaps drawn
        // as gaps.
        //
        // Leaving the fill off also lets the tile's own hover tint show through
        // the control instead of stopping at its outline. The strip below is the
        // other case — its controls stand on a darker band, so they are filled
        // one step above it (`STRIP_CONTROL_STEP`).

        // The capsule around a group of glyphs. Its width is the caller's,
        // because the reference gives each pop-up a different one: 75 for the
        // pairs, 82 for the indent, and 151 for the syntax pop-up, which is
        // wide enough that its tile has to grow with it (`tile_width`).
        let capsule = |width: f32, content: AnyElement| {
            h_flex()
                .w(rems(width))
                .h(rems(TILE_CONTROL))
                .px_1p5()
                .flex_none()
                .items_center()
                .justify_center()
                .gap_1()
                .rounded_full()
                .border(px(0.5))
                .border_color(hairline)
                .text_color(ink)
                .child(content)
                .into_any_element()
        };
        // The hairline that splits a two-glyph capsule into two buttons. AppKit
        // draws it between the halves, inset from the capsule's top and bottom,
        // and without it the pair reads as one wide button with two pictures in
        // it — which is what our palette showed before. Unlike the capsule's
        // outline this one is a full point: two device pixels at 2×, measured
        // at 227.
        let divider = || div().flex_none().w(px(1.)).h(rems(1.25)).bg(hairline);
        // A pair is two equal buttons, not a tight cluster of two pictures: the
        // capture puts each half's ink in the middle of its own segment, 12
        // points in from the capsule's end, with 20.5 points of paper between
        // the two pictures and the divider centred in it. Centring the pair as
        // one group instead leaves 19.5 points of padding and only 9 between
        // the pictures, which reads as one wide button with two glyphs.
        let pair_halves = |left: AnyElement, right: AnyElement| {
            h_flex()
                .w_full()
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .justify_center()
                        .child(left),
                )
                .child(divider())
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .justify_center()
                        .child(right),
                )
                .into_any_element()
        };
        // The outline around a single glyph: a circle, not a rounded square.
        // The palette in the capture draws every one-button item as a circle
        // roughly as tall as a capsule.
        let circle = |content: AnyElement| {
            div()
                .w(rems(TILE_CONTROL))
                .h(rems(TILE_CONTROL))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(0.5))
                .border_color(hairline)
                .text_color(ink)
                .child(content)
                .into_any_element()
        };
        // Sized explicitly: an unsized icon renders at the ambient text size,
        // about 13 points, and the reference palette draws these at 18 — the
        // glyphs read as pictures of controls, not as small type.
        //
        // The colour goes on the `Icon` itself: `Icon::render` resolves its
        // tint from `cx.theme().foreground` unless one is set, so a wrapper's
        // `text_color` would be silently ignored.
        let glyph = |icon: IconName| {
            Icon::new(icon)
                .size(px(18.))
                .text_color(ink)
                .into_any_element()
        };
        // The frame of a two-button control, fixed at the caller's width inside
        // a 125-point cell, so the rows read as a grid of controls rather than
        // of glyph clusters.
        let pair_frame = |width: f32, content: AnyElement| {
            h_flex()
                .w(rems(width))
                .h(rems(TILE_CONTROL))
                .flex_none()
                .items_center()
                .justify_center()
                .gap_1()
                .rounded_full()
                .border(px(0.5))
                .border_color(hairline)
                .child(content)
                .into_any_element()
        };
        // A picture that is cheaper as a path than as boxes — a solid arrow, the
        // comment mark, the tab glyph. Rasterised at `size` and tinted like any
        // icon, so it follows the theme. The rules-based pair pictures live
        // above, shared with the live toolbar.
        let shape = |svg: &[u8], size: f32| art_shape(svg, size, ink);

        match item {
            ToolbarItemId::Syntax => {
                let name = if self.syntax_name.is_empty() {
                    SharedString::from(localization::t("Syntax"))
                } else {
                    self.syntax_name.clone()
                };
                capsule(
                    SYNTAX_POPUP_WIDTH,
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .justify_between()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(14.))
                                // The pop-up's title is drawn in the control's
                                // own ink. `muted_foreground` looked plausible
                                // but is the cottheme's comment colour, so the
                                // name picked up a warm cast the reference does
                                // not have.
                                .text_color(ink)
                                .child(name),
                        )
                        .child(shape(CHEVRONS_UP_DOWN, 9.))
                        .into_any_element(),
                )
            }
            // The pair's own glyphs are script-dependent, so it is built here
            // rather than through `pair`.
            ToolbarItemId::TextSize => pair_frame(
                PAIR_WIDTH,
                h_flex()
                    // The two characters are the picture; AppKit leaves a
                    // character's worth of air on either side of the divider
                    // so they read as two buttons and not as one word.
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(16.))
                            .text_color(ink)
                            .child(text_size_glyph(true)),
                    )
                    .child(divider())
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(16.))
                            .text_color(ink)
                            .child(text_size_glyph(false)),
                    )
                    .into_any_element(),
            ),
            // Four bars per side, the long and short ones alternating and each
            // side flush against its end of the capsule: the left block reads
            // as text that starts at the margin, the right one as text that
            // ends at it. AppKit greys this picture out — the command acts on
            // the selection, and the palette shows it as unavailable.
            ToolbarItemId::WritingDirection => pair_frame(
                PAIR_WIDTH,
                pair_halves(
                    writing_direction_glyph(false, ink),
                    writing_direction_glyph(true, ink),
                ),
            ),
            // Horizontal text on the left of the divider, vertical text on the
            // right: stacked bars against strokes that stand on end, both
            // starting from the top so the contrast reads as rotation.
            ToolbarItemId::TextOrientation => pair_frame(
                PAIR_WIDTH,
                pair_halves(
                    text_orientation_glyph(false, ink),
                    text_orientation_glyph(true, ink),
                ),
            ),
            // A block of four lines with a solid triangle pushing into it from
            // the right (outdent) or from the left (indent). AppKit's arrows
            // are filled here, not the outlined chevrons the Lucide set has.
            ToolbarItemId::Indent => pair_frame(
                INDENT_WIDTH,
                pair_halves(indent_glyph(true, ink), indent_glyph(false, ink)),
            ),
            // Two slashes and four lines of code — the mark CotEditor gives
            // its comment command, not a quotation mark. The lines are drawn
            // into the picture so they stay on the reference's 9-pixel pitch.
            ToolbarItemId::Comment => circle(shape(COMMENT_MARK, 20.5)),
            // A pop-up with two glyphs — the tab style and its menu — drawn
            // like every other pair: a divider between the halves says the two
            // do different things. Without it the tile reads as a single
            // button with an unexplained arrow.
            ToolbarItemId::TabStyle => capsule(
                PAIR_WIDTH,
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(shape(TAB_GLYPH, 22.))
                    .child(divider())
                    .child(glyph(IconName::ChevronDown))
                    .into_any_element(),
            ),
            // The four symbol assets the system supplies for these commands,
            // plus the indent guide. All five go through `symbol_art` with the
            // command off, which is the state AppKit's palette draws — so the
            // picture here and the picture on the live toolbar are one
            // definition, slashes included.
            //
            // The size is the box *width* in points, and GPUI rasters an SVG
            // at twice that width (the smooth-svg scale factor) and lets the
            // height follow the viewBox. Each custom glyph therefore asks for
            // half the ink it should occupy on screen — 26, 39 and 27 pixels
            // in the reference are 13, 13 and 13.5 points here.
            ToolbarItemId::Invisibles
            | ToolbarItemId::WrapLines
            | ToolbarItemId::Editable
            | ToolbarItemId::KeepOnTop
            | ToolbarItemId::IndentGuides => {
                let (svg, size) =
                    symbol_art(item, false).expect("every item in this arm has system art");
                circle(shape(&svg, size))
            }
            // "Abc" over a dotted rule: the spell checker's picture, which is
            // type rather than a tick — the tick belongs to the *state* of the
            // command, and the palette shows the command.
            ToolbarItemId::SpellCheck => capsule(3.125, spell_check_glyph(ink)),
            // The font menu's picture in the system sheet is the word for
            // "format" set in an oval, not an "Aa" and not a typeface glyph.
            // The word is wider than a circle, so the oval grows with it
            // (`FONTS_WORD_WIDTH`); English keeps the "Aa" CotEditor puts on
            // the item itself, in the 37-point circle.
            ToolbarItemId::Fonts => capsule(
                if localization::language() == localization::Language::Chinese {
                    FONTS_WORD_WIDTH
                } else {
                    TILE_CONTROL
                },
                fonts_glyph(),
            ),
            // A fixed space is a gap you can see, and the palette draws it as an
            // empty square — square corners, unlike every other control here.
            // Measured off the 2× capture: 64 device pixels a side (32 pt) with
            // a full-point outline at 204, both smaller and darker than a tile's
            // 37 pt capsule. Everything else in the sheet is a capsule or
            // hairline, so the square has to be told apart *by* those two
            // numbers rather than by reusing `TILE_CONTROL` and `hairline`.
            ToolbarItemId::Space => div()
                .w(rems(2.0))
                .h(rems(2.0))
                .flex_none()
                .border(px(1.))
                .border_color(ink.opacity(0.24))
                .into_any_element(),
            // Unreachable from the grid — the flexible space is immovable — but
            // the preview strip draws it, so the arm stays for the match's sake
            // and must not drift from the strip's picture.
            //
            // AppKit does not draw the flexible space like the fixed one: the
            // box is 32×29 points rather than 32 square, its outline is a full
            // point at quaternary grey, and it holds a dotted double-headed
            // arrow instead of being empty. The box is deliberately not
            // `TILE_CONTROL` tall — it is the one picture on the sheet that is
            // wider than it is high.
            ToolbarItemId::FlexibleSpace => div()
                .w(rems(2.0))
                .h(rems(1.8125))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .border(px(1.))
                .border_color(ink.opacity(0.24))
                .child(shape(STRETCH_ARROW, 28.))
                .into_any_element(),
            // The emoji picker's face is the one symbol here that is a picture
            // of a *character*, not of a command's effect: AppKit draws it
            // 17.25 points across inside the 37-point circle, larger than the
            // type it sits beside because it is an illustration, not a label.
            ToolbarItemId::EmojiAndSymbols => circle(face_smiling_glyph(17.25, ink)),
            // Every remaining single-button item is its own circle, including
            // the inspector. `symbol_art` supplies the inspector's picture and
            // the `glyph` fallback covers the rest of the Lucide set.
            _ => circle(match symbol_art(item, false) {
                Some((svg, size)) => shape(&svg, size),
                None => glyph(item.icon()),
            }),
        }
    }

    /// The picture of the shipped toolbar, which is also a drag source.
    ///
    /// AppKit ends the palette with a drawing of the default toolbar under the
    /// caption "… or drag the default set into the toolbar." Dragging the
    /// drawing itself restores the shipped set in the real toolbar above;
    /// dragging one of its items inserts that item. The row is *fixed* —
    /// unlike the old preview, it does not follow the user's current toolbar,
    /// because the real toolbar is now a few dozen points above it and is the
    /// surface every edit lands on. That also matches the reference: its strip
    /// stays the shipped set while the toolbar it describes is being edited.
    fn render_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        // The strip's own drop target: the space to the left of the items is
        // most of the strip, so it has to accept a tile as well. A row dragged
        // out of the real toolbar and dropped anywhere in the palette removes
        // that item, which is why `Row` is accepted here too.
        let sheet = cx.entity().downgrade();
        let dropped = move |drag: &ToolbarDrag, _window: &mut Window, cx: &mut App| {
            let sheet = sheet.clone();
            sheet
                .update(cx, |sheet, cx| {
                    match drag {
                        ToolbarDrag::Palette { item, .. } => sheet.add(*item),
                        ToolbarDrag::Row { item, .. } => sheet.remove(*item),
                        ToolbarDrag::DefaultSet => sheet.reset(),
                    }
                    sheet.write(cx);
                })
                .ok();
        };
        let default_set = ToolbarDrag::DefaultSet;

        // AppKit's default toolbar list is syntax, tracking separator,
        // flexible space, inspector — and the separator is an *item*: it
        // always follows the syntax pop-up and moves to the front when that
        // pop-up is gone. Upstream keeps it in the immovable set, never in the
        // palette, which is why it is drawn here instead of being stored.
        let separator_at = Some(1);
        let hairline = cx.theme().foreground.opacity(0.08);
        // 28 points, not the 37 of a control: the reference strip's line is a
        // divider between items rather than an item's own height, and it is
        // lighter than the frame the window draws around the same separator.
        let separator = || div().w(px(1.)).h(rems(1.75)).flex_none().bg(hairline);

        let mut items = h_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_2()
            .justify_end()
            .on_drop::<ToolbarDrag>(dropped.clone());
        for (index, item) in ToolbarItemId::DEFAULT.into_iter().enumerate() {
            if separator_at == Some(index) {
                items = items.child(separator());
            }
            items = items.child(self.render_preview_item(index, item, cx));
        }
        if separator_at == Some(ToolbarItemId::DEFAULT.len()) {
            items = items.child(separator());
        }

        // The whole band is the default-set drag source. A drag that starts on
        // one of its items is claimed by the item, which is a nearer source;
        // the gaps around them are the band's.
        let band = h_flex()
            .w_full()
            .items_center()
            .gap_3()
            // 18 points at the ends: the reference band leaves that much
            // between its last control and its right edge, and the band is the
            // one element in the sheet whose padding the capture pins down.
            .px(rems(1.125))
            // Asymmetric on purpose: the reference's 78-point band leaves 13.5
            // points above the controls but only 10.5 under the caption's line
            // box, which is what makes the caption look attached to the control
            // above it rather than centred in the band.
            .pt(px(13.5))
            .pb(px(10.5))
            .rounded_lg()
            // The strip is a band a shade below the sheet, and both derive from
            // `sheet_fill` — one token, so the relationship holds whatever the
            // theme does rather than pinning a fixed grey that could fall on
            // the wrong side of the sheet. `BAND_STEP` carries the measurement.
            .bg(sheet_fill(cx).darken(BAND_STEP))
            // Dragging the band itself is AppKit's "drag the default set into
            // the toolbar": dropping it on the toolbar replaces the user's list
            // with the shipped one. The id is the one the UI test points at.
            .id("toolbar-default-set-source")
            .cursor_grab()
            .on_drag(default_set, |drag: &ToolbarDrag, _, _, cx| drag.preview(cx))
            .on_drop::<ToolbarDrag>(dropped)
            .child(
                // The document's name is the one piece of type in the strip that
                // is not a label, so it carries weight: AppKit sets it bold and
                // dark while the item captions stay grey.
                div()
                    .flex_none()
                    .max_w(rems(12.))
                    .truncate()
                    .text_sm()
                    .font_bold()
                    .child(self.document_name.clone()),
            )
            .child(items);
        band.into_any_element()
    }

    fn render_preview_item(
        &self,
        index: usize,
        item: ToolbarItemId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // A strip item is a copy of the item in the shipped set, so dragging it
        // is a *palette* drag: it inserts that item, it does not reorder the
        // strip (which is a picture and cannot be reordered). Reordering happens
        // on the real toolbar above, which is the surface AppKit reorders too.
        let dragged = ToolbarDrag::Palette {
            item,
            label: localization::t(item.palette_label()).into(),
        };
        let dropped = {
            let sheet = cx.entity().downgrade();
            move |dragged: &ToolbarDrag, _window: &mut Window, cx: &mut App| {
                sheet
                    .update(cx, |sheet, cx| {
                        match dragged {
                            ToolbarDrag::Palette { item, .. } => sheet.insert_at(*item, index),
                            // A row dragged off the real toolbar and dropped on
                            // the palette is gone from the toolbar — the other
                            // half of AppKit's drag in/out gesture.
                            ToolbarDrag::Row { item, .. } => sheet.remove(*item),
                            ToolbarDrag::DefaultSet => sheet.reset(),
                        }
                        sheet.write(cx);
                    })
                    .ok();
            }
        };
        let target = cx.theme().accent.opacity(0.15);
        // The strip's controls are a step above both the sheet and the band it
        // sits on: in the reference the capsule measures 251 against a 234 band
        // and a 240-242 sheet, so it reads as lifted back to the page's tone.
        // That is not the step the grid above uses — its controls sit directly
        // on the sheet and carry no fill at all — which is why the two surfaces
        // name their different values separately. `STRIP_CONTROL_STEP` is a
        // lighten, so in dark mode the capsule still sits above the band that
        // darkens below the sheet.
        let fill = sheet_fill(cx).lighten(STRIP_CONTROL_STEP);
        // AppKit's quaternary separator, the same 0.12 ink the palette's own
        // capsules are outlined with: 227 on a 253 sheet in the reference.
        let ink = cx.theme().foreground;
        let hairline = ink.opacity(0.12);
        let caption = strip_caption(cx);

        let body: AnyElement = match item {
            // The strip draws the syntax item as the pop-up it is, name and
            // chevron included, because that is the only way the preview can
            // say *which* syntax the window is on.
            ToolbarItemId::Syntax => {
                let name = if self.syntax_name.is_empty() {
                    SharedString::from(localization::t("Syntax"))
                } else {
                    self.syntax_name.clone()
                };
                h_flex()
                    .h(rems(TILE_CONTROL))
                    .w(rems(SYNTAX_POPUP_WIDTH))
                    .min_w_0()
                    // The title and the chevron sit at the capsule's two edges,
                    // not next to each other: in the reference the name's ink
                    // starts 13 points in and the chevron's ends 14 points from
                    // the other side, so a pop-up whose name is short still
                    // reads as a wide control waiting to be opened.
                    .px(rems(0.75))
                    .items_center()
                    .rounded_full()
                    .border(px(0.5))
                    .border_color(hairline)
                    .bg(fill)
                    .text_color(ink)
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .justify_between()
                            .items_center()
                            .gap_1()
                            .child(div().min_w_0().truncate().text_sm().child(name))
                            .child(
                                Icon::default()
                                    .data(CHEVRONS_UP_DOWN)
                                    .size(px(9.))
                                    .text_color(ink),
                            ),
                    )
                    .into_any_element()
            }
            // A fixed space is a gap, so the strip draws it as the empty square
            // the grid draws: same corners, same outline, no fill. Its outline
            // is heavier than a button's hairline — 198 on the band's 246 in the
            // reference — because an empty box needs a visible edge to be a box
            // at all.
            ToolbarItemId::Space => div()
                .w(rems(TILE_CONTROL))
                .h(rems(TILE_CONTROL))
                .flex_none()
                .border(px(1.))
                .border_color(ink.opacity(0.24))
                .into_any_element(),
            // The flexible space is the one box on the sheet that is wider than
            // it is high — 32×29 points, square-cornered, full 1-point outline —
            // and it holds AppKit's dotted double-headed arrow rather than being
            // empty. That contrast with the fixed space's empty square is the
            // whole point: it says "this one stretches". The box stays one item
            // wide rather than spanning the strip; the strip is a picture of the
            // toolbar, and a stretched box would claim room the item only takes
            // when the window is wide.
            // The box sits in a slot wider than itself. AppKit gives the
            // flexible-space item more room than either the box or its
            // caption: at 2x the reference leaves 39 pixels between the
            // separator and the box and 42 between the box and the inspector
            // circle, while the 8-point item gap plus a caption-sized slot
            // would leave 27. A 56-point slot (the caption measures 42.5)
            // with the 32-point box centred reproduces both gaps within a
            // point.
            ToolbarItemId::FlexibleSpace => h_flex()
                .w(rems(3.5))
                .h(rems(1.8125))
                .flex_none()
                .justify_center()
                .child(
                    div()
                        .w(rems(2.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .border(px(1.))
                        .border_color(ink.opacity(0.24))
                        .child(
                            Icon::default()
                                .data(STRETCH_ARROW)
                                .size(px(28.))
                                .text_color(ink.opacity(0.375)),
                        ),
                )
                .into_any_element(),
            // One-button items are circles here too, the same shape the grid
            // above draws them in — but filled, because the band is a shade
            // darker: the fill alone lifts them off it, and the hairline is the
            // same 0.5-point feint as the syntax capsule's.
            // The picture is the grid's picture — `symbol_art` is the one place
            // the two surfaces meet.
            _ => div()
                .w(rems(TILE_CONTROL))
                .h(rems(TILE_CONTROL))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border(px(0.5))
                .border_color(hairline)
                .bg(fill)
                .text_color(ink)
                .child(match symbol_art(item, false) {
                    Some((svg, size)) => Icon::default()
                        .data(&svg)
                        .size(px(size))
                        .text_color(ink)
                        .into_any_element(),
                    None => Icon::new(item.icon())
                        .size(px(18.))
                        .text_color(ink)
                        .into_any_element(),
                })
                .into_any_element(),
        };

        div()
            .id(("toolbar-preview-item", item as usize))
            .cursor_grab()
            .flex_none()
            .on_drag(dragged, |drag: &ToolbarDrag, _, _, cx| drag.preview(cx))
            .on_drop::<ToolbarDrag>(dropped)
            .drag_over::<ToolbarDrag>(move |style, _, _, _| style.bg(target))
            .child(
                // The caption is part of the strip the way it is in AppKit's
                // palette: the glyphs alone cannot say that the boxed arrow is
                // a flexible space and the circle is the inspector.
                v_flex().items_center().gap_1().child(body).child(
                    div()
                        .truncate()
                        // 11 points, not the 12 of `text_xs`: the reference
                        // caption's two characters measure 41 pixels at 2x, and
                        // the natural line height would make the band taller
                        // than the 78 points AppKit draws.
                        .text_size(px(11.))
                        .line_height(rems(0.8125))
                        .whitespace_nowrap()
                        .text_color(caption)
                        .child(localization::t(item.palette_label())),
                ),
            )
            // The strip is the only thing here that a hand can drag, and it is
            // the only part the reorder test can point at: a bare `div` with an
            // id never enters the observation registry, so `window.find` cannot
            // see it. `test_support` is the switch that puts it there, and it
            // has to come last — it hands back a wrapper, not a stateful div.
            .test_support()
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.display_mode;
        let sheet = cx.entity().downgrade();

        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().child(localization::t("Display")))
                    .child(
                        Button::new("toolbar-display-mode")
                            .small()
                            // A pop-up button, which AppKit draws grey-filled
                            // with the label and a stacked up/down chevron —
                            // measured 224 for the fill and 38 for the ink in
                            // the capture, i.e. the secondary style, not a
                            // tinted accent.
                            .secondary()
                            // AppKit sizes the pop-up to its longest title
                            // rather than to the current one: the capture's
                            // fill is 127 points wide whatever it says, with the
                            // title ink 13 points in from the left and the
                            // chevron ink 10.5 points in from the right. `min_w`
                            // holds that width, and the flexible spacer pushes
                            // the chevron to the right edge instead of letting it
                            // hug the title — `Button` centres label and children
                            // as one group, so a fixed spacer would simply widen
                            // the button with its content. Padding overrides the
                            // button's own 8/8: 12 on the left lands the ink
                            // where the capture has it, and 7 on the right
                            // compensates for the 14-point icon box centring a
                            // 7-point glyph.
                            .min_w(rems(7.9375))
                            .pl(rems(0.75))
                            .pr(rems(0.4375))
                            .label(localization::t(current.label()))
                            .child(div().flex_1())
                            .child(
                                Icon::new(IconName::ChevronsUpDown)
                                    .size(px(14.))
                                    .text_color(cx.theme().foreground),
                            )
                            .accessibility_label(localization::t("Display"))
                            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                                let mut menu = menu;
                                for mode in ToolbarDisplayMode::ALL {
                                    let sheet = sheet.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(localization::t(mode.label()))
                                            .checked(mode == current)
                                            .on_click(move |_, _, cx| {
                                                sheet
                                                    .update(cx, |sheet, cx| {
                                                        sheet.display_mode = mode;
                                                        sheet
                                                            .workspace
                                                            .update(cx, |workspace, cx| {
                                                                workspace.set_toolbar_display_mode(
                                                                    mode, cx,
                                                                )
                                                            })
                                                            .ok();
                                                        cx.notify();
                                                    })
                                                    .ok();
                                            }),
                                    );
                                }
                                menu
                            }),
                    ),
            )
            .child(
                h_flex().items_center().gap_2().child(
                    // 完成 is the sheet's default button, and AppKit paints
                    // its default button in the app accent: in the capture
                    // it is a 70-by-24-point fill of the asset catalog's
                    // accent — 108/166/9 — under a white label. `primary`
                    // is exactly that pair in `project_theme`, so this one
                    // really is a tinted button, unlike the sheet's other
                    // controls.
                    //
                    // Closing is the workspace's job, not a dialog's: the
                    // palette is an overlay the workspace draws, so there is
                    // no dialog to close.
                    Button::new("toolbar-done")
                        .small()
                        .primary()
                        .min_w(rems(4.375))
                        .label(localization::t("Done"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.workspace
                                .update(cx, |workspace, cx| workspace.close_toolbar_sheet(cx))
                                .ok();
                        })),
                ),
            )
            .into_any_element()
    }
}

impl Render for ToolbarSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // AppKit's sheet has no title bar and no close button: the caption at
        // the top left says what the surface is for, and Done or Escape is how
        // it goes away. Two captions, not one — the first explains the grid,
        // the second the strip that follows it, and each sits directly above
        // the part it explains.
        // The instructions are *not* palette labels: AppKit sets them in the
        // normal label colour — 44 in the capture — while everything under a
        // control stays tertiary grey. They are the only type on the sheet a
        // user is meant to read as a sentence, so they take the body ink; the
        // weight stays regular, and what makes them read heavy is the ink
        // against the strip's grey captions, not a bold face. The size is 13
        // points, one below the 14 of `text_sm`: the reference's first line
        // measures 301×23 pixels at 2× where ours measured 325×25, and every
        // glyph's advance scales by the same 13/14.
        let ink = cx.theme().foreground;
        let caption = |text: &'static str| {
            div()
                .w_full()
                .text_size(px(13.))
                .text_color(ink)
                .child(localization::t(text))
        };

        // A row dragged out of the real toolbar and released anywhere on the
        // palette is removed from the toolbar, which is the other half of
        // AppKit's drag-in / drag-out gesture. Tiles and the default set are
        // dropped *onto the toolbar*, so they are ignored here.
        let sheet = cx.entity().downgrade();
        let dropped = move |drag: &ToolbarDrag, _window: &mut Window, cx: &mut App| {
            let ToolbarDrag::Row { item, .. } = drag else {
                return;
            };
            sheet
                .update(cx, |sheet, cx| {
                    sheet.remove(*item);
                    sheet.write(cx);
                })
                .ok();
        };

        v_flex()
            .id("toolbar-sheet")
            .on_drop::<ToolbarDrag>(dropped)
            // The spacing is explicit because the sheet is a drawing of the
            // AppKit palette, whose gaps are unequal: the instruction sits close
            // to the grid it explains, the second half of the same sentence sits
            // further down over the strip, and the footer is separated by a
            // hairline with room on both sides. A uniform `gap_3` cannot express
            // that and would drift the sheet out of proportion. The quarter-rem
            // values are the capture's own gaps (9.5, 38.5, 13.5, 15.5 points),
            // which is why they are not on the 4-point scale the rest of the app
            // uses.
            .child(caption("Drag your favorite items into the toolbar…"))
            .child(div().h(rems(0.59375)))
            .child(self.render_grid(cx))
            .child(div().h(rems(2.40625)))
            .child(caption("… or drag the default set into the toolbar."))
            .child(div().h(rems(0.84375)))
            .child(self.render_preview(cx))
            .child(div().h(rems(0.96875)))
            .child(div().w_full().h(px(1.)).bg(cx.theme().border))
            .child(div().h(rems(1.)))
            .child(self.render_footer(cx))
            // As with the preview strip below the grid, an id alone never
            // enters the observation registry: without this a UI test cannot
            // tell an open palette from a closed one, and *Done*'s only
            // visible effect is exactly that difference.
            .test_support()
    }
}

/// What is in flight while the pointer is down.
///
/// Three things can be dragged while the palette is open: a row of the *real*
/// toolbar (reorder, or removal when dropped on the palette), a tile of the
/// grid or an item of the default-set strip (add), and the default set itself
/// (replace the whole list). They share one payload so that every drop target
/// can accept any of them, and the target — not the source — decides what the
/// drop means. The types are `pub(crate)` because the workspace owns the real
/// toolbar's drag sources and drop targets.
#[derive(Clone)]
pub(crate) enum ToolbarDrag {
    Row {
        from: usize,
        item: ToolbarItemId,
        label: SharedString,
    },
    Palette {
        item: ToolbarItemId,
        label: SharedString,
    },
    DefaultSet,
}

impl ToolbarDrag {
    /// The caption drawn under the pointer while this is in flight.
    fn label(&self) -> SharedString {
        match self {
            Self::Row { label, .. } | Self::Palette { label, .. } => label.clone(),
            Self::DefaultSet => localization::t("Default Set").into(),
        }
    }

    /// The row GPUI draws under the pointer.
    pub(crate) fn preview(&self, cx: &mut App) -> Entity<DragPreview> {
        let label = self.label();
        cx.new(|_| DragPreview { label })
    }
}

/// The row under the pointer while it is in flight.
pub(crate) struct DragPreview {
    label: SharedString,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md()
            .text_sm()
            .child(self.label.clone())
    }
}

/// A fixed-width spacer, upstream's `.space` item.
pub fn fixed_space() -> AnyElement {
    div().w(px(16.)).flex_none().into_any_element()
}

/// The space that pushes everything after it to the trailing edge, upstream's
/// `.flexibleSpace`.
pub fn flexible_space() -> AnyElement {
    div().flex_1().min_w_0().into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet with no workspace behind it: every edit is a pure list edit, so
    /// the rules can be driven without a document or a window.
    fn sheet(items: Vec<ToolbarItemId>) -> ToolbarSheet {
        ToolbarSheet::new(
            WeakEntity::new_invalid(),
            items,
            "",
            "",
            ToolbarDisplayMode::IconOnly,
        )
    }

    #[test]
    fn the_default_set_is_coteditors_text_document_toolbar() {
        // Upstream's `toolbarDefaultItemIdentifiers` for a document that is not
        // a directory: syntax, a flexible space, the inspector toggle.
        assert_eq!(
            ToolbarItemId::DEFAULT,
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
        assert_eq!(
            ToolbarItemId::sanitize(&ToolbarItemId::DEFAULT),
            ToolbarItemId::DEFAULT
        );
    }

    #[test]
    fn the_label_only_bar_keeps_the_caption_strip() {
        // Measured on macOS 26 with a window built from CotEditor's item
        // shapes: `.labelOnly` and `.iconAndLabel` are the same 66-point bar,
        // and only `.iconOnly` drops the caption band to 52 points.
        assert_eq!(ToolbarDisplayMode::IconAndText.title_bar_height(), 4.125);
        assert_eq!(ToolbarDisplayMode::TextOnly.title_bar_height(), 4.125);
        assert_eq!(ToolbarDisplayMode::IconOnly.title_bar_height(), 3.25);
    }

    #[test]
    fn the_immovable_items_are_the_ones_appkit_will_not_take_out() {
        assert_eq!(
            ToolbarItemId::IMMOVABLE,
            [ToolbarItemId::FlexibleSpace, ToolbarItemId::ToggleInspector]
        );
        assert!(ToolbarItemId::FlexibleSpace.is_immovable());
        assert!(ToolbarItemId::ToggleInspector.is_immovable());
        assert!(!ToolbarItemId::Syntax.is_immovable());
    }

    #[test]
    fn every_allowed_item_is_listed_once_and_has_its_own_identity() {
        let mut slugs: Vec<&str> = ToolbarItemId::ALLOWED
            .iter()
            .map(|item| item.slug())
            .collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), ToolbarItemId::ALLOWED.len());
        assert!(!slugs.contains(&"unknown"));
    }

    #[test]
    fn sanitize_drops_unknown_identifiers_and_duplicates() {
        let items = [
            ToolbarItemId::Syntax,
            ToolbarItemId::Unknown,
            ToolbarItemId::Comment,
            ToolbarItemId::Syntax,
            ToolbarItemId::FlexibleSpace,
            ToolbarItemId::ToggleInspector,
        ];
        assert_eq!(
            ToolbarItemId::sanitize(&items),
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::Comment,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
    }

    #[test]
    fn sanitize_puts_the_immovable_items_back() {
        // A hand-edited file that names only one item still gets the inspector
        // toggle, and a flexible space lands before it.
        assert_eq!(
            ToolbarItemId::sanitize(&[ToolbarItemId::Find]),
            [
                ToolbarItemId::Find,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
        assert_eq!(
            ToolbarItemId::sanitize(&[ToolbarItemId::Find, ToolbarItemId::ToggleInspector]),
            [
                ToolbarItemId::Find,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
        assert_eq!(ToolbarItemId::sanitize(&[]), ToolbarItemId::IMMOVABLE);
    }

    #[test]
    fn moving_an_item_puts_it_at_the_index_it_was_dropped_on() {
        let mut items = vec![
            ToolbarItemId::Syntax,
            ToolbarItemId::Comment,
            ToolbarItemId::Find,
            ToolbarItemId::FlexibleSpace,
        ];
        ToolbarItemId::move_item(&mut items, 0, 2);
        assert_eq!(
            items,
            [
                ToolbarItemId::Comment,
                ToolbarItemId::Find,
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace
            ]
        );
        ToolbarItemId::move_item(&mut items, 3, 0);
        assert_eq!(
            items,
            [
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::Comment,
                ToolbarItemId::Find,
                ToolbarItemId::Syntax
            ]
        );
    }

    #[test]
    fn a_move_that_does_not_fit_the_list_changes_nothing() {
        let mut items = vec![ToolbarItemId::Syntax, ToolbarItemId::FlexibleSpace];
        let before = items.clone();
        ToolbarItemId::move_item(&mut items, 5, 0);
        ToolbarItemId::move_item(&mut items, 0, 5);
        ToolbarItemId::move_item(&mut items, 1, 1);
        assert_eq!(items, before);
    }

    #[test]
    fn the_palette_adds_before_the_inspector_toggle_and_refuses_duplicates() {
        let mut sheet = sheet(ToolbarItemId::DEFAULT.to_vec());
        sheet.add(ToolbarItemId::Find);
        sheet.add(ToolbarItemId::Find);
        assert_eq!(
            sheet.items(),
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::Find,
                ToolbarItemId::ToggleInspector
            ]
        );
    }

    #[test]
    fn the_palette_will_not_remove_an_immovable_item() {
        let mut sheet = sheet(ToolbarItemId::DEFAULT.to_vec());
        sheet.remove(ToolbarItemId::FlexibleSpace);
        sheet.remove(ToolbarItemId::ToggleInspector);
        assert_eq!(sheet.items(), ToolbarItemId::DEFAULT);
        sheet.remove(ToolbarItemId::Syntax);
        assert_eq!(
            sheet.items(),
            [ToolbarItemId::FlexibleSpace, ToolbarItemId::ToggleInspector]
        );
    }

    #[test]
    fn default_set_restores_the_shipped_toolbar() {
        let mut sheet = sheet(vec![ToolbarItemId::Find, ToolbarItemId::FlexibleSpace]);
        sheet.reset();
        assert_eq!(sheet.items(), ToolbarItemId::DEFAULT);
    }

    #[test]
    fn the_saved_names_are_upstream_identifiers() {
        let names = serde_json::to_string(&[
            ToolbarItemId::Syntax,
            ToolbarItemId::FlexibleSpace,
            ToolbarItemId::ToggleInspector,
            ToolbarItemId::EmojiAndSymbols,
        ])
        .expect("serialize");
        assert_eq!(
            names,
            r#"["syntaxStyle","flexibleSpace","toggleInspector","emojiAndSymbols"]"#
        );

        // An identifier a newer version might add does not poison the file: it
        // becomes `Unknown`, and `sanitize` drops it.
        let parsed: Vec<ToolbarItemId> =
            serde_json::from_str(r#"["syntaxStyle","somethingNew","flexibleSpace"]"#)
                .expect("unknown identifiers are tolerated");
        assert_eq!(
            ToolbarItemId::sanitize(&parsed),
            [
                ToolbarItemId::Syntax,
                ToolbarItemId::FlexibleSpace,
                ToolbarItemId::ToggleInspector
            ]
        );
    }
}
