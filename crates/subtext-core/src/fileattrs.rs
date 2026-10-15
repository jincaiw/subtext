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
        Self { mask: (mask & 0o7777) as u16 }
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
        for (group, special) in
            [(6u16, 0o4u16), (3, 0o2), (0, 0o1)]
        {
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
                0o4 | 0o2 => if executable { 's' } else { 'S' },
                _ => if executable { 't' } else { 'T' },
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
        libc::getpwuid_r(uid, &mut entry, buffer.as_mut_ptr(), buffer.len(), &mut result)
    };
    if status != 0 || result.is_null() || entry.pw_name.is_null() {
        return None;
    }
    // SAFETY: the call succeeded, so `pw_name` points into `buffer` at a
    // NUL-terminated string.
    unsafe { CStr::from_ptr(entry.pw_name) }.to_str().ok().map(str::to_string)
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
        assert_eq!(FilePermissions::from_mask(0o100644).formatted(), "644 (-rw-r--r--)");
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
            let owner = attributes.owner.as_deref().expect("a unix file has an owner");
            assert!(!owner.is_empty());
            assert!(owner.chars().any(|c| c.is_alphanumeric()), "{owner:?}");
            assert!(!owner.chars().all(|c| c.is_ascii_digit()), "{owner:?} is a uid");
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
        assert!(formatted.ends_with(" AM") || formatted.ends_with(" PM"), "{formatted:?}");
        assert!(formatted.contains(" at "), "{formatted:?}");
        assert!(formatted.contains("2026"), "{formatted:?}");
        assert!(formatted.contains(','), "{formatted:?}");
        // No zero padding on the day, which is what `.abbreviated` produces.
        let day = formatted.split_once(", ").expect("a comma after the day").0;
        assert!(!day.ends_with('0') || day.len() == 2, "{formatted:?}");
    }
}
