//! CSS colour codes.
//!
//! CotEditor's *Edit Color Code* panel opens the system colour panel with an
//! accessory view that turns the picked colour into a colour code and inserts
//! it at the caret. The conversion behind that accessory view lives in
//! `WFColorCode`, a separate MIT-licensed package; this module is a port of it,
//! so the eleven notations and — more importantly — the exact strings they
//! produce match CotEditor character for character.
//!
//! Three things are worth knowing before changing anything here:
//!
//! * **Component values are `0.0..=1.0`.** Not 0–255, not 0–100. That is what
//!   the upstream `RGB` / `HSL` / `HWB` types hold, and every formatting rule
//!   below is written against that range.
//! * **Hue is a fraction of a turn, not degrees.** Upstream stores `hue / 360`
//!   and multiplies back on the way out. Keeping the fraction means the
//!   conversion to GPUI's `Hsla` — whose `h` is also `0..1` — is a copy.
//! * **Detection order is significant.** `ColorCodeType::ALL` is tried in
//!   order, and the first notation that parses the string wins. `#ffffff` is
//!   a `Hex` rather than a `HexWithAlpha` only because `Hex` is tried first.

use std::sync::OnceLock;

use regex::Regex;

/// The colour-code notations CotEditor's panel offers.
///
/// The order of the variants is the order CotEditor declares them in, which is
/// also the order [`Color::from_code`] tries them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorCodeType {
    /// `#ffffff`
    #[default]
    Hex,
    /// `#ffffffaa`
    HexWithAlpha,
    /// `#fff`
    ShortHex,
    /// `#fffa`
    ShortHexWithAlpha,
    /// `rgb(255,255,255)`
    CssRgb,
    /// `rgba(255,255,255,1)`
    CssRgba,
    /// `hsl(0,0%,100%)`
    CssHsl,
    /// `hsla(0,0%,100%,1)`
    CssHsla,
    /// `hwb(0 0% 100%)`
    CssHwb,
    /// `hwb(0 0% 100% / 1)`
    CssHwbWithAlpha,
    /// `white`
    CssKeyword,
}

impl ColorCodeType {
    /// Every notation, in the order CotEditor tries them.
    pub const ALL: [Self; 11] = [
        Self::Hex,
        Self::HexWithAlpha,
        Self::ShortHex,
        Self::ShortHexWithAlpha,
        Self::CssRgb,
        Self::CssRgba,
        Self::CssHsl,
        Self::CssHsla,
        Self::CssHwb,
        Self::CssHwbWithAlpha,
        Self::CssKeyword,
    ];

    /// The notations the panel's *Hex* section holds.
    pub const HEX_TYPES: [Self; 4] = [
        Self::Hex,
        Self::HexWithAlpha,
        Self::ShortHex,
        Self::ShortHexWithAlpha,
    ];

    /// The notations the panel's *CSS* section holds.
    pub const CSS_TYPES: [Self; 7] = [
        Self::CssRgb,
        Self::CssRgba,
        Self::CssHsl,
        Self::CssHsla,
        Self::CssHwb,
        Self::CssHwbWithAlpha,
        Self::CssKeyword,
    ];

    /// The panel's menu label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Hex => "Hexadecimal",
            Self::HexWithAlpha => "Hexadecimal with Alpha",
            Self::ShortHex => "Hexadecimal (Short)",
            Self::ShortHexWithAlpha => "Hexadecimal with Alpha (Short)",
            Self::CssRgb => "CSS RGB",
            Self::CssRgba => "CSS RGBa",
            Self::CssHsl => "CSS HSL",
            Self::CssHsla => "CSS HSLa",
            Self::CssHwb => "CSS HWB",
            Self::CssHwbWithAlpha => "CSS HWB with Alpha",
            Self::CssKeyword => "CSS Keyword",
        }
    }

    /// The integer CotEditor stores for this notation in `colorCodeType`.
    ///
    /// The mapping is kept so a settings file written by CotEditor can be read
    /// back without a translation table.
    pub fn raw_value(self) -> i32 {
        match self {
            Self::Hex => 1,
            Self::HexWithAlpha => 2,
            Self::ShortHex => 3,
            Self::ShortHexWithAlpha => 4,
            Self::CssRgb => 5,
            Self::CssRgba => 6,
            Self::CssHsl => 7,
            Self::CssHsla => 8,
            Self::CssHwb => 9,
            Self::CssHwbWithAlpha => 10,
            Self::CssKeyword => 11,
        }
    }

    /// The notation a stored integer names, falling back to [`Self::Hex`].
    ///
    /// CotEditor's own default is `1`, so an unreadable or absent value landing
    /// on `Hex` is the same thing the upstream panel does.
    pub fn from_raw_value(value: i32) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.raw_value() == value)
            .unwrap_or(Self::Hex)
    }

    /// Whether this notation is written in hexadecimal.
    pub fn is_hex(self) -> bool {
        Self::HEX_TYPES.contains(&self)
    }
}

/// A colour in the sRGB space, with components in `0.0..=1.0`.
///
/// Deliberately not GPUI's `Hsla`: this type is what the parsing and formatting
/// rules are written against, and keeping it in `subtext-core` is what lets
/// them be tested without a window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl Color {
    pub const BLACK: Self = Self {
        red: 0.,
        green: 0.,
        blue: 0.,
        alpha: 1.,
    };

    pub const fn new(red: f64, green: f64, blue: f64, alpha: f64) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// A colour from 8-bit channels and an alpha in `0.0..=1.0`.
    pub fn from_rgba8(red: u8, green: u8, blue: u8, alpha: f64) -> Self {
        Self::new(
            f64::from(red) / 255.,
            f64::from(green) / 255.,
            f64::from(blue) / 255.,
            alpha,
        )
    }

    // ---- reading ---------------------------------------------------------

    /// The first notation that parses `code`, and the colour it names.
    ///
    /// The string is trimmed first and the notations are tried in
    /// [`ColorCodeType::ALL`] order, exactly as upstream's
    /// `ColorCodeType.colorComponents(colorCode:)` does.
    pub fn from_code(code: &str) -> Option<(ColorCodeType, Self)> {
        let code = code.trim();
        ColorCodeType::ALL
            .into_iter()
            .find_map(|kind| kind.parse(code).map(|color| (kind, color)))
    }

    // ---- writing ---------------------------------------------------------

    /// `code` written in `kind`, or `None` when it cannot be.
    ///
    /// Only two notations can fail: `ShortHex` and `ShortHexWithAlpha` need
    /// every channel to be a multiple of 17, and `CssKeyword` needs the colour
    /// to be one of the 148 named ones.
    pub fn code(self, kind: ColorCodeType) -> Option<String> {
        // Upstream formats `self.rgb.finite`. A colour that arrived with a
        // non-finite channel — which the picker can produce from a NaN alpha —
        // is written as zero rather than as the text `NaN`.
        let color = self.finite();
        match kind {
            ColorCodeType::Hex => Some(color.hex(false)),
            ColorCodeType::HexWithAlpha => Some(color.hex(true)),
            ColorCodeType::ShortHex => color.short_hex(false),
            ColorCodeType::ShortHexWithAlpha => color.short_hex(true),
            ColorCodeType::CssRgb => Some(color.css_rgb(false)),
            ColorCodeType::CssRgba => Some(color.css_rgb(true)),
            ColorCodeType::CssHsl => Some(color.css_hsl(false)),
            ColorCodeType::CssHsla => Some(color.css_hsl(true)),
            ColorCodeType::CssHwb => Some(color.css_hwb(false)),
            ColorCodeType::CssHwbWithAlpha => Some(color.css_hwb(true)),
            ColorCodeType::CssKeyword => {
                keyword_for_value(color.hex_value()).map(|keyword| keyword.to_string())
            }
        }
    }

    /// The colour with every non-finite channel replaced by zero.
    fn finite(self) -> Self {
        let finite = |value: f64| if value.is_finite() { value } else { 0. };
        Self::new(
            finite(self.red),
            finite(self.green),
            finite(self.blue),
            finite(self.alpha),
        )
    }

    /// The four channels as 8-bit values, rounded.
    ///
    /// Upstream's `byteComponents`, and the rounding is the same one: half away
    /// from zero, via `rounded()`.
    pub fn byte_components(self) -> [u8; 4] {
        let channel = |value: f64| {
            let value = if value.is_finite() { value } else { 0. };
            (255. * value).round().clamp(0., 255.) as u8
        };
        [
            channel(self.red),
            channel(self.green),
            channel(self.blue),
            channel(self.alpha),
        ]
    }

    /// The 24-bit `0xRRGGBB` value, alpha dropped.
    pub fn hex_value(self) -> u32 {
        let [red, green, blue, _] = self.byte_components();
        (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
    }

    /// Hue as a fraction of a turn, in `0.0..1.0`.
    ///
    /// Upstream's `RGB.hue`. The `green < blue` term is what keeps the value
    /// from going negative around the red end of the wheel.
    fn hue(self) -> f64 {
        let max = self.red.max(self.green).max(self.blue);
        let min = self.red.min(self.green).min(self.blue);
        let diff = max - min;
        // Upstream's `guard diff > 0`. Spelled out rather than negated because
        // a non-finite channel makes the difference `NaN`, and `NaN` is not a
        // hue either.
        if diff.is_nan() || diff <= 0. {
            return 0.;
        }

        let mut hue = if max == self.red {
            (self.green - self.blue) / diff + if self.green < self.blue { 6. } else { 0. }
        } else if max == self.green {
            (self.blue - self.red) / diff + 2.
        } else {
            (self.red - self.green) / diff + 4.
        };
        hue /= 6.;
        if hue >= 1. { hue - 1. } else { hue }
    }

    /// `(hue, saturation, lightness)`, hue as a fraction of a turn.
    fn hsl(self) -> (f64, f64, f64) {
        let max = self.red.max(self.green).max(self.blue);
        let min = self.red.min(self.green).min(self.blue);
        let diff = max - min;

        // The epsilon is upstream's: a hair of difference is not a colour.
        let saturation = if diff > 0.00001 {
            diff / (1. - ((max + min) - 1.).abs())
        } else {
            0.
        };
        let lightness = (max + min) / 2.;
        (self.hue(), saturation, lightness)
    }

    /// `(hue, whiteness, blackness)`, hue as a fraction of a turn.
    fn hwb(self) -> (f64, f64, f64) {
        let max = self.red.max(self.green).max(self.blue);
        let min = self.red.min(self.green).min(self.blue);
        (self.hue(), min, 1. - max)
    }

    // ---- formatting ------------------------------------------------------

    /// `#rrggbb`, or `#rrggbbaa`, in lower case.
    fn hex(self, with_alpha: bool) -> String {
        let bytes = self.byte_components();
        let bytes = if with_alpha { &bytes[..] } else { &bytes[..3] };
        let mut text = String::with_capacity(1 + bytes.len() * 2);
        text.push('#');
        for byte in bytes {
            text.push_str(&format!("{byte:02x}"));
        }
        text
    }

    /// `#rgb`, or `#rgba`, when every channel is a multiple of 17.
    fn short_hex(self, with_alpha: bool) -> Option<String> {
        let bytes = self.byte_components();
        let bytes = if with_alpha { &bytes[..] } else { &bytes[..3] };
        if !bytes.iter().all(|byte| byte % 17 == 0) {
            return None;
        }
        let mut text = String::with_capacity(1 + bytes.len());
        text.push('#');
        for byte in bytes {
            text.push_str(&format!("{:x}", byte / 17));
        }
        Some(text)
    }

    fn css_rgb(self, with_alpha: bool) -> String {
        let [red, green, blue, _] = self.byte_components();
        if with_alpha {
            format!("rgba({red},{green},{blue},{})", css_alpha(self.alpha))
        } else {
            format!("rgb({red},{green},{blue})")
        }
    }

    fn css_hsl(self, with_alpha: bool) -> String {
        let (hue, saturation, lightness) = self.hsl();
        // A colour with no saturation has no hue to speak of; upstream reports
        // 0 rather than whatever the arithmetic happened to leave behind.
        let hue = if saturation > 0. {
            (360. * hue).round()
        } else {
            0.
        };
        let saturation = (100. * saturation).round();
        let lightness = (100. * lightness).round();
        if with_alpha {
            format!(
                "hsla({},{saturation}%,{lightness}%,{})",
                hue as i64,
                css_alpha(self.alpha)
            )
        } else {
            format!("hsl({},{saturation}%,{lightness}%)", hue as i64)
        }
    }

    fn css_hwb(self, with_alpha: bool) -> String {
        let (hue, whiteness, blackness) = self.hwb();
        let hue = (360. * hue).round() as i64;
        let whiteness = (100. * whiteness).round() as i64;
        let blackness = (100. * blackness).round() as i64;
        if with_alpha {
            format!(
                "hwb({hue} {whiteness}% {blackness}% / {})",
                css_alpha(self.alpha)
            )
        } else {
            format!("hwb({hue} {whiteness}% {blackness}%)")
        }
    }

    // ---- the view ---------------------------------------------------------

    /// The colour as GPUI's `(hue, saturation, lightness, alpha)`, hue as a
    /// fraction of a turn.
    ///
    /// The colour picker speaks `Hsla`, whose components are in the same ranges
    /// as this module's, so nothing is converted but the numbers' type.
    pub fn hsla(self) -> (f32, f32, f32, f32) {
        let (hue, saturation, lightness) = self.hsl();
        (
            hue as f32,
            saturation as f32,
            lightness as f32,
            self.alpha as f32,
        )
    }
}

/// An alpha channel as CSS writes it: at most two decimals, no trailing zeros.
///
/// Swift formats this with `precision(.fractionLength(0...2))` in a POSIX
/// locale, which drops a trailing `.00` and rounds halves away from zero. Rust's
/// `{:.2}` rounds halves to even, so the rounding is done here first and the
/// formatting only pads.
fn css_alpha(alpha: f64) -> String {
    if !alpha.is_finite() {
        return "0".to_string();
    }
    let negative = alpha < 0.;
    let rounded = ((alpha.abs() * 100.) + 0.5).floor() / 100.;
    let mut text = format!("{rounded:.2}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if negative && text != "0" {
        format!("-{text}")
    } else {
        text
    }
}

/// The CSS colour names, in the order upstream lists them.
///
/// The order matters twice over: `keyword_for_value` takes the *first* name for
/// a value, so `#00ffff` reports `aqua` rather than the later `cyan`, and
/// `#ff00ff` reports `fuchsia` rather than `magenta`.
pub static KEYWORD_COLORS: [KeywordColor; 148] = [
    KeywordColor {
        value: 0x000000,
        keyword: "black",
        version: 1,
    },
    KeywordColor {
        value: 0x000080,
        keyword: "navy",
        version: 1,
    },
    KeywordColor {
        value: 0x0000ff,
        keyword: "blue",
        version: 1,
    },
    KeywordColor {
        value: 0x008000,
        keyword: "green",
        version: 1,
    },
    KeywordColor {
        value: 0x00ff00,
        keyword: "lime",
        version: 1,
    },
    KeywordColor {
        value: 0x00ffff,
        keyword: "aqua",
        version: 1,
    },
    KeywordColor {
        value: 0x008080,
        keyword: "teal",
        version: 1,
    },
    KeywordColor {
        value: 0x800000,
        keyword: "maroon",
        version: 1,
    },
    KeywordColor {
        value: 0x800080,
        keyword: "purple",
        version: 1,
    },
    KeywordColor {
        value: 0x808000,
        keyword: "olive",
        version: 1,
    },
    KeywordColor {
        value: 0x808080,
        keyword: "gray",
        version: 1,
    },
    KeywordColor {
        value: 0xc0c0c0,
        keyword: "silver",
        version: 1,
    },
    KeywordColor {
        value: 0xff0000,
        keyword: "red",
        version: 1,
    },
    KeywordColor {
        value: 0xff00ff,
        keyword: "fuchsia",
        version: 1,
    },
    KeywordColor {
        value: 0xffff00,
        keyword: "yellow",
        version: 1,
    },
    KeywordColor {
        value: 0xffffff,
        keyword: "white",
        version: 1,
    },
    KeywordColor {
        value: 0xffa500,
        keyword: "orange",
        version: 2,
    },
    KeywordColor {
        value: 0x00008b,
        keyword: "darkblue",
        version: 3,
    },
    KeywordColor {
        value: 0x0000cd,
        keyword: "mediumblue",
        version: 3,
    },
    KeywordColor {
        value: 0x006400,
        keyword: "darkgreen",
        version: 3,
    },
    KeywordColor {
        value: 0x008b8b,
        keyword: "darkcyan",
        version: 3,
    },
    KeywordColor {
        value: 0x00bfff,
        keyword: "deepskyblue",
        version: 3,
    },
    KeywordColor {
        value: 0x00ced1,
        keyword: "darkturquoise",
        version: 3,
    },
    KeywordColor {
        value: 0x00fa9a,
        keyword: "mediumspringgreen",
        version: 3,
    },
    KeywordColor {
        value: 0x00ff7f,
        keyword: "springgreen",
        version: 3,
    },
    KeywordColor {
        value: 0x00ffff,
        keyword: "cyan",
        version: 3,
    },
    KeywordColor {
        value: 0x191970,
        keyword: "midnightblue",
        version: 3,
    },
    KeywordColor {
        value: 0x1e90ff,
        keyword: "dodgerblue",
        version: 3,
    },
    KeywordColor {
        value: 0x20b2aa,
        keyword: "lightseagreen",
        version: 3,
    },
    KeywordColor {
        value: 0x228b22,
        keyword: "forestgreen",
        version: 3,
    },
    KeywordColor {
        value: 0x2e8b57,
        keyword: "seagreen",
        version: 3,
    },
    KeywordColor {
        value: 0x2f4f4f,
        keyword: "darkslategray",
        version: 3,
    },
    KeywordColor {
        value: 0x2f4f4f,
        keyword: "darkslategrey",
        version: 3,
    },
    KeywordColor {
        value: 0x32cd32,
        keyword: "limegreen",
        version: 3,
    },
    KeywordColor {
        value: 0x3cb371,
        keyword: "mediumseagreen",
        version: 3,
    },
    KeywordColor {
        value: 0x40e0d0,
        keyword: "turquoise",
        version: 3,
    },
    KeywordColor {
        value: 0x4169e1,
        keyword: "royalblue",
        version: 3,
    },
    KeywordColor {
        value: 0x4682b4,
        keyword: "steelblue",
        version: 3,
    },
    KeywordColor {
        value: 0x483d8b,
        keyword: "darkslateblue",
        version: 3,
    },
    KeywordColor {
        value: 0x48d1cc,
        keyword: "mediumturquoise",
        version: 3,
    },
    KeywordColor {
        value: 0x4b0082,
        keyword: "indigo",
        version: 3,
    },
    KeywordColor {
        value: 0x556b2f,
        keyword: "darkolivegreen",
        version: 3,
    },
    KeywordColor {
        value: 0x5f9ea0,
        keyword: "cadetblue",
        version: 3,
    },
    KeywordColor {
        value: 0x6495ed,
        keyword: "cornflowerblue",
        version: 3,
    },
    KeywordColor {
        value: 0x66cdaa,
        keyword: "mediumaquamarine",
        version: 3,
    },
    KeywordColor {
        value: 0x696969,
        keyword: "dimgray",
        version: 3,
    },
    KeywordColor {
        value: 0x696969,
        keyword: "dimgrey",
        version: 3,
    },
    KeywordColor {
        value: 0x6a5acd,
        keyword: "slateblue",
        version: 3,
    },
    KeywordColor {
        value: 0x6b8e23,
        keyword: "olivedrab",
        version: 3,
    },
    KeywordColor {
        value: 0x708090,
        keyword: "slategray",
        version: 3,
    },
    KeywordColor {
        value: 0x708090,
        keyword: "slategrey",
        version: 3,
    },
    KeywordColor {
        value: 0x778899,
        keyword: "lightslategray",
        version: 3,
    },
    KeywordColor {
        value: 0x778899,
        keyword: "lightslategrey",
        version: 3,
    },
    KeywordColor {
        value: 0x7b68ee,
        keyword: "mediumslateblue",
        version: 3,
    },
    KeywordColor {
        value: 0x7cfc00,
        keyword: "lawngreen",
        version: 3,
    },
    KeywordColor {
        value: 0x7fff00,
        keyword: "chartreuse",
        version: 3,
    },
    KeywordColor {
        value: 0x7fffd4,
        keyword: "aquamarine",
        version: 3,
    },
    KeywordColor {
        value: 0x808080,
        keyword: "grey",
        version: 3,
    },
    KeywordColor {
        value: 0x87ceeb,
        keyword: "skyblue",
        version: 3,
    },
    KeywordColor {
        value: 0x87cefa,
        keyword: "lightskyblue",
        version: 3,
    },
    KeywordColor {
        value: 0x8a2be2,
        keyword: "blueviolet",
        version: 3,
    },
    KeywordColor {
        value: 0x8b0000,
        keyword: "darkred",
        version: 3,
    },
    KeywordColor {
        value: 0x8b008b,
        keyword: "darkmagenta",
        version: 3,
    },
    KeywordColor {
        value: 0x8b4513,
        keyword: "saddlebrown",
        version: 3,
    },
    KeywordColor {
        value: 0x8fbc8f,
        keyword: "darkseagreen",
        version: 3,
    },
    KeywordColor {
        value: 0x90ee90,
        keyword: "lightgreen",
        version: 3,
    },
    KeywordColor {
        value: 0x9370db,
        keyword: "mediumpurple",
        version: 3,
    },
    KeywordColor {
        value: 0x9400d3,
        keyword: "darkviolet",
        version: 3,
    },
    KeywordColor {
        value: 0x98fb98,
        keyword: "palegreen",
        version: 3,
    },
    KeywordColor {
        value: 0x9932cc,
        keyword: "darkorchid",
        version: 3,
    },
    KeywordColor {
        value: 0x9acd32,
        keyword: "yellowgreen",
        version: 3,
    },
    KeywordColor {
        value: 0xa0522d,
        keyword: "sienna",
        version: 3,
    },
    KeywordColor {
        value: 0xa52a2a,
        keyword: "brown",
        version: 3,
    },
    KeywordColor {
        value: 0xa9a9a9,
        keyword: "darkgray",
        version: 3,
    },
    KeywordColor {
        value: 0xa9a9a9,
        keyword: "darkgrey",
        version: 3,
    },
    KeywordColor {
        value: 0xadd8e6,
        keyword: "lightblue",
        version: 3,
    },
    KeywordColor {
        value: 0xadff2f,
        keyword: "greenyellow",
        version: 3,
    },
    KeywordColor {
        value: 0xafeeee,
        keyword: "paleturquoise",
        version: 3,
    },
    KeywordColor {
        value: 0xb0c4de,
        keyword: "lightsteelblue",
        version: 3,
    },
    KeywordColor {
        value: 0xb0e0e6,
        keyword: "powderblue",
        version: 3,
    },
    KeywordColor {
        value: 0xb22222,
        keyword: "firebrick",
        version: 3,
    },
    KeywordColor {
        value: 0xb8860b,
        keyword: "darkgoldenrod",
        version: 3,
    },
    KeywordColor {
        value: 0xba55d3,
        keyword: "mediumorchid",
        version: 3,
    },
    KeywordColor {
        value: 0xbc8f8f,
        keyword: "rosybrown",
        version: 3,
    },
    KeywordColor {
        value: 0xbdb76b,
        keyword: "darkkhaki",
        version: 3,
    },
    KeywordColor {
        value: 0xc71585,
        keyword: "mediumvioletred",
        version: 3,
    },
    KeywordColor {
        value: 0xcd5c5c,
        keyword: "indianred",
        version: 3,
    },
    KeywordColor {
        value: 0xcd853f,
        keyword: "peru",
        version: 3,
    },
    KeywordColor {
        value: 0xd2691e,
        keyword: "chocolate",
        version: 3,
    },
    KeywordColor {
        value: 0xd2b48c,
        keyword: "tan",
        version: 3,
    },
    KeywordColor {
        value: 0xd3d3d3,
        keyword: "lightgray",
        version: 3,
    },
    KeywordColor {
        value: 0xd3d3d3,
        keyword: "lightgrey",
        version: 3,
    },
    KeywordColor {
        value: 0xd8bfd8,
        keyword: "thistle",
        version: 3,
    },
    KeywordColor {
        value: 0xda70d6,
        keyword: "orchid",
        version: 3,
    },
    KeywordColor {
        value: 0xdaa520,
        keyword: "goldenrod",
        version: 3,
    },
    KeywordColor {
        value: 0xdb7093,
        keyword: "palevioletred",
        version: 3,
    },
    KeywordColor {
        value: 0xdc143c,
        keyword: "crimson",
        version: 3,
    },
    KeywordColor {
        value: 0xdcdcdc,
        keyword: "gainsboro",
        version: 3,
    },
    KeywordColor {
        value: 0xdda0dd,
        keyword: "plum",
        version: 3,
    },
    KeywordColor {
        value: 0xdeb887,
        keyword: "burlywood",
        version: 3,
    },
    KeywordColor {
        value: 0xe0ffff,
        keyword: "lightcyan",
        version: 3,
    },
    KeywordColor {
        value: 0xe6e6fa,
        keyword: "lavender",
        version: 3,
    },
    KeywordColor {
        value: 0xe9967a,
        keyword: "darksalmon",
        version: 3,
    },
    KeywordColor {
        value: 0xee82ee,
        keyword: "violet",
        version: 3,
    },
    KeywordColor {
        value: 0xeee8aa,
        keyword: "palegoldenrod",
        version: 3,
    },
    KeywordColor {
        value: 0xf08080,
        keyword: "lightcoral",
        version: 3,
    },
    KeywordColor {
        value: 0xf0e68c,
        keyword: "khaki",
        version: 3,
    },
    KeywordColor {
        value: 0xf0f8ff,
        keyword: "aliceblue",
        version: 3,
    },
    KeywordColor {
        value: 0xf0fff0,
        keyword: "honeydew",
        version: 3,
    },
    KeywordColor {
        value: 0xf0ffff,
        keyword: "azure",
        version: 3,
    },
    KeywordColor {
        value: 0xf4a460,
        keyword: "sandybrown",
        version: 3,
    },
    KeywordColor {
        value: 0xf5deb3,
        keyword: "wheat",
        version: 3,
    },
    KeywordColor {
        value: 0xf5f5dc,
        keyword: "beige",
        version: 3,
    },
    KeywordColor {
        value: 0xf5f5f5,
        keyword: "whitesmoke",
        version: 3,
    },
    KeywordColor {
        value: 0xf5fffa,
        keyword: "mintcream",
        version: 3,
    },
    KeywordColor {
        value: 0xf8f8ff,
        keyword: "ghostwhite",
        version: 3,
    },
    KeywordColor {
        value: 0xfa8072,
        keyword: "salmon",
        version: 3,
    },
    KeywordColor {
        value: 0xfaebd7,
        keyword: "antiquewhite",
        version: 3,
    },
    KeywordColor {
        value: 0xfaf0e6,
        keyword: "linen",
        version: 3,
    },
    KeywordColor {
        value: 0xfafad2,
        keyword: "lightgoldenrodyellow",
        version: 3,
    },
    KeywordColor {
        value: 0xfdf5e6,
        keyword: "oldlace",
        version: 3,
    },
    KeywordColor {
        value: 0xff00ff,
        keyword: "magenta",
        version: 3,
    },
    KeywordColor {
        value: 0xff1493,
        keyword: "deeppink",
        version: 3,
    },
    KeywordColor {
        value: 0xff4500,
        keyword: "orangered",
        version: 3,
    },
    KeywordColor {
        value: 0xff6347,
        keyword: "tomato",
        version: 3,
    },
    KeywordColor {
        value: 0xff69b4,
        keyword: "hotpink",
        version: 3,
    },
    KeywordColor {
        value: 0xff7f50,
        keyword: "coral",
        version: 3,
    },
    KeywordColor {
        value: 0xff8c00,
        keyword: "darkorange",
        version: 3,
    },
    KeywordColor {
        value: 0xffa07a,
        keyword: "lightsalmon",
        version: 3,
    },
    KeywordColor {
        value: 0xffb6c1,
        keyword: "lightpink",
        version: 3,
    },
    KeywordColor {
        value: 0xffc0cb,
        keyword: "pink",
        version: 3,
    },
    KeywordColor {
        value: 0xffd700,
        keyword: "gold",
        version: 3,
    },
    KeywordColor {
        value: 0xffdab9,
        keyword: "peachpuff",
        version: 3,
    },
    KeywordColor {
        value: 0xffdead,
        keyword: "navajowhite",
        version: 3,
    },
    KeywordColor {
        value: 0xffe4b5,
        keyword: "moccasin",
        version: 3,
    },
    KeywordColor {
        value: 0xffe4c4,
        keyword: "bisque",
        version: 3,
    },
    KeywordColor {
        value: 0xffe4e1,
        keyword: "mistyrose",
        version: 3,
    },
    KeywordColor {
        value: 0xffebcd,
        keyword: "blanchedalmond",
        version: 3,
    },
    KeywordColor {
        value: 0xffefd5,
        keyword: "papayawhip",
        version: 3,
    },
    KeywordColor {
        value: 0xfff0f5,
        keyword: "lavenderblush",
        version: 3,
    },
    KeywordColor {
        value: 0xfff5ee,
        keyword: "seashell",
        version: 3,
    },
    KeywordColor {
        value: 0xfff8dc,
        keyword: "cornsilk",
        version: 3,
    },
    KeywordColor {
        value: 0xfffacd,
        keyword: "lemonchiffon",
        version: 3,
    },
    KeywordColor {
        value: 0xfffaf0,
        keyword: "floralwhite",
        version: 3,
    },
    KeywordColor {
        value: 0xfffafa,
        keyword: "snow",
        version: 3,
    },
    KeywordColor {
        value: 0xffffe0,
        keyword: "lightyellow",
        version: 3,
    },
    KeywordColor {
        value: 0xfffff0,
        keyword: "ivory",
        version: 3,
    },
    KeywordColor {
        value: 0x663399,
        keyword: "rebeccapurple",
        version: 4,
    },
];

/// A CSS colour name and the value it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeywordColor {
    /// The 24-bit `0xRRGGBB` value.
    pub value: u32,
    /// The name, lower case.
    pub keyword: &'static str,
    /// The stylesheet level that introduced it: 1, 2, 3 or 4.
    pub version: u8,
}

/// The first name for a 24-bit value, or `None` when it has none.
pub fn keyword_for_value(value: u32) -> Option<&'static str> {
    KEYWORD_COLORS
        .iter()
        .find(|color| color.value == value)
        .map(|color| color.keyword)
}

/// The value a name stands for, case-insensitively.
pub fn value_for_keyword(keyword: &str) -> Option<u32> {
    let keyword = keyword.to_lowercase();
    KEYWORD_COLORS
        .iter()
        .find(|color| color.keyword == keyword)
        .map(|color| color.value)
}

/// A colour built from an HSL triple.
///
/// The `p`/`q` formulation, which is what AppKit's
/// `NSColor(calibratedHue:saturation:lightness:alpha:)` computes. Using it is
/// what makes `hsla(...)` survive a trip through the panel unchanged.
pub fn from_hsl(hue: f64, saturation: f64, lightness: f64, alpha: f64) -> Color {
    // A colour with no saturation is grey, and grey is every hue at once, so
    // there is nothing to interpolate. `is_nan` is checked separately for the
    // same reason as in `Color::hue`.
    if saturation.is_nan() || saturation <= 0. {
        return Color::new(lightness, lightness, lightness, alpha);
    }
    let q = if lightness < 0.5 {
        lightness * (1. + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2. * lightness - q;
    let channel = |mut t: f64| {
        if t < 0. {
            t += 1.;
        }
        if t > 1. {
            t -= 1.;
        }
        if t < 1. / 6. {
            p + (q - p) * 6. * t
        } else if t < 1. / 2. {
            q
        } else if t < 2. / 3. {
            p + (q - p) * (2. / 3. - t) * 6.
        } else {
            p
        }
    };
    Color::new(
        channel(hue + 1. / 3.),
        channel(hue),
        channel(hue - 1. / 3.),
        alpha,
    )
}

/// A colour built from an HWB triple.
///
/// Upstream reaches this through `HWB.rgb`.
pub fn from_hwb(hue: f64, whiteness: f64, blackness: f64, alpha: f64) -> Color {
    if whiteness + blackness >= 1. {
        let gray = whiteness / (whiteness + blackness);
        return Color::new(gray, gray, gray, alpha);
    }
    let channel = |value: f64| {
        let k = (value + hue * 12.).rem_euclid(12.);
        // Upstream's `0.5 - 0.5 * max(-1, min(k - 3, 9 - k, 1))`.
        0.5 - 0.5 * (k - 3.).min(9. - k).clamp(-1., 1.)
    };
    let factor = 1. - whiteness - blackness;
    Color::new(
        channel(0.) * factor + whiteness,
        channel(8.) * factor + whiteness,
        channel(4.) * factor + whiteness,
        alpha,
    )
}

/// Upstream's `wholeMatch(of:)`: the pattern has to cover the whole string.
///
/// Rust's `Regex` searches rather than anchoring, so the span is checked against
/// the string's bounds instead of wrapping the pattern in `^…$` — which would
/// change what a `^` or `$` inside a pattern means.
fn whole_match<'a>(regex: &Regex, text: &'a str) -> Option<regex::Captures<'a>> {
    let captures = regex.captures(text)?;
    let whole = captures.get(0)?;
    (whole.start() == 0 && whole.end() == text.len()).then_some(captures)
}

/// Declares a lazily compiled pattern.
///
/// Compiling on first use keeps the eleven patterns out of the way of a program
/// that never opens the colour panel, and the `expect` is honest: the patterns
/// are literals in this file, so a failure is a typo caught by the first test
/// that touches them.
macro_rules! pattern {
    ($name:ident, $pattern:literal) => {
        fn $name() -> &'static Regex {
            static COMPILED: OnceLock<Regex> = OnceLock::new();
            COMPILED.get_or_init(|| {
                Regex::new($pattern).expect("the colour-code patterns are literals")
            })
        }
    };
}

pattern!(hex_re, r"#([0-9a-fA-F]{6})");
pattern!(hex_with_alpha_re, r"#([0-9a-fA-F]{6})([0-9a-fA-F]{2})");
pattern!(short_hex_re, r"#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])");
pattern!(
    short_hex_with_alpha_re,
    r"#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])"
);
pattern!(
    css_rgb_comma_re,
    r"rgb\( *([0-9]{1,3}) *, *([0-9]{1,3}) *, *([0-9]{1,3}) *\)"
);
pattern!(
    css_rgb_space_re,
    r"rgb\( *([0-9.]+%?) +([0-9.]+%?) +([0-9.]+%?) *\)"
);
pattern!(
    css_rgba_comma_re,
    r"rgba\( *([0-9]{1,3}) *, *([0-9]{1,3}) *, *([0-9]{1,3})(?: *, *([0-9.]+))? *\)"
);
pattern!(
    css_rgba_space_re,
    r"rgba?\( *([0-9.]+%?) +([0-9.]+%?) +([0-9.]+%?)(?: *\/ *([0-9.]+%?))? *\)"
);
pattern!(
    css_hsl_comma_re,
    r"hsl\( *([0-9]{1,3}) *, *([0-9.]+)% *, *([0-9.]+)% *\)"
);
pattern!(
    css_hsl_space_re,
    r"hsl\( *([0-9.]+) +([0-9.]+%?) +([0-9.]+%?) *\)"
);
pattern!(
    css_hsla_comma_re,
    r"hsla\( *([0-9]{1,3}) *, *([0-9.]+)% *, *([0-9.]+)%(?: *, *([0-9.]+))? *\)"
);
pattern!(
    css_hsla_space_re,
    r"hsla?\( *([0-9.]+) +([0-9.]+%?) +([0-9.]+%?)(?: *\/ *([0-9.]+%?))? *\)"
);
pattern!(
    css_hwb_re,
    r"hwb\( *([0-9.]+) +([0-9.]+%?) +([0-9.]+%?) *\)"
);
pattern!(
    css_hwb_with_alpha_re,
    r"hwb\( *([0-9.]+) +([0-9.]+%?) +([0-9.]+%?) *\/ *([0-9.]+%?) *\)"
);
pattern!(keyword_re, r"[a-zA-Z]+");

/// A decimal number, or `None` when the text is not one.
fn number(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// A capture group read as a number.
fn group(captures: &regex::Captures<'_>, index: usize) -> Option<f64> {
    number(captures.get(index)?.as_str())
}

/// A capture group read as a base-16 integer.
///
/// Separate from [`group`] because the hex notations match `a`-`f` as readily
/// as `0`-`9`, and `"ff".parse::<f64>()` is an error rather than 255.
fn hex_group(captures: &regex::Captures<'_>, index: usize) -> Option<u32> {
    u32::from_str_radix(captures.get(index)?.as_str(), 16).ok()
}

/// Upstream's `rgbComponent`: a percentage in `0..=100`, or a number in
/// `0..=255`, either way as a fraction.
fn rgb_component(text: &str) -> Option<f64> {
    if let Some(percent) = text.strip_suffix('%') {
        let component = number(percent)?;
        return (0. ..=100.).contains(&component).then(|| component / 100.);
    }
    let component = number(text)?;
    (0. ..=255.).contains(&component).then(|| component / 255.)
}

/// Upstream's `percentageComponent`: the number, percent sign or not, and
/// *without* a range check — the callers apply their own.
fn percentage_component(text: &str) -> Option<f64> {
    number(text.strip_suffix('%').unwrap_or(text))
}

/// Upstream's `alphaComponent`: a percentage becomes a fraction, a bare number
/// is taken as one.
fn alpha_component(text: &str) -> Option<f64> {
    if let Some(percent) = text.strip_suffix('%') {
        return Some(number(percent)? / 100.);
    }
    number(text)
}

/// Upstream's `optionalAlphaComponent`: an absent group means fully opaque.
fn optional_alpha_component(text: Option<&str>) -> Option<f64> {
    match text {
        Some(text) => alpha_component(text),
        None => Some(1.),
    }
}

/// The colour a 24-bit value and an alpha name.
fn from_hex(hex: u32, alpha: f64) -> Option<Color> {
    if hex > 0x00ff_ffff || !(0. ..=1.).contains(&alpha) {
        return None;
    }
    Some(Color::from_rgba8(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
        alpha,
    ))
}

/// The three decimal channels of a `rgb(...)` / `hsl(...)` style pattern.
fn channels(captures: &regex::Captures<'_>) -> Option<(f64, f64, f64)> {
    Some((
        group(captures, 1)?,
        group(captures, 2)?,
        group(captures, 3)?,
    ))
}

impl ColorCodeType {
    /// The colour `code` names in this notation, or `None` when it does not.
    fn parse(self, code: &str) -> Option<Color> {
        match self {
            Self::Hex => {
                let captures = whole_match(hex_re(), code)?;
                from_hex(hex_group(&captures, 1)?, 1.)
            }
            Self::HexWithAlpha => {
                let captures = whole_match(hex_with_alpha_re(), code)?;
                let hex = hex_group(&captures, 1)?;
                let alpha = hex_group(&captures, 2)?;
                from_hex(hex, f64::from(alpha) / 255.)
            }
            Self::ShortHex => {
                let captures = whole_match(short_hex_re(), code)?;
                let red = hex_group(&captures, 1)?;
                let green = hex_group(&captures, 2)?;
                let blue = hex_group(&captures, 3)?;
                Some(Color::new(
                    f64::from(red) / 15.,
                    f64::from(green) / 15.,
                    f64::from(blue) / 15.,
                    1.,
                ))
            }
            Self::ShortHexWithAlpha => {
                let captures = whole_match(short_hex_with_alpha_re(), code)?;
                let red = hex_group(&captures, 1)?;
                let green = hex_group(&captures, 2)?;
                let blue = hex_group(&captures, 3)?;
                let alpha = hex_group(&captures, 4)?;
                Some(Color::new(
                    f64::from(red) / 15.,
                    f64::from(green) / 15.,
                    f64::from(blue) / 15.,
                    f64::from(alpha) / 15.,
                ))
            }
            Self::CssRgb => {
                if let Some(captures) = whole_match(css_rgb_comma_re(), code)
                    && let Some((red, green, blue)) = channels(&captures)
                    && [red, green, blue]
                        .iter()
                        .all(|value| (0. ..=255.).contains(value))
                {
                    return Some(Color::new(red / 255., green / 255., blue / 255., 1.));
                }
                let captures = whole_match(css_rgb_space_re(), code)?;
                let red = rgb_component(captures.get(1)?.as_str())?;
                let green = rgb_component(captures.get(2)?.as_str())?;
                let blue = rgb_component(captures.get(3)?.as_str())?;
                Some(Color::new(red, green, blue, 1.))
            }
            Self::CssRgba => {
                if let Some(captures) = whole_match(css_rgba_comma_re(), code)
                    && let Some((red, green, blue)) = channels(&captures)
                    && let Some(alpha) =
                        optional_alpha_component(captures.get(4).map(|part| part.as_str()))
                    && [red, green, blue]
                        .iter()
                        .all(|value| (0. ..=255.).contains(value))
                    && (0. ..=1.).contains(&alpha)
                {
                    return Some(Color::new(red / 255., green / 255., blue / 255., alpha));
                }
                let captures = whole_match(css_rgba_space_re(), code)?;
                let red = rgb_component(captures.get(1)?.as_str())?;
                let green = rgb_component(captures.get(2)?.as_str())?;
                let blue = rgb_component(captures.get(3)?.as_str())?;
                let alpha = optional_alpha_component(captures.get(4).map(|part| part.as_str()))?;
                (0. ..=1.)
                    .contains(&alpha)
                    .then(|| Color::new(red, green, blue, alpha))
            }
            Self::CssHsl => {
                if let Some(captures) = whole_match(css_hsl_comma_re(), code)
                    && let Some((hue, saturation, lightness)) = channels(&captures)
                    && (0. ..=360.).contains(&hue)
                    && (0. ..=100.).contains(&saturation)
                    && (0. ..=100.).contains(&lightness)
                {
                    return Some(from_hsl(
                        hue / 360.,
                        saturation / 100.,
                        lightness / 100.,
                        1.,
                    ));
                }
                let captures = whole_match(css_hsl_space_re(), code)?;
                let hue = group(&captures, 1)?;
                let saturation = percentage_component(captures.get(2)?.as_str())?;
                let lightness = percentage_component(captures.get(3)?.as_str())?;
                if !(0. ..=360.).contains(&hue)
                    || !(0. ..=100.).contains(&saturation)
                    || !(0. ..=100.).contains(&lightness)
                {
                    return None;
                }
                Some(from_hsl(
                    hue / 360.,
                    saturation / 100.,
                    lightness / 100.,
                    1.,
                ))
            }
            Self::CssHsla => {
                if let Some(captures) = whole_match(css_hsla_comma_re(), code)
                    && let Some((hue, saturation, lightness)) = channels(&captures)
                    && let Some(alpha) =
                        optional_alpha_component(captures.get(4).map(|part| part.as_str()))
                    && (0. ..=360.).contains(&hue)
                    && (0. ..=100.).contains(&saturation)
                    && (0. ..=100.).contains(&lightness)
                    && (0. ..=1.).contains(&alpha)
                {
                    return Some(from_hsl(
                        hue / 360.,
                        saturation / 100.,
                        lightness / 100.,
                        alpha,
                    ));
                }
                let captures = whole_match(css_hsla_space_re(), code)?;
                let hue = group(&captures, 1)?;
                let saturation = percentage_component(captures.get(2)?.as_str())?;
                let lightness = percentage_component(captures.get(3)?.as_str())?;
                let alpha = optional_alpha_component(captures.get(4).map(|part| part.as_str()))?;
                if !(0. ..=360.).contains(&hue)
                    || !(0. ..=100.).contains(&saturation)
                    || !(0. ..=100.).contains(&lightness)
                    || !(0. ..=1.).contains(&alpha)
                {
                    return None;
                }
                Some(from_hsl(
                    hue / 360.,
                    saturation / 100.,
                    lightness / 100.,
                    alpha,
                ))
            }
            Self::CssHwb => {
                let captures = whole_match(css_hwb_re(), code)?;
                let hue = group(&captures, 1)?;
                let whiteness = percentage_component(captures.get(2)?.as_str())?;
                let blackness = percentage_component(captures.get(3)?.as_str())?;
                if !(0. ..=360.).contains(&hue)
                    || !(0. ..=100.).contains(&whiteness)
                    || !(0. ..=100.).contains(&blackness)
                {
                    return None;
                }
                Some(from_hwb(hue / 360., whiteness / 100., blackness / 100., 1.))
            }
            Self::CssHwbWithAlpha => {
                let captures = whole_match(css_hwb_with_alpha_re(), code)?;
                let hue = group(&captures, 1)?;
                let whiteness = percentage_component(captures.get(2)?.as_str())?;
                let blackness = percentage_component(captures.get(3)?.as_str())?;
                let alpha = alpha_component(captures.get(4)?.as_str())?;
                if !(0. ..=360.).contains(&hue)
                    || !(0. ..=100.).contains(&whiteness)
                    || !(0. ..=100.).contains(&blackness)
                    || !(0. ..=1.).contains(&alpha)
                {
                    return None;
                }
                Some(from_hwb(
                    hue / 360.,
                    whiteness / 100.,
                    blackness / 100.,
                    alpha,
                ))
            }
            Self::CssKeyword => {
                // A keyword is letters and nothing else, so a code that parses
                // as one is the only thing that reaches the table.
                whole_match(keyword_re(), code)?;
                let value = value_for_keyword(code)?;
                from_hex(value, 1.)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compares two colours channel by channel, to the tolerance the upstream
    /// suite uses for a value that has been through a round trip.
    fn close(left: Color, right: Color) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        near(left.red, right.red)
            && near(left.green, right.green)
            && near(left.blue, right.blue)
            && near(left.alpha, right.alpha)
    }

    fn rgb(red: f64, green: f64, blue: f64, alpha: f64) -> Color {
        Color::new(red, green, blue, alpha)
    }

    // ---- reading ---------------------------------------------------------

    #[test]
    fn hex_codes_read_at_all_four_widths() {
        assert!(close(
            Color::from_code("#336699").unwrap().1,
            rgb(0.2, 0.4, 0.6, 1.)
        ));
        assert!(close(
            Color::from_code("#33669980").unwrap().1,
            rgb(0.2, 0.4, 0.6, f64::from(0x80u8) / 255.)
        ));
        assert!(close(
            Color::from_code("#369").unwrap().1,
            rgb(0.2, 0.4, 0.6, 1.)
        ));
        assert!(close(
            Color::from_code("#369c").unwrap().1,
            rgb(0.2, 0.4, 0.6, 0.8)
        ));
    }

    #[test]
    fn css_rgb_codes_read_both_syntaxes() {
        let expected = rgb(0.2, 0.4, 0.6, 1.);
        assert!(close(
            Color::from_code("rgb(51, 102, 153)").unwrap().1,
            expected
        ));
        assert!(close(
            Color::from_code("rgb(20% 40% 60%)").unwrap().1,
            expected
        ));
        assert!(close(
            Color::from_code("rgba(51, 102, 153, 0.5)").unwrap().1,
            rgb(0.2, 0.4, 0.6, 0.5)
        ));
        assert!(close(
            Color::from_code("rgb(20% 40% 60% / 50%)").unwrap().1,
            rgb(0.2, 0.4, 0.6, 0.5)
        ));
    }

    #[test]
    fn css_hsl_codes_read_both_syntaxes() {
        // Upstream compares the parsed HSL triple; here the triple is folded
        // into RGB, so the assertion is on the RGB the same triple names.
        let expected = from_hsl(0.5, 0.5, 0.25, 1.);
        assert!(close(
            Color::from_code("hsl(180, 50%, 25%)").unwrap().1,
            expected
        ));
        assert!(close(
            Color::from_code("hsl(180 50 25)").unwrap().1,
            expected
        ));
        let translucent = from_hsl(0.5, 0.5, 0.25, 0.5);
        assert!(close(
            Color::from_code("hsla(180, 50%, 25%, 0.5)").unwrap().1,
            translucent
        ));
        assert!(close(
            Color::from_code("hsl(180 50% 25% / 50%)").unwrap().1,
            translucent
        ));
    }

    #[test]
    fn css_hwb_codes_read_both_syntaxes() {
        let expected = from_hwb(0.5, 0.2, 0.3, 1.);
        assert!(close(
            Color::from_code("hwb(180 20% 30%)").unwrap().1,
            expected
        ));
        let translucent = from_hwb(0.5, 0.2, 0.3, 0.5);
        assert!(close(
            Color::from_code("hwb(180 20% 30% / 50%)").unwrap().1,
            translucent
        ));
    }

    #[test]
    fn css_keywords_are_case_insensitive() {
        let midnight = rgb(
            f64::from(0x19u8) / 255.,
            f64::from(0x19u8) / 255.,
            f64::from(0x70u8) / 255.,
            1.,
        );
        for spelling in ["MidnightBlue", "midnightblue", "MIDNIGHTBLUE"] {
            let (kind, color) = Color::from_code(spelling).unwrap();
            assert_eq!(kind, ColorCodeType::CssKeyword);
            assert!(close(color, midnight), "{spelling}");
        }
    }

    #[test]
    fn malformed_codes_are_rejected() {
        for code in [
            "#33669",
            "#3366998",
            "#36",
            "rgb(256, 0, 0)",
            "rgba(0, 0, 0, 1.1)",
            "rgba(255,255,255,.)",
            "hsl(361 0% 0%)",
            "hsl(0 0% 0% / 101%)",
            "hwb(0 101% 0%)",
            "hwb(0 0% 0% / 101%)",
            "foobar",
            "",
            "   ",
        ] {
            assert!(Color::from_code(code).is_none(), "{code} should not parse");
        }
    }

    #[test]
    fn the_notation_that_parses_a_code_is_reported() {
        // Upstream's `testColorCreation`, which also insists every one of these
        // names the same colour — white.
        let white = rgb(1., 1., 1., 1.);
        for (code, expected) in [
            ("#ffffff", ColorCodeType::Hex),
            ("#ffffffff", ColorCodeType::HexWithAlpha),
            ("#fff", ColorCodeType::ShortHex),
            ("#ffff", ColorCodeType::ShortHexWithAlpha),
            ("rgb(255,255,255)", ColorCodeType::CssRgb),
            ("rgba(255,255,255,1)", ColorCodeType::CssRgba),
            ("hsl(0,0%,100%)", ColorCodeType::CssHsl),
            ("hsla(0,0%,100%,1)", ColorCodeType::CssHsla),
            ("hwb(0 100% 0%)", ColorCodeType::CssHwb),
            ("hwb(0 100% 0% / 1)", ColorCodeType::CssHwbWithAlpha),
            ("white", ColorCodeType::CssKeyword),
        ] {
            let (kind, color) = Color::from_code(code).unwrap_or_else(|| panic!("{code}"));
            assert_eq!(kind, expected, "{code}");
            assert!(close(color, white), "{code}");
        }
    }

    #[test]
    fn the_shortest_hex_notation_is_not_confused_with_a_longer_one() {
        // `#369` is a three-digit hex, so it must not be read as a four-digit
        // one that happens to be a prefix of nothing else.
        assert_eq!(Color::from_code("#369").unwrap().0, ColorCodeType::ShortHex);
        assert_eq!(
            Color::from_code("#369c").unwrap().0,
            ColorCodeType::ShortHexWithAlpha
        );
        // And a four-digit code is not read as a three-digit one with a
        // trailing character left over.
        assert_eq!(
            Color::from_code("#ffff").unwrap().0,
            ColorCodeType::ShortHexWithAlpha
        );
    }

    // ---- writing ---------------------------------------------------------

    #[test]
    fn white_writes_every_notation() {
        let white = Color::from_code("#ffffff").unwrap().1;
        assert_eq!(white.code(ColorCodeType::Hex).unwrap(), "#ffffff");
        assert_eq!(
            white.code(ColorCodeType::HexWithAlpha).unwrap(),
            "#ffffffff"
        );
        assert_eq!(white.code(ColorCodeType::ShortHex).unwrap(), "#fff");
        assert_eq!(
            white.code(ColorCodeType::ShortHexWithAlpha).unwrap(),
            "#ffff"
        );
        assert_eq!(
            white.code(ColorCodeType::CssRgb).unwrap(),
            "rgb(255,255,255)"
        );
        assert_eq!(
            white.code(ColorCodeType::CssRgba).unwrap(),
            "rgba(255,255,255,1)"
        );
        assert_eq!(white.code(ColorCodeType::CssHsl).unwrap(), "hsl(0,0%,100%)");
        assert_eq!(
            white.code(ColorCodeType::CssHsla).unwrap(),
            "hsla(0,0%,100%,1)"
        );
        assert_eq!(white.code(ColorCodeType::CssKeyword).unwrap(), "white");
    }

    #[test]
    fn black_writes_every_notation() {
        let black = Color::from_code("#000000").unwrap().1;
        assert_eq!(black.code(ColorCodeType::Hex).unwrap(), "#000000");
        assert_eq!(
            black.code(ColorCodeType::HexWithAlpha).unwrap(),
            "#000000ff"
        );
        assert_eq!(black.code(ColorCodeType::ShortHex).unwrap(), "#000");
        assert_eq!(
            black.code(ColorCodeType::ShortHexWithAlpha).unwrap(),
            "#000f"
        );
        assert_eq!(black.code(ColorCodeType::CssRgb).unwrap(), "rgb(0,0,0)");
        assert_eq!(black.code(ColorCodeType::CssRgba).unwrap(), "rgba(0,0,0,1)");
        assert_eq!(black.code(ColorCodeType::CssHsl).unwrap(), "hsl(0,0%,0%)");
        assert_eq!(
            black.code(ColorCodeType::CssHsla).unwrap(),
            "hsla(0,0%,0%,1)"
        );
        assert_eq!(black.code(ColorCodeType::CssKeyword).unwrap(), "black");
    }

    #[test]
    fn short_hex_is_refused_for_colours_that_do_not_shrink() {
        let lossy = Color::from_code("#123456").unwrap().1;
        assert_eq!(lossy.code(ColorCodeType::ShortHex), None);
        assert_eq!(lossy.code(ColorCodeType::ShortHexWithAlpha), None);

        let shrinkable = Color::from_code("#0066aa").unwrap().1;
        assert_eq!(shrinkable.code(ColorCodeType::ShortHex).unwrap(), "#06a");
        assert_eq!(
            shrinkable.code(ColorCodeType::ShortHexWithAlpha).unwrap(),
            "#06af"
        );

        // The alpha channel is checked too, not just the three colour ones.
        let opaque = Color::from_code("#ff660080").unwrap().1;
        assert_eq!(opaque.code(ColorCodeType::ShortHexWithAlpha), None);
    }

    #[test]
    fn a_short_hex_with_alpha_round_trips() {
        let (kind, color) = Color::from_code("#f60c").unwrap();
        assert_eq!(kind, ColorCodeType::ShortHexWithAlpha);
        assert_eq!(color.code(ColorCodeType::Hex).unwrap(), "#ff6600");
        assert_eq!(
            color.code(ColorCodeType::HexWithAlpha).unwrap(),
            "#ff6600cc"
        );
        assert_eq!(
            color.code(ColorCodeType::ShortHexWithAlpha).unwrap(),
            "#f60c"
        );
    }

    #[test]
    fn a_keyword_is_only_written_for_a_named_colour() {
        let named = Color::from_code("#191970").unwrap().1;
        assert_eq!(
            named.code(ColorCodeType::CssKeyword).unwrap(),
            "midnightblue"
        );
        let unnamed = Color::from_code("#123456").unwrap().1;
        assert_eq!(unnamed.code(ColorCodeType::CssKeyword), None);
    }

    #[test]
    fn alpha_is_written_to_two_places_at_most() {
        // Upstream's `testCSSAlphaColorCodeRounding`.
        let color = Color::from_code("#33669955").unwrap().1;
        assert_eq!(
            color.code(ColorCodeType::CssRgb).unwrap(),
            "rgb(51,102,153)"
        );
        assert_eq!(
            color.code(ColorCodeType::CssRgba).unwrap(),
            "rgba(51,102,153,0.33)"
        );
        assert_eq!(
            color.code(ColorCodeType::CssHsla).unwrap(),
            "hsla(210,50%,40%,0.33)"
        );
        assert_eq!(
            color.code(ColorCodeType::CssHwbWithAlpha).unwrap(),
            "hwb(210 20% 40% / 0.33)"
        );
    }

    #[test]
    fn a_trailing_zero_is_dropped_from_the_alpha() {
        // The rounding rule upstream gets from `fractionLength(0...2)`: one
        // decimal where one will do, none where none will.
        assert_eq!(css_alpha(1.), "1");
        assert_eq!(css_alpha(0.5), "0.5");
        assert_eq!(css_alpha(0.), "0");
        assert_eq!(css_alpha(0.25), "0.25");
        // Halves round away from zero, which `{:.2}` alone would not do.
        assert_eq!(css_alpha(0.125), "0.13");
        assert_eq!(css_alpha(0.135), "0.14");
    }

    #[test]
    fn a_non_finite_channel_writes_as_zero() {
        // Upstream's `testNaNAlphaColorCode`.
        let color = rgb(0., 0., 0., f64::NAN);
        assert_eq!(
            color.code(ColorCodeType::HexWithAlpha).unwrap(),
            "#00000000"
        );
        assert_eq!(
            color.code(ColorCodeType::ShortHexWithAlpha).unwrap(),
            "#0000"
        );
        assert_eq!(color.code(ColorCodeType::CssRgba).unwrap(), "rgba(0,0,0,0)");
        assert_eq!(
            color.code(ColorCodeType::CssHsla).unwrap(),
            "hsla(0,0%,0%,0)"
        );
        assert_eq!(
            color.code(ColorCodeType::CssHwbWithAlpha).unwrap(),
            "hwb(0 0% 100% / 0)"
        );
    }

    #[test]
    fn hwb_survives_a_trip_through_hex() {
        // Upstream's `testHWBColorCode`.
        let (kind, color) = Color::from_code("hwb(150 20% 10%)").unwrap();
        assert_eq!(kind, ColorCodeType::CssHwb);
        assert_eq!(color.code(ColorCodeType::Hex).unwrap(), "#33e68c");
        assert_eq!(
            color.code(ColorCodeType::CssHwb).unwrap(),
            "hwb(150 20% 10%)"
        );
    }

    #[test]
    fn hsla_survives_a_trip_through_rgb() {
        // Upstream's `testHSLaColorCode`: the notation has to come back out
        // exactly as it went in, which is the whole point of the `p`/`q`
        // conversion rather than a hue-in-degrees shortcut.
        let code = "hsla(203,10%,20%,0.3)";
        let (kind, color) = Color::from_code(code).unwrap();
        assert_eq!(kind, ColorCodeType::CssHsla);
        assert_eq!(color.code(ColorCodeType::CssHsla).unwrap(), code);
    }

    #[test]
    fn a_hwb_with_alpha_keeps_its_alpha() {
        let (kind, color) = Color::from_code("hwb(150 20% 10% / .5)").unwrap();
        assert_eq!(kind, ColorCodeType::CssHwbWithAlpha);
        assert!((color.alpha - 0.5).abs() < 1e-9);
        assert_eq!(
            color.code(ColorCodeType::CssHwbWithAlpha).unwrap(),
            "hwb(150 20% 10% / 0.5)"
        );
    }

    #[test]
    fn a_grey_spelling_resolves_to_its_value() {
        // Upstream's `testGreyAliases`: the `grey` spellings are aliases, and
        // both spellings of a value have to be present for that to work.
        for (keyword, value, hex) in [
            ("darkslategrey", 0x2f4f4f_u32, "#2f4f4f"),
            ("dimgrey", 0x696969, "#696969"),
            ("slategrey", 0x708090, "#708090"),
            ("lightslategrey", 0x778899, "#778899"),
            ("grey", 0x808080, "#808080"),
            ("darkgrey", 0xa9a9a9, "#a9a9a9"),
            ("lightgrey", 0xd3d3d3, "#d3d3d3"),
        ] {
            assert_eq!(value_for_keyword(keyword), Some(value), "{keyword}");
            let color = Color::from_code(keyword).unwrap().1;
            assert_eq!(color.code(ColorCodeType::Hex).unwrap(), hex, "{keyword}");
        }
    }

    #[test]
    fn the_first_name_for_a_value_is_the_one_reported() {
        // Two names share `#00ffff` and two share `#ff00ff`; upstream reports
        // whichever it lists first, so the table's order is load-bearing.
        assert_eq!(keyword_for_value(0x00ffff), Some("aqua"));
        assert_eq!(keyword_for_value(0xff00ff), Some("fuchsia"));
        assert_eq!(keyword_for_value(0x808080), Some("gray"));
        assert_eq!(keyword_for_value(0x123456), None);
        assert_eq!(KEYWORD_COLORS.len(), 148);
    }

    #[test]
    fn every_notation_has_a_stable_stored_value() {
        // The numbers are CotEditor's `ColorCodeType` raw values; they are what
        // a settings file written by CotEditor carries.
        for (kind, raw) in [
            (ColorCodeType::Hex, 1),
            (ColorCodeType::HexWithAlpha, 2),
            (ColorCodeType::ShortHex, 3),
            (ColorCodeType::ShortHexWithAlpha, 4),
            (ColorCodeType::CssRgb, 5),
            (ColorCodeType::CssRgba, 6),
            (ColorCodeType::CssHsl, 7),
            (ColorCodeType::CssHsla, 8),
            (ColorCodeType::CssHwb, 9),
            (ColorCodeType::CssHwbWithAlpha, 10),
            (ColorCodeType::CssKeyword, 11),
        ] {
            assert_eq!(kind.raw_value(), raw);
            assert_eq!(ColorCodeType::from_raw_value(raw), kind);
        }
        assert_eq!(ColorCodeType::from_raw_value(0), ColorCodeType::Hex);
        assert_eq!(ColorCodeType::from_raw_value(99), ColorCodeType::Hex);
        assert_eq!(ColorCodeType::default(), ColorCodeType::Hex);
    }

    #[test]
    fn the_two_menu_sections_hold_every_notation_once() {
        let mut seen = ColorCodeType::HEX_TYPES.to_vec();
        seen.extend(ColorCodeType::CSS_TYPES);
        assert_eq!(seen.len(), ColorCodeType::ALL.len());
        for kind in ColorCodeType::ALL {
            assert!(seen.contains(&kind), "{kind:?} is in no section");
            assert!(kind.is_hex() == ColorCodeType::HEX_TYPES.contains(&kind));
        }
    }

    #[test]
    fn a_colour_hands_the_view_the_components_the_picker_wants() {
        // GPUI's `Hsla` keeps the hue as a fraction of a turn too, so this is a
        // widening rather than a conversion.
        let (hue, saturation, lightness, alpha) = Color::from_code("#336699").unwrap().1.hsla();
        assert!((hue - 0.583_333_3).abs() < 1e-5, "{hue}");
        assert!((saturation - 0.5).abs() < 1e-5, "{saturation}");
        assert!((lightness - 0.4).abs() < 1e-5, "{lightness}");
        assert!((alpha - 1.).abs() < 1e-5);
    }

    #[test]
    fn a_grey_has_no_hue_to_report() {
        let (hue, saturation, ..) = Color::from_code("#808080").unwrap().1.hsla();
        assert_eq!(hue, 0.);
        assert_eq!(saturation, 0.);
    }
}
