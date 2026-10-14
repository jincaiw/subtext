//! Text encodings, byte-order marks and decoding/encoding of documents.
//!
//! CotEditor's defining trait is that it handles *any* plain-text file
//! correctly rather than assuming UTF-8. This module reproduces that surface:
//! a named encoding registry, BOM detection and preservation, and a detection
//! pipeline that falls back through UTF-8 validity, a statistical charset
//! guess, and finally the user's default encoding.

use std::fmt;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextEncoding {
    /// The name CotEditor shows in the encoding menu.
    pub name: &'static str,
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
    const fn rs(name: &'static str, encoding: &'static Encoding) -> Self {
        Self { name, kind: Kind::Rs(encoding) }
    }

    pub const UTF8: Self = Self::rs("Unicode (UTF-8)", UTF_8);
    pub const UTF16_LE: Self = Self { name: "Unicode (UTF-16 Little-Endian)", kind: Kind::Utf16Le };
    pub const UTF16_BE: Self = Self { name: "Unicode (UTF-16 Big-Endian)", kind: Kind::Utf16Be };
    pub const UTF32_LE: Self = Self { name: "Unicode (UTF-32 Little-Endian)", kind: Kind::Utf32Le };
    pub const UTF32_BE: Self = Self { name: "Unicode (UTF-32 Big-Endian)", kind: Kind::Utf32Be };

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

    /// A stable identifier, used in settings files and the status bar.
    pub fn identifier(self) -> String {
        match self.kind {
            Kind::Rs(e) => e.name().to_ascii_lowercase(),
            Kind::Utf16Le => "utf-16le".into(),
            Kind::Utf16Be => "utf-16be".into(),
            Kind::Utf32Le => "utf-32le".into(),
            Kind::Utf32Be => "utf-32be".into(),
        }
    }

    /// Look an encoding up by its `encoding_rs` / WHATWG name.
    pub fn from_identifier(name: &str) -> Option<Self> {
        let lowered = name.trim().to_ascii_lowercase();
        match lowered.as_str() {
            "utf-16le" | "utf-16-le" => return Some(Self::UTF16_LE),
            "utf-16be" | "utf-16-be" => return Some(Self::UTF16_BE),
            "utf-32le" | "utf-32-le" => return Some(Self::UTF32_LE),
            "utf-32be" | "utf-32-be" => return Some(Self::UTF32_BE),
            _ => {}
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

macro_rules! encodings {
    ($( $name:literal => $enc:expr ),* $(,)?) => {
        /// Every encoding SubText can read and write, in menu order.
        pub static ALL: &[TextEncoding] = &[
            TextEncoding::UTF8,
            TextEncoding::UTF16_LE,
            TextEncoding::UTF16_BE,
            TextEncoding::UTF32_LE,
            TextEncoding::UTF32_BE,
            $( TextEncoding::rs($name, $enc), )*
        ];
    };
}

encodings! {
    // Japanese
    "Japanese (Shift JIS)" => encoding_rs::SHIFT_JIS,
    "Japanese (Shift JIS X0213)" => encoding_rs::SHIFT_JIS,
    "Japanese (EUC-JP)" => encoding_rs::EUC_JP,
    "Japanese (ISO-2022-JP)" => encoding_rs::ISO_2022_JP,
    // Simplified Chinese
    "Chinese (GB 18030)" => encoding_rs::GB18030,
    "Chinese (GBK)" => encoding_rs::GBK,
    "Chinese (GB 2312)" => encoding_rs::GBK,
    // Traditional Chinese / Hong Kong
    "Chinese (Big5)" => encoding_rs::BIG5,
    "Chinese (Big5-HKSCS)" => encoding_rs::BIG5,
    // Korean
    "Korean (EUC-KR)" => encoding_rs::EUC_KR,
    // Unicode variants
    "Unicode (UTF-8 with BOM)" => encoding_rs::UTF_8,
    // Western
    "Western (ISO Latin 1)" => encoding_rs::WINDOWS_1252,
    "Western (ISO Latin 2)" => encoding_rs::ISO_8859_2,
    "Western (ISO Latin 3)" => encoding_rs::ISO_8859_3,
    "Western (ISO Latin 4)" => encoding_rs::ISO_8859_4,
    "Western (ISO Latin 5)" => encoding_rs::ISO_8859_5,
    "Western (ISO Latin 6)" => encoding_rs::ISO_8859_6,
    "Western (ISO Latin 7)" => encoding_rs::ISO_8859_7,
    "Western (ISO Latin 8)" => encoding_rs::ISO_8859_8,
    "Western (ISO Latin 9)" => encoding_rs::ISO_8859_10,
    "Western (ISO Latin 10)" => encoding_rs::ISO_8859_13,
    "Western (Windows Latin 1)" => encoding_rs::WINDOWS_1252,
    "Western (Windows Latin 2)" => encoding_rs::WINDOWS_1250,
    "Western (Mac OS Roman)" => encoding_rs::MACINTOSH,
    // Cyrillic / Greek / Hebrew / Arabic
    "Cyrillic (ISO 8859-5)" => encoding_rs::ISO_8859_5,
    "Cyrillic (Windows)" => encoding_rs::WINDOWS_1251,
    "Cyrillic (KOI8-R)" => encoding_rs::KOI8_R,
    "Cyrillic (KOI8-U)" => encoding_rs::KOI8_U,
    "Greek (ISO 8859-7)" => encoding_rs::ISO_8859_7,
    "Greek (Windows)" => encoding_rs::WINDOWS_1253,
    "Hebrew (ISO 8859-8)" => encoding_rs::ISO_8859_8,
    "Hebrew (Windows)" => encoding_rs::WINDOWS_1255,
    "Arabic (ISO 8859-6)" => encoding_rs::ISO_8859_6,
    "Arabic (Windows)" => encoding_rs::WINDOWS_1256,
    // Baltic / Central European
    "Baltic (ISO 8859-4)" => encoding_rs::ISO_8859_4,
    "Baltic (Windows)" => encoding_rs::WINDOWS_1257,
    "Central European (ISO 8859-2)" => encoding_rs::ISO_8859_2,
    "Central European (Windows)" => encoding_rs::WINDOWS_1250,
    // Thai / Vietnamese / Turkish
    "Thai (ISO 8859-11)" => encoding_rs::WINDOWS_874,
    "Thai (Windows)" => encoding_rs::WINDOWS_874,
    "Vietnamese (Windows)" => encoding_rs::WINDOWS_1258,
    "Turkish (ISO 8859-9)" => encoding_rs::WINDOWS_1254,
    "Turkish (Windows)" => encoding_rs::WINDOWS_1254,
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
}
