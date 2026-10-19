//! Text encodings, byte-order marks and decoding/encoding of documents.
//!
//! CotEditor's defining trait is that it handles *any* plain-text file
//! correctly rather than assuming UTF-8. This module reproduces that surface:
//! a named encoding registry, BOM detection and preservation, the user's
//! priority list, and a detection pipeline that follows upstream's order —
//! the byte-order mark, the declaration the file carries, the priority list,
//! and finally the user's default encoding.
//!
//! # The priority list is the detector
//!
//! There is no statistical guess here, and upstream has none either:
//! `String.string(data:options:)` tries the candidates in order and takes the
//! first the bytes decode in. That is why the list is worth editing at all —
//! moving an encoding up is how the user says "try this one sooner", and taking
//! one out is how they say "never mind this one". See
//! [`EncodingList`] and `about_autodetect_encoding.html`.

use std::fmt;
use std::sync::LazyLock;

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

/// The byte-order marks SubText recognises, in the order they must be tested.
///
/// UTF-32 has to come first: `FF FE 00 00` also starts with the UTF-16LE mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Bom {
    #[default]
    None,
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

impl Bom {
    /// The mark's bytes, or an empty slice for [`Bom::None`].
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Bom::None => &[],
            Bom::Utf8 => &[0xEF, 0xBB, 0xBF],
            Bom::Utf16Le => &[0xFF, 0xFE],
            Bom::Utf16Be => &[0xFE, 0xFF],
            Bom::Utf32Le => &[0xFF, 0xFE, 0x00, 0x00],
            Bom::Utf32Be => &[0x00, 0x00, 0xFE, 0xFF],
        }
    }

    /// How many bytes the mark occupies. `Bom::None` occupies none.
    pub fn len(self) -> usize {
        self.bytes().len()
    }

    /// Whether the file carries no byte-order mark at all.
    pub fn is_empty(self) -> bool {
        self.bytes().is_empty()
    }

    /// The encoding a mark implies, if any. A BOM is authoritative.
    pub fn encoding(self) -> Option<TextEncoding> {
        match self {
            Bom::None => None,
            Bom::Utf8 => Some(TextEncoding::UTF8),
            // Both byte orders map to the **unspecified-order** form, which is
            // what upstream's `Unicode.BOM.encoding` does (`.utf16` / `.utf32`)
            // and what its detection tests assert: a BOM-bearing file is
            // reported as "Unicode (UTF-16)", never as "…LE"/"…BE".
            Bom::Utf16Le | Bom::Utf16Be => Some(TextEncoding::UTF16),
            Bom::Utf32Le | Bom::Utf32Be => Some(TextEncoding::UTF32),
        }
    }
}

impl fmt::Display for Bom {
    /// How the Document inspector names the mark.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Bom::None => "None",
            Bom::Utf8 => "UTF-8",
            Bom::Utf16Le => "UTF-16 LE",
            Bom::Utf16Be => "UTF-16 BE",
            Bom::Utf32Le => "UTF-32 LE",
            Bom::Utf32Be => "UTF-32 BE",
        })
    }
}

/// Sniff the byte-order mark at the head of `bytes`.
pub fn detect_bom(bytes: &[u8]) -> Bom {
    const MARKS: [Bom; 5] = [
        Bom::Utf32Le,
        Bom::Utf32Be,
        Bom::Utf8,
        Bom::Utf16Le,
        Bom::Utf16Be,
    ];
    for bom in MARKS {
        if bytes.starts_with(bom.bytes()) {
            return bom;
        }
    }
    Bom::None
}

/// A named text encoding.
///
/// Most variants wrap an [`encoding_rs`] codec. UTF-16 and UTF-32 are handled
/// by this crate directly, because `encoding_rs` deliberately omits them.
///
/// # The identifier
///
/// An encoding's identifier is *not* derived from its codec. It cannot be:
/// several of the encodings CotEditor lists share a codec here, because
/// `encoding_rs` implements the WHATWG Encoding Standard rather than Apple's
/// codec set. `encoding_rs::BIG5` is what both "Traditional Chinese (Big 5)"
/// and "Traditional Chinese (Big 5 HKSCS)" decode with; `encoding_rs::SHIFT_JIS`
/// is what both Japanese Shift JIS entries use. Deriving the identifier from the
/// codec would give those pairs the same one, and an encoding the user cannot
/// name is an encoding they cannot choose — the menu item would run whichever of
/// the two happened to come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextEncoding {
    /// The name CotEditor shows in the encoding menu.
    pub name: &'static str,
    /// The spelling this encoding is stored and chosen by.
    id: &'static str,
    kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Rs(&'static Encoding),
    /// "Unicode (UTF-16)": the byte order is written as the platform's own and
    /// the file carries a mark; reading follows the mark. Foundation's `.utf16`,
    /// which is what upstream uses.
    Utf16,
    /// "Unicode (UTF-16LE)": this order, **no** mark — `data(using:
    /// .utf16LittleEndian)` does not write one.
    Utf16Le,
    Utf16Be,
    /// "Unicode (UTF-32)", by the same rules as [`Kind::Utf16`].
    Utf32,
    Utf32Le,
    Utf32Be,
}

/// Whether this build's native byte order is little-endian.
///
/// The "unspecified order" forms write in it — Foundation does the same, which
/// is why upstream's saved file is not reproducible across architectures.
const NATIVE_IS_LITTLE: bool = cfg!(target_endian = "little");

impl TextEncoding {
    const fn rs(name: &'static str, id: &'static str, encoding: &'static Encoding) -> Self {
        Self {
            name,
            id,
            kind: Kind::Rs(encoding),
        }
    }

    pub const UTF8: Self = Self::rs("Unicode (UTF-8)", "utf-8", UTF_8);
    // The four wide forms are named the way Foundation names them — "Unicode
    // (UTF-16BE)", not "Unicode (UTF-16 Big-Endian)" — because that is the string
    // upstream's menu shows, and the whole registry is spelled that way.
    pub const UTF16: Self = Self {
        name: "Unicode (UTF-16)",
        id: "utf-16",
        kind: Kind::Utf16,
    };
    pub const UTF32: Self = Self {
        name: "Unicode (UTF-32)",
        id: "utf-32",
        kind: Kind::Utf32,
    };
    pub const UTF16_LE: Self = Self {
        name: "Unicode (UTF-16LE)",
        id: "utf-16le",
        kind: Kind::Utf16Le,
    };
    pub const UTF16_BE: Self = Self {
        name: "Unicode (UTF-16BE)",
        id: "utf-16be",
        kind: Kind::Utf16Be,
    };
    pub const UTF32_LE: Self = Self {
        name: "Unicode (UTF-32LE)",
        id: "utf-32le",
        kind: Kind::Utf32Le,
    };
    pub const UTF32_BE: Self = Self {
        name: "Unicode (UTF-32BE)",
        id: "utf-32be",
        kind: Kind::Utf32Be,
    };

    /// Whether this encoding leaves the byte order to the mark.
    ///
    /// True for "Unicode (UTF-16)" and "Unicode (UTF-32)" — the two forms that
    /// write a mark and read it back — and false for the endian-specific ones,
    /// which are written without a mark and never guess.
    pub fn is_unspecified_order(self) -> bool {
        matches!(self.kind, Kind::Utf16 | Kind::Utf32)
    }

    /// Whether a byte-order mark is meaningful for this encoding.
    ///
    /// Only the Unicode family has one; a BOM written in front of, say,
    /// Shift JIS would just be mojibake.
    pub fn supports_bom(self) -> bool {
        self.is_unicode()
    }

    pub fn is_unicode(self) -> bool {
        match self.kind {
            Kind::Utf16 | Kind::Utf16Le | Kind::Utf16Be => true,
            Kind::Utf32 | Kind::Utf32Le | Kind::Utf32Be => true,
            Kind::Rs(e) => e == UTF_8 || e == UTF_16LE || e == UTF_16BE,
        }
    }

    /// The byte-order mark a document in this encoding is written with.
    ///
    /// Not the inverse of [`Bom::encoding`]: UTF-8 is written without one by
    /// default, and CotEditor keeps the mark as a property of the *file* rather
    /// than of the encoding — see [`FileEncoding`]. Only the two forms whose
    /// byte order is unspecified imply a mark; the endian-specific ones are
    /// unambiguous and Foundation writes none for them.
    pub fn bom(self) -> Bom {
        match self.kind {
            Kind::Utf16 => {
                if NATIVE_IS_LITTLE {
                    Bom::Utf16Le
                } else {
                    Bom::Utf16Be
                }
            }
            Kind::Utf32 => {
                if NATIVE_IS_LITTLE {
                    Bom::Utf32Le
                } else {
                    Bom::Utf32Be
                }
            }
            // The endian-specific forms are written **without** a mark:
            // Foundation's `data(using: .utf16LittleEndian)` produces none, and
            // upstream writes whatever Foundation produces.
            Kind::Utf16Le | Kind::Utf16Be | Kind::Utf32Le | Kind::Utf32Be => Bom::None,
            Kind::Rs(_) => Bom::None,
        }
    }

    /// A stable identifier, used in settings files and as a menu item's
    /// payload.
    pub fn identifier(self) -> &'static str {
        self.id
    }

    /// Look an encoding up by its identifier.
    ///
    /// Three spellings resolve, in this order: the identifier itself, one of the
    /// spellings in [`LEGACY_IDENTIFIERS`], and a WHATWG label — `"shift_jis"`,
    /// `"euc-jp"`, `"windows-1252"`. The label route is what a settings file
    /// written before the identifiers were made unique holds, and it is why the
    /// identifiers here are labels wherever a label exists.
    ///
    /// The label route answers with the first entry in [`AVAILABLE`] that uses
    /// the codec, not with the entry the label is named after: several entries
    /// share a codec, and a label can only name the codec. `"iso-8859-9"` is the
    /// Turkish standard, whose label the Encoding Standard resolves to
    /// windows-1254, so it lands on "Turkish (ISO Latin 5)" rather than on the
    /// unlisted "Turkish (Windows Latin 5)" — the same codec either way.
    pub fn from_identifier(name: &str) -> Option<Self> {
        let lowered = name.trim().to_ascii_lowercase();
        match lowered.as_str() {
            "utf-16" => return Some(Self::UTF16),
            "utf-32" => return Some(Self::UTF32),
            "utf-16le" | "utf-16-le" => return Some(Self::UTF16_LE),
            "utf-16be" | "utf-16-be" => return Some(Self::UTF16_BE),
            "utf-32le" | "utf-32-le" => return Some(Self::UTF32_LE),
            "utf-32be" | "utf-32-be" => return Some(Self::UTF32_BE),
            _ => {}
        }
        if let Some(found) = AVAILABLE.iter().copied().find(|e| e.id == lowered) {
            return Some(found);
        }
        if let Some((_, id)) = LEGACY_IDENTIFIERS.iter().find(|(old, _)| *old == lowered) {
            return AVAILABLE.iter().copied().find(|e| e.id == *id);
        }
        let encoding = Encoding::for_label(lowered.as_bytes())?;
        AVAILABLE
            .iter()
            .copied()
            .find(|e| matches!(e.kind, Kind::Rs(x) if x == encoding))
    }

    /// Decode `bytes` (a BOM, if present, is expected to be already stripped).
    pub fn decode(self, bytes: &[u8]) -> (String, bool) {
        match self.kind {
            Kind::Rs(encoding) => {
                let (text, _, had_errors) = encoding.decode(bytes);
                (text.into_owned(), had_errors)
            }
            // The unspecified-order forms read the mark themselves, because
            // the body cannot be interpreted without it. Without one, Foundation
            // assumes **big**-endian for UTF-16 and refuses UTF-32 outright —
            // both measured, not guessed.
            Kind::Utf16 => match detect_bom(bytes) {
                Bom::Utf16Le => decode_utf16(&bytes[2..], true),
                Bom::Utf16Be => decode_utf16(&bytes[2..], false),
                _ => decode_utf16(bytes, false),
            },
            Kind::Utf32 => match detect_bom(bytes) {
                Bom::Utf32Le => decode_utf32(&bytes[4..], true),
                Bom::Utf32Be => decode_utf32(&bytes[4..], false),
                _ => (String::new(), true),
            },
            Kind::Utf16Le => decode_utf16(bytes, true),
            Kind::Utf16Be => decode_utf16(bytes, false),
            Kind::Utf32Le => decode_utf32(bytes, true),
            Kind::Utf32Be => decode_utf32(bytes, false),
        }
    }

    /// Encode `text`. Returns the bytes and whether any character was replaced.
    pub fn encode(self, text: &str) -> (Vec<u8>, bool) {
        match self.kind {
            Kind::Rs(encoding) => {
                let (bytes, _, had_errors) = encoding.encode(text);
                (bytes.into_owned(), had_errors)
            }
            // The mark is the caller's business (`DecodedFile::encode_checked`
            // writes it), so the body is bare in every case.
            Kind::Utf16 => (encode_utf16(text, NATIVE_IS_LITTLE), false),
            Kind::Utf16Le => (encode_utf16(text, true), false),
            Kind::Utf16Be => (encode_utf16(text, false), false),
            Kind::Utf32 => (encode_utf32(text, NATIVE_IS_LITTLE), false),
            Kind::Utf32Le => (encode_utf32(text, true), false),
            Kind::Utf32Be => (encode_utf32(text, false), false),
        }
    }
}

impl fmt::Display for TextEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> (String, bool) {
    // `as_chunks` rather than `chunks_exact`: clippy asks for it when the width
    // is a constant, and it hands back the remainder instead of making the
    // caller work out that there is one.
    let (pairs, remainder) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes(*pair)
            } else {
                u16::from_be_bytes(*pair)
            }
        })
        .collect();
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut lossy = !remainder.is_empty();
    for decoded in char::decode_utf16(units) {
        match decoded {
            Ok(c) => out.push(c),
            Err(_) => {
                out.push(char::REPLACEMENT_CHARACTER);
                lossy = true;
            }
        }
    }
    (out, lossy)
}

fn decode_utf32(bytes: &[u8], little_endian: bool) -> (String, bool) {
    let (chunks, remainder) = bytes.as_chunks::<4>();
    let mut out = String::with_capacity(bytes.len() / 4);
    let mut lossy = !remainder.is_empty();
    for chunk in chunks {
        let value = if little_endian {
            u32::from_le_bytes(*chunk)
        } else {
            u32::from_be_bytes(*chunk)
        };
        match char::from_u32(value) {
            Some(c) => out.push(c),
            None => {
                out.push(char::REPLACEMENT_CHARACTER);
                lossy = true;
            }
        }
    }
    (out, lossy)
}

fn encode_utf16(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    let mut buf = [0u16; 2];
    for c in text.chars() {
        for unit in c.encode_utf16(&mut buf) {
            let bytes = if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            };
            out.extend_from_slice(&bytes);
        }
    }
    out
}

fn encode_utf32(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 4);
    for c in text.chars() {
        let value = c as u32;
        let bytes = if little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

/// The encodings the `Format ▸ Text Encoding` menu lists, cut into the groups
/// CotEditor's default encoding list cuts them into.
///
/// The groups are the table. [`MENU`] is derived from them, so the flat list and
/// the menu cannot drift apart: the menu is the registry, separated in the same
/// places upstream separates its own. Nothing here is generated — the registry
/// is code, and only the menu *bar* comes out of `tools/`.
///
/// # The names are upstream's, not `encoding_rs`'s
///
/// Every name here is what `String.localizedName(of:)` answers for the
/// `CFStringEncoding` upstream lists, because that is the string upstream's menu
/// shows. It is not the name of the codec standing behind it: the menu item
/// "Traditional Chinese (Big 5 HKSCS)" is drawn by `encoding_rs`'s `Big5`.
/// Where upstream's source comment disagrees with Foundation — it writes
/// "Chinese (GB18030)" where Foundation says "Chinese (GB 18030)" — the code
/// follows Foundation.
///
/// # Why this is not upstream's list entry for entry
///
/// `encoding_rs` implements the WHATWG Encoding Standard, a Windows-centric
/// subset of Apple's codec set, so upstream's sixty default entries split three
/// ways.
///
/// **Listed**, under upstream's name. Every entry whose codec `encoding_rs` has.
///
/// **Moved to [`UNLISTED`]**, six entries SubText used to list here and upstream
/// can add from its full set but does not list by default. The menu has to agree
/// with upstream's, so they left it; they stayed in the registry.
///
/// **Absent**, twenty-two entries, each named with its reason in the gaps table
/// of `docs/PLAN.md`:
///
/// - eleven Mac OS codecs — `macJapanese`, `macChineseTrad`, `macChineseSimp`,
///   `macKorean`, `macArabic`, `macGreek`, `macHebrew`, `macCyrillic`,
///   `macCentralEurRoman`, `macTurkish`, `macIcelandic`. `encoding_rs` has one
///   Mac codec of any kind, and it is not Mac Cyrillic — see [`UNLISTED`].
/// - `NextStep Latin`, `ASCII`, and `Latin-US (DOS)` (code page 437), which no
///   codec here covers.
/// - `DOS Japanese` (cp932), `DOS Korean` (cp949), the two DOS Chinese entries
///   (cp936, cp950) and `Big 5-E`: the Windows code pages that the WHATWG
///   `Shift_JIS`, `EUC_KR`, `GBK` and `Big5` tables already *are*, so a second
///   name on the same codec would add a menu item and no capability.
/// - `EUC-TW`, which no codec here covers.
/// - `Unicode (UTF-16)` and `Unicode (UTF-32)`, the two entries that mean "let
///   the byte-order mark decide". SubText's `FileEncoding` makes the byte order
///   explicit instead; see the gaps table.
///
/// # The listed entries are not all exact
///
/// Eight of the thirty-eight are served by a codec that is a superset of the
/// standard they name, which is a deliberate trade rather than an oversight — a
/// superset decodes everything the standard would, and refuses only a few byte
/// sequences the standard would refuse. Two are forced by the Encoding Standard
/// resolving the label to a Windows code page: "Western (ISO Latin 1)" decodes
/// with windows-1252 and "Turkish (ISO Latin 5)" with windows-1254. Six are
/// forced by the standard having no such codec at all:
///
/// - "Thai (ISO 8859-11)" decodes with windows-874, which differs only in
///   `0x80`–`0x9F`;
/// - "Japanese (Shift JIS)" and "Japanese (Shift JIS X0213)" both decode with
///   the Windows Shift JIS — cp932, a superset of JIS X 0208 Shift JIS that has
///   none of JIS X 0213 plane 2;
/// - "Simplified Chinese (GB 2312)" decodes with GBK and "Korean (EUC)" with
///   what is really cp949, both supersets;
/// - "Traditional Chinese (Big 5 HKSCS)" decodes with the WHATWG Big 5 table,
///   which carries the HKSCS extensions but maps them to their real code points
///   rather than to the Private Use Area.
pub static MENU_GROUPS: &[&[TextEncoding]] = &[
    // Unicode (UTF-8). Upstream's first group is one entry, and its menu
    // builder splices "Unicode (UTF-8) with BOM" in right after it — see
    // `FileEncoding::menu_groups`.
    &[TextEncoding::UTF8],
    // Japanese. Upstream's group is six: Shift JIS, EUC, DOS Shift JIS, Shift
    // JIS X0213, Mac OS, ISO 2022-JP. The DOS one is cp932, which is what
    // `encoding_rs`'s one Shift JIS codec is, and the Mac OS one has no codec.
    &[
        TextEncoding::rs("Japanese (Shift JIS)", "shift_jis", encoding_rs::SHIFT_JIS),
        TextEncoding::rs("Japanese (EUC)", "euc-jp", encoding_rs::EUC_JP),
        TextEncoding::rs(
            "Japanese (Shift JIS X0213)",
            "shift_jis_x0213",
            encoding_rs::SHIFT_JIS,
        ),
        TextEncoding::rs(
            "Japanese (ISO 2022-JP)",
            "iso-2022-jp",
            encoding_rs::ISO_2022_JP,
        ),
    ],
    // Western. Upstream's two entries are exactly these.
    &[
        TextEncoding::rs(
            "Western (Mac OS Roman)",
            "macintosh",
            encoding_rs::MACINTOSH,
        ),
        TextEncoding::rs(
            "Western (Windows Latin 1)",
            "windows-1252",
            encoding_rs::WINDOWS_1252,
        ),
    ],
    // Chinese. Upstream's group is ten: GB 18030, Big 5 HKSCS, Big 5-E, Big 5,
    // the two Mac OS ones, EUC-TW, GB 2312, and the two DOS ones. Big 5 and
    // Big 5-HKSCS are one codec here — the WHATWG Big 5 table already carries
    // the HKSCS extensions — and so are GB 2312 and GBK.
    &[
        TextEncoding::rs("Chinese (GB 18030)", "gb18030", encoding_rs::GB18030),
        TextEncoding::rs(
            "Traditional Chinese (Big 5 HKSCS)",
            "big5_hkscs",
            encoding_rs::BIG5,
        ),
        TextEncoding::rs("Traditional Chinese (Big 5)", "big5", encoding_rs::BIG5),
        TextEncoding::rs("Simplified Chinese (GB 2312)", "gb2312", encoding_rs::GBK),
    ],
    // Korean. Upstream's group is three: the Mac OS one, EUC, and the DOS one
    // (cp949, which is what `encoding_rs`'s EUC-KR codec is).
    &[TextEncoding::rs(
        "Korean (EUC)",
        "euc-kr",
        encoding_rs::EUC_KR,
    )],
    // Thai, in upstream's order: the DOS one (cp874, which Windows shares) first,
    // then ISO 8859-11.
    &[
        TextEncoding::rs(
            "Thai (Windows, DOS)",
            "windows-874",
            encoding_rs::WINDOWS_874,
        ),
        TextEncoding::rs(
            "Thai (ISO 8859-11)",
            "iso-8859-11",
            encoding_rs::WINDOWS_874,
        ),
    ],
    // Arabic, Greek, Hebrew, Cyrillic. Upstream's group of sixteen is mostly Mac
    // OS codecs. The three Cyrillic entries are in upstream's order — ISO 8859-5,
    // Windows, DOS — and the DOS one is cp866, which `encoding_rs` has as IBM866.
    // `macCyrillic` is not here: see `UNLISTED`. `macCentralEurRoman`,
    // `macTurkish` and `macIcelandic` have no codec at all, and the Turkish
    // standard is in the ISO Latin group below, which is where upstream's own
    // Turkish entry is.
    &[
        TextEncoding::rs("Arabic (ISO 8859-6)", "iso-8859-6", encoding_rs::ISO_8859_6),
        TextEncoding::rs(
            "Arabic (Windows)",
            "windows-1256",
            encoding_rs::WINDOWS_1256,
        ),
        TextEncoding::rs("Greek (ISO 8859-7)", "iso-8859-7", encoding_rs::ISO_8859_7),
        TextEncoding::rs("Greek (Windows)", "windows-1253", encoding_rs::WINDOWS_1253),
        TextEncoding::rs("Hebrew (ISO 8859-8)", "iso-8859-8", encoding_rs::ISO_8859_8),
        TextEncoding::rs(
            "Hebrew (Windows)",
            "windows-1255",
            encoding_rs::WINDOWS_1255,
        ),
        TextEncoding::rs(
            "Cyrillic (ISO 8859-5)",
            "iso-8859-5",
            encoding_rs::ISO_8859_5,
        ),
        TextEncoding::rs(
            "Cyrillic (Windows)",
            "windows-1251",
            encoding_rs::WINDOWS_1251,
        ),
        TextEncoding::rs("Russian (DOS)", "ibm866", encoding_rs::IBM866),
    ],
    // ISO Latin 1 through 10, in upstream's order and under upstream's names —
    // which are the language each one serves, not the number's ordinal.
    //
    // `encoding_rs` has neither ISO 8859-1 nor ISO 8859-9: the Encoding
    // Standard resolves both labels to the Windows code pages, so ISO Latin 1
    // is decoded as windows-1252 and ISO Latin 5 as windows-1254. Each is a
    // superset of the standard it stands for, which is why the pair is offered
    // rather than dropped — but the item says "ISO Latin 1" and answers with
    // windows-1252, which is recorded in the gaps table of `docs/PLAN.md`.
    //
    // The three entries upstream's group seven also holds — ISO Latin 2, ISO
    // Latin 4 and the Turkish Windows code page — are not repeated here:
    // upstream names them once, in this group.
    &[
        TextEncoding::rs(
            "Western (ISO Latin 1)",
            "iso-8859-1",
            encoding_rs::WINDOWS_1252,
        ),
        TextEncoding::rs(
            "Central European (ISO Latin 2)",
            "iso-latin-2",
            encoding_rs::ISO_8859_2,
        ),
        TextEncoding::rs(
            "Western (ISO Latin 3)",
            "iso-8859-3",
            encoding_rs::ISO_8859_3,
        ),
        TextEncoding::rs(
            "Central European (ISO Latin 4)",
            "iso-latin-4",
            encoding_rs::ISO_8859_4,
        ),
        TextEncoding::rs(
            "Turkish (ISO Latin 5)",
            "iso-latin-5",
            encoding_rs::WINDOWS_1254,
        ),
        TextEncoding::rs(
            "Nordic (ISO Latin 6)",
            "iso-8859-10",
            encoding_rs::ISO_8859_10,
        ),
        TextEncoding::rs(
            "Baltic (ISO Latin 7)",
            "iso-8859-13",
            encoding_rs::ISO_8859_13,
        ),
        TextEncoding::rs(
            "Celtic (ISO Latin 8)",
            "iso-8859-14",
            encoding_rs::ISO_8859_14,
        ),
        TextEncoding::rs(
            "Western (ISO Latin 9)",
            "iso-8859-15",
            encoding_rs::ISO_8859_15,
        ),
        TextEncoding::rs(
            "Romanian (ISO Latin 10)",
            "iso-8859-16",
            encoding_rs::ISO_8859_16,
        ),
    ],
    // Central European, Windows code page. Upstream's group of four is a DOS
    // Latin-US, this one, a NextStep Latin and ASCII; `encoding_rs` has none of
    // the other three. This one entry is upstream's `windowsLatin2`, and it is
    // the only place windows-1250 appears — the two names SubText used to carry
    // for it were one code page.
    &[TextEncoding::rs(
        "Central European (Windows Latin 2)",
        "windows-1250",
        encoding_rs::WINDOWS_1250,
    )],
    // Unicode, wide. Upstream's group is six: UTF-16, then its two byte orders,
    // then UTF-32 and its two — in that order, which is why the unspecified ones
    // come first in each pair.
    &[
        TextEncoding::UTF16,
        TextEncoding::UTF16_BE,
        TextEncoding::UTF16_LE,
        TextEncoding::UTF32,
        TextEncoding::UTF32_BE,
        TextEncoding::UTF32_LE,
    ],
];

/// Encodings SubText can read and write that the shipped list does not name.
///
/// Upstream's encoding list is a *subset* of `String.availableStringEncodings`:
/// the settings pane can add any encoding the platform has, and the default list
/// is a curated sixty of them. These are the ones SubText can serve that
/// upstream can add but does not list by default — the shipped list has to
/// agree with upstream's, so they are not in it, but they stay in the registry
/// for two reasons: a settings file written by an earlier build spells six of
/// them, and the encoding list editor's Add menu offers them.
///
/// Six of the seven were in the menu until the list was aligned with upstream's;
/// the seventh is "Cyrillic (Mac OS Ukrainian)", and it is here rather than in the
/// menu because of a collision worth knowing about. Upstream lists `macCyrillic`
/// — "Cyrillic (Mac OS)", code page 10007 — while `encoding_rs`'s only Mac
/// Cyrillic-family codec, `X_MAC_CYRILLIC`, documents itself as Apple's
/// *MacUkrainian* (code page 10017) even though it carries Mac Cyrillic's IANA
/// name. Naming that codec after upstream's entry would name one table and
/// decode with another, so it is offered under the name Foundation gives the
/// table it really is.
pub static UNLISTED: &[TextEncoding] = &[
    TextEncoding::rs("Cyrillic (KOI8-R)", "koi8-r", encoding_rs::KOI8_R),
    TextEncoding::rs("Ukrainian (KOI8-U)", "koi8-u", encoding_rs::KOI8_U),
    TextEncoding::rs(
        "Turkish (Windows Latin 5)",
        "windows-1254",
        encoding_rs::WINDOWS_1254,
    ),
    TextEncoding::rs(
        "Baltic (Windows)",
        "windows-1257",
        encoding_rs::WINDOWS_1257,
    ),
    TextEncoding::rs(
        "Vietnamese (Windows)",
        "windows-1258",
        encoding_rs::WINDOWS_1258,
    ),
    TextEncoding::rs("Chinese (GBK)", "gbk", encoding_rs::GBK),
    TextEncoding::rs(
        "Cyrillic (Mac OS Ukrainian)",
        "x-mac-cyrillic",
        encoding_rs::X_MAC_CYRILLIC,
    ),
];

/// The encoding list SubText ships with, in menu order.
///
/// [`MENU_GROUPS`] flattened. Derived rather than written out, so an encoding
/// cannot end up in the registry but not in the menu, or the other way round.
///
/// This is upstream's *default* encoding list — `DefaultSettings.encodings`,
/// which is what a fresh install's `encodingList` holds. The list in force is
/// the user's, [`EncodingList`], which starts here and can be edited; the menu,
/// the open dialog and the default-encoding picker all read that one. [`MENU`]
/// is still what a build with no settings to consult falls back to, and what
/// the editor's *Restore Defaults* puts back. [`AVAILABLE`] is the wider set.
pub static MENU: LazyLock<Vec<TextEncoding>> = LazyLock::new(|| {
    MENU_GROUPS
        .iter()
        .flat_map(|group| group.iter().copied())
        .collect()
});

/// Every encoding SubText can read and write: the shipped list, then
/// [`UNLISTED`].
///
/// Upstream's counterpart is `String.availableStringEncodings`, and this is
/// what the encoding list editor's Add menu offers. Only
/// [`from_identifier`](TextEncoding::from_identifier) searches this one; the
/// menu, the detector and the settings pickers all work from
/// [`EncodingList`], which is the narrower set upstream builds them from too.
pub static AVAILABLE: LazyLock<Vec<TextEncoding>> = LazyLock::new(|| {
    MENU.iter()
        .copied()
        .chain(UNLISTED.iter().copied())
        .collect()
});

/// Spellings this registry used to carry that are not WHATWG labels, so the
/// label fallback in [`from_identifier`](TextEncoding::from_identifier) cannot
/// find them.
///
/// One entry, and it is here because aligning the names with upstream merged two
/// entries into one. "Western (Windows Latin 2)" and "Central European
/// (Windows)" were both windows-1250 and upstream lists that code page once, as
/// "Central European (Windows Latin 2)"; the survivor took the WHATWG spelling
/// `windows-1250`, which the label fallback resolves unaided, and the other
/// spelling — which is not a label — needs this table.
///
/// The four other identifiers this alignment dropped resolve without help,
/// because each is a WHATWG label that the fallback maps to the same codec:
/// `iso-8859-2`, `iso-8859-4` and `iso-8859-9` name the standards the surviving
/// entries name, and the sixth dropped identifier was a second spelling of
/// `windows-1250`.
static LEGACY_IDENTIFIERS: &[(&str, &str)] = &[("windows-latin-2", "windows-1250")];

/// The encoding priority list: what the Text Encoding menus offer, and the
/// order the detector tries encodings in.
///
/// CotEditor's `encodingList`, and the one list behind three things — the
/// `Format ▸ Text Encoding` submenu, the default-encoding picker in the Format
/// settings pane, and the order [`DecodedFile::decode`] works through. Upstream
/// stores a flat array of `CFStringEncoding` with `kCFStringEncodingInvalidId`
/// standing for a separator; `None` says the same thing without a sentinel, and
/// it is the shape upstream's own `EncodingManager.fileEncodings(from:)`
/// produces before it splices the UTF-8-with-BOM item in.
///
/// # What the list refuses
///
/// Two entries cannot be taken out, and both refusals are
/// [`can_remove`](Self::can_remove)'s rather than the editor's: UTF-8, because
/// it is what a file with no mark and no declaration is tried against first,
/// and the encoding new documents are created with, because the picker has to
/// have something to show. The editor asks before it acts, so a caller that
/// forgets to ask still cannot get a list upstream would not accept.
///
/// # The default
///
/// [`defaults`](Self::defaults) is the menu's groups with a separator between
/// each pair, which is what upstream ships. A list that ends up empty — a
/// settings file holding nothing but separators, or one this build cannot read
/// a single entry of — falls back to it, which is what upstream's
/// `EncodingManager.init` does with the same case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingList(Vec<Option<TextEncoding>>);

impl Default for EncodingList {
    fn default() -> Self {
        Self::defaults()
    }
}

/// Why an entry cannot be taken out of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalError {
    /// UTF-8 is always tried first, and upstream keeps it there.
    Utf8,
    /// The encoding new documents are created with.
    DefaultEncoding,
}

impl RemovalError {
    /// What the editor says about the refusal.
    ///
    /// `name` is the encoding's display name, which only the first message
    /// needs — upstream's second one does not name the encoding, because the
    /// user cannot see which one it is from the list alone.
    pub fn message(self, name: &str) -> String {
        match self {
            RemovalError::Utf8 => format!("{name} can’t be removed."),
            RemovalError::DefaultEncoding => {
                "The encoding set as default can’t be removed.".to_string()
            }
        }
    }
}

impl EncodingList {
    /// Upstream's default list: the menu's groups, with a separator between
    /// each pair.
    pub fn defaults() -> Self {
        let mut items = Vec::with_capacity(MENU.len() + MENU_GROUPS.len());
        for (index, group) in MENU_GROUPS.iter().enumerate() {
            if index > 0 {
                items.push(None);
            }
            items.extend(group.iter().copied().map(Some));
        }
        Self(items)
    }

    /// The list a settings file spells, as identifiers.
    ///
    /// An entry that does not resolve is dropped rather than kept as a gap: the
    /// spellings that resolve are the ones this build can read, and an encoding
    /// it cannot read is one the menu could not offer and the detector could
    /// not decode with. Upstream reaches the same place from the other side —
    /// `fileEncodings(from:)` filters its list against
    /// `String.availableStringEncodings` on the way into the menu. A separator
    /// is not an entry that failed to resolve; it is one the user put there.
    pub fn from_identifiers<S: AsRef<str>>(identifiers: &[Option<S>]) -> Self {
        let items: Vec<Option<TextEncoding>> = identifiers
            .iter()
            .filter_map(|identifier| match identifier {
                Some(name) => TextEncoding::from_identifier(name.as_ref()).map(Some),
                None => Some(None),
            })
            .collect();
        // A list that is nothing but separators is as useless as an empty one,
        // and it is what a file written by a build with a different registry
        // decays into. Upstream tests only for the empty array, but its
        // sanitizer turns every unreadable value into a separator first, so the
        // two cases arrive together.
        if items.iter().all(Option::is_none) {
            return Self::defaults();
        }
        Self(items)
    }

    /// The list as identifiers, for a settings file.
    pub fn identifiers(&self) -> Vec<Option<&'static str>> {
        self.0
            .iter()
            .map(|item| item.map(TextEncoding::identifier))
            .collect()
    }

    /// How many entries the list holds, separators included.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The entry at `index`: an encoding, a separator, or `None` when the index
    /// is past the end.
    pub fn item(&self, index: usize) -> Option<Option<TextEncoding>> {
        self.0.get(index).copied()
    }

    /// The encodings the list holds, in order, with the separators dropped.
    ///
    /// This is what the detector works through and what the default-encoding
    /// picker offers. Upstream's `EncodingManager.fileEncodingCandidates`.
    pub fn encodings(&self) -> impl Iterator<Item = TextEncoding> + '_ {
        self.0.iter().filter_map(|item| *item)
    }

    pub fn contains(&self, encoding: TextEncoding) -> bool {
        self.0.contains(&Some(encoding))
    }

    /// Whether the list is the one [`defaults`](Self::defaults) builds.
    ///
    /// The editor's *Restore Defaults* is disabled while this is true, which is
    /// upstream's `canRestore`.
    pub fn is_default(&self) -> bool {
        self.0 == Self::defaults().0
    }

    /// Whether the entry at `index` can be taken out, and why not when it
    /// cannot. `None` for an index past the end, which is not removable either.
    pub fn can_remove(&self, index: usize, default_encoding: TextEncoding) -> Option<RemovalError> {
        match self.0.get(index).copied().flatten()? {
            encoding if encoding == TextEncoding::UTF8 => Some(RemovalError::Utf8),
            encoding if encoding == default_encoding => Some(RemovalError::DefaultEncoding),
            _ => None,
        }
    }

    /// Take the entry at `index` out. Returns whether anything went.
    ///
    /// The refusal is [`can_remove`](Self::can_remove)'s, and this does not ask
    /// it: a caller that means to remove has to check first. Upstream splits it
    /// the same way — its `remove(ids:)` filters through `canRemove` itself,
    /// but its model does not, which is what lets *Restore Defaults* put UTF-8
    /// back without the check getting in the way.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.0.len() {
            return false;
        }
        self.0.remove(index);
        true
    }

    /// Put `item` in below the last entry of `selection`, and return where it
    /// landed.
    ///
    /// `selection` is the row the user had selected when they reached for the
    /// Add button, which is how upstream's help page describes it: "select the
    /// item at the location to insert, click the Add + button". With nothing
    /// selected the entry goes to the top, which is upstream's
    /// `index?.advanced(by: 1) ?? 0`.
    pub fn insert_after(&mut self, selection: Option<usize>, item: Option<TextEncoding>) -> usize {
        let index = selection
            .filter(|index| *index < self.0.len())
            .map_or(0, |index| index + 1);
        self.0.insert(index, item);
        index
    }

    /// Move the entry at `index` one place up, and return where it landed.
    pub fn move_up(&mut self, index: usize) -> Option<usize> {
        let target = index.checked_sub(1)?;
        if index >= self.0.len() {
            return None;
        }
        self.0.swap(index, target);
        Some(target)
    }

    /// Move the entry at `index` one place down, and return where it landed.
    pub fn move_down(&mut self, index: usize) -> Option<usize> {
        let target = index + 1;
        if target >= self.0.len() {
            return None;
        }
        self.0.swap(index, target);
        Some(target)
    }

    /// Put the default list back.
    pub fn restore_defaults(&mut self) {
        self.0 = Self::defaults().0;
    }

    /// Every encoding the user could add, in the order the Add menu lists them.
    ///
    /// The whole registry less what the list already holds, sorted by display
    /// name — upstream sorts `String.availableStringEncodings` the same way and
    /// then offers it as one flat list. [`UNLISTED`] is here rather than in the
    /// menu for exactly this: upstream can add any encoding the platform has,
    /// and these seven are the ones SubText can serve that its default list
    /// does not hold.
    pub fn addable(&self) -> Vec<TextEncoding> {
        let mut encodings: Vec<TextEncoding> = AVAILABLE
            .iter()
            .copied()
            .filter(|encoding| !self.contains(*encoding))
            .collect();
        encodings.sort_by_key(|encoding| encoding.name);
        encodings
    }
}

/// An encoding as a *file* is written with it: a codec, plus whether a UTF-8
/// file carries a byte-order mark.
///
/// CotEditor's `FileEncoding`, and the reason its `Format ▸ Text Encoding` menu
/// has a "Unicode (UTF-8) with BOM" item at all: the mark is a property of the
/// file, not of UTF-8, so it cannot be a registry entry beside the codec — that
/// would leave the registry claiming two encodings where there is one. The flag
/// is only ever set for UTF-8; upstream asserts as much, and so does
/// [`from_identifier`](Self::from_identifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileEncoding {
    pub encoding: TextEncoding,
    pub with_utf8_bom: bool,
}

impl FileEncoding {
    /// `encoding`, written without a mark.
    pub const fn of(encoding: TextEncoding) -> Self {
        Self {
            encoding,
            with_utf8_bom: false,
        }
    }

    /// UTF-8 written with a mark. Upstream's one such encoding.
    pub const fn utf8_with_bom() -> Self {
        Self {
            encoding: TextEncoding::UTF8,
            with_utf8_bom: true,
        }
    }

    /// The file encoding of a document that was read as `encoding` with `bom`.
    pub fn of_document(encoding: TextEncoding, bom: Bom) -> Self {
        Self {
            encoding,
            with_utf8_bom: encoding == TextEncoding::UTF8 && bom == Bom::Utf8,
        }
    }

    /// The name CotEditor shows — `"Unicode (UTF-8) with BOM"`.
    pub fn name(self) -> String {
        if self.with_utf8_bom {
            format!("{} with BOM", self.encoding.name)
        } else {
            self.encoding.name.to_string()
        }
    }

    /// The spelling a menu item carries, and the one settings are written in.
    pub fn identifier(self) -> String {
        if self.with_utf8_bom {
            format!("{}-bom", self.encoding.identifier())
        } else {
            self.encoding.identifier().to_string()
        }
    }

    /// Look a file encoding up by its identifier. The inverse of
    /// [`identifier`](Self::identifier).
    pub fn from_identifier(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        if let Some(base) = trimmed.strip_suffix("-bom") {
            let encoding = TextEncoding::from_identifier(base)?;
            return (encoding == TextEncoding::UTF8).then(Self::utf8_with_bom);
        }
        Some(Self::of(TextEncoding::from_identifier(trimmed)?))
    }

    /// The byte-order mark this file encoding is written with.
    pub fn bom(self) -> Bom {
        if self.with_utf8_bom {
            Bom::Utf8
        } else {
            self.encoding.bom()
        }
    }

    /// `list` as a flat sequence: the UTF-8-with-BOM item spliced in just after
    /// UTF-8, and `None` wherever the list holds a separator.
    ///
    /// This is what a picker needs — upstream's *Default text encoding* picker
    /// draws its separators as `Divider()`s — and it is the single place the
    /// splice happens, so the menu and the picker cannot disagree about it.
    ///
    /// Upstream does the splice inside `EncodingManager.fileEncodings(from:)`,
    /// once, so that a user's reordered list still gets the pair together: the
    /// two are always handled as a set, and only the UTF-8 entry can produce
    /// the second one.
    pub fn menu_items(list: &EncodingList) -> Vec<Option<Self>> {
        list.0
            .iter()
            .flat_map(|item| match item {
                None => vec![None],
                Some(encoding) => {
                    let plain = Self::of(*encoding);
                    if *encoding == TextEncoding::UTF8 {
                        vec![Some(plain), Some(Self::utf8_with_bom())]
                    } else {
                        vec![Some(plain)]
                    }
                }
            })
            .collect()
    }

    /// `list` as the `Format ▸ Text Encoding` menu lists it: cut into groups at
    /// its separators, with the UTF-8-with-BOM item spliced in just after
    /// UTF-8.
    ///
    /// The separators become cuts between groups because that is what the menu
    /// builder wants. Cutting [`menu_items`](Self::menu_items) rather than
    /// mapping the list a second time is what makes the menu and the picker the
    /// same sequence of entries by construction — including for the awkward
    /// lists, a leading separator, a trailing one, or two in a row, which is
    /// what `the_separators_survive_the_cut_and_join` checks.
    pub fn menu_groups(list: &EncodingList) -> Vec<Vec<Self>> {
        Self::menu_items(list)
            .split(|item| item.is_none())
            .map(|group| group.iter().flatten().copied().collect())
            .collect()
    }
}

impl serde::Serialize for EncodingList {
    /// As the identifiers a settings file spells, with `null` for a separator.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.identifiers(), serializer)
    }
}

impl<'de> serde::Deserialize<'de> for EncodingList {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let identifiers: Vec<Option<String>> = serde::Deserialize::deserialize(deserializer)?;
        Ok(Self::from_identifiers(&identifiers))
    }
}

/// How an encoding was chosen for a document.
///
/// The four answers are upstream's four steps, in its order — see
/// `about_autodetect_encoding.html`: "BOMs (byte order marks), encoding
/// information stored in the file's extended attributes, the encoding
/// declaration in the document if enabled, and the text encoding priorities you
/// have configured". The extended-attribute step has no variant because SubText
/// does not read `com.apple.TextEncoding`; the rest are one for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    /// A byte-order mark decided it.
    Bom,
    /// The declaration the file carries decided it.
    Declared,
    /// The priority list decided it: the first candidate the bytes decode in.
    Candidate,
    /// Nothing matched; the caller's default was used.
    Default,
}

/// What the detector is allowed to try, and whether a declaration outranks the
/// list.
///
/// Upstream's `String.DetectionOptions` less its extended-attribute field. The
/// two that remain are the two the Format settings own, which is why they travel
/// together: the priority list is `Settings::encoding_list` and the switch is
/// `Settings::refer_to_encoding_tag`.
#[derive(Debug, Clone, Copy)]
pub struct DetectionOptions<'a> {
    /// The encodings to try, in order — the user's priority list.
    pub candidates: &'a [TextEncoding],
    /// Whether an encoding declaration in the file is honoured.
    pub respects_declaration: bool,
}

impl DetectionOptions<'_> {
    /// What a caller with no settings to hand wants: the registry's default
    /// list, and declarations honoured.
    ///
    /// Folder Find decodes files it did not open and has no document to take a
    /// list from, so it works from this. The list is [`MENU`], which is
    /// [`EncodingList::defaults`] with its separators dropped — the same
    /// encodings in the same order, which is what
    /// `the_default_list_is_the_menu` checks.
    pub fn defaults() -> DetectionOptions<'static> {
        DetectionOptions {
            candidates: &MENU,
            respects_declaration: true,
        }
    }
}

/// The outcome of reading a file: its text plus everything needed to write it
/// back out byte-for-byte identically.
#[derive(Debug, Clone)]
pub struct DecodedFile {
    pub text: String,
    pub encoding: TextEncoding,
    pub bom: Bom,
    pub detection: Detection,
}

impl DecodedFile {
    /// Read a file's text, detecting its encoding and BOM.
    ///
    /// The order is upstream's `String.string(data:options:)`, and it is the
    /// whole of what the priority list is for:
    ///
    /// 1. a byte-order mark, if the list holds the encoding that mark implies —
    ///    upstream asks the same question, so a mark whose encoding the user
    ///    took out of the list is not honoured;
    /// 2. the declaration the file carries, when `options` honours them and the
    ///    list holds what it names;
    /// 3. the candidates in the order the user put them in, taking the first
    ///    the bytes decode in without error;
    /// 4. `fallback` — CotEditor's "default encoding" setting.
    ///
    /// Upstream throws when step 3 finds nothing rather than reaching step 4;
    /// SubText reads the file with the default encoding instead, which is the
    /// same choice made one step earlier, because a document it refuses to open
    /// is not a document.
    pub fn decode(bytes: &[u8], fallback: TextEncoding, options: &DetectionOptions<'_>) -> Self {
        let bom = detect_bom(bytes);
        if let Some(encoding) = bom.encoding()
            && options.candidates.contains(&encoding)
        {
            // The unspecified-order forms read the mark themselves — stripping
            // it first would leave them guessing the order — so they get the
            // whole slice and the others get the body.
            let body = if encoding.is_unspecified_order() {
                bytes
            } else {
                &bytes[bom.len()..]
            };
            let (text, _) = encoding.decode(body);
            return Self {
                text,
                encoding,
                bom,
                detection: Detection::Bom,
            };
        }

        if options.respects_declaration
            && let Some(encoding) = scan_declaration(bytes)
            && options.candidates.contains(&encoding)
        {
            let (text, _) = encoding.decode(bytes);
            return Self {
                text,
                encoding,
                bom: Bom::None,
                detection: Detection::Declared,
            };
        }

        for encoding in options.candidates.iter().copied() {
            let (text, had_errors) = encoding.decode(bytes);
            if !had_errors {
                return Self {
                    text,
                    encoding,
                    bom: Bom::None,
                    detection: Detection::Candidate,
                };
            }
        }

        let (text, _) = fallback.decode(bytes);
        Self {
            text,
            encoding: fallback,
            bom: Bom::None,
            detection: Detection::Default,
        }
    }

    /// Serialize back to bytes, re-attaching the BOM when the document has one.
    /// Encode for writing, and say whether anything had to be substituted.
    ///
    /// The flag is CotEditor's `LossyEncodingError`: a document whose text its
    /// encoding cannot spell must not be written silently, because what lands
    /// on disk is then not what the user typed. Its own answer is to ask; the
    /// caller decides here.
    pub fn encode_checked(&self) -> (Vec<u8>, bool) {
        let (body, lossy) = self.encoding.encode(&self.text);
        let mark = self.written_mark();
        if mark.is_empty() {
            return (body, lossy);
        }
        let mut out = Vec::with_capacity(mark.len() + body.len());
        out.extend_from_slice(mark);
        out.extend_from_slice(&body);
        (out, lossy)
    }

    pub fn encode(&self) -> Vec<u8> {
        let (body, _) = self.encoding.encode(&self.text);
        let mark = self.written_mark();
        if mark.is_empty() {
            return body;
        }
        let mut out = Vec::with_capacity(mark.len() + body.len());
        out.extend_from_slice(mark);
        out.extend_from_slice(&body);
        out
    }

    /// The byte-order mark this file is written with.
    ///
    /// One rule, used by both writers — they disagreed once, and a file written
    /// with a big-endian mark over a little-endian body is unreadable.
    ///
    /// An unspecified-order encoding always writes its **own** mark: upstream's
    /// `FileEncoding.hasBOM` is true for `.utf16`/`.utf32` whatever mark the
    /// file it was read from carried, and the body goes out in the order that
    /// mark names. Every other encoding keeps the mark the *file* has, because
    /// UTF-8's mark is a property of the document rather than of the encoding.
    fn written_mark(&self) -> &'static [u8] {
        if self.encoding.is_unspecified_order() {
            self.encoding.bom().bytes()
        } else {
            self.bom.bytes()
        }
    }
}

/// How far into a file a declaration is looked for, in bytes.
///
/// Upstream's 1024, which is the largest span any of the specs allows — HTML's
/// `charset=` has to appear within the first 1024 bytes.
const DECLARATION_WINDOW: usize = 1024;

/// `@charset "…";` — CSS, and only at the very start of the file.
static CSS_DECLARATION: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"^@charset "(?<encoding>[-_.a-zA-Z0-9]+)";"#).expect("valid pattern")
});

/// `encoding:` / `coding:` / `fileencoding=` in a comment.
///
/// Python's PEP 263, Ruby's and Emacs's forms, and Vim's `fileencoding=`, which
/// upstream also reads here. Vim allows the line in the first *or last* five
/// lines; upstream reads only the first two, and so does this.
static COMMENT_DECLARATION: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?:(?:file)?encoding|coding)[:=] *["']? *(?<encoding>[-_.a-zA-Z0-9]+)"#)
        .expect("valid pattern")
});

/// `<meta charset="…">`, or `charset="…"` anywhere in a tag.
static HTML_DECLARATION: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?i)\scharset\s*= *["'](?<encoding>[-_.a-zA-Z0-9]+)["']"#)
        .expect("valid pattern")
});

/// The encoding a document declares about itself, if it declares one.
///
/// Upstream's `Data.scanEncodingDeclaration()`. Only the first
/// [`DECLARATION_WINDOW`] bytes are read, and they are read as Latin-1 rather
/// than as UTF-8: Latin-1 cannot fail, and it keeps every byte one character so
/// that an offset into the decoded head is the offset into the file. The four
/// patterns are all ASCII, so nothing else can be mistaken for a match.
///
/// The styles are not tried in order — the *earliest* match in the file wins,
/// which is what upstream's `min(by: range.lowerBound)` picks. A stylesheet
/// that opens with `@charset "…";` and later mentions `charset=` inside a
/// comment is a stylesheet.
///
/// The name is resolved as a WHATWG label, which is what SubText's identifiers
/// are wherever a label exists — so `Shift_JIS`, `EUC-JP` and `GB18030` all
/// land, and so do a few spellings Foundation would refuse (`latin1`, say).
/// The detector then has to find the answer in the priority list anyway, so a
/// name that resolves to something the user does not list changes nothing.
pub fn scan_declaration(bytes: &[u8]) -> Option<TextEncoding> {
    let head: String = bytes[..bytes.len().min(DECLARATION_WINDOW)]
        .iter()
        .map(|byte| char::from(*byte))
        .collect();

    // The whole match's start, not the capture's: upstream orders the three by
    // where the declaration begins.
    fn earliest<'a>(pattern: &regex::Regex, text: &'a str) -> Option<(usize, &'a str)> {
        let found = pattern.captures(text)?;
        let whole = found.get(0)?;
        Some((whole.start(), found.name("encoding")?.as_str()))
    }

    /// `text`'s first `count` lines, each with the offset it starts at.
    ///
    /// The offset is the point: the comment styles are matched line by line, and
    /// upstream can compare their matches with the other two styles' because a
    /// Swift `Substring` carries its parent's indices. A Rust `&str` slice
    /// starts counting from zero, so the offset has to be carried by hand or a
    /// declaration on the second line reads as one at the top of the file.
    fn first_lines(text: &str, count: usize) -> impl Iterator<Item = (usize, &str)> {
        let mut offset = 0;
        text.split(['\n', '\r']).take(count).map(move |line| {
            let start = offset;
            offset += line.len() + 1;
            (start, line)
        })
    }

    let html = earliest(&HTML_DECLARATION, &head)
        // Upstream refuses a `charset=` that anything non-ASCII precedes: in a
        // Latin-1 head those bytes are the accented characters, and a page with
        // prose before its meta tag has already declared itself by other means.
        .filter(|(start, _)| head[..*start].is_ascii());

    let comment = first_lines(&head, 2).find_map(|(offset, line)| {
        earliest(&COMMENT_DECLARATION, line).map(|(start, name)| (offset + start, name))
    });

    let (_, name) = [earliest(&CSS_DECLARATION, &head), comment, html]
        .into_iter()
        .flatten()
        .min_by_key(|(start, _)| *start)?;

    TextEncoding::from_identifier(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The options a caller with no settings to hand gets. Most of the tests
    /// here are about the pipeline rather than about the list, and the default
    /// list is what upstream would be working from too.
    fn defaults() -> DetectionOptions<'static> {
        DetectionOptions::defaults()
    }

    #[test]
    fn utf32_bom_is_not_read_as_utf16() {
        assert_eq!(detect_bom(&[0xFF, 0xFE, 0x00, 0x00, b'a']), Bom::Utf32Le);
        assert_eq!(detect_bom(&[0xFF, 0xFE, b'a', 0x00]), Bom::Utf16Le);
    }

    #[test]
    fn utf8_round_trips_with_bom() {
        let file = DecodedFile {
            text: "こんにちは".into(),
            encoding: TextEncoding::UTF8,
            bom: Bom::Utf8,
            detection: Detection::Bom,
        };
        let bytes = file.encode();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);

        let again = DecodedFile::decode(&bytes, TextEncoding::UTF8, &defaults());
        assert_eq!(again.text, "こんにちは");
        assert_eq!(again.bom, Bom::Utf8);
    }

    #[test]
    fn a_utf16_file_with_a_mark_round_trips() {
        let original = "日本語テキスト";
        // Without a BOM, UTF-16 is not reliably distinguishable from other
        // encodings; the BOM is what makes it unambiguous, so test that path.
        let mut bytes = Bom::Utf16Le.bytes().to_vec();
        bytes.extend_from_slice(&encode_utf16(original, true));

        let file = DecodedFile::decode(&bytes, TextEncoding::UTF8, &defaults());
        assert_eq!(file.text, original);
        // A mark maps to the unspecified-order form, so the document says
        // "Unicode (UTF-16)" and not "…LE" — upstream's detection does the same.
        assert_eq!(file.encoding, TextEncoding::UTF16);
        assert_eq!(file.detection, Detection::Bom);
        assert_eq!(file.encode(), bytes);
    }

    /// A big-endian file comes back **little**-endian on a little-endian
    /// machine, because the encoding it is detected as is the unspecified-order
    /// one and Foundation writes that in the platform's order.
    ///
    /// This is upstream's behaviour, verified against its own detection test
    /// (`unicodeBOMWithEndianSpecificCandidate` expects `.utf16` for both byte
    /// orders). It is a quirk worth pinning down rather than a property worth
    /// admiring.
    #[test]
    fn a_big_endian_file_is_rewritten_in_the_platform_order() {
        let original = "ab";
        let mut bytes = Bom::Utf16Be.bytes().to_vec();
        bytes.extend_from_slice(&encode_utf16(original, false));

        let file = DecodedFile::decode(&bytes, TextEncoding::UTF8, &defaults());
        assert_eq!(file.text, original);
        assert_eq!(file.encoding, TextEncoding::UTF16);

        let rewritten = file.encode();
        assert_eq!(
            &rewritten[..2],
            Bom::Utf16Le.bytes(),
            "the mark follows the order written"
        );
        if cfg!(target_endian = "little") {
            assert_ne!(
                rewritten, bytes,
                "the byte order changed, as upstream's does"
            );
            assert_eq!(
                DecodedFile::decode(&rewritten, TextEncoding::UTF8, &defaults()).text,
                original
            );
        }
    }

    #[test]
    fn a_utf32_file_with_a_mark_round_trips() {
        let original = "abc😀";
        let mut bytes = Bom::Utf32Le.bytes().to_vec();
        bytes.extend_from_slice(&encode_utf32(original, true));

        let file = DecodedFile::decode(&bytes, TextEncoding::UTF8, &defaults());
        assert_eq!(file.text, original);
        assert_eq!(file.encoding, TextEncoding::UTF32);
    }

    /// The two forms Foundation refuses or guesses, measured rather than assumed:
    /// a mark-less UTF-16 body is read **big**-endian, and a mark-less UTF-32 one
    /// does not decode at all.
    #[test]
    fn a_body_without_a_mark_follows_foundation() {
        let be = encode_utf16("a", false);
        assert_eq!(TextEncoding::UTF16.decode(&be), ("a".to_string(), false));
        let le = encode_utf16("a", true);
        assert_eq!(
            TextEncoding::UTF16.decode(&le).0,
            "\u{6100}",
            "little-endian is not assumed"
        );

        let (text, lossy) = TextEncoding::UTF32.decode(&encode_utf32("a", true));
        assert!(text.is_empty());
        assert!(
            lossy,
            "Foundation returns nil here, which is a failed decode"
        );
    }

    /// The endian-specific forms are written **without** a mark: Foundation's
    /// `data(using: .utf16LittleEndian)` produces none, and upstream writes what
    /// Foundation produces.
    #[test]
    fn an_endian_specific_form_is_written_bare() {
        for encoding in [
            TextEncoding::UTF16_LE,
            TextEncoding::UTF16_BE,
            TextEncoding::UTF32_LE,
            TextEncoding::UTF32_BE,
        ] {
            assert_eq!(
                encoding.bom(),
                Bom::None,
                "{encoding} should not imply a mark"
            );
        }
        let file = DecodedFile {
            text: "a".to_string(),
            encoding: TextEncoding::UTF16_BE,
            bom: Bom::None,
            detection: Detection::Default,
        };
        assert_eq!(file.encode(), encode_utf16("a", false));
    }

    /// A Shift JIS file comes back as Shift JIS because the list is worked
    /// through in order and Shift JIS is the first candidate that takes the
    /// bytes — not because anything guessed. UTF-8 is tried first and refuses
    /// them, which is what puts the Japanese group next.
    #[test]
    fn shift_jis_is_found_by_working_through_the_list() {
        let text = "これは日本語のテキストです。文字コードの判定を確認します。";
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(text);
        let file = DecodedFile::decode(&bytes, TextEncoding::UTF8, &defaults());
        assert_eq!(file.text, text);
        assert_eq!(
            file.encoding,
            TextEncoding::from_identifier("shift_jis").unwrap()
        );
        assert_eq!(file.detection, Detection::Candidate);
    }

    /// Names have to be unique across both tiers, not just within the menu: a
    /// name is what the user picks an encoding by, and two entries answering to
    /// one name are two entries the user cannot tell apart.
    #[test]
    fn encoding_registry_names_are_unique() {
        let mut names: Vec<_> = AVAILABLE.iter().map(|e| e.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate encoding display names");
    }

    /// The menu item's payload is the identifier, so two encodings sharing one
    /// would be two items running one command — and the check beside the
    /// encoding in force would land on whichever came first. Several of these
    /// encodings do share a codec; none may share an identifier.
    #[test]
    fn every_encoding_has_its_own_identifier() {
        let mut ids: Vec<_> = AVAILABLE.iter().map(|e| e.identifier()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate encoding identifiers");
    }

    #[test]
    fn every_encoding_round_trips_through_its_identifier() {
        for encoding in AVAILABLE.iter() {
            assert_eq!(
                TextEncoding::from_identifier(encoding.identifier()),
                Some(*encoding),
                "{} did not come back",
                encoding.name
            );
        }
    }

    /// A settings file written before the identifiers were made unique spells
    /// the encoding the way `encoding_rs` names it. Those spellings still have
    /// to resolve, and to the same codec as before — not to the same *entry*,
    /// because the entries that used to collide cannot all keep the spelling.
    #[test]
    fn the_old_whatwg_spellings_still_resolve() {
        for (spelling, name) in [
            ("shift_jis", "Japanese (Shift JIS)"),
            ("euc-jp", "Japanese (EUC)"),
            ("iso-2022-jp", "Japanese (ISO 2022-JP)"),
            ("gb18030", "Chinese (GB 18030)"),
            ("gbk", "Chinese (GBK)"),
            ("big5", "Traditional Chinese (Big 5)"),
            ("euc-kr", "Korean (EUC)"),
            ("macintosh", "Western (Mac OS Roman)"),
            ("windows-1252", "Western (Windows Latin 1)"),
            ("windows-1250", "Central European (Windows Latin 2)"),
            ("windows-1254", "Turkish (Windows Latin 5)"),
            ("windows-874", "Thai (Windows, DOS)"),
            ("koi8-u", "Ukrainian (KOI8-U)"),
            ("ibm866", "Russian (DOS)"),
            ("x-mac-cyrillic", "Cyrillic (Mac OS Ukrainian)"),
        ] {
            let found = TextEncoding::from_identifier(spelling)
                .unwrap_or_else(|| panic!("{spelling} did not resolve"));
            assert_eq!(
                found.name, name,
                "{spelling} resolved to the wrong encoding"
            );
        }
    }

    /// The identifiers this alignment dropped still resolve, and to the codec
    /// they used to name. Three of them name the same standard as the entry that
    /// survived the merge and one is a second spelling of windows-1250, so all
    /// four land on the same codec as before; only the *entry* changes.
    #[test]
    fn the_identifiers_this_alignment_dropped_still_resolve() {
        for (dropped, codec, name) in [
            (
                "windows-latin-2",
                encoding_rs::WINDOWS_1250,
                "Central European (Windows Latin 2)",
            ),
            (
                "iso-8859-2",
                encoding_rs::ISO_8859_2,
                "Central European (ISO Latin 2)",
            ),
            (
                "iso-8859-4",
                encoding_rs::ISO_8859_4,
                "Central European (ISO Latin 4)",
            ),
            (
                "iso-8859-9",
                encoding_rs::WINDOWS_1254,
                "Turkish (ISO Latin 5)",
            ),
        ] {
            let found = TextEncoding::from_identifier(dropped)
                .unwrap_or_else(|| panic!("{dropped} did not resolve"));
            assert_eq!(found.name, name, "{dropped} resolved to the wrong entry");
            assert!(
                matches!(found.kind, Kind::Rs(x) if x == codec),
                "{dropped} no longer decodes with the codec it named"
            );
        }
    }

    /// `MENU_GROUPS` and the flat list are two views of one table, so neither
    /// can gain an encoding the other does not have.
    #[test]
    fn the_menu_groups_are_the_registry_in_order() {
        let flattened: Vec<TextEncoding> = MENU_GROUPS
            .iter()
            .flat_map(|group| group.iter().copied())
            .collect();
        assert_eq!(flattened, MENU.to_vec());
    }

    /// Upstream's default encoding list, entry for entry, in its order.
    ///
    /// These are the strings `String.localizedName(of:)` answers for the
    /// `CFStringEncoding` values `DefaultSettings+Encodings.swift` holds, read
    /// off macOS — not transcribed from that file's comments, which are right
    /// about all but one of them. The comment on `GB_18030_2000` says
    /// "Chinese (GB18030)" and Foundation says "Chinese (GB 18030)"; the code
    /// follows Foundation.
    const UPSTREAM_DEFAULT_LIST: &[&str] = &[
        "Unicode (UTF-8)",
        "Japanese (Shift JIS)",
        "Japanese (EUC)",
        "Japanese (Windows, DOS)",
        "Japanese (Shift JIS X0213)",
        "Japanese (Mac OS)",
        "Japanese (ISO 2022-JP)",
        "Western (Mac OS Roman)",
        "Western (Windows Latin 1)",
        "Chinese (GB 18030)",
        "Traditional Chinese (Big 5 HKSCS)",
        "Traditional Chinese (Big 5-E)",
        "Traditional Chinese (Big 5)",
        "Traditional Chinese (Mac OS)",
        "Simplified Chinese (Mac OS)",
        "Traditional Chinese (EUC)",
        "Simplified Chinese (GB 2312)",
        "Traditional Chinese (Windows, DOS)",
        "Simplified Chinese (Windows, DOS)",
        "Korean (Mac OS)",
        "Korean (EUC)",
        "Korean (Windows, DOS)",
        "Thai (Windows, DOS)",
        "Thai (ISO 8859-11)",
        "Arabic (Mac OS)",
        "Arabic (ISO 8859-6)",
        "Arabic (Windows)",
        "Greek (Mac OS)",
        "Greek (ISO 8859-7)",
        "Greek (Windows)",
        "Hebrew (Mac OS)",
        "Hebrew (ISO 8859-8)",
        "Hebrew (Windows)",
        "Cyrillic (Mac OS)",
        "Cyrillic (ISO 8859-5)",
        "Cyrillic (Windows)",
        "Russian (DOS)",
        "Central European (Mac OS)",
        "Turkish (Mac OS)",
        "Icelandic (Mac OS)",
        "Western (ISO Latin 1)",
        "Central European (ISO Latin 2)",
        "Western (ISO Latin 3)",
        "Central European (ISO Latin 4)",
        "Turkish (ISO Latin 5)",
        "Nordic (ISO Latin 6)",
        "Baltic (ISO Latin 7)",
        "Celtic (ISO Latin 8)",
        "Western (ISO Latin 9)",
        "Romanian (ISO Latin 10)",
        "Latin-US (DOS)",
        "Central European (Windows Latin 2)",
        "Western (NextStep)",
        "Western (ASCII)",
        "Unicode (UTF-16)",
        "Unicode (UTF-16BE)",
        "Unicode (UTF-16LE)",
        "Unicode (UTF-32)",
        "Unicode (UTF-32BE)",
        "Unicode (UTF-32LE)",
    ];

    /// The entries of that list the menu can hold, because `encoding_rs` has a
    /// codec for the standard each one names. In upstream's order, which is the
    /// order the menu is in — every group keeps upstream's sequence, with the
    /// entries that have no codec dropped out of it.
    const UPSTREAM_LISTED: &[&str] = &[
        "Unicode (UTF-8)",
        "Japanese (Shift JIS)",
        "Japanese (EUC)",
        "Japanese (Shift JIS X0213)",
        "Japanese (ISO 2022-JP)",
        "Western (Mac OS Roman)",
        "Western (Windows Latin 1)",
        "Chinese (GB 18030)",
        "Traditional Chinese (Big 5 HKSCS)",
        "Traditional Chinese (Big 5)",
        "Simplified Chinese (GB 2312)",
        "Korean (EUC)",
        "Thai (Windows, DOS)",
        "Thai (ISO 8859-11)",
        "Arabic (ISO 8859-6)",
        "Arabic (Windows)",
        "Greek (ISO 8859-7)",
        "Greek (Windows)",
        "Hebrew (ISO 8859-8)",
        "Hebrew (Windows)",
        "Cyrillic (ISO 8859-5)",
        "Cyrillic (Windows)",
        "Russian (DOS)",
        "Western (ISO Latin 1)",
        "Central European (ISO Latin 2)",
        "Western (ISO Latin 3)",
        "Central European (ISO Latin 4)",
        "Turkish (ISO Latin 5)",
        "Nordic (ISO Latin 6)",
        "Baltic (ISO Latin 7)",
        "Celtic (ISO Latin 8)",
        "Western (ISO Latin 9)",
        "Romanian (ISO Latin 10)",
        "Central European (Windows Latin 2)",
        "Unicode (UTF-16)",
        "Unicode (UTF-16BE)",
        "Unicode (UTF-16LE)",
        "Unicode (UTF-32)",
        "Unicode (UTF-32BE)",
        "Unicode (UTF-32LE)",
    ];

    /// The reconciliation, machine-checked. Two lists that have to keep adding
    /// up to upstream's sixty are what stops a rename or an omission from
    /// drifting in unnoticed: adding an encoding to the menu without moving it
    /// out of the omitted set, or the reverse, fails here.
    #[test]
    fn the_menu_is_upstream_s_default_list_less_the_entries_no_codec_covers() {
        assert_eq!(
            UPSTREAM_DEFAULT_LIST.len(),
            60,
            "upstream's default list is sixty entries"
        );
        assert_eq!(UPSTREAM_LISTED.len(), 40, "forty of them have a codec here");

        for name in UPSTREAM_LISTED {
            assert!(
                UPSTREAM_DEFAULT_LIST.contains(name),
                "{name} is not in upstream's default list"
            );
        }

        let listed: Vec<&str> = MENU.iter().map(|e| e.name).collect();
        assert_eq!(
            listed, UPSTREAM_LISTED,
            "the menu is not upstream's list in order"
        );

        let omitted: Vec<&str> = UPSTREAM_DEFAULT_LIST
            .iter()
            .copied()
            .filter(|name| !UPSTREAM_LISTED.contains(name))
            .collect();
        assert_eq!(omitted.len(), 20, "sixty less forty");
        for name in omitted {
            assert!(
                !AVAILABLE.iter().any(|e| e.name == name),
                "{name} has no codec, so it must not be in the registry under that name"
            );
        }
    }

    /// The encodings SubText knows but does not list. Each has to resolve — a
    /// settings file written by an earlier build spells six of them — and none
    /// may be in the menu, because upstream's default list does not hold them.
    #[test]
    fn the_unlisted_tier_holds_the_encodings_upstream_can_add_but_does_not_list() {
        let ids: Vec<&str> = UNLISTED.iter().map(|e| e.identifier()).collect();
        assert_eq!(
            ids,
            [
                "koi8-r",
                "koi8-u",
                "windows-1254",
                "windows-1257",
                "windows-1258",
                "gbk",
                "x-mac-cyrillic",
            ]
        );

        for encoding in UNLISTED {
            assert!(
                !MENU.contains(encoding),
                "{} is in the menu as well as the unlisted tier",
                encoding.name
            );
            assert_eq!(
                TextEncoding::from_identifier(encoding.identifier()),
                Some(*encoding),
                "{} did not come back",
                encoding.name
            );
            assert!(
                !UPSTREAM_DEFAULT_LIST.contains(&encoding.name),
                "{} is in upstream's default list, so it belongs in the menu",
                encoding.name
            );
        }
    }

    /// The two tiers are one list and a tail: the menu first, in menu order,
    /// then what upstream can add. Nothing is in both, and nothing is in
    /// neither.
    #[test]
    fn the_available_tier_is_the_menu_plus_the_unlisted_one() {
        assert_eq!(MENU.len(), 40);
        assert_eq!(UNLISTED.len(), 7);
        assert_eq!(AVAILABLE.len(), MENU.len() + UNLISTED.len());
        assert_eq!(&AVAILABLE[..MENU.len()], MENU.as_slice());
        assert_eq!(&AVAILABLE[MENU.len()..], UNLISTED);
    }

    /// The encodings the rest of the crate names directly have to be the ones in
    /// the table — a const that fell out of it would be an encoding the menu
    /// could not offer and the detector could not find.
    #[test]
    fn the_named_encodings_are_in_the_registry() {
        for encoding in [
            TextEncoding::UTF8,
            TextEncoding::UTF16,
            TextEncoding::UTF16_LE,
            TextEncoding::UTF16_BE,
            TextEncoding::UTF32,
            TextEncoding::UTF32_LE,
            TextEncoding::UTF32_BE,
        ] {
            assert!(
                MENU.contains(&encoding),
                "{} is not in the menu",
                encoding.name
            );
        }
    }

    /// The registry's first group is UTF-8 alone, and the menu's version of it
    /// is the pair — that is the whole reason [`FileEncoding`] exists.
    #[test]
    fn the_menu_splices_utf8_with_bom_in_after_utf8() {
        let groups = FileEncoding::menu_groups(&EncodingList::defaults());
        assert_eq!(groups.len(), MENU_GROUPS.len());
        assert_eq!(
            groups[0],
            vec![
                FileEncoding::of(TextEncoding::UTF8),
                FileEncoding::utf8_with_bom()
            ]
        );
        assert_eq!(
            groups[0][1].name(),
            "Unicode (UTF-8) with BOM",
            "upstream builds this name as the UTF-8 one plus a suffix"
        );

        // Every other group is the registry's, one for one.
        for (group, registry) in groups.iter().skip(1).zip(MENU_GROUPS.iter().skip(1)) {
            assert_eq!(
                group,
                &registry
                    .iter()
                    .copied()
                    .map(FileEncoding::of)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_file_encoding_round_trips_through_its_identifier() {
        for group in FileEncoding::menu_groups(&EncodingList::defaults()) {
            for file_encoding in group {
                assert_eq!(
                    FileEncoding::from_identifier(&file_encoding.identifier()),
                    Some(file_encoding),
                    "{} did not come back",
                    file_encoding.name()
                );
            }
        }
    }

    /// Only UTF-8 can be written with a mark. A `-bom` suffix on anything else
    /// names nothing rather than naming a file encoding that cannot exist.
    #[test]
    fn the_bom_item_is_only_utf8() {
        assert_eq!(
            FileEncoding::from_identifier("utf-8-bom"),
            Some(FileEncoding::utf8_with_bom())
        );
        assert_eq!(FileEncoding::from_identifier("shift_jis-bom"), None);
        assert_eq!(FileEncoding::from_identifier("utf-16le-bom"), None);
        assert_eq!(FileEncoding::from_identifier("no-such-encoding"), None);
    }

    /// A document's file encoding is the pair, so a UTF-8 document with a mark
    /// is not the same setting as one without — upstream checks one menu item
    /// for each.
    #[test]
    fn a_document_s_file_encoding_carries_the_mark() {
        assert_eq!(
            FileEncoding::of_document(TextEncoding::UTF8, Bom::Utf8),
            FileEncoding::utf8_with_bom()
        );
        assert_eq!(
            FileEncoding::of_document(TextEncoding::UTF8, Bom::None),
            FileEncoding::of(TextEncoding::UTF8)
        );
        // No other encoding has a mark of its own to be mistaken for one.
        assert_eq!(
            FileEncoding::of_document(TextEncoding::UTF16_LE, Bom::Utf16Le),
            FileEncoding::of(TextEncoding::UTF16_LE)
        );
    }

    /// Only the forms whose byte order is unspecified leave a mark — those are
    /// the ones whose bytes would be ambiguous without it. Upstream's
    /// `FileEncoding.hasBOM` says the same: true for `.utf16`/`.utf32`, false for
    /// the four endian-specific ones.
    #[test]
    fn the_unspecified_order_forms_are_written_with_a_mark() {
        let native = if cfg!(target_endian = "little") {
            Bom::Utf16Le
        } else {
            Bom::Utf16Be
        };
        assert_eq!(FileEncoding::of(TextEncoding::UTF16).bom(), native);
        let native = if cfg!(target_endian = "little") {
            Bom::Utf32Le
        } else {
            Bom::Utf32Be
        };
        assert_eq!(FileEncoding::of(TextEncoding::UTF32).bom(), native);

        assert_eq!(FileEncoding::of(TextEncoding::UTF16_LE).bom(), Bom::None);
        assert_eq!(FileEncoding::of(TextEncoding::UTF16_BE).bom(), Bom::None);
        assert_eq!(FileEncoding::of(TextEncoding::UTF32_LE).bom(), Bom::None);
        assert_eq!(FileEncoding::of(TextEncoding::UTF32_BE).bom(), Bom::None);
        assert_eq!(FileEncoding::of(TextEncoding::UTF8).bom(), Bom::None);
        assert_eq!(FileEncoding::utf8_with_bom().bom(), Bom::Utf8);
    }

    /// The mark a wide encoding implies is the one its own bytes decode with.
    #[test]
    fn a_mark_names_the_encoding_it_was_written_for() {
        for encoding in [
            TextEncoding::UTF8,
            TextEncoding::UTF16_LE,
            TextEncoding::UTF16_BE,
            TextEncoding::UTF32_LE,
            TextEncoding::UTF32_BE,
        ] {
            let mark = FileEncoding::of(encoding).bom();
            if mark == Bom::None {
                continue;
            }
            assert_eq!(
                mark.encoding(),
                Some(encoding),
                "{} lost its mark",
                encoding.name
            );
        }
    }

    // ---- the priority list ------------------------------------------------

    /// A list spelled the way a settings file spells it.
    fn list(names: &[Option<&str>]) -> EncodingList {
        EncodingList::from_identifiers(
            &names
                .iter()
                .map(|name| name.map(str::to_owned))
                .collect::<Vec<_>>(),
        )
    }

    /// The list mapped item by item, which is what upstream's menu builder
    /// does — a separator for every `None`, and the UTF-8-with-BOM item spliced
    /// in where `fileEncodings(from:)` puts it.
    fn flat(list: &EncodingList) -> Vec<Option<FileEncoding>> {
        let mut items = Vec::new();
        for index in 0..list.len() {
            match list.item(index).expect("in range") {
                Some(encoding) if encoding == TextEncoding::UTF8 => {
                    items.push(Some(FileEncoding::of(TextEncoding::UTF8)));
                    items.push(Some(FileEncoding::utf8_with_bom()));
                }
                Some(encoding) => items.push(Some(FileEncoding::of(encoding))),
                None => items.push(None),
            }
        }
        items
    }

    /// The groups the menu builder gets, joined back up the way it joins them.
    fn joined(groups: &[Vec<FileEncoding>]) -> Vec<Option<FileEncoding>> {
        let mut items = Vec::new();
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                items.push(None);
            }
            items.extend(group.iter().copied().map(Some));
        }
        items
    }

    /// The default list is the menu with separators in it, so the two cannot
    /// drift: the detector's candidates and the menu's items are one list.
    #[test]
    fn the_default_list_is_the_menu() {
        let defaults = EncodingList::defaults();
        assert_eq!(defaults.encodings().collect::<Vec<_>>(), MENU.to_vec());
        assert_eq!(defaults.len(), MENU.len() + MENU_GROUPS.len() - 1);
        assert!(defaults.is_default());
        assert_eq!(EncodingList::default(), defaults);
    }

    /// Cutting the list at its separators and re-joining it with separators is
    /// the same sequence of menu items as mapping it item by item — including
    /// where the awkward cases land, which is what makes the two
    /// representations interchangeable.
    ///
    /// The picker reads [`FileEncoding::menu_items`] and the menu reads
    /// [`FileEncoding::menu_groups`]; `menu_groups` is *defined* as
    /// `menu_items` cut at its separators, so the assertion that both equal the
    /// independent oracle above is what proves the two surfaces offer the same
    /// encodings in the same order.
    #[test]
    fn the_separators_survive_the_cut_and_join() {
        let utf8 = Some("utf-8");
        let shift = Some("shift_jis");
        let latin = Some("windows-1252");

        for items in [
            vec![utf8, shift, latin],
            vec![utf8, None, shift],
            vec![None, utf8, shift],
            vec![utf8, None],
            vec![utf8, None, None, shift],
            vec![None, utf8, None, None, shift, None],
        ] {
            let list = list(&items);
            assert_eq!(
                FileEncoding::menu_items(&list),
                flat(&list),
                "{items:?} did not survive the splice"
            );
            assert_eq!(
                joined(&FileEncoding::menu_groups(&list)),
                flat(&list),
                "{items:?} did not survive the cut and join"
            );
        }
    }

    #[test]
    fn a_list_round_trips_through_its_identifiers() {
        let defaults = EncodingList::defaults();
        assert_eq!(
            EncodingList::from_identifiers(&defaults.identifiers()),
            defaults
        );

        let edited = list(&[
            Some("windows-1252"),
            None,
            Some("utf-8"),
            None,
            Some("koi8-r"),
        ]);
        assert_eq!(
            EncodingList::from_identifiers(&edited.identifiers()),
            edited
        );
        assert_eq!(
            edited.identifiers(),
            vec![
                Some("windows-1252"),
                None,
                Some("utf-8"),
                None,
                Some("koi8-r")
            ]
        );
    }

    /// A settings file this build cannot read a single entry of leaves a list
    /// with nothing in it but separators, which is as useless as an empty one.
    /// Upstream restores the default for the empty array; its sanitizer turns
    /// every unreadable value into a separator first, so the two arrive
    /// together and one rule covers both.
    #[test]
    fn a_list_that_resolves_to_nothing_falls_back_to_the_defaults() {
        for items in [
            vec![],
            vec![None],
            vec![None, None],
            vec![Some("no-such-encoding")],
            vec![None, Some("no-such-encoding"), None],
        ] {
            assert_eq!(
                list(&items),
                EncodingList::defaults(),
                "{items:?} did not fall back"
            );
        }
    }

    /// An entry that does not resolve is dropped, not turned into a gap: the
    /// menu could not offer it and the detector could not decode with it.
    #[test]
    fn an_encoding_this_build_cannot_read_is_dropped() {
        let list = list(&[
            Some("utf-8"),
            Some("no-such-encoding"),
            None,
            Some("shift_jis"),
        ]);
        assert_eq!(
            list.identifiers(),
            vec![Some("utf-8"), None, Some("shift_jis")]
        );
    }

    /// The two refusals, and the one that is not a refusal.
    #[test]
    fn utf8_and_the_default_encoding_cannot_be_removed() {
        let list = EncodingList::defaults();
        let utf8 = list.item(0).expect("in range").expect("an encoding");
        assert_eq!(utf8, TextEncoding::UTF8);
        assert_eq!(
            list.can_remove(0, TextEncoding::UTF8),
            Some(RemovalError::Utf8)
        );

        let shift_jis = TextEncoding::from_identifier("shift_jis").expect("in the registry");
        let index = (0..list.len())
            .find(|index| list.item(*index) == Some(Some(shift_jis)))
            .expect("shift_jis is in the default list");
        assert_eq!(list.can_remove(index, TextEncoding::UTF8), None);
        assert_eq!(
            list.can_remove(index, shift_jis),
            Some(RemovalError::DefaultEncoding),
            "the default encoding is refused wherever it sits"
        );

        // A separator can go, and so can an index past the end — the second is
        // not removable, but it is not a refusal with a message either.
        assert_eq!(list.can_remove(1, TextEncoding::UTF8), None);
        assert_eq!(list.can_remove(list.len(), TextEncoding::UTF8), None);

        assert_eq!(
            RemovalError::Utf8.message("Unicode (UTF-8)"),
            "Unicode (UTF-8) can’t be removed."
        );
        assert_eq!(
            RemovalError::DefaultEncoding.message("Japanese (Shift JIS)"),
            "The encoding set as default can’t be removed."
        );
    }

    /// The Add button puts the entry below the row the user had selected, and
    /// at the top when they had selected nothing — upstream's
    /// `index?.advanced(by: 1) ?? 0`.
    #[test]
    fn an_entry_is_added_below_the_selection() {
        let mut list = list(&[Some("utf-8"), Some("shift_jis")]);

        let koi8 = Some(TextEncoding::from_identifier("koi8-r").expect("in the registry"));
        assert_eq!(list.insert_after(Some(0), koi8), 1);
        assert_eq!(list.item(1), Some(koi8));
        assert_eq!(list.len(), 3);

        assert_eq!(
            list.insert_after(None, None),
            0,
            "nothing selected means the top"
        );
        assert_eq!(list.item(0), Some(None));

        // A selection past the end is treated as no selection rather than as a
        // panic: the row can go out from under the button between two renders.
        assert_eq!(list.insert_after(Some(99), None), 0);
    }

    #[test]
    fn moving_an_entry_swaps_it_with_its_neighbour() {
        let mut list = list(&[Some("utf-8"), Some("shift_jis"), Some("windows-1252")]);
        let utf8 = list.item(0).expect("in range");
        let shift = list.item(1).expect("in range");
        let latin = list.item(2).expect("in range");

        assert_eq!(list.move_down(0), Some(1));
        assert_eq!(list.item(0), Some(shift));
        assert_eq!(list.item(1), Some(utf8));

        assert_eq!(list.move_up(2), Some(1));
        assert_eq!(list.item(1), Some(latin));
        assert_eq!(list.item(2), Some(utf8));

        // The ends do not wrap, and an index past the end is not a panic.
        assert_eq!(list.move_up(0), None);
        assert_eq!(list.move_down(2), None);
        assert_eq!(list.move_up(9), None);
        assert_eq!(list.move_down(9), None);
    }

    #[test]
    fn removing_an_entry_takes_exactly_that_one() {
        let mut list = list(&[Some("utf-8"), None, Some("shift_jis")]);
        assert!(list.remove(1));
        assert_eq!(list.identifiers(), vec![Some("utf-8"), Some("shift_jis")]);

        assert!(!list.remove(9), "an index past the end removes nothing");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn restoring_the_defaults_puts_the_shipped_list_back() {
        let mut list = list(&[Some("windows-1252"), None, Some("koi8-r")]);
        assert!(!list.is_default());

        list.restore_defaults();
        assert_eq!(list, EncodingList::defaults());
        assert!(list.is_default());
    }

    /// The Add menu offers the whole registry less what the list holds, sorted
    /// by name. On the default list that is exactly the unlisted tier — which
    /// is what that tier is for.
    #[test]
    fn the_add_menu_offers_what_the_list_lacks() {
        let defaults = EncodingList::defaults();
        let addable = defaults.addable();

        let mut expected = UNLISTED.to_vec();
        expected.sort_by_key(|encoding| encoding.name);
        assert_eq!(addable, expected);
        assert!(
            addable.windows(2).all(|pair| pair[0].name <= pair[1].name),
            "the Add menu is sorted by display name"
        );

        // Adding one takes it out of the menu and leaves the rest in place.
        let mut list = list(&[Some("utf-8")]);
        let addable = list.addable();
        assert_eq!(addable.len(), AVAILABLE.len() - 1);
        assert!(!addable.contains(&TextEncoding::UTF8));

        list.insert_after(None, Some(TextEncoding::from_identifier("koi8-r").unwrap()));
        assert!(
            !list
                .addable()
                .contains(&TextEncoding::from_identifier("koi8-r").unwrap())
        );
    }

    // ---- the encoding declaration ----------------------------------------

    #[test]
    fn a_css_declaration_is_read_at_the_very_start() {
        assert_eq!(
            scan_declaration(br#"@charset "shift_jis";"#),
            TextEncoding::from_identifier("shift_jis")
        );
        // The same text one line in is a declaration no browser would honour,
        // and upstream's `prefixMatch` refuses it too.
        assert_eq!(
            scan_declaration(b"/* a comment */\n@charset \"shift_jis\";"),
            None
        );
    }

    #[test]
    fn a_comment_declaration_is_read_in_the_first_two_lines() {
        assert_eq!(
            scan_declaration(b"# -*- coding: utf-8 -*-\nprint('hi')\n"),
            TextEncoding::from_identifier("utf-8")
        );
        assert_eq!(
            scan_declaration(b"#!/usr/bin/env ruby\n# encoding: euc-jp\n"),
            TextEncoding::from_identifier("euc-jp")
        );
        assert_eq!(
            scan_declaration(b"\" vim: fileencoding=windows-1252\n"),
            TextEncoding::from_identifier("windows-1252")
        );
        // The third line is past the window, which is what upstream reads.
        assert_eq!(scan_declaration(b"one\ntwo\n# coding: shift_jis\n"), None);
    }

    #[test]
    fn an_html_declaration_is_read_only_after_ascii() {
        assert_eq!(
            scan_declaration(br#"<meta charset="gb18030">"#),
            TextEncoding::from_identifier("gb18030")
        );
        assert_eq!(
            scan_declaration(br#"<meta http-equiv="Content-Type" charset='big5'>"#),
            TextEncoding::from_identifier("big5")
        );
        // Upstream refuses a `charset=` that anything non-ASCII precedes: a
        // Latin-1 head has already been read as text by then.
        assert_eq!(scan_declaration("<p>caf\u{e9}</p>".as_bytes()), None);
        assert_eq!(
            scan_declaration(b"<p>cafe</p><meta charset=\"gb18030\">"),
            TextEncoding::from_identifier("gb18030")
        );
    }

    /// The earliest declaration in the file wins, not the first style tried.
    #[test]
    fn the_earliest_declaration_wins() {
        // `@charset` is at offset 0; the `charset=` is later in a comment.
        let bytes = br#"@charset "shift_jis";
<!-- charset="gb18030" -->"#;
        assert_eq!(
            scan_declaration(bytes),
            TextEncoding::from_identifier("shift_jis")
        );

        // And the other way round.
        let bytes = br#"<!-- charset="gb18030" -->
# coding: shift_jis"#;
        assert_eq!(
            scan_declaration(bytes),
            TextEncoding::from_identifier("gb18030")
        );
    }

    #[test]
    fn a_declaration_that_names_nothing_is_ignored() {
        assert_eq!(scan_declaration(b"# coding: no-such-encoding\n"), None);
        assert_eq!(scan_declaration(b""), None);
        assert_eq!(scan_declaration(b"just text"), None);
    }

    #[test]
    fn a_declaration_is_only_looked_for_in_the_first_kilobyte() {
        let mut bytes = vec![b'a'; DECLARATION_WINDOW];
        bytes.extend_from_slice(b"\n# coding: shift_jis\n");
        assert_eq!(scan_declaration(&bytes), None);

        // The same declaration inside the window is found.
        let mut bytes = vec![b'a'; DECLARATION_WINDOW - 32];
        bytes.extend_from_slice(b"\n# coding: shift_jis\n");
        assert_eq!(
            scan_declaration(&bytes),
            TextEncoding::from_identifier("shift_jis")
        );
    }

    // ---- the pipeline ----------------------------------------------------

    /// The list decides which encoding a file is read in, and moving an entry
    /// up is how the user says "try this one sooner".
    #[test]
    fn the_list_decides_which_encoding_a_file_is_read_in() {
        let bytes = b"plain ASCII\n";
        let utf8 = TextEncoding::from_identifier("utf-8").unwrap();
        let shift_jis = TextEncoding::from_identifier("shift_jis").unwrap();

        let utf8_first = [utf8, shift_jis];
        let read = DecodedFile::decode(
            bytes,
            utf8,
            &DetectionOptions {
                candidates: &utf8_first,
                respects_declaration: false,
            },
        );
        assert_eq!(read.encoding, utf8);

        let shift_first = [shift_jis, utf8];
        let read = DecodedFile::decode(
            bytes,
            utf8,
            &DetectionOptions {
                candidates: &shift_first,
                respects_declaration: false,
            },
        );
        assert_eq!(
            read.encoding, shift_jis,
            "the list's order was not honoured"
        );
        assert_eq!(read.detection, Detection::Candidate);
    }

    /// A mark whose encoding the user took out of the list is not honoured —
    /// upstream asks the candidates the same question before it trusts a mark.
    #[test]
    fn a_mark_the_list_does_not_hold_is_not_honoured() {
        let mut bytes = Bom::Utf16Le.bytes().to_vec();
        bytes.extend_from_slice(&encode_utf16("hi", true));

        let without = [TextEncoding::UTF8];
        let read = DecodedFile::decode(
            &bytes,
            TextEncoding::UTF8,
            &DetectionOptions {
                candidates: &without,
                respects_declaration: false,
            },
        );
        assert_ne!(read.detection, Detection::Bom);
        assert_ne!(read.encoding, TextEncoding::UTF16);

        // A mark maps to the *unspecified-order* form, as upstream's
        // `Unicode.BOM.encoding` does — so a file that says "UTF-16 LE" comes
        // back as "Unicode (UTF-16)".
        let with = [TextEncoding::UTF8, TextEncoding::UTF16];
        let read = DecodedFile::decode(
            &bytes,
            TextEncoding::UTF8,
            &DetectionOptions {
                candidates: &with,
                respects_declaration: false,
            },
        );
        assert_eq!(read.encoding, TextEncoding::UTF16);
        assert_eq!(read.detection, Detection::Bom);
    }

    /// The declaration is read before the list is worked through, which is what
    /// makes a file that declares an encoding it is not written in come out as
    /// the encoding it declares.
    #[test]
    fn the_declaration_outranks_the_list() {
        let utf8 = TextEncoding::from_identifier("utf-8").unwrap();
        let shift_jis = TextEncoding::from_identifier("shift_jis").unwrap();
        let candidates = [utf8, shift_jis];

        let bytes = b"# coding: shift_jis\nplain ASCII\n";
        let read = DecodedFile::decode(
            bytes,
            utf8,
            &DetectionOptions {
                candidates: &candidates,
                respects_declaration: true,
            },
        );
        assert_eq!(read.encoding, shift_jis);
        assert_eq!(read.detection, Detection::Declared);

        // With the switch off the same bytes are valid UTF-8, so they are.
        let read = DecodedFile::decode(
            bytes,
            utf8,
            &DetectionOptions {
                candidates: &candidates,
                respects_declaration: false,
            },
        );
        assert_eq!(read.encoding, utf8);
        assert_eq!(read.detection, Detection::Candidate);
    }

    /// A declaration naming something the list does not hold is skipped rather
    /// than honoured — upstream asks `candidates.contains` first.
    #[test]
    fn a_declaration_the_list_does_not_hold_is_skipped() {
        let utf8 = TextEncoding::from_identifier("utf-8").unwrap();
        let candidates = [utf8];
        let bytes = b"# coding: shift_jis\nplain ASCII\n";

        let read = DecodedFile::decode(
            bytes,
            utf8,
            &DetectionOptions {
                candidates: &candidates,
                respects_declaration: true,
            },
        );
        assert_eq!(read.encoding, utf8);
        assert_eq!(read.detection, Detection::Candidate);
    }

    /// Nothing in the list takes the bytes, so the user's default encoding is
    /// used — and the caller is told that is what happened.
    #[test]
    fn the_default_encoding_is_the_last_resort() {
        // 0xFF is not valid UTF-8 and not a byte KOI8-R leaves undefined, so
        // the list has to be one that refuses it outright.
        let bytes = [0x80, 0x81, 0x82];
        let candidates = [TextEncoding::from_identifier("utf-8").unwrap()];

        let read = DecodedFile::decode(
            &bytes,
            TextEncoding::UTF8,
            &DetectionOptions {
                candidates: &candidates,
                respects_declaration: false,
            },
        );
        assert_eq!(read.detection, Detection::Default);
        assert_eq!(read.encoding, TextEncoding::UTF8);
        assert!(read.text.contains('\u{fffd}'));
    }
}
