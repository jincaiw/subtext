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
/// codec set. `encoding_rs::BIG5` is what both "Chinese (Big5)" and
/// "Chinese (Big5-HKSCS)" decode with; `encoding_rs::GBK` is what all three
/// Chinese Simplified entries use. Deriving the identifier from the codec
/// would give those pairs the same one, and an encoding the user cannot name
/// is an encoding they cannot choose — the menu item would run whichever of
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
    pub const UTF16_LE: Self =
        Self { name: "Unicode (UTF-16 Little-Endian)", id: "utf-16le", kind: Kind::Utf16Le };
    pub const UTF16_BE: Self =
        Self { name: "Unicode (UTF-16 Big-Endian)", id: "utf-16be", kind: Kind::Utf16Be };
    pub const UTF32_LE: Self =
        Self { name: "Unicode (UTF-32 Little-Endian)", id: "utf-32le", kind: Kind::Utf32Le };
    pub const UTF32_BE: Self =
        Self { name: "Unicode (UTF-32 Big-Endian)", id: "utf-32be", kind: Kind::Utf32Be };

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
    /// Also accepts the WHATWG label for an encoding — `"shift_jis"`,
    /// `"euc-jp"` — which is what a settings file written before the
    /// identifiers were made unique holds. Those are not quite the same set:
    /// the label for GB 2312 is `"gbk"`, and both spellings land on
    /// `encoding_rs`'s one GBK codec either way.
    pub fn from_identifier(name: &str) -> Option<Self> {
        let lowered = name.trim().to_ascii_lowercase();
        match lowered.as_str() {
            "utf-16le" | "utf-16-le" => return Some(Self::UTF16_LE),
            "utf-16be" | "utf-16-be" => return Some(Self::UTF16_BE),
            "utf-32le" | "utf-32-le" => return Some(Self::UTF32_LE),
            "utf-32be" | "utf-32-be" => return Some(Self::UTF32_BE),
            _ => {}
        }
        if let Some(found) = ALL.iter().copied().find(|e| e.id == lowered) {
            return Some(found);
        }
        let encoding = Encoding::for_label(lowered.as_bytes())?;
        ALL.iter().copied().find(|e| matches!(e.kind, Kind::Rs(x) if x == encoding))
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

/// The encoding registry, cut into the groups CotEditor's default encoding list
/// cuts it into.
///
/// The groups are the table. [`ALL`] is derived from them, so the flat list and
/// the menu cannot drift apart: the menu is the registry, separated in the same
/// places upstream separates its own. Nothing here is generated — the registry
/// is code, and only the menu *bar* comes out of `tools/`.
///
/// Each group's comment names what upstream's group holds and, where the two
/// differ, which entries `encoding_rs` cannot honour. It implements the WHATWG
/// Encoding Standard, which is a Windows-centric subset of Apple's codec set,
/// so about a third of upstream's sixty default entries have no counterpart
/// here at all.
pub static MENU_GROUPS: &[&[TextEncoding]] = &[
    // Unicode (UTF-8). Upstream's first group is one entry, and its menu
    // builder splices "Unicode (UTF-8) with BOM" in right after it — see
    // `FileEncoding::menu_groups`.
    &[TextEncoding::UTF8],
    // Japanese. Upstream's group also names a DOS Shift JIS (which is what
    // `encoding_rs`'s one Shift JIS codec already is) and a Mac OS one
    // (`encoding_rs` has no Mac Japanese).
    &[
        TextEncoding::rs("Japanese (Shift JIS)", "shift_jis", encoding_rs::SHIFT_JIS),
        TextEncoding::rs("Japanese (EUC-JP)", "euc-jp", encoding_rs::EUC_JP),
        TextEncoding::rs(
            "Japanese (Shift JIS X0213)",
            "shift_jis_x0213",
            encoding_rs::SHIFT_JIS,
        ),
        TextEncoding::rs("Japanese (ISO-2022-JP)", "iso-2022-jp", encoding_rs::ISO_2022_JP),
    ],
    // Western. Upstream's two entries are exactly these.
    &[
        TextEncoding::rs("Western (Mac OS Roman)", "macintosh", encoding_rs::MACINTOSH),
        TextEncoding::rs("Western (Windows Latin 1)", "windows-1252", encoding_rs::WINDOWS_1252),
    ],
    // Chinese. Upstream's group is ten entries; the six `encoding_rs` cannot
    // honour are the two Mac OS ones, Big 5-E, EUC-TW and the two DOS ones.
    // Big 5 and Big 5-HKSCS are one codec here — the WHATWG Big 5 table already
    // carries the HKSCS extensions — and so are GB 2312 and GBK.
    &[
        TextEncoding::rs("Chinese (GB 18030)", "gb18030", encoding_rs::GB18030),
        TextEncoding::rs("Chinese (Big5)", "big5", encoding_rs::BIG5),
        TextEncoding::rs("Chinese (Big5-HKSCS)", "big5_hkscs", encoding_rs::BIG5),
        TextEncoding::rs("Chinese (GB 2312)", "gb2312", encoding_rs::GBK),
        TextEncoding::rs("Chinese (GBK)", "gbk", encoding_rs::GBK),
    ],
    // Korean. Upstream's group also names a Mac OS one.
    &[TextEncoding::rs("Korean (EUC-KR)", "euc-kr", encoding_rs::EUC_KR)],
    // Thai, in upstream's order: the Windows one first, then ISO 8859-11.
    &[
        TextEncoding::rs("Thai (Windows)", "windows-874", encoding_rs::WINDOWS_874),
        TextEncoding::rs("Thai (ISO 8859-11)", "iso-8859-11", encoding_rs::WINDOWS_874),
    ],
    // Arabic, Greek, Hebrew, Cyrillic, Turkish. Upstream's group of sixteen is
    // mostly Mac OS codecs; these are the ones `encoding_rs` has, plus four
    // Windows codecs upstream lists among its *available* encodings rather than
    // in this group (Vietnamese, Baltic, KOI8-R and KOI8-U).
    &[
        TextEncoding::rs("Arabic (ISO 8859-6)", "iso-8859-6", encoding_rs::ISO_8859_6),
        TextEncoding::rs("Arabic (Windows)", "windows-1256", encoding_rs::WINDOWS_1256),
        TextEncoding::rs("Greek (ISO 8859-7)", "iso-8859-7", encoding_rs::ISO_8859_7),
        TextEncoding::rs("Greek (Windows)", "windows-1253", encoding_rs::WINDOWS_1253),
        TextEncoding::rs("Hebrew (ISO 8859-8)", "iso-8859-8", encoding_rs::ISO_8859_8),
        TextEncoding::rs("Hebrew (Windows)", "windows-1255", encoding_rs::WINDOWS_1255),
        TextEncoding::rs("Cyrillic (ISO 8859-5)", "iso-8859-5", encoding_rs::ISO_8859_5),
        TextEncoding::rs("Cyrillic (Windows)", "windows-1251", encoding_rs::WINDOWS_1251),
        TextEncoding::rs("Cyrillic (KOI8-R)", "koi8-r", encoding_rs::KOI8_R),
        TextEncoding::rs("Cyrillic (KOI8-U)", "koi8-u", encoding_rs::KOI8_U),
        TextEncoding::rs("Turkish (ISO 8859-9)", "iso-8859-9", encoding_rs::WINDOWS_1254),
        TextEncoding::rs("Turkish (Windows)", "windows-1254", encoding_rs::WINDOWS_1254),
        TextEncoding::rs("Vietnamese (Windows)", "windows-1258", encoding_rs::WINDOWS_1258),
        TextEncoding::rs("Baltic (ISO 8859-4)", "iso-8859-4", encoding_rs::ISO_8859_4),
        TextEncoding::rs("Baltic (Windows)", "windows-1257", encoding_rs::WINDOWS_1257),
        TextEncoding::rs("Central European (ISO 8859-2)", "iso-8859-2", encoding_rs::ISO_8859_2),
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
    // the other three.
    &[
        TextEncoding::rs("Western (Windows Latin 2)", "windows-latin-2", encoding_rs::WINDOWS_1250),
        TextEncoding::rs("Central European (Windows)", "windows-1250", encoding_rs::WINDOWS_1250),
    ],
    // Unicode, wide. Upstream's order: UTF-16, then the two byte orders, then
    // UTF-32 and its two.
    &[
        TextEncoding::UTF16_LE,
        TextEncoding::UTF16_BE,
        TextEncoding::UTF32_LE,
        TextEncoding::UTF32_BE,
    ],
];

/// Every encoding SubText can read and write, in menu order.
///
/// [`MENU_GROUPS`] flattened. Derived rather than written out, so an encoding
/// cannot end up in the registry but not in the menu, or the other way round.
pub static ALL: LazyLock<Vec<TextEncoding>> = LazyLock::new(|| {
    MENU_GROUPS.iter().flat_map(|group| group.iter().copied()).collect()
});

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
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(bytes, true);
        let guessed = detector.guess(None, true);
        if let Some(encoding) =
            ALL.iter().copied().find(|e| matches!(e.kind, Kind::Rs(x) if x == guessed))
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

    #[test]
    fn encoding_registry_names_are_unique() {
        let mut names: Vec<_> = ALL.iter().map(|e| e.name).collect();
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
        let mut ids: Vec<_> = ALL.iter().map(|e| e.identifier()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate encoding identifiers");
    }

    #[test]
    fn every_encoding_round_trips_through_its_identifier() {
        for encoding in ALL.iter() {
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
            ("euc-jp", "Japanese (EUC-JP)"),
            ("iso-2022-jp", "Japanese (ISO-2022-JP)"),
            ("gb18030", "Chinese (GB 18030)"),
            ("gbk", "Chinese (GBK)"),
            ("big5", "Chinese (Big5)"),
            ("euc-kr", "Korean (EUC-KR)"),
            ("macintosh", "Western (Mac OS Roman)"),
            ("windows-1252", "Western (Windows Latin 1)"),
            ("windows-1250", "Central European (Windows)"),
            ("windows-1254", "Turkish (Windows)"),
            ("windows-874", "Thai (Windows)"),
            ("koi8-u", "Cyrillic (KOI8-U)"),
        ] {
            let found = TextEncoding::from_identifier(spelling)
                .unwrap_or_else(|| panic!("{spelling} did not resolve"));
            assert_eq!(found.name, name, "{spelling} resolved to the wrong encoding");
        }
    }

    /// `ALL` and the menu's grouping are two views of one table, so neither can
    /// gain an encoding the other does not have.
    #[test]
    fn the_menu_groups_are_the_registry_in_order() {
        let flattened: Vec<TextEncoding> =
            MENU_GROUPS.iter().flat_map(|group| group.iter().copied()).collect();
        assert_eq!(flattened, ALL.to_vec());
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
                ALL.contains(&encoding),
                "{} is not in the registry",
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
