//! File attributes — the **File** section of CotEditor's Document Inspector.
//!
//! Upstream reads these out of `FileManager.attributesOfItem`, i.e. Foundation's
//! `FileAttributes` dictionary, and caches them on the document. Everything here
//! is optional for the same reason it is there: a document that has never been
//! saved has no file, and the section then draws empty values rather than zeros.

use std::path::Path;
use std::time::SystemTime;

/// What the operating system knows about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAttributes {
    /// The creation time — Foundation's `.creationDate`.
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    /// The size on disk, which is what upstream's `.size` is. Note that this is
    /// *not* the length of the text encoded in the document's own encoding —
    /// that is [`crate::charinfo::CharacterCount::encoded_bytes`], and the two
    /// differ for any file SubText has edited but not saved.
    pub size: u64,
    pub permissions: FilePermissions,
    /// The owning account's name, or `None` where the platform cannot name it.
    /// See [`FileAttributes::read`].
    pub owner: Option<String>,
    /// The Finder tags, read out of the file's extended attributes.
    ///
    /// Empty on platforms without the Finder, and on files the Finder has not
    /// tagged — the two are indistinguishable to the inspector, which draws an
    /// empty value either way.
    pub tags: Vec<FinderTag>,
}

impl FileAttributes {
    /// Read the attributes of `path`.
    ///
    /// Returns `None` when the file cannot be stat'ed at all — an unsaved
    /// document, or one whose file has been moved away underneath it.
    ///
    /// **Owner is unix-only.** Foundation hands CotEditor the owner's account
    /// name as part of `.ownerAccountName`; the Rust standard library exposes
    /// the numeric uid and nothing else, so the name is looked up through
    /// `getpwuid_r`. On Windows there is no equivalent call in `std`, and this
    /// returns `None` rather than a numeric stand-in.
    pub fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            size: metadata.len(),
            permissions: FilePermissions::of(&metadata),
            owner: owner_name(&metadata),
            tags: finder_tags_value(path).map_or_else(Vec::new, |data| parse_finder_tags(&data)),
        })
    }
}

/// The POSIX permission mask, in the three groups CotEditor draws it.
///
/// A transcription of its `FilePermissions`: the same three groups of three
/// bits, the same three set-id / sticky bits above them, and the same two
/// renderings — the octal mask and the symbolic one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilePermissions {
    /// The nine low bits: user, group and others, three bits each.
    mask: u16,
}

/// Read, write and execute, in that bit order.
const PERMISSION_BITS: [(u16, char); 3] = [(0o4, 'r'), (0o2, 'w'), (0o1, 'x')];

impl FilePermissions {
    /// From a POSIX mask. Bits above the twelve that mean something are
    /// dropped, as they are upstream.
    pub fn from_mask(mask: u32) -> Self {
        Self {
            mask: (mask & 0o7777) as u16,
        }
    }

    /// The nine permission bits, without the special ones.
    pub fn mask(self) -> u16 {
        self.mask & 0o777
    }

    /// The three special bits — setuid, setgid, sticky — as one octal digit.
    pub fn special(self) -> u16 {
        (self.mask >> 9) & 0o7
    }

    /// The nine permission bits of `metadata`.
    ///
    /// Off unix there is no POSIX mask to read, and a read-only file is the
    /// only distinction the standard library draws — so a writable file reports
    /// `666` and a read-only one `444`, which is the truth the platform will
    /// tell.
    pub fn of(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Self::from_mask(metadata.mode())
        }
        #[cfg(not(unix))]
        {
            if metadata.permissions().readonly() {
                Self::from_mask(0o444)
            } else {
                Self::from_mask(0o666)
            }
        }
    }

    /// The symbolic expression, e.g. `rwxr-xr-x`.
    ///
    /// The special bits ride in the execute position of their group: setuid in
    /// the user's, setgid in the group's, sticky in the others' — as a lower
    /// case letter when the group is also executable and an upper case one when
    /// it is not, which is what `chmod` prints.
    pub fn symbolic(self) -> String {
        let mut out = String::with_capacity(9);
        for (group, special) in [(6u16, 0o4u16), (3, 0o2), (0, 0o1)] {
            let bits = (self.mask >> group) & 0o7;
            for (bit, letter) in PERMISSION_BITS {
                if bits & bit != 0 {
                    out.push(letter);
                } else {
                    out.push('-');
                }
            }
            if self.special() & special == 0 {
                continue;
            }
            let executable = bits & 0o1 != 0;
            let letter = match special {
                0o4 | 0o2 => {
                    if executable {
                        's'
                    } else {
                        'S'
                    }
                }
                _ => {
                    if executable {
                        't'
                    } else {
                        'T'
                    }
                }
            };
            out.pop();
            out.push(letter);
        }
        out
    }

    /// The octal expression, e.g. `644` — four digits once a special bit is
    /// set, three otherwise.
    pub fn octal(self) -> String {
        let octal = format!("{:o}", self.mask);
        let width = if self.special() == 0 { 3 } else { 4 };
        format!("{octal:0>width$}")
    }

    /// `644 (-rw-r--r--)` — upstream's `formatted()`.
    pub fn formatted(self) -> String {
        format!("{} (-{})", self.octal(), self.symbolic())
    }
}

/// The name of the account that owns `metadata`, where the platform can say.
fn owner_name(metadata: &std::fs::Metadata) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        lookup_user(metadata.uid())
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

/// Resolve a uid through the account database.
///
/// `getpwuid_r` rather than `getpwuid`: the latter hands back a pointer into a
/// static buffer that the next call overwrites, and the inspector can be
/// redrawn while another thread is inside it.
#[cfg(unix)]
fn lookup_user(uid: u32) -> Option<String> {
    use std::ffi::CStr;

    // `sysconf(_SC_GETPW_R_SIZE_MAX)` is the documented way to size this, but
    // it may return -1 for "no limit", and every account name in practice fits
    // in a kilobyte. The buffer is only borrowed for the call.
    let mut buffer = vec![0 as libc::c_char; 1024];
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: `entry` and `result` are valid pointers for the duration of the
    // call, and `buffer` outlives it.
    let status = unsafe {
        libc::getpwuid_r(
            uid,
            &mut entry,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        )
    };
    if status != 0 || result.is_null() || entry.pw_name.is_null() {
        return None;
    }
    // SAFETY: the call succeeded, so `pw_name` points into `buffer` at a
    // NUL-terminated string.
    unsafe { CStr::from_ptr(entry.pw_name) }
        .to_str()
        .ok()
        .map(str::to_string)
}

/// A timestamp in the shape CotEditor's File section shows it.
///
/// Upstream formats with `.formatted(date: .abbreviated, time: .shortened)`,
/// which is a *localized* format — `Oct 15, 2026 at 11:04 PM` in en_US. SubText
/// has no localization machinery, so this is that shape, written out. The time
/// zone is the machine's, read from the OS.
///
/// `None` for a timestamp the clock cannot represent, which is also what the
/// File section shows as an empty value.
pub fn format_timestamp(time: SystemTime) -> Option<String> {
    let timestamp = jiff::Timestamp::try_from(time).ok()?;
    let zoned = timestamp.to_zoned(jiff::tz::TimeZone::system());
    zoned.strftime("%b %-d, %Y at %-I:%M %p").to_string().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_permission_renderings_are_the_two_chmod_prints() {
        let permissions = FilePermissions::from_mask(0o644);
        assert_eq!(permissions.symbolic(), "rw-r--r--");
        assert_eq!(permissions.octal(), "644");
        assert_eq!(permissions.formatted(), "644 (-rw-r--r--)");

        assert_eq!(FilePermissions::from_mask(0o755).symbolic(), "rwxr-xr-x");
        assert_eq!(FilePermissions::from_mask(0o600).symbolic(), "rw-------");
        assert_eq!(FilePermissions::from_mask(0o777).symbolic(), "rwxrwxrwx");
        assert_eq!(FilePermissions::from_mask(0).symbolic(), "---------");
        assert_eq!(FilePermissions::from_mask(0).octal(), "000");
    }

    #[test]
    fn the_special_bits_ride_in_the_execute_position() {
        // setuid with execute is `s`, without it `S` — and the same for setgid
        // and for sticky, which is what `chmod` prints.
        let setuid = FilePermissions::from_mask(0o4755);
        assert_eq!(setuid.symbolic(), "rwsr-xr-x");
        assert_eq!(setuid.octal(), "4755");
        assert_eq!(setuid.formatted(), "4755 (-rwsr-xr-x)");

        assert_eq!(FilePermissions::from_mask(0o4644).symbolic(), "rwSr--r--");
        assert_eq!(FilePermissions::from_mask(0o2755).symbolic(), "rwxr-sr-x");
        assert_eq!(FilePermissions::from_mask(0o1777).symbolic(), "rwxrwxrwt");
        assert_eq!(FilePermissions::from_mask(0o1666).symbolic(), "rw-rw-rwT");
    }

    #[test]
    fn bits_above_the_twelve_are_dropped() {
        // The file-type bits of `st_mode` are not permissions.
        assert_eq!(
            FilePermissions::from_mask(0o100644).formatted(),
            "644 (-rw-r--r--)"
        );
        assert_eq!(FilePermissions::from_mask(0o100644).mask(), 0o644);
    }

    #[test]
    fn reading_a_real_file() {
        let path = std::env::temp_dir().join("subtext-fileattrs-test.txt");
        std::fs::write(&path, b"twelve bytes").unwrap();

        let attributes = FileAttributes::read(&path).expect("the file was just written");
        assert_eq!(attributes.size, 12);
        assert!(attributes.modified.is_some());
        // The size is the file's, and it is a different question from the
        // length of the text in some encoding.
        assert_ne!(attributes.size, 0);

        // The owner is a real account name, not a number.
        #[cfg(unix)]
        {
            let owner = attributes
                .owner
                .as_deref()
                .expect("a unix file has an owner");
            assert!(!owner.is_empty());
            assert!(owner.chars().any(|c| c.is_alphanumeric()), "{owner:?}");
            assert!(
                !owner.chars().all(|c| c.is_ascii_digit()),
                "{owner:?} is a uid"
            );
        }

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_file_has_no_attributes() {
        let path = std::env::temp_dir().join("subtext-fileattrs-does-not-exist");
        std::fs::remove_file(&path).ok();
        assert_eq!(FileAttributes::read(&path), None);
    }

    #[test]
    fn a_timestamp_is_written_in_the_shape_the_inspector_shows() {
        // 2026-10-15 12:00:00 UTC. The zone is the machine's, so only the
        // *shape* is asserted here — the day and the clock time move with it.
        let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_792_065_600);
        let formatted = format_timestamp(time).expect("a representable time");
        assert!(
            formatted.ends_with(" AM") || formatted.ends_with(" PM"),
            "{formatted:?}"
        );
        assert!(formatted.contains(" at "), "{formatted:?}");
        assert!(formatted.contains("2026"), "{formatted:?}");
        assert!(formatted.contains(','), "{formatted:?}");
        // No zero padding on the day, which is what `.abbreviated` produces.
        let day = formatted.split_once(", ").expect("a comma after the day").0;
        assert!(!day.ends_with('0') || day.len() == 2, "{formatted:?}");
    }
}

// ---- Finder tags -------------------------------------------------------

/// The colour half of a Finder tag.
///
/// Upstream's `FinderTag.Color`, whose raw values are what the extended
/// attribute stores: 0 for "no colour", then the seven tinted ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinderColor {
    None,
    Gray,
    Green,
    Purple,
    Blue,
    Yellow,
    Red,
    Orange,
}

impl FinderColor {
    /// The colour an extended attribute's index names.
    ///
    /// An index outside 0–7 is not a colour Finder can draw, and upstream
    /// leaves the tag uncoloured rather than guessing.
    pub fn from_index(index: u8) -> Self {
        match index {
            1 => Self::Gray,
            2 => Self::Green,
            3 => Self::Purple,
            4 => Self::Blue,
            5 => Self::Yellow,
            6 => Self::Red,
            7 => Self::Orange,
            _ => Self::None,
        }
    }

    /// The colour as macOS draws it, as red/green/blue.
    ///
    /// These are the system tag colours from the Finder's palette, not theme
    /// roles: a file tagged red is red whatever theme SubText is wearing, which
    /// is the whole point of a tag. They are the fourth of the recorded
    /// colour-literal exceptions (see FEATURE-MATRIX).
    pub fn rgb(self) -> Option<(u8, u8, u8)> {
        Some(match self {
            Self::None => return None,
            Self::Gray => (0x8E, 0x8E, 0x93),
            Self::Green => (0x28, 0xCD, 0x41),
            Self::Purple => (0xAF, 0x52, 0xDE),
            Self::Blue => (0x00, 0x7A, 0xFF),
            Self::Yellow => (0xFF, 0xCC, 0x00),
            Self::Red => (0xFF, 0x3B, 0x30),
            Self::Orange => (0xFF, 0x95, 0x00),
        })
    }
}

/// One Finder tag: a name and the colour it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderTag {
    pub name: String,
    pub color: FinderColor,
}

impl FinderTag {
    /// Parse one string out of the extended attribute.
    ///
    /// The attribute stores `"Name"` or `"Name\nColourIndex"` — a newline and
    /// the decimal index, which is what the Finder writes. A name containing a
    /// newline would be split here too, exactly as upstream's
    /// `split(separator: "\n")` does; the Finder does not allow one in a tag.
    pub fn parse(string: &str) -> Option<Self> {
        let mut parts = string.split('\n');
        let name = parts.next()?;
        if name.is_empty() {
            return None;
        }
        let color = parts
            .next()
            .and_then(|index| index.parse::<u8>().ok())
            .map_or(FinderColor::None, FinderColor::from_index);
        Some(Self {
            name: name.to_string(),
            color,
        })
    }
}

/// The Finder tags stored in a `com.apple.metadata:_kMDItemUserTags` value.
///
/// That value is a **binary property list** holding one array of strings. A
/// value that is not one — a truncated attribute, a standard (XML) plist, a
/// different type altogether — yields no tags rather than an error: a document
/// with no tags and a document with an unreadable tag attribute look the same
/// in the inspector either way.
pub fn parse_finder_tags(data: &[u8]) -> Vec<FinderTag> {
    bplist_string_array(data)
        .unwrap_or_default()
        .iter()
        .filter_map(|string| FinderTag::parse(string))
        .collect()
}

/// The strings of a binary property list whose root is an array of strings.
///
/// Only what a tag attribute needs: `bplist00`, one root array, strings in
/// ASCII or UTF-16. Anything else is `None`. The format is Apple's
/// [binary property list][bplist], and the parts read here are the header, the
/// trailer's four numbers, the offset table, and the two string markers.
///
/// [bplist]: https://opensource.apple.com/source/CF/CF-550/CFBinaryPList.c
fn bplist_string_array(data: &[u8]) -> Option<Vec<String>> {
    const HEADER: usize = 8;
    const TRAILER: usize = 32;

    if !data.starts_with(b"bplist00") || data.len() < HEADER + TRAILER {
        return None;
    }
    let trailer = &data[data.len() - TRAILER..];
    // The trailer's first six bytes are padding.
    let offset_size = *trailer.get(6)? as usize;
    let reference_size = *trailer.get(7)? as usize;
    let object_count = big_endian(&trailer[8..16]) as usize;
    let root = big_endian(&trailer[16..24]) as usize;
    let table = big_endian(&trailer[24..32]) as usize;
    if offset_size == 0 || offset_size > 8 || reference_size == 0 || reference_size > 8 {
        return None;
    }

    // Where the object numbered `index` starts.
    let offset_of = |index: usize| -> Option<usize> {
        if index >= object_count {
            return None;
        }
        let start = table.checked_add(index.checked_mul(offset_size)?)?;
        big_endian(data.get(start..start + offset_size)?)
            .try_into()
            .ok()
    };

    let root_offset = offset_of(root)?;
    let (count, mut cursor) = bplist_count(data, root_offset)?;
    if data.get(root_offset)? >> 4 != 0xA {
        return None;
    }

    let mut strings = Vec::with_capacity(count);
    for _ in 0..count {
        let reference = big_endian(data.get(cursor..cursor + reference_size)?) as usize;
        cursor += reference_size;
        strings.push(bplist_string(data, offset_of(reference)?)?);
    }
    Some(strings)
}

/// The element count at `offset`, and where its body begins.
///
/// A marker's low nibble is the count, except for `0xF`, which means "the count
/// is the next object" — an integer whose own low nibble is the power of two
/// number of bytes it occupies.
fn bplist_count(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    let marker = *data.get(offset)?;
    let low = (marker & 0x0F) as usize;
    if low != 0x0F {
        return Some((low, offset + 1));
    }
    let integer = *data.get(offset + 1)?;
    if integer >> 4 != 0x1 {
        return None;
    }
    let bytes = 1usize << (integer & 0x0F);
    let start = offset + 2;
    let count = big_endian(data.get(start..start + bytes)?) as usize;
    Some((count, start + bytes))
}

/// One string object at `offset`: ASCII (`0x5`) or UTF-16BE (`0x6`).
fn bplist_string(data: &[u8], offset: usize) -> Option<String> {
    let marker = *data.get(offset)?;
    let kind = marker >> 4;
    let (length, body) = bplist_count(data, offset)?;
    match kind {
        0x5 => {
            let bytes = data.get(body..body + length)?;
            // The marker promises ASCII; anything else would be a writer bug,
            // and a lossy read is better than dropping the tag.
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        0x6 => {
            let bytes = data.get(body..body + length * 2)?;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect();
            String::from_utf16(&units).ok()
        }
        _ => None,
    }
}

/// A big-endian unsigned integer of however many bytes were handed over.
fn big_endian(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
}

/// The `com.apple.metadata:_kMDItemUserTags` value of a file, on macOS.
///
/// The attribute is an extended attribute, which `std` has no API for, so this
/// is `getxattr` twice: once for the size, once for the value. Other platforms
/// have no Finder and no such attribute, and return `None`.
#[cfg(target_os = "macos")]
fn finder_tags_value(path: &Path) -> Option<Vec<u8>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;

    const NAME: &str = "com.apple.metadata:_kMDItemUserTags";
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let name = CString::new(NAME).ok()?;

    // SAFETY: both C strings outlive the calls, and a null buffer with a zero
    // size is the documented way to ask for the value's length.
    let size =
        unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
    if size <= 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    // SAFETY: `buffer` is writable for exactly `size` bytes, which is what the
    // call was told it has.
    let read = unsafe {
        libc::getxattr(
            path.as_ptr(),
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            0,
            0,
        )
    };
    if read <= 0 {
        return None;
    }
    buffer.truncate(read as usize);
    Some(buffer)
}

#[cfg(not(target_os = "macos"))]
fn finder_tags_value(_: &Path) -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tag_tests {
    use super::*;

    /// Real `bplist00` output, produced by `plistlib` with `FMT_BINARY`.
    const TWO: &[u8] = &[
        0x62, 0x70, 0x6c, 0x69, 0x73, 0x74, 0x30, 0x30, 0xa2, 0x01, 0x02, 0x57, 0x47, 0x72, 0x65,
        0x65, 0x6e, 0x0a, 0x32, 0x54, 0x4e, 0x6f, 0x6e, 0x65, 0x08, 0x0b, 0x13, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18,
    ];
    const ONE_RED: &[u8] = &[
        0x62, 0x70, 0x6c, 0x69, 0x73, 0x74, 0x30, 0x30, 0xa1, 0x01, 0x55, 0x52, 0x65, 0x64, 0x0a,
        0x36, 0x08, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x10,
    ];
    const EMPTY: &[u8] = &[
        0x62, 0x70, 0x6c, 0x69, 0x73, 0x74, 0x30, 0x30, 0xa0, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09,
    ];
    /// `["中文\n4"]` — a UTF-16BE string object, which is what a tag whose name
    /// is not ASCII is stored as.
    const CJK: &[u8] = &[
        0x62, 0x70, 0x6c, 0x69, 0x73, 0x74, 0x30, 0x30, 0xa1, 0x01, 0x64, 0x4e, 0x2d, 0x65, 0x87,
        0x00, 0x0a, 0x00, 0x34, 0x08, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13,
    ];

    #[test]
    fn tags_come_back_with_the_colour_the_attribute_names() {
        // `"Green\n2"` is green, and `"None"` has no colour at all.
        assert_eq!(
            parse_finder_tags(TWO),
            vec![
                FinderTag {
                    name: "Green".into(),
                    color: FinderColor::Green
                },
                FinderTag {
                    name: "None".into(),
                    color: FinderColor::None
                },
            ]
        );
        assert_eq!(
            parse_finder_tags(ONE_RED),
            vec![FinderTag {
                name: "Red".into(),
                color: FinderColor::Red
            }]
        );
    }

    #[test]
    fn an_empty_array_is_no_tags() {
        assert!(parse_finder_tags(EMPTY).is_empty());
    }

    #[test]
    fn a_name_that_is_not_ascii_survives() {
        // The attribute stores non-ASCII names as UTF-16, not UTF-8.
        assert_eq!(
            parse_finder_tags(CJK),
            vec![FinderTag {
                name: "中文".into(),
                color: FinderColor::Blue
            }]
        );
    }

    #[test]
    fn a_value_that_is_not_a_tag_list_is_no_tags() {
        // Truncated, wrong magic, an XML plist, and plain text: each is "this
        // file has no tags", never a panic and never an error dialog.
        assert!(parse_finder_tags(&TWO[..TWO.len() - 8]).is_empty());
        assert!(parse_finder_tags(b"not a plist at all").is_empty());
        assert!(parse_finder_tags(b"<?xml version=\"1.0\"?><plist><array/></plist>").is_empty());
        assert!(parse_finder_tags(&[]).is_empty());
    }

    #[test]
    fn a_colour_index_macos_does_not_use_leaves_the_tag_uncoloured() {
        assert_eq!(FinderColor::from_index(0), FinderColor::None);
        assert_eq!(FinderColor::from_index(1), FinderColor::Gray);
        assert_eq!(FinderColor::from_index(7), FinderColor::Orange);
        assert_eq!(FinderColor::from_index(8), FinderColor::None);
        // A name with no index is a tag with no colour.
        assert_eq!(
            FinderTag::parse("Later"),
            Some(FinderTag {
                name: "Later".into(),
                color: FinderColor::None
            })
        );
        // An index that is not a number is treated the same way.
        assert_eq!(
            FinderTag::parse("Later\nred"),
            Some(FinderTag {
                name: "Later".into(),
                color: FinderColor::None
            })
        );
        // An empty name is not a tag.
        assert_eq!(FinderTag::parse(""), None);
        assert_eq!(FinderTag::parse("\n3"), None);
    }
}
