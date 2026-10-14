//! Text encodings, byte-order marks and decoding/encoding of documents.
//!
//! CotEditor's defining trait is that it handles *any* plain-text file
//! correctly rather than assuming UTF-8. This module reproduces that surface:
//! a named encoding registry, BOM detection and preservation, and a detection
//! pipeline that falls back through UTF-8 validity, a statistical charset
//! guess, and finally the user's default encoding.

use std::fmt;
use std::sync::LazyLock;

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};

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
            Bom::Utf16Le => Some(TextEncoding::UTF16_LE),
            Bom::Utf16Be => Some(TextEncoding::UTF16_BE),
            Bom::Utf32Le => Some(TextEncoding::UTF32_LE),
            Bom::Utf32Be => Some(TextEncoding::UTF32_BE),
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
    const MARKS: [Bom; 5] = [Bom::Utf32Le, Bom::Utf32Be, Bom::Utf8, Bom::Utf16Le, Bom::Utf16Be];
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
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

impl TextEncoding {
    const fn rs(name: &'static str, id: &'static str, encoding: &'static Encoding) -> Self {
        Self { name, id, kind: Kind::Rs(encoding) }
    }

    pub const UTF8: Self = Self::rs("Unicode (UTF-8)", "utf-8", UTF_8);
    // The four wide forms are named the way Foundation names them — "Unicode
    // (UTF-16BE)", not "Unicode (UTF-16 Big-Endian)" — because that is the string
    // upstream's menu shows, and the whole registry is spelled that way.
    pub const UTF16_LE: Self =
        Self { name: "Unicode (UTF-16LE)", id: "utf-16le", kind: Kind::Utf16Le };
    pub const UTF16_BE: Self =
        Self { name: "Unicode (UTF-16BE)", id: "utf-16be", kind: Kind::Utf16Be };
    pub const UTF32_LE: Self =
        Self { name: "Unicode (UTF-32LE)", id: "utf-32le", kind: Kind::Utf32Le };
    pub const UTF32_BE: Self =
        Self { name: "Unicode (UTF-32BE)", id: "utf-32be", kind: Kind::Utf32Be };

    /// Whether a byte-order mark is meaningful for this encoding.
    ///
    /// Only the Unicode family has one; a BOM written in front of, say,
    /// Shift JIS would just be mojibake.
    pub fn supports_bom(self) -> bool {
        self.is_unicode()
    }

    pub fn is_unicode(self) -> bool {
        match self.kind {
            Kind::Utf16Le | Kind::Utf16Be | Kind::Utf32Le | Kind::Utf32Be => true,
            Kind::Rs(e) => e == UTF_8 || e == UTF_16LE || e == UTF_16BE,
        }
    }

    /// The byte-order mark a document in this encoding is written with.
    ///
    /// Not the inverse of [`Bom::encoding`]: UTF-8 is written without one by
    /// default, and CotEditor keeps the mark as a property of the *file* rather
    /// than of the encoding — see [`FileEncoding`]. This is only what the
    /// encoding itself implies, and for the four wide Unicode forms that
    /// implication is a mark, because the bytes are ambiguous without one.
    pub fn bom(self) -> Bom {
        match self.kind {
            Kind::Utf16Le => Bom::Utf16Le,
            Kind::Utf16Be => Bom::Utf16Be,
            Kind::Utf32Le => Bom::Utf32Le,
            Kind::Utf32Be => Bom::Utf32Be,
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
        AVAILABLE.iter().copied().find(|e| matches!(e.kind, Kind::Rs(x) if x == encoding))
    }

    /// Decode `bytes` (a BOM, if present, is expected to be already stripped).
    pub fn decode(self, bytes: &[u8]) -> (String, bool) {
        match self.kind {
            Kind::Rs(encoding) => {
                let (text, _, had_errors) = encoding.decode(bytes);
                (text.into_owned(), had_errors)
            }
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
            Kind::Utf16Le => (encode_utf16(text, true), false),
            Kind::Utf16Be => (encode_utf16(text, false), false),
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
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            if little_endian { u16::from_le_bytes([a, b]) } else { u16::from_be_bytes([a, b]) }
        })
        .collect();
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut lossy = !bytes.len().is_multiple_of(2);
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
    let mut out = String::with_capacity(bytes.len() / 4);
    let mut lossy = !bytes.len().is_multiple_of(4);
    for chunk in bytes.chunks_exact(4) {
        let value = if little_endian {
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        } else {
            u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
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
            let bytes = if little_endian { unit.to_le_bytes() } else { unit.to_be_bytes() };
            out.extend_from_slice(&bytes);
        }
    }
    out
}

fn encode_utf32(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 4);
    for c in text.chars() {
        let value = c as u32;
        let bytes = if little_endian { value.to_le_bytes() } else { value.to_be_bytes() };
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
        TextEncoding::rs("Japanese (ISO 2022-JP)", "iso-2022-jp", encoding_rs::ISO_2022_JP),
    ],
    // Western. Upstream's two entries are exactly these.
    &[
        TextEncoding::rs("Western (Mac OS Roman)", "macintosh", encoding_rs::MACINTOSH),
        TextEncoding::rs("Western (Windows Latin 1)", "windows-1252", encoding_rs::WINDOWS_1252),
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
    &[TextEncoding::rs("Korean (EUC)", "euc-kr", encoding_rs::EUC_KR)],
    // Thai, in upstream's order: the DOS one (cp874, which Windows shares) first,
    // then ISO 8859-11.
    &[
        TextEncoding::rs("Thai (Windows, DOS)", "windows-874", encoding_rs::WINDOWS_874),
        TextEncoding::rs("Thai (ISO 8859-11)", "iso-8859-11", encoding_rs::WINDOWS_874),
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
        TextEncoding::rs("Arabic (Windows)", "windows-1256", encoding_rs::WINDOWS_1256),
        TextEncoding::rs("Greek (ISO 8859-7)", "iso-8859-7", encoding_rs::ISO_8859_7),
        TextEncoding::rs("Greek (Windows)", "windows-1253", encoding_rs::WINDOWS_1253),
        TextEncoding::rs("Hebrew (ISO 8859-8)", "iso-8859-8", encoding_rs::ISO_8859_8),
        TextEncoding::rs("Hebrew (Windows)", "windows-1255", encoding_rs::WINDOWS_1255),
        TextEncoding::rs("Cyrillic (ISO 8859-5)", "iso-8859-5", encoding_rs::ISO_8859_5),
        TextEncoding::rs("Cyrillic (Windows)", "windows-1251", encoding_rs::WINDOWS_1251),
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
        TextEncoding::rs("Western (ISO Latin 1)", "iso-8859-1", encoding_rs::WINDOWS_1252),
        TextEncoding::rs(
            "Central European (ISO Latin 2)",
            "iso-latin-2",
            encoding_rs::ISO_8859_2,
        ),
        TextEncoding::rs("Western (ISO Latin 3)", "iso-8859-3", encoding_rs::ISO_8859_3),
        TextEncoding::rs(
            "Central European (ISO Latin 4)",
            "iso-latin-4",
            encoding_rs::ISO_8859_4,
        ),
        TextEncoding::rs("Turkish (ISO Latin 5)", "iso-latin-5", encoding_rs::WINDOWS_1254),
        TextEncoding::rs("Nordic (ISO Latin 6)", "iso-8859-10", encoding_rs::ISO_8859_10),
        TextEncoding::rs("Baltic (ISO Latin 7)", "iso-8859-13", encoding_rs::ISO_8859_13),
        TextEncoding::rs("Celtic (ISO Latin 8)", "iso-8859-14", encoding_rs::ISO_8859_14),
        TextEncoding::rs("Western (ISO Latin 9)", "iso-8859-15", encoding_rs::ISO_8859_15),
        TextEncoding::rs("Romanian (ISO Latin 10)", "iso-8859-16", encoding_rs::ISO_8859_16),
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
    // then UTF-32 and its two. The two unspecified ones are absent — they mean
    // "let the byte-order mark decide", and `FileEncoding` names the order
    // instead; see the gaps table of `docs/PLAN.md`.
    &[
        TextEncoding::UTF16_LE,
        TextEncoding::UTF16_BE,
        TextEncoding::UTF32_LE,
        TextEncoding::UTF32_BE,
    ],
];

/// Encodings SubText can read and write that the menu does not list.
///
/// Upstream's encoding list is a *subset* of `String.availableStringEncodings`:
/// the settings pane can add any encoding the platform has, and the default list
/// is a curated sixty of them. These are the ones SubText can serve that
/// upstream can add but does not list by default — the menu has to agree with
/// upstream's, so they are not in it, but they stay in the registry for two
/// reasons: a settings file written by an earlier build spells six of them, and
/// the list editor the gaps table still owes will have to offer them.
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
    TextEncoding::rs("Turkish (Windows Latin 5)", "windows-1254", encoding_rs::WINDOWS_1254),
    TextEncoding::rs("Baltic (Windows)", "windows-1257", encoding_rs::WINDOWS_1257),
    TextEncoding::rs("Vietnamese (Windows)", "windows-1258", encoding_rs::WINDOWS_1258),
    TextEncoding::rs("Chinese (GBK)", "gbk", encoding_rs::GBK),
    TextEncoding::rs("Cyrillic (Mac OS Ukrainian)", "x-mac-cyrillic", encoding_rs::X_MAC_CYRILLIC),
];

/// Every encoding the menu lists, in menu order.
///
/// [`MENU_GROUPS`] flattened. Derived rather than written out, so an encoding
/// cannot end up in the registry but not in the menu, or the other way round.
///
/// This is upstream's *encoding list*: the set the encoding menu is drawn from,
/// the set the open dialog tries in order, and the set the default-encoding
/// picker offers. [`AVAILABLE`] is the wider one.
pub static MENU: LazyLock<Vec<TextEncoding>> = LazyLock::new(|| {
    MENU_GROUPS.iter().flat_map(|group| group.iter().copied()).collect()
});

/// Every encoding SubText can read and write: the menu's list, then [`UNLISTED`].
///
/// Upstream's counterpart is `String.availableStringEncodings`. Only
/// [`from_identifier`](TextEncoding::from_identifier) searches this one — the
/// menu, the detector and the settings pickers all work from [`MENU`], which is
/// the narrower set upstream builds them from too.
pub static AVAILABLE: LazyLock<Vec<TextEncoding>> = LazyLock::new(|| {
    MENU.iter().copied().chain(UNLISTED.iter().copied()).collect()
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
        Self { encoding, with_utf8_bom: false }
    }

    /// UTF-8 written with a mark. Upstream's one such encoding.
    pub const fn utf8_with_bom() -> Self {
        Self { encoding: TextEncoding::UTF8, with_utf8_bom: true }
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

    /// The registry as the `Format ▸ Text Encoding` menu lists it: the same
    /// groups, with the UTF-8-with-BOM item spliced in just after UTF-8.
    ///
    /// Upstream does this inside `EncodingManager.fileEncodings(from:)`, once,
    /// so that a user's reordered list still gets the pair together — the two
    /// are always handled as a set, and only the UTF-8 entry can produce the
    /// second one.
    pub fn menu_groups() -> Vec<Vec<Self>> {
        MENU_GROUPS
            .iter()
            .map(|group| {
                group
                    .iter()
                    .copied()
                    .flat_map(|encoding| {
                        let plain = Self::of(encoding);
                        if encoding == TextEncoding::UTF8 {
                            vec![plain, Self::utf8_with_bom()]
                        } else {
                            vec![plain]
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

/// How an encoding was chosen for a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    /// A byte-order mark decided it.
    Bom,
    /// The bytes are valid UTF-8, so they are UTF-8.
    Utf8Valid,
    /// A statistical guess from the charset detector.
    Guessed,
    /// Nothing matched; the caller's default was used.
    Default,
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
    /// `fallback` is used when no BOM is present and the detector is not
    /// confident — CotEditor's "default encoding" setting.
    pub fn decode(bytes: &[u8], fallback: TextEncoding) -> Self {
        let bom = detect_bom(bytes);
        if let Some(encoding) = bom.encoding() {
            let body = &bytes[bom.len()..];
            let (text, _) = encoding.decode(body);
            return Self { text, encoding, bom, detection: Detection::Bom };
        }

        if let Ok(text) = std::str::from_utf8(bytes) {
            return Self {
                text: text.to_owned(),
                encoding: TextEncoding::UTF8,
                bom: Bom::None,
                detection: Detection::Utf8Valid,
            };
        }

        // `chardetng` wants the whole document in one call for a stable answer.
        // The guess is then matched against the menu's list, which is the set
        // upstream tries too — a codec the menu does not offer is a codec the
        // document cannot be *kept* in, so guessing it would be a dead end.
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(bytes, true);
        let guessed = detector.guess(None, true);
        if let Some(encoding) =
            MENU.iter().copied().find(|e| matches!(e.kind, Kind::Rs(x) if x == guessed))
        {
            let (text, had_errors) = encoding.decode(bytes);
            if !had_errors {
                return Self {
                    text,
                    encoding,
                    bom: Bom::None,
                    detection: Detection::Guessed,
                };
            }
        }

        let (text, _) = fallback.decode(bytes);
        Self { text, encoding: fallback, bom: Bom::None, detection: Detection::Default }
    }

    /// Serialize back to bytes, re-attaching the BOM when the document has one.
    pub fn encode(&self) -> Vec<u8> {
        let (body, _) = self.encoding.encode(&self.text);
        let mark = self.bom.bytes();
        if mark.is_empty() {
            return body;
        }
        let mut out = Vec::with_capacity(mark.len() + body.len());
        out.extend_from_slice(mark);
        out.extend_from_slice(&body);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let again = DecodedFile::decode(&bytes, TextEncoding::UTF8);
        assert_eq!(again.text, "こんにちは");
        assert_eq!(again.bom, Bom::Utf8);
    }

    #[test]
    fn utf16_le_round_trips() {
        let original = "日本語テキスト";
        // Without a BOM, UTF-16 is not reliably distinguishable from other
        // encodings; the BOM is what makes it unambiguous, so test that path.
        let mut bytes = Bom::Utf16Le.bytes().to_vec();
        bytes.extend_from_slice(&encode_utf16(original, true));

        let file = DecodedFile::decode(&bytes, TextEncoding::UTF8);
        assert_eq!(file.text, original);
        assert_eq!(file.encoding, TextEncoding::UTF16_LE);
        assert_eq!(file.detection, Detection::Bom);
        assert_eq!(file.encode(), bytes);
    }

    #[test]
    fn utf32_le_round_trips() {
        let original = "abc😀";
        let mut bytes = Bom::Utf32Le.bytes().to_vec();
        bytes.extend_from_slice(&encode_utf32(original, true));

        let file = DecodedFile::decode(&bytes, TextEncoding::UTF8);
        assert_eq!(file.text, original);
        assert_eq!(file.encoding, TextEncoding::UTF32_LE);
    }

    #[test]
    fn shift_jis_is_guessed() {
        let text = "これは日本語のテキストです。文字コードの判定を確認します。";
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(text);
        let file = DecodedFile::decode(&bytes, TextEncoding::UTF8);
        assert_eq!(file.text, text);
        assert_eq!(file.encoding, TextEncoding::from_identifier("shift_jis").unwrap());
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
            assert_eq!(found.name, name, "{spelling} resolved to the wrong encoding");
        }
    }

    /// The identifiers this alignment dropped still resolve, and to the codec
    /// they used to name. Three of them name the same standard as the entry that
    /// survived the merge and one is a second spelling of windows-1250, so all
    /// four land on the same codec as before; only the *entry* changes.
    #[test]
    fn the_identifiers_this_alignment_dropped_still_resolve() {
        for (dropped, codec, name) in [
            ("windows-latin-2", encoding_rs::WINDOWS_1250, "Central European (Windows Latin 2)"),
            ("iso-8859-2", encoding_rs::ISO_8859_2, "Central European (ISO Latin 2)"),
            ("iso-8859-4", encoding_rs::ISO_8859_4, "Central European (ISO Latin 4)"),
            ("iso-8859-9", encoding_rs::WINDOWS_1254, "Turkish (ISO Latin 5)"),
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
        let flattened: Vec<TextEncoding> =
            MENU_GROUPS.iter().flat_map(|group| group.iter().copied()).collect();
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
        "Unicode (UTF-16LE)",
        "Unicode (UTF-16BE)",
        "Unicode (UTF-32LE)",
        "Unicode (UTF-32BE)",
    ];

    /// The reconciliation, machine-checked. Two lists that have to keep adding
    /// up to upstream's sixty are what stops a rename or an omission from
    /// drifting in unnoticed: adding an encoding to the menu without moving it
    /// out of the omitted set, or the reverse, fails here.
    #[test]
    fn the_menu_is_upstream_s_default_list_less_the_entries_no_codec_covers() {
        assert_eq!(UPSTREAM_DEFAULT_LIST.len(), 60, "upstream's default list is sixty entries");
        assert_eq!(UPSTREAM_LISTED.len(), 38, "thirty-eight of them have a codec here");

        for name in UPSTREAM_LISTED {
            assert!(
                UPSTREAM_DEFAULT_LIST.contains(name),
                "{name} is not in upstream's default list"
            );
        }

        let listed: Vec<&str> = MENU.iter().map(|e| e.name).collect();
        assert_eq!(listed, UPSTREAM_LISTED, "the menu is not upstream's list in order");

        let omitted: Vec<&str> = UPSTREAM_DEFAULT_LIST
            .iter()
            .copied()
            .filter(|name| !UPSTREAM_LISTED.contains(name))
            .collect();
        assert_eq!(omitted.len(), 22, "sixty less thirty-eight");
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
        assert_eq!(MENU.len(), 38);
        assert_eq!(UNLISTED.len(), 7);
        assert_eq!(AVAILABLE.len(), MENU.len() + UNLISTED.len());
        assert_eq!(&AVAILABLE[..MENU.len()], MENU.as_slice());
        assert_eq!(&AVAILABLE[MENU.len()..], UNLISTED);
    }

    /// The five encodings the rest of the crate names directly have to be the
    /// ones in the table — a const that fell out of it would be an encoding the
    /// menu could not offer and the detector could not find.
    #[test]
    fn the_named_encodings_are_in_the_registry() {
        for encoding in [
            TextEncoding::UTF8,
            TextEncoding::UTF16_LE,
            TextEncoding::UTF16_BE,
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
        let groups = FileEncoding::menu_groups();
        assert_eq!(groups.len(), MENU_GROUPS.len());
        assert_eq!(
            groups[0],
            vec![FileEncoding::of(TextEncoding::UTF8), FileEncoding::utf8_with_bom()]
        );
        assert_eq!(
            groups[0][1].name(),
            "Unicode (UTF-8) with BOM",
            "upstream builds this name as the UTF-8 one plus a suffix"
        );

        // Every other group is the registry's, one for one.
        for (group, registry) in groups.iter().skip(1).zip(MENU_GROUPS.iter().skip(1)) {
            assert_eq!(group, &registry.iter().copied().map(FileEncoding::of).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_file_encoding_round_trips_through_its_identifier() {
        for group in FileEncoding::menu_groups() {
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

    /// Picking a wide Unicode encoding has to leave a mark, or the file it
    /// writes is ambiguous.
    #[test]
    fn the_wide_unicode_encodings_are_written_with_a_mark() {
        assert_eq!(FileEncoding::of(TextEncoding::UTF16_LE).bom(), Bom::Utf16Le);
        assert_eq!(FileEncoding::of(TextEncoding::UTF16_BE).bom(), Bom::Utf16Be);
        assert_eq!(FileEncoding::of(TextEncoding::UTF32_LE).bom(), Bom::Utf32Le);
        assert_eq!(FileEncoding::of(TextEncoding::UTF32_BE).bom(), Bom::Utf32Be);
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
            assert_eq!(mark.encoding(), Some(encoding), "{} lost its mark", encoding.name);
        }
    }
}
