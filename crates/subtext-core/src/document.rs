//! The document: a text buffer plus everything needed to write it back
//! byte-for-byte, and the file lifecycle CotEditor exposes in its File menu.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::encoding::{Bom, DecodedFile, Detection, DetectionOptions, FileEncoding, TextEncoding};
use crate::line_ending::{self, LineEnding};

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0} is not valid UTF-8 and no other encoding matched")]
    Undecodable(PathBuf),
    /// The text holds characters the document's encoding cannot spell.
    ///
    /// CotEditor's `LossyEncodingError`, whose own wording this is. It offers
    /// *Change Encoding* and *Cancel*, and SubText does the second of those by
    /// returning this: writing anyway would put a file on disk that no longer
    /// holds what the user typed, and the encoding picker in the status bar is
    /// how the first is reached.
    #[error("The document contains characters incompatible with \"{0}\"")]
    LossyEncoding(&'static str),
}

/// What changed on disk since the document was last read or written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// The file on disk still matches what we hold.
    Unchanged,
    /// The file changed underneath us.
    Modified,
    /// The file is gone.
    Missing,
    /// The document has never been saved.
    Untitled,
}

/// An open text document.
#[derive(Debug, Clone)]
pub struct Document {
    path: Option<PathBuf>,
    text: String,
    encoding: TextEncoding,
    bom: Bom,
    line_ending: LineEnding,
    detection: Detection,
    dirty: bool,
    /// The file's modification time when we last read or wrote it.
    saved_at: Option<SystemTime>,
    /// The file's length when we last read or wrote it, used as a cheap
    /// second signal when the filesystem's timestamps are coarse.
    saved_len: Option<u64>,
    /// Whether a lossy save is permitted without asking again — upstream's
    /// `allowsLossySaving`, set for the rest of the session once the user has
    /// answered *Save Available Text*.
    allows_lossy_saving: bool,
}

impl Document {
    /// A new, empty, unsaved document.
    ///
    /// `default_encoding` is a [`FileEncoding`] rather than a bare codec,
    /// because the byte-order mark is half of what the user chose: CotEditor's
    /// *Default text encoding* picker selects a `FileEncoding`, and picking
    /// *Unicode (UTF-8) with BOM* is a promise about the file this document will
    /// become. The mark comes from the encoding itself —
    /// [`FileEncoding::bom`] gives UTF-16 and UTF-32 their marks too, which is
    /// what upstream's `hasBOM` does.
    pub fn untitled(default_encoding: FileEncoding, default_line_ending: LineEnding) -> Self {
        Self {
            path: None,
            text: String::new(),
            encoding: default_encoding.encoding,
            bom: default_encoding.bom(),
            line_ending: default_line_ending,
            detection: Detection::Default,
            dirty: false,
            saved_at: None,
            saved_len: None,
            allows_lossy_saving: false,
        }
    }

    /// Read a document from disk, detecting encoding and line endings.
    ///
    /// `options` is what the detector is allowed to try, which is the user's
    /// priority list and the *Refer to encoding declaration* switch. It is the
    /// caller's rather than a constant because the list lives in the settings,
    /// and a document opened from Folder Find has no document to take one from
    /// — see [`DetectionOptions::defaults`].
    pub fn open(
        path: impl AsRef<Path>,
        fallback: TextEncoding,
        options: &DetectionOptions<'_>,
    ) -> Result<Self, DocumentError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| DocumentError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let metadata = std::fs::metadata(path).ok();

        let decoded = DecodedFile::decode(&bytes, fallback, options);
        let line_ending = line_ending::detect(&decoded.text);

        Ok(Self {
            path: Some(path.to_path_buf()),
            text: decoded.text,
            encoding: decoded.encoding,
            bom: decoded.bom,
            line_ending,
            detection: decoded.detection,
            dirty: false,
            saved_at: metadata.as_ref().and_then(|m| m.modified().ok()),
            saved_len: metadata.as_ref().map(|m| m.len()),
            allows_lossy_saving: false,
        })
    }

    /// Adopt the text of an already-decoded file (used when reopening in a
    /// different encoding, and by the CLI).
    pub fn from_decoded(path: Option<PathBuf>, decoded: DecodedFile) -> Self {
        let line_ending = line_ending::detect(&decoded.text);
        Self {
            path,
            text: decoded.text,
            encoding: decoded.encoding,
            bom: decoded.bom,
            line_ending,
            detection: decoded.detection,
            dirty: false,
            saved_at: None,
            saved_len: None,
            allows_lossy_saving: false,
        }
    }

    // ---- accessors -------------------------------------------------------

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn display_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || "Untitled".to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn encoding(&self) -> TextEncoding {
        self.encoding
    }

    pub fn bom(&self) -> Bom {
        self.bom
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn detection(&self) -> Detection {
        self.detection
    }

    /// The size the file had when it was last read or written, or `None` for a
    /// document that has never been saved.
    ///
    /// This is what the status bar shows. Upstream reads
    /// `FileManager.attributesOfItem` and caches it the same way — the point of
    /// the cache is that a status bar is redrawn on every keystroke and a
    /// `stat` per frame is a syscall nobody asked for.
    pub fn file_size(&self) -> Option<u64> {
        self.saved_len
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether lossy saves go through without asking — upstream's
    /// `allowsLossySaving`.
    pub fn allows_lossy_saving(&self) -> bool {
        self.allows_lossy_saving
    }

    /// Set [`Self::allows_lossy_saving`], from the *Save Available Text*
    /// answer.
    pub fn set_allows_lossy_saving(&mut self, allowed: bool) {
        self.allows_lossy_saving = allowed;
    }

    pub fn is_untitled(&self) -> bool {
        self.path.is_none()
    }

    pub fn character_count(&self) -> usize {
        self.text.chars().count()
    }

    pub fn line_count(&self) -> usize {
        line_ending::split_lines(&self.text).len()
    }

    // ---- mutation --------------------------------------------------------

    /// Replace the buffer. Marks the document dirty unless `dirty` is false.
    pub fn set_text(&mut self, text: impl Into<String>, dirty: bool) {
        self.text = text.into();
        if dirty {
            self.dirty = true;
        }
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Rewrite every line ending in the buffer.
    pub fn set_line_ending(&mut self, ending: LineEnding) {
        if self.line_ending == ending {
            return;
        }
        self.text = line_ending::convert(&self.text, ending);
        self.line_ending = ending;
        self.dirty = true;
    }

    /// Change the encoding the buffer will be *written* in, without touching
    /// the text — CotEditor's "Convert Encoding".
    pub fn convert_encoding(&mut self, encoding: TextEncoding, bom: Bom) {
        self.encoding = encoding;
        self.bom = if encoding.supports_bom() {
            bom
        } else {
            Bom::None
        };
        self.dirty = true;
    }

    /// Re-read the file's bytes as a different encoding — CotEditor's
    /// "Reinterpret". The on-disk bytes are untouched; only the buffer's
    /// interpretation of them changes.
    pub fn reinterpret(&mut self, encoding: TextEncoding) -> Result<(), DocumentError> {
        let Some(path) = self.path.clone() else {
            // Nothing on disk to reinterpret: re-encode what we already have.
            let (text, _) = encoding.decode(self.text.as_bytes());
            self.text = text;
            self.encoding = encoding;
            self.bom = Bom::None;
            self.dirty = true;
            return Ok(());
        };
        let bytes = std::fs::read(&path).map_err(|source| DocumentError::Read {
            path: path.clone(),
            source,
        })?;
        let bom = crate::encoding::detect_bom(&bytes);
        // An unspecified-order encoding reads the mark itself.
        let body = if encoding.is_unspecified_order() {
            &bytes[..]
        } else {
            &bytes[bom.len()..]
        };
        let (text, _) = encoding.decode(body);
        self.text = text;
        self.encoding = encoding;
        self.bom = if encoding.supports_bom() {
            bom
        } else {
            Bom::None
        };
        self.line_ending = line_ending::detect(&self.text);
        Ok(())
    }

    // ---- file I/O --------------------------------------------------------

    /// Write the buffer back to `self.path`, or to `path` when given.
    pub fn save(&mut self, path: Option<PathBuf>) -> Result<(), DocumentError> {
        let target = match path {
            Some(p) => p,
            None => self.path.clone().ok_or_else(|| DocumentError::Write {
                path: PathBuf::from("Untitled"),
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "no path"),
            })?,
        };

        let file = DecodedFile {
            text: self.text.clone(),
            encoding: self.encoding,
            bom: self.bom,
            detection: self.detection,
        };
        let (bytes, lossy) = file.encode_checked();
        // Upstream's `allowsLossySaving`: once the user has answered *Save
        // Available Text*, the rest of the session saves through without
        // asking again.
        if lossy && !self.allows_lossy_saving {
            return Err(DocumentError::LossyEncoding(file.encoding.name));
        }

        // Write to a sibling temp file and rename, so a crash mid-write cannot
        // truncate the user's document. This is what an editor must do.
        let temp = target.with_extension("subtext-tmp");
        std::fs::write(&temp, &bytes).map_err(|source| DocumentError::Write {
            path: temp.clone(),
            source,
        })?;
        std::fs::rename(&temp, &target).map_err(|source| DocumentError::Write {
            path: target.clone(),
            source,
        })?;

        let metadata = std::fs::metadata(&target).ok();
        self.path = Some(target);
        self.saved_at = metadata.as_ref().and_then(|m| m.modified().ok());
        self.saved_len = metadata.as_ref().map(|m| m.len());
        self.dirty = false;
        Ok(())
    }

    /// Whether the file on disk has changed since we read it.
    pub fn file_status(&self) -> FileStatus {
        let Some(path) = &self.path else {
            return FileStatus::Untitled;
        };
        let Ok(metadata) = std::fs::metadata(path) else {
            return FileStatus::Missing;
        };
        let modified = metadata.modified().ok();
        let len = metadata.len();
        let changed_time = match (modified, self.saved_at) {
            (Some(now), Some(before)) => now > before,
            _ => false,
        };
        let changed_len = self.saved_len.is_some_and(|before| before != len);
        if changed_time || changed_len {
            FileStatus::Modified
        } else {
            FileStatus::Unchanged
        }
    }

    /// Reload the file from disk, discarding the buffer.
    ///
    /// The encoding the document is *kept* in does not move: a revert re-reads
    /// the bytes, and the user's choice of how to interpret them is theirs. The
    /// detector runs again only to find the encoding, and its answer is thrown
    /// away in favour of the one already in force.
    pub fn revert(&mut self, options: &DetectionOptions<'_>) -> Result<(), DocumentError> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        let reloaded = Self::open(&path, self.encoding, options)?;
        let encoding = self.encoding;
        *self = reloaded;
        self.encoding = encoding;
        Ok(())
    }

    /// Move the document to the trash and forget its path.
    pub fn forget_path(&mut self) {
        self.path = None;
        self.saved_at = None;
        self.saved_len = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("subtext-test-{}-{name}", std::process::id()));
        path
    }

    /// A new document is created in the default *file* encoding, mark and all.
    ///
    /// The mark is half of what the user chose: CotEditor's *Default text
    /// encoding* picker selects a `FileEncoding`, so *Unicode (UTF-8) with BOM*
    /// is a promise about the file this document will become. It used to be
    /// dropped on the floor — `untitled` took a bare codec and always set
    /// `Bom::None`, so the setting was inert.
    #[test]
    fn a_new_document_carries_the_mark_of_its_default_encoding() {
        let plain = Document::untitled(FileEncoding::of(TextEncoding::UTF8), LineEnding::LF);
        assert_eq!(plain.bom(), Bom::None);
        assert_eq!(plain.encoding(), TextEncoding::UTF8);

        let with_bom = Document::untitled(FileEncoding::utf8_with_bom(), LineEnding::LF);
        assert_eq!(
            with_bom.bom(),
            Bom::Utf8,
            "the mark the user asked for was dropped"
        );

        // Only the *unspecified-order* forms carry a mark of their own, which is
        // what upstream's `FileEncoding.hasBOM` says: true for `.utf16`/`.utf32`,
        // false for `.utf16LittleEndian` — Foundation writes those bare.
        let utf16 = Document::untitled(FileEncoding::of(TextEncoding::UTF16), LineEnding::LF);
        assert_eq!(utf16.bom(), Bom::Utf16Le);
        let utf16_le = Document::untitled(FileEncoding::of(TextEncoding::UTF16_LE), LineEnding::LF);
        assert_eq!(
            utf16_le.bom(),
            Bom::None,
            "an endian-specific form is written bare"
        );
    }

    /// The two forms that leave the byte order to the mark, and the four that do
    /// not — measured against what Foundation writes.
    ///
    /// `"a".data(using: .utf16)` is `FF FE 61 00` on this machine (a mark, then
    /// the platform's order); `"a".data(using: .utf16LittleEndian)` is `61 00`
    /// with no mark at all. Upstream writes whichever Foundation produces, so
    /// the bytes on disk have to match that, not a nicer-looking convention.
    #[test]
    fn the_wide_encodings_write_the_mark_foundation_writes() {
        let cases: [(TextEncoding, &[u8]); 3] = [
            (
                TextEncoding::UTF16,
                if cfg!(target_endian = "little") {
                    &[0xFF, 0xFE, 0x61, 0x00]
                } else {
                    &[0xFE, 0xFF, 0x00, 0x61]
                },
            ),
            (TextEncoding::UTF16_LE, &[0x61, 0x00]),
            (TextEncoding::UTF16_BE, &[0x00, 0x61]),
        ];

        for (index, (encoding, expected)) in cases.into_iter().enumerate() {
            let path = temp_path(&format!("wide-{index}.txt"));
            let mut document = Document::untitled(FileEncoding::of(encoding), LineEnding::LF);
            document.set_text("a".to_string(), true);
            document
                .save(Some(path.clone()))
                .expect("the save must succeed");

            let bytes = std::fs::read(&path).expect("the file was written");
            assert_eq!(bytes, expected, "{encoding} wrote the wrong bytes");

            // Only the mark-bearing form reads back as itself: a bare UTF-16
            // body is ambiguous, and `61 00` is valid UTF-8 too, so detection
            // comes back as "a\0" in either editor. That ambiguity is the
            // reason the mark-bearing form exists.
            if encoding == TextEncoding::UTF16 {
                let reopened =
                    Document::open(&path, TextEncoding::UTF8, &DetectionOptions::defaults())
                        .expect("the file must open");
                assert_eq!(reopened.text(), "a");
                assert_eq!(reopened.encoding(), TextEncoding::UTF16);
            }

            std::fs::remove_file(&path).ok();
        }
    }

    /// …and the mark reaches the file, then comes back.
    #[test]
    fn the_default_mark_is_written_to_disk() {
        let path = temp_path("bom-default.txt");
        let mut document = Document::untitled(FileEncoding::utf8_with_bom(), LineEnding::LF);
        document.set_text("hello\n", true);
        document.save(Some(path.clone())).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "no mark on disk");
        assert_eq!(bytes.len(), 3 + "hello\n".len());

        let reopened =
            Document::open(&path, TextEncoding::UTF8, &DetectionOptions::defaults()).unwrap();
        assert_eq!(reopened.bom(), Bom::Utf8);
        assert_eq!(reopened.text(), "hello\n");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_document_the_encoding_cannot_spell_is_not_written() {
        // CotEditor's `LossyEncodingError`. Writing anyway would put a file on
        // disk holding `&#65533;` where the user typed an emoji — the file
        // would look saved and would not be the document.
        let encoding = TextEncoding::from_identifier("shift_jis").expect("Shift JIS");
        let mut document = Document::untitled(
            FileEncoding {
                encoding,
                with_utf8_bom: false,
            },
            LineEnding::LF,
        );
        document.set_text("hello 👾 world".to_string(), true);

        let path = std::env::temp_dir().join("subtext-lossy-save.txt");
        let error = document
            .save(Some(path.clone()))
            .expect_err("the save must fail");
        assert!(matches!(
            error,
            DocumentError::LossyEncoding("Japanese (Shift JIS)")
        ));
        // …and nothing was written, not even a half file.
        assert!(!path.exists());

        // Text the encoding *can* spell saves as before.
        document.set_text("hello world".to_string(), true);
        assert!(document.save(Some(path.clone())).is_ok());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn allowing_lossy_saving_writes_the_substituted_file() {
        // Upstream's *Save Available Text*: once allowed, the save goes
        // through with the incompatible characters substituted, and the rest
        // of the session saves through without asking again.
        let encoding = TextEncoding::from_identifier("shift_jis").expect("Shift JIS");
        let mut document = Document::untitled(
            FileEncoding {
                encoding,
                with_utf8_bom: false,
            },
            LineEnding::LF,
        );
        document.set_text("hello 👾 world".to_string(), true);

        let path = std::env::temp_dir().join("subtext-lossy-allowed.txt");
        assert!(document.save(Some(path.clone())).is_err());

        document.set_allows_lossy_saving(true);
        assert!(document.save(Some(path.clone())).is_ok());
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains('👾'));
        // A second save does not ask again — the flag is per session.
        document.set_text("hello 👾👾 world".to_string(), true);
        assert!(document.save(Some(path.clone())).is_ok());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_then_open_round_trips_everything() {
        let path = temp_path("round-trip.txt");
        let mut document =
            Document::untitled(FileEncoding::of(TextEncoding::UTF8), LineEnding::CRLF);
        document.set_text("first\r\nsecond\r\n", true);
        document.set_line_ending(LineEnding::CRLF);
        document.save(Some(path.clone())).unwrap();

        let reopened =
            Document::open(&path, TextEncoding::UTF8, &DetectionOptions::defaults()).unwrap();
        assert_eq!(reopened.text(), "first\r\nsecond\r\n");
        assert_eq!(reopened.line_ending(), LineEnding::CRLF);
        assert!(!reopened.is_dirty());
        assert_eq!(
            reopened.display_name(),
            path.file_name().unwrap().to_string_lossy()
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn line_ending_change_rewrites_the_buffer() {
        let mut document = Document::untitled(FileEncoding::of(TextEncoding::UTF8), LineEnding::LF);
        document.set_text("a\nb\nc", false);
        document.set_line_ending(LineEnding::CRLF);
        assert_eq!(document.text(), "a\r\nb\r\nc");
        assert_eq!(document.line_count(), 3);
    }

    #[test]
    fn unsaved_document_is_untitled_and_clean() {
        let document = Document::untitled(FileEncoding::of(TextEncoding::UTF8), LineEnding::LF);
        assert!(document.is_untitled());
        assert!(!document.is_dirty());
        assert_eq!(document.file_status(), FileStatus::Untitled);
        assert_eq!(document.display_name(), "Untitled");
    }

    /// The invariant behind a real bug: an untitled document has nowhere to
    /// write, so `save(None)` refuses instead of inventing a path. A caller
    /// that means to save one has to supply a path — which is why the close
    /// alert has to go through the save panel rather than straight to a write.
    #[test]
    fn an_untitled_document_refuses_to_save_without_a_path() {
        let mut document = Document::untitled(FileEncoding::of(TextEncoding::UTF8), LineEnding::LF);
        document.set_text("draft", true);
        assert!(matches!(
            document.save(None),
            Err(DocumentError::Write { .. })
        ));
        // Nothing reached a disk, so the buffer is still unsaved.
        assert!(document.is_dirty());
        assert!(document.is_untitled());
    }

    #[test]
    fn external_modification_is_detected() {
        let path = temp_path("external.txt");
        std::fs::write(&path, "original").unwrap();
        let document =
            Document::open(&path, TextEncoding::UTF8, &DetectionOptions::defaults()).unwrap();
        assert_eq!(document.file_status(), FileStatus::Unchanged);

        std::fs::write(&path, "changed and longer").unwrap();
        assert_eq!(document.file_status(), FileStatus::Modified);

        std::fs::remove_file(&path).ok();
        assert_eq!(document.file_status(), FileStatus::Missing);
    }

    #[test]
    fn convert_encoding_does_not_change_the_text() {
        let mut document = Document::untitled(FileEncoding::of(TextEncoding::UTF8), LineEnding::LF);
        document.set_text("日本語", false);
        document.convert_encoding(
            TextEncoding::from_identifier("shift_jis").unwrap(),
            Bom::None,
        );
        assert_eq!(document.text(), "日本語");
        assert!(document.is_dirty());
    }

    #[test]
    fn bom_is_dropped_for_encodings_that_cannot_carry_one() {
        let mut document = Document::untitled(FileEncoding::of(TextEncoding::UTF8), LineEnding::LF);
        document.convert_encoding(
            TextEncoding::from_identifier("shift_jis").unwrap(),
            Bom::Utf8,
        );
        assert_eq!(document.bom(), Bom::None);
    }
}
