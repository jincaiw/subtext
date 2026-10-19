//! CotEditor's Folder Find: one query, run over every text file under a folder.
//!
//! Ported from `Packages/EditorCore/Sources/FolderFind/` — `FolderFind.swift`,
//! `Search.swift`, `FileScope.swift`, `FolderFindProgress.swift` and
//! `URL+Binary.swift`. This is the model behind the *Folder Find* sidebar pane;
//! the find bar's in-document query lives in [`crate::finder`].
//!
//! # What a search returns
//!
//! [`Search::run`] walks the folder depth-first and returns a [`Summary`]: the
//! files that matched, in the order they were visited, each with the matches it
//! holds and enough of the surrounding line to display them. The walk is
//! depth-first with directories first, each group ordered the way the Finder
//! orders names, which is what makes the result list read like a file browser.
//!
//! # Two limits, two truncations
//!
//! [`Options::maximum_match_count_per_file`] stops listing matches inside one
//! file, and the file's [`FileResult::is_truncated`] says so. The search itself
//! keeps counting matches in that file — only the listing stops.
//!
//! [`Options::maximum_match_count`] stops the whole walk. The file that reached
//! the limit is kept (with what it had collected), the rest of the folder is
//! never looked at, and [`Metrics::has_unsearched_files`] says the totals are
//! lower bounds. The distinction matters: the first is a display decision, the
//! second means the answer is incomplete.
//!
//! # Deviations from upstream
//!
//! * **Offsets are bytes, not UTF-16 units.** Every offset in this module —
//!   `Match::range`, `Match::range_in_line` — counts UTF-8 bytes, because that
//!   is what the editor underneath speaks. Upstream's equivalent numbers are
//!   UTF-16 units. The *contents* of a displayed line are the same; only the
//!   arithmetic differs, and every consumer here is byte-based.
//!
//! * **A file that cannot be decoded is skipped.** Upstream tries the user's
//!   encoding candidates and gives up when they all fail. SubText decodes with
//!   [`DecodedFile`] and skips the file when the result came from the fallback
//!   rather than from a byte-order mark, valid UTF-8, or a confident guess —
//!   that is the same file set, decided by the same evidence. A BOM-less
//!   UTF-16 or UTF-32 file is recognised on top of that, which upstream needs a
//!   candidate list to do.
//!
//! * **File types are recognised by extension.** Upstream asks `UTType` whether
//!   a file conforms to `public.text` or `public.property-list`. There is no
//!   type database on Windows or Linux, so [`is_searchable_text`] uses a list of
//!   extensions plus upstream's "a file with no extension is text" rule. The
//!   caller can widen this: the pane also searches any file whose name maps to
//!   one of CotEditor's syntax definitions.

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::encoding::{DecodedFile, Detection, DetectionOptions, TextEncoding};
use crate::finder::{FindOptions, Searcher};

/// The longest a displayed line fragment may be, in bytes.
///
/// Upstream's `maximumLineLength` is 512 UTF-16 units; the fragment is then
/// grown to whole grapheme clusters, so the result can be a little longer.
const MAXIMUM_LINE_LENGTH: usize = 512;

/// How much of a line to keep before the match.
///
/// Upstream's `headPadding`, so a match sits 64 units in from the left edge of
/// the fragment rather than at it.
const HEAD_PADDING: usize = 64;

/// How the find string is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mode {
    /// Treat the find string as a regular expression.
    pub uses_regular_expression: bool,
    /// Ignore character case.
    pub ignores_case: bool,
}

/// A folder search query: a find string and the mode it is read in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub find_string: String,
    pub mode: Mode,
}

impl Query {
    pub fn new(find_string: impl Into<String>, mode: Mode) -> Self {
        Self {
            find_string: find_string.into(),
            mode,
        }
    }

    /// Compile the query into a searcher.
    ///
    /// Fails on an empty find string, and on a regular expression that does not
    /// compile — the same two failures upstream's `TextFind.Pattern` reports,
    /// and both are reported before the folder is touched.
    pub fn searcher(&self) -> Result<Searcher, QueryError> {
        if self.find_string.is_empty() {
            return Err(QueryError::EmptyFindString);
        }
        // Folder find has no whole-word switch and no escape sequences: upstream
        // builds its mode from exactly these two flags.
        let options = FindOptions {
            regex: self.mode.uses_regular_expression,
            case_sensitive: !self.mode.ignores_case,
            whole_word: false,
            wrap_around: false,
            escape_sequences: false,
            in_selection: false,
            singleline: false,
            multiline: true,
        };
        Searcher::new(&self.find_string, &options)
            .map_err(|error| QueryError::InvalidRegularExpression(error.to_string()))
    }
}

/// Why a query could not be compiled.
///
/// Deliberately its own type rather than a reuse of
/// [`crate::multireplace::PatternError`]: the two features report the same two
/// failures but are otherwise unrelated, and a shared type would tie the folder
/// pane's error to the replace sheet's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// The find string is empty, so it would match everywhere.
    EmptyFindString,
    /// The pattern does not compile.
    InvalidRegularExpression(String),
}

impl QueryError {
    /// The one-line explanation the pane shows.
    pub fn message(&self) -> String {
        match self {
            QueryError::EmptyFindString => "The find string is empty.".to_string(),
            QueryError::InvalidRegularExpression(reason) => {
                format!("[Invalid regular expression] {reason}")
            }
        }
    }
}

/// What to search, and what to leave alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Search files that do not look like plain text. They still have to decode.
    pub includes_other_file_types: bool,
    /// Descend into dot-files and dot-directories.
    pub includes_hidden_files: bool,
    /// Names never visited, files or directories alike.
    pub excluded_names: BTreeSet<String>,
    /// Skip files larger than this, in bytes.
    pub maximum_file_size: u64,
    /// Stop the whole walk once this many matches have been collected.
    pub maximum_match_count: usize,
    /// Stop *listing* matches once one file has this many.
    pub maximum_match_count_per_file: usize,
    /// Restrict the search to files the scope accepts.
    pub file_scope: Option<FileScope>,
}

impl Default for Options {
    /// Upstream's defaults, limit for limit.
    fn default() -> Self {
        Self {
            includes_other_file_types: false,
            includes_hidden_files: false,
            excluded_names: [".DS_Store", ".git"]
                .iter()
                .map(|name| name.to_string())
                .collect(),
            maximum_file_size: 1_000_000_000,
            maximum_match_count: 500_000,
            maximum_match_count_per_file: 5_000,
            file_scope: None,
        }
    }
}

/// How far a search has got, readable while it is still running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metrics {
    pub find_string: String,
    pub match_count: usize,
    pub matched_file_count: usize,
    /// The walk stopped before every file had been looked at, so the counts are
    /// lower bounds.
    pub has_unsearched_files: bool,
}

impl Metrics {
    fn new(find_string: impl Into<String>) -> Self {
        Self {
            find_string: find_string.into(),
            match_count: 0,
            matched_file_count: 0,
            has_unsearched_files: false,
        }
    }
}

/// One hit inside a file, with the line it sits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The hit, in bytes from the start of the file.
    pub range: Range<usize>,
    /// The line fragment to display, possibly shortened at both ends.
    pub line: String,
    /// Where the hit sits inside [`Match::line`], in bytes.
    pub range_in_line: Range<usize>,
}

/// A file with at least one hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    pub path: PathBuf,
    /// The directories between the search root and the file, one component each.
    /// Empty when the file sits directly in the root.
    pub directory_path_components: Vec<String>,
    pub matches: Vec<Match>,
    /// The file has more matches than [`FileResult::matches`] lists.
    pub is_truncated: bool,
}

impl FileResult {
    /// The file's name, which is what the result row leads with.
    pub fn filename(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// The directory path between the root and the file, slash-separated.
    pub fn directory_path(&self) -> String {
        self.directory_path_components.join("/")
    }
}

/// The result of a completed search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub metrics: Metrics,
    pub files: Vec<FileResult>,
}

impl Summary {
    /// Whether more matches exist than were counted, so the totals are lower
    /// bounds — either because the walk stopped, or because a file's listing did.
    pub fn is_truncated(&self) -> bool {
        self.metrics.has_unsearched_files || self.files.iter().any(|file| file.is_truncated)
    }
}

/// A handle onto a running search's counters.
///
/// The search writes into it as it goes and the pane reads it while rendering,
/// which is how the metrics bar moves before the search finishes.
#[derive(Debug, Clone)]
pub struct Progress(Arc<Mutex<Metrics>>);

impl Progress {
    pub fn new(find_string: impl Into<String>) -> Self {
        Self(Arc::new(Mutex::new(Metrics::new(find_string))))
    }

    /// The counters as they stand right now.
    pub fn snapshot(&self) -> Metrics {
        self.0
            .lock()
            .expect("progress mutex is never poisoned")
            .clone()
    }

    fn update(&self, metrics: &Metrics) {
        *self.0.lock().expect("progress mutex is never poisoned") = metrics.clone();
    }
}

/// A handle that stops a running search.
///
/// Upstream cancels the search task when the find string changes; this is the
/// same idea without a task to cancel — the search checks the flag as it walks
/// and returns [`Cancelled`].
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A search that stopped because it was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

/// A file the walk is considering.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    path: PathBuf,
    is_directory: bool,
    file_size: u64,
}

/// A predicate that widens what counts as a searchable text file.
///
/// It is consulted only after [`is_searchable_text`] says no, so it can add
/// files to a search but never take them away.
pub type InclusionPredicate = Box<dyn Fn(&Path) -> bool + Send + Sync>;

/// A folder search, configured and ready to run.
///
/// Construction compiles the file scope, so an invalid scope fails here rather
/// than halfway through a walk.
pub struct Search {
    root: PathBuf,
    searcher: Searcher,
    options: Options,
    file_scope: Option<FileScopeMatcher>,
    is_included: Option<InclusionPredicate>,
    metrics: Metrics,
    files: Vec<FileResult>,
}

impl Search {
    /// Build a search.
    ///
    /// `is_included` widens the set of files that count as text beyond
    /// [`is_searchable_text`]; the pane passes a predicate that accepts anything
    /// CotEditor has a syntax for. It can only add files, never remove them.
    pub fn new(
        root: impl Into<PathBuf>,
        query: &Query,
        options: Options,
        is_included: Option<InclusionPredicate>,
    ) -> Result<Self, SearchError> {
        let searcher = query.searcher()?;
        let file_scope = match &options.file_scope {
            Some(scope) if !scope.is_empty() => Some(FileScopeMatcher::new(scope)?),
            _ => None,
        };
        Ok(Self {
            root: root.into(),
            searcher,
            options,
            file_scope,
            is_included,
            metrics: Metrics::new(&query.find_string),
            files: Vec::new(),
        })
    }

    /// Walk the folder and collect the matches.
    pub fn run(
        &mut self,
        progress: Option<&Progress>,
        cancel: Option<&Cancellation>,
    ) -> Result<Summary, Cancelled> {
        self.search_directory(&self.root.clone(), progress, cancel)?;
        let summary = Summary {
            metrics: self.metrics.clone(),
            files: std::mem::take(&mut self.files),
        };
        if let Some(progress) = progress {
            progress.update(&summary.metrics);
        }
        Ok(summary)
    }

    fn search_directory(
        &mut self,
        directory: &Path,
        progress: Option<&Progress>,
        cancel: Option<&Cancellation>,
    ) -> Result<(), Cancelled> {
        check_cancelled(cancel)?;

        // An unreadable directory is skipped, not an error: upstream returns
        // early from `contentsOfDirectory` failing, and a folder the user
        // cannot read should not fail the whole search.
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Ok(());
        };

        let mut candidates: Vec<Candidate> = Vec::new();
        for entry in entries.flatten() {
            check_cancelled(cancel)?;

            let path = entry.path();
            let Some(name) = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            if self.options.excluded_names.contains(&name) {
                continue;
            }
            if !self.options.includes_hidden_files && name.starts_with('.') {
                continue;
            }
            // `symlink_metadata`, so a symbolic link is seen as a link rather
            // than followed: upstream never follows aliases or symlinks.
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_symlink() {
                continue;
            }
            let is_directory = metadata.is_dir();
            let candidate = Candidate {
                path,
                is_directory,
                file_size: metadata.len(),
            };
            if !is_directory && !self.includes(&candidate) {
                continue;
            }
            candidates.push(candidate);
        }

        // Directories first, then files, each group in Finder order.
        candidates.sort_by(|lhs, rhs| match (lhs.is_directory, rhs.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let lhs = lhs
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
                let rhs = rhs
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
                natural_compare(&lhs.unwrap_or_default(), &rhs.unwrap_or_default())
            }
        });

        for candidate in candidates {
            check_cancelled(cancel)?;

            if self.metrics.match_count >= self.options.maximum_match_count {
                // The rest of the folder is never looked at, so the counts are
                // lower bounds from here on.
                self.metrics.has_unsearched_files = true;
                if let Some(progress) = progress {
                    progress.update(&self.metrics);
                }
                return Ok(());
            }

            if candidate.is_directory {
                self.search_directory(&candidate.path, progress, cancel)?;
            } else {
                self.search_file(&candidate, progress);
            }
        }

        Ok(())
    }

    fn search_file(&mut self, candidate: &Candidate, progress: Option<&Progress>) {
        if candidate.file_size > self.options.maximum_file_size {
            return;
        }
        if is_binary(&candidate.path).unwrap_or(true) {
            return;
        }
        let Ok(bytes) = std::fs::read(&candidate.path) else {
            return;
        };
        // A file that had to be decoded with the fallback is not text we can
        // search honestly; see the module note.
        let Some(text) = decode_text(&bytes) else {
            return;
        };

        let maximum = self.options.maximum_match_count_per_file.min(
            self.options
                .maximum_match_count
                .saturating_sub(self.metrics.match_count),
        );
        if maximum == 0 {
            return;
        }
        let (matches, is_truncated) = self.matches(&text, maximum);
        if matches.is_empty() {
            return;
        }

        self.metrics.match_count += matches.len();
        self.metrics.matched_file_count += 1;
        if let Some(progress) = progress {
            progress.update(&self.metrics);
        }

        let root_components = normalized_components(&self.root);
        let directory_components =
            normalized_components(candidate.path.parent().unwrap_or_else(|| Path::new("")));
        let Some(relative) = directory_components.strip_prefix(root_components.as_slice()) else {
            return;
        };

        self.files.push(FileResult {
            path: candidate.path.clone(),
            directory_path_components: relative
                .iter()
                .map(|component| component.to_string_lossy().into_owned())
                .collect(),
            matches,
            is_truncated,
        });
    }

    /// Collect the matches in one file's text, stopping at `maximum`.
    ///
    /// The count returned by [`FileResult::is_truncated`] is "the listing
    /// stopped", not "the file stopped matching": upstream stops enumerating as
    /// soon as the limit is hit, so a file with 10 000 hits reports 5 000.
    fn matches(&self, text: &str, maximum: usize) -> (Vec<Match>, bool) {
        let scope = 0..text.len();
        let mut matches = Vec::new();
        let mut is_truncated = false;

        for range in self.searcher.find_all(text, &scope) {
            if matches.len() >= maximum {
                is_truncated = true;
                break;
            }
            matches.push(display_match(text, range));
        }

        (matches, is_truncated)
    }

    /// Whether a candidate file is one this search should read.
    fn includes(&self, candidate: &Candidate) -> bool {
        let is_text = self.options.includes_other_file_types
            || is_searchable_text(&candidate.path)
            || self
                .is_included
                .as_ref()
                .is_some_and(|predicate| predicate(&candidate.path));

        is_text
            && self
                .file_scope
                .as_ref()
                .is_none_or(|scope| scope.contains(candidate, &self.root))
    }
}

/// Why a search could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    /// The query did not compile.
    Query(QueryError),
    /// The file scope did not compile.
    Scope(FileScopeError),
}

impl SearchError {
    /// The one-line explanation the pane shows.
    pub fn message(&self) -> String {
        match self {
            SearchError::Query(error) => error.message(),
            SearchError::Scope(error) => error.message(),
        }
    }
}

impl From<QueryError> for SearchError {
    fn from(error: QueryError) -> Self {
        SearchError::Query(error)
    }
}

impl From<FileScopeError> for SearchError {
    fn from(error: FileScopeError) -> Self {
        SearchError::Scope(error)
    }
}

fn check_cancelled(cancel: Option<&Cancellation>) -> Result<(), Cancelled> {
    match cancel {
        Some(cancel) if cancel.is_cancelled() => Err(Cancelled),
        _ => Ok(()),
    }
}

/// Build the display form of one match: the line it sits on, shortened.
fn display_match(text: &str, range: Range<usize>) -> Match {
    let line = line_contents_range(text, &range);
    let fragment = clamp_around(&line, &range, MAXIMUM_LINE_LENGTH, HEAD_PADDING);
    let fragment = grapheme_range(text, fragment);

    let line = text[fragment.clone()].to_string();
    let start = range.start.max(fragment.start) - fragment.start;
    let end = range.end.min(fragment.end).max(range.start) - fragment.start;

    Match {
        range,
        line,
        range_in_line: start..end,
    }
}

/// The line-content range covering `range`, terminators excluded.
///
/// This is upstream's `NSString.lineContentsRange(for:)`: the union of the
/// contents of every line the range touches, where a line is touched when its
/// own extent — contents *and* terminator — meets the range, and an empty range
/// touches the line its offset falls in.
///
/// The terminator counts as part of the line it ends, which is what makes an
/// offset sitting exactly on a `\n` report the line *above* it. Working in
/// bytes rather than UTF-16 units is the one difference; see the module note.
fn line_contents_range(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = floor_char_boundary(text, range.start.min(text.len()));
    let end = ceil_char_boundary(text, range.end.min(text.len()));
    if start >= end {
        // An empty range belongs to the line it sits in, and an offset at the
        // very end belongs to the last line.
        let (line_start, content_end) = line_at(text, start);
        return line_start..content_end;
    }

    let mut lower: Option<usize> = None;
    let mut upper = 0;
    for (line_start, content_end, line_end) in lines(text) {
        if line_end <= start {
            continue;
        }
        if line_start >= end {
            break;
        }
        lower.get_or_insert(line_start);
        upper = content_end;
    }

    match lower {
        Some(lower) => lower..upper.max(lower),
        // A range past the end of the text still belongs to the last line.
        None => {
            let (line_start, content_end) = line_at(text, text.len());
            line_start..content_end
        }
    }
}

/// The line containing `offset`: `(content_start, content_end)`.
fn line_at(text: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let mut last = (0, 0);
    for (line_start, content_end, line_end) in lines(text) {
        last = (line_start, content_end);
        if offset < line_end {
            return (line_start, content_end);
        }
    }
    last
}

/// `(content_start, content_end, line_end)` for every logical line.
///
/// `line_end` includes the terminator, so it is the end of `content_end` plus
/// one `\r\n`, one `\r`, or one `\n`.
fn lines(text: &str) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                let end = if bytes.get(index + 1) == Some(&b'\n') {
                    index + 2
                } else {
                    index + 1
                };
                ranges.push((start, index, end));
                index = end;
                start = end;
            }
            b'\n' => {
                ranges.push((start, index, index + 1));
                index += 1;
                start = index;
            }
            _ => index += 1,
        }
    }
    ranges.push((start, bytes.len(), bytes.len()));
    ranges.into_iter()
}

/// Shrink `range` to at most `max_length` around `target`, keeping `head_padding`
/// bytes of lead-in where it can.
///
/// Upstream's `NSRange.clamped(around:maxLength:headPadding:)`.
fn clamp_around(
    range: &Range<usize>,
    target: &Range<usize>,
    max_length: usize,
    head_padding: usize,
) -> Range<usize> {
    if range.len() <= max_length {
        return range.clone();
    }
    let target = target.start.clamp(range.start, range.end);
    let head = head_padding.min(target - range.start);
    let start = range.start.max(target - head);
    let end = range.end.min(start + max_length);
    start..end
}

/// Grow `range` to whole grapheme clusters.
///
/// Upstream's `rangeOfComposedCharacterSequences(for:)`. The cluster boundaries
/// are taken from the line the fragment sits on, which is the smallest region
/// that can contain a boundary the fragment needs — a cluster never spans a line
/// terminator in practice, and scanning the whole file per match would make the
/// display cost quadratic.
fn grapheme_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = floor_char_boundary(text, range.start);
    let end = ceil_char_boundary(text, range.end);
    if start >= end {
        return start..end;
    }

    let (line_start, content_end) = line_at(text, start);
    let window_end = content_end.max(end);
    let slice = &text[line_start..window_end];
    let local_start = start - line_start;
    let local_end = end - line_start;

    let mut lower = 0;
    let mut upper = slice.len();
    for (offset, grapheme) in slice.grapheme_indices(true) {
        let next = offset + grapheme.len();
        if next <= local_start {
            lower = next;
        }
        if offset >= local_end {
            upper = offset;
            break;
        }
    }

    line_start + lower..line_start + upper
}

/// The nearest char boundary at or below `offset`.
fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The nearest char boundary at or above `offset`.
fn ceil_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

/// The path's components, with the root and any trailing separator dropped.
fn normalized_components(path: &Path) -> Vec<std::ffi::OsString> {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_os_string()),
            std::path::Component::RootDir => Some(std::ffi::OsString::from(
                std::path::MAIN_SEPARATOR.to_string(),
            )),
            _ => None,
        })
        .collect()
}

/// Compare two file names the way the Finder sorts them.
///
/// Upstream asks `localizedStandardCompare`, which is case-insensitive and reads
/// digit runs as numbers, so `a2` sorts before `a10`. There is no locale
/// database here, so this is that comparison's two defining properties and
/// nothing else: digit runs compare numerically, everything else compares
/// case-insensitively, and an exact tie falls back to the byte order so the
/// result is total.
pub fn natural_compare(lhs: &str, rhs: &str) -> std::cmp::Ordering {
    let mut lhs_chars = lhs.chars().peekable();
    let mut rhs_chars = rhs.chars().peekable();

    loop {
        match (lhs_chars.peek().copied(), rhs_chars.peek().copied()) {
            (None, None) => return lhs.cmp(rhs),
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(left), Some(right)) => {
                if left.is_ascii_digit() && right.is_ascii_digit() {
                    let left_run = take_digits(&mut lhs_chars);
                    let right_run = take_digits(&mut rhs_chars);
                    let left_number = left_run.trim_start_matches('0');
                    let right_number = right_run.trim_start_matches('0');
                    match left_number.len().cmp(&right_number.len()) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    }
                    match left_number.cmp(right_number) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    }
                    // Same value, different padding: keep the shorter run first
                    // so `a2` and `a02` do not compare equal.
                    match left_run.len().cmp(&right_run.len()) {
                        std::cmp::Ordering::Equal => {}
                        ordering => return ordering,
                    }
                } else {
                    lhs_chars.next();
                    rhs_chars.next();
                    for (left, right) in left.to_lowercase().zip(right.to_lowercase()) {
                        match left.cmp(&right) {
                            std::cmp::Ordering::Equal => {}
                            ordering => return ordering,
                        }
                    }
                }
            }
        }
    }
}

fn take_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut run = String::new();
    while let Some(character) = chars.peek().copied() {
        if !character.is_ascii_digit() {
            break;
        }
        run.push(character);
        chars.next();
    }
    run
}

/// Whether a file looks like plain text, judged by its extension.
///
/// Upstream's `FolderFind.isSearchableText`, minus `UTType`: text and property
/// list types are text, anything with no extension is text, and everything else
/// is not. The list is the union of the types `public.text` and
/// `public.property-list` actually cover in practice.
pub fn is_searchable_text(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        // No extension at all: upstream treats that as text, which is what
        // makes `Makefile` and `README` searchable.
        return path.file_name().is_some();
    };
    let extension = extension.to_string_lossy().to_lowercase();
    TEXT_EXTENSIONS.contains(&extension.as_str())
}

/// Extensions treated as text without asking the file's contents.
const TEXT_EXTENSIONS: &[&str] = &[
    // Plain text and the languages people keep in it.
    "txt",
    "text",
    "md",
    "markdown",
    "rst",
    "adoc",
    "asciidoc",
    "org",
    "tex",
    "latex",
    "bib",
    "log",
    "csv",
    "tsv",
    "tab",
    "diff",
    "patch",
    "srt",
    "vtt",
    // Data and configuration.
    "json",
    "json5",
    "jsonc",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "config",
    "env",
    "properties",
    "xml",
    "xsd",
    "xsl",
    "xslt",
    "dtd",
    "plist",
    "strings",
    "stringsdict",
    "plistdict",
    "entitlements",
    "pbxproj",
    "xcconfig",
    "gradle",
    "pom",
    // Web.
    "html",
    "htm",
    "xhtml",
    "css",
    "scss",
    "sass",
    "less",
    "js",
    "mjs",
    "cjs",
    "jsx",
    "ts",
    "tsx",
    "vue",
    "svelte",
    "svg",
    // Programming languages.
    "c",
    "h",
    "cc",
    "cpp",
    "cxx",
    "hpp",
    "hh",
    "hxx",
    "m",
    "mm",
    "cs",
    "java",
    "kt",
    "kts",
    "scala",
    "groovy",
    "go",
    "rs",
    "swift",
    "py",
    "pyi",
    "rb",
    "rake",
    "gemspec",
    "php",
    "pl",
    "pm",
    "lua",
    "tcl",
    "r",
    "jl",
    "dart",
    "hs",
    "lhs",
    "ml",
    "mli",
    "fs",
    "fsx",
    "erl",
    "hrl",
    "ex",
    "exs",
    "clj",
    "cljs",
    "edn",
    "scm",
    "lisp",
    "el",
    "vim",
    "asm",
    "s",
    "f",
    "f90",
    "f95",
    "for",
    "pas",
    "pp",
    "d",
    "zig",
    "nim",
    "cr",
    "v",
    "sv",
    "vhd",
    "vhdl",
    "sol",
    "sh",
    "bash",
    "zsh",
    "fish",
    "ksh",
    "csh",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "awk",
    "sed",
    "sql",
    "graphql",
    "gql",
    "proto",
    "thrift",
    "make",
    "mk",
    "cmake",
    "am",
    "ac",
    "in",
    "dockerfile",
    "gitignore",
    "gitattributes",
    "editorconfig",
    "htaccess",
    "service",
    "desktop",
    "spec",
];

/// Whether a file looks binary.
///
/// Upstream's `URL.isBinary`, which is `grep`'s and Git's heuristic: a binary
/// property list, or a NUL byte in the first 8 KiB that is not accounted for by
/// a Unicode byte-order mark or a UTF-16/UTF-32 decode without NULs.
pub fn is_binary(path: &Path) -> std::io::Result<bool> {
    use std::io::Read as _;

    const SAMPLE: usize = 8 * 1024;

    let mut head = Vec::with_capacity(SAMPLE);
    let mut file = std::fs::File::open(path)?;
    file.by_ref().take(SAMPLE as u64).read_to_end(&mut head)?;

    if head.starts_with(b"bplist") {
        return Ok(true);
    }
    if !head.contains(&0) {
        return Ok(false);
    }
    if crate::encoding::detect_bom(&head) != crate::encoding::Bom::None {
        return Ok(false);
    }

    Ok(unicode_without_bom(&head, head.len() == SAMPLE).is_none())
}

/// The Unicode encoding a NUL-bearing sample decodes cleanly as.
///
/// This is the second half of `is_binary`'s heuristic, and it is also what lets
/// a BOM-less UTF-16 file be searched: `is_full_sample` allows the last unit to
/// be short, because an 8 KiB read can end in the middle of a surrogate pair.
///
/// The caller must already know the sample carries a NUL — without one, any
/// even-length run of bytes decodes as UTF-16.
fn unicode_without_bom(sample: &[u8], is_full_sample: bool) -> Option<TextEncoding> {
    let trailing = if is_full_sample { 2 } else { 0 };
    for trim in (0..=trailing).step_by(2) {
        let body = &sample[..sample.len() - trim];
        for (encoding, unit) in [
            (TextEncoding::UTF16_LE, 2),
            (TextEncoding::UTF16_BE, 2),
            (TextEncoding::UTF32_LE, 4),
            (TextEncoding::UTF32_BE, 4),
        ] {
            if !body.len().is_multiple_of(unit) {
                continue;
            }
            let (text, had_errors) = encoding.decode(body);
            if !had_errors && !text.contains('\0') {
                return Some(encoding);
            }
        }
    }
    None
}

/// Decode a file's bytes for searching, or `None` when they are not text.
///
/// The fallback encoding is UTF-8, which is upstream's default candidate list.
fn decode_text(bytes: &[u8]) -> Option<String> {
    if bytes.contains(&0)
        && let Some(encoding) = unicode_without_bom(bytes, false)
    {
        let (text, _) = encoding.decode(bytes);
        return Some(text);
    }
    let decoded = DecodedFile::decode(bytes, TextEncoding::UTF8, &DetectionOptions::defaults());
    (decoded.detection != Detection::Default).then_some(decoded.text)
}

/// A rule set that decides which files a search looks at.
///
/// Serializable so the pane can keep the user's named scopes in the settings
/// file — upstream keeps them in `UserDefaults`, which is the same store.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileScope {
    pub conjunction: Conjunction,
    pub rules: Vec<Rule>,
}

impl FileScope {
    pub fn new(conjunction: Conjunction, rules: Vec<Rule>) -> Self {
        Self { conjunction, rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Compile the scope, reporting the first rule that cannot be used.
    pub fn validate(&self) -> Result<(), FileScopeError> {
        if self.is_empty() {
            return Ok(());
        }
        FileScopeMatcher::new(self).map(|_| ())
    }
}

/// How a scope's rules combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Conjunction {
    /// A file is included when any rule matches.
    #[default]
    Any,
    /// A file is included when every rule matches.
    All,
}

impl Conjunction {
    /// The word that goes between *Match* and *of the following conditions:*.
    pub fn label(self) -> &'static str {
        match self {
            Conjunction::Any => "any",
            Conjunction::All => "all",
        }
    }
}

/// One condition in a file scope.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rule {
    pub target: Target,
    pub comparison: Comparison,
    pub value: String,
}

impl Rule {
    pub fn new(target: Target, comparison: Comparison, value: impl Into<String>) -> Self {
        Self {
            target,
            comparison,
            value: value.into(),
        }
    }

    /// Whether the rule can be used as it stands.
    pub fn is_valid(&self) -> bool {
        FileScope::new(Conjunction::Any, vec![self.clone()])
            .validate()
            .is_ok()
    }
}

/// What a rule looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Target {
    #[default]
    Filename,
    FilePath,
    FileExtension,
}

impl Target {
    pub const ALL: [Target; 3] = [Target::Filename, Target::FilePath, Target::FileExtension];

    pub fn label(self) -> &'static str {
        match self {
            Target::Filename => "File name",
            Target::FilePath => "File path",
            Target::FileExtension => "File extension",
        }
    }
}

/// How a rule compares.
///
/// Every comparison but the regular expression is case-insensitive, which is
/// upstream's rule. The regular expression must match the whole value and is
/// case-sensitive unless the pattern says otherwise with `(?i)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Comparison {
    #[default]
    Contains,
    DoesNotContain,
    IsEqualTo,
    IsNotEqualTo,
    StartsWith,
    EndsWith,
    MatchesRegularExpression,
}

impl Comparison {
    pub const ALL: [Comparison; 7] = [
        Comparison::Contains,
        Comparison::DoesNotContain,
        Comparison::IsEqualTo,
        Comparison::IsNotEqualTo,
        Comparison::StartsWith,
        Comparison::EndsWith,
        Comparison::MatchesRegularExpression,
    ];

    /// The word that goes immediately before the value field.
    pub fn label(self) -> &'static str {
        match self {
            Comparison::Contains => "contains",
            Comparison::DoesNotContain => "does not contain",
            Comparison::IsEqualTo => "is",
            Comparison::IsNotEqualTo => "is not",
            Comparison::StartsWith => "begins with",
            Comparison::EndsWith => "ends with",
            Comparison::MatchesRegularExpression => "matches regular expression",
        }
    }
}

/// Why a file scope cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileScopeError {
    /// A rule has no value, so it would match everything or nothing.
    EmptyValue,
    /// A regular expression rule does not compile.
    InvalidRegularExpression(String),
}

impl FileScopeError {
    /// The one-line explanation the scope editor shows.
    pub fn message(&self) -> String {
        match self {
            FileScopeError::EmptyValue => "The scope contains a rule without a value.".to_string(),
            FileScopeError::InvalidRegularExpression(_) => {
                "The scope contains an invalid regular expression.".to_string()
            }
        }
    }
}

/// A compiled file scope.
struct FileScopeMatcher {
    conjunction: Conjunction,
    rules: Vec<CompiledRule>,
}

impl FileScopeMatcher {
    fn new(scope: &FileScope) -> Result<Self, FileScopeError> {
        Ok(Self {
            conjunction: scope.conjunction,
            rules: scope
                .rules
                .iter()
                .map(CompiledRule::new)
                .collect::<Result<_, _>>()?,
        })
    }

    fn contains(&self, candidate: &Candidate, root: &Path) -> bool {
        if self.rules.is_empty() {
            return true;
        }
        let values = ScopeValues::new(candidate, root);
        match self.conjunction {
            Conjunction::Any => self.rules.iter().any(|rule| rule.matches(&values)),
            Conjunction::All => self.rules.iter().all(|rule| rule.matches(&values)),
        }
    }
}

struct CompiledRule {
    target: Target,
    comparison: Comparison,
    value: String,
    regular_expression: Option<Regex>,
}

impl CompiledRule {
    fn new(rule: &Rule) -> Result<Self, FileScopeError> {
        if rule.value.is_empty() {
            return Err(FileScopeError::EmptyValue);
        }
        let regular_expression = if rule.comparison == Comparison::MatchesRegularExpression {
            // Anchored at both ends so the pattern must match the whole value.
            // The trailing flag group is upstream's trick for letting a pattern
            // end in an extended-mode comment: the newline terminates the
            // comment before `\z`, and is itself ignored as whitespace.
            let pattern = format!("\\A(?:{}(?x)\n)\\z", rule.value);
            Some(
                Regex::new(&pattern)
                    .map_err(|error| FileScopeError::InvalidRegularExpression(error.to_string()))?,
            )
        } else {
            None
        };
        Ok(Self {
            target: rule.target,
            comparison: rule.comparison,
            value: rule.value.clone(),
            regular_expression,
        })
    }

    fn matches(&self, values: &ScopeValues) -> bool {
        let target = values.value(self.target);
        // Case-insensitive by folding both sides; upstream asks Foundation for
        // `.caseInsensitive`, which is the same comparison without a locale.
        let target_folded = target.to_lowercase();
        let value_folded = self.value.to_lowercase();

        match self.comparison {
            Comparison::Contains => target_folded.contains(&value_folded),
            Comparison::DoesNotContain => !target_folded.contains(&value_folded),
            Comparison::IsEqualTo => target_folded == value_folded,
            Comparison::IsNotEqualTo => target_folded != value_folded,
            Comparison::StartsWith => target_folded.starts_with(&value_folded),
            Comparison::EndsWith => target_folded.ends_with(&value_folded),
            Comparison::MatchesRegularExpression => self
                .regular_expression
                .as_ref()
                .is_some_and(|regex| regex.is_match(target)),
        }
    }
}

/// The three strings a rule can look at.
struct ScopeValues {
    filename: String,
    file_path: String,
    file_extension: String,
}

impl ScopeValues {
    fn new(candidate: &Candidate, root: &Path) -> Self {
        let filename = candidate
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file_extension = candidate
            .path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file_path = relative_path(&candidate.path, root);
        Self {
            filename,
            file_path,
            file_extension,
        }
    }

    fn value(&self, target: Target) -> &str {
        match target {
            Target::Filename => &self.filename,
            Target::FilePath => &self.file_path,
            Target::FileExtension => &self.file_extension,
        }
    }
}

/// The slash-separated path of `path` relative to `root`.
///
/// Falls back to the absolute path when the file is not under the root, which is
/// upstream's `searchPath(under:)`.
fn relative_path(path: &Path, root: &Path) -> String {
    let path_components = normalized_components(path);
    let root_components = normalized_components(root);
    let Some(relative) = path_components.strip_prefix(root_components.as_slice()) else {
        return path.to_string_lossy().into_owned();
    };
    relative
        .iter()
        .map(|component| component.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textual(query: &str) -> Query {
        Query::new(query, Mode::default())
    }

    // ---- the model ------------------------------------------------------

    #[test]
    fn an_empty_find_string_does_not_compile() {
        assert_eq!(
            textual("").searcher().unwrap_err(),
            QueryError::EmptyFindString
        );
    }

    #[test]
    fn a_broken_regular_expression_does_not_compile() {
        let query = Query::new(
            "([",
            Mode {
                uses_regular_expression: true,
                ignores_case: false,
            },
        );
        assert!(matches!(
            query.searcher().unwrap_err(),
            QueryError::InvalidRegularExpression(_)
        ));
    }

    #[test]
    fn a_query_is_case_sensitive_unless_asked_otherwise() {
        // Upstream's `caseInsensitiveSearch`, from both sides: folder find's
        // default is case-*sensitive*, which is the opposite of the find bar's.
        let text = "Needle\nneedle\nNEEDLE\n";
        let scope = 0..text.len();

        let searcher = textual("needle").searcher().unwrap();
        assert_eq!(searcher.find_all(text, &scope).len(), 1);

        let query = Query::new(
            "needle",
            Mode {
                uses_regular_expression: false,
                ignores_case: true,
            },
        );
        let searcher = query.searcher().unwrap();
        assert_eq!(searcher.find_all(text, &scope).len(), 3);
    }

    #[test]
    fn a_text_file_without_an_extension_is_searchable() {
        assert!(is_searchable_text(Path::new("/tmp/Makefile")));
        assert!(is_searchable_text(Path::new("/tmp/README")));
        assert!(is_searchable_text(Path::new("/tmp/notes.md")));
        assert!(is_searchable_text(Path::new("/tmp/data.plist")));
        assert!(!is_searchable_text(Path::new("/tmp/image.png")));
        assert!(!is_searchable_text(Path::new("/tmp/archive.zip")));
    }

    #[test]
    fn the_finder_ordering_reads_digit_runs_as_numbers() {
        let mut names = vec!["b.txt", "a10.txt", "a2.txt"];
        names.sort_by(|lhs, rhs| natural_compare(lhs, rhs));
        assert_eq!(names, ["a2.txt", "a10.txt", "b.txt"]);
    }

    #[test]
    fn the_finder_ordering_ignores_case_and_breaks_ties() {
        assert_eq!(natural_compare("Apple", "apple"), std::cmp::Ordering::Less);
        assert_eq!(natural_compare("a2", "a02"), std::cmp::Ordering::Less);
        assert_eq!(natural_compare("same", "same"), std::cmp::Ordering::Equal);
    }

    // ---- line display ---------------------------------------------------

    #[test]
    fn a_line_fragment_covers_the_lines_the_match_touches() {
        // Upstream's `LineRangeCalculatingTests.lineContentsRange`.
        let text = "dog \n\n cat \n cow";
        assert_eq!(line_contents_range(text, &(0..0)), 0..4);
        assert_eq!(line_contents_range(text, &(0..3)), 0..4);
        assert_eq!(line_contents_range(text, &(4..6)), 0..5);
        assert_eq!(line_contents_range(text, &(5..6)), 5..5);
        assert_eq!(line_contents_range(text, &(5..7)), 5..11);
        assert_eq!(line_contents_range(text, &(6..6)), 6..11);
        assert_eq!(line_contents_range(text, &(7..13)), 6..16);
        assert_eq!(line_contents_range(text, &(16..16)), 12..16);
    }

    #[test]
    fn a_line_fragment_handles_a_bare_carriage_return_pair() {
        let text = "\r\n";
        assert_eq!(line_contents_range(text, &(0..0)), 0..0);
        assert_eq!(line_contents_range(text, &(0..2)), 0..0);
        assert_eq!(line_contents_range(text, &(1..1)), 0..0);
        assert_eq!(line_contents_range(text, &(2..2)), 2..2);
    }

    #[test]
    fn a_long_match_is_clipped_with_its_lead_in_kept() {
        // Upstream's `longMatchIsClippedToDisplayedFragment`.
        let text = format!("{}{}{}", "x".repeat(80), "a".repeat(600), "tail");
        let matched = display_match(&text, 80..680);

        assert_eq!(matched.line.len(), MAXIMUM_LINE_LENGTH);
        assert_eq!(matched.line, "x".repeat(64) + &"a".repeat(448));
        assert_eq!(matched.range_in_line, 64..512);
    }

    #[test]
    fn a_long_match_on_a_short_line_is_not_clipped() {
        let text = "one needle two";
        let matched = display_match(text, 4..10);
        assert_eq!(matched.line, text);
        assert_eq!(matched.range_in_line, 4..10);
    }

    #[test]
    fn a_fragment_is_grown_to_whole_grapheme_clusters() {
        // Upstream's `displayedFragmentPreservesGraphemeClusters`. The cluster
        // is two code points, so a fragment cut to a byte count would land
        // between them; the fragment has to grow outward instead.
        let cluster = "e\u{301}";
        let text = format!("{}{}match{}", cluster.repeat(80), "x", cluster.repeat(300));
        let matched = display_match(&text, 241..246);

        assert!(matched.line.starts_with(cluster));
        assert!(matched.line.ends_with(cluster));
        assert_eq!(matched.range_in_line, 64..69);
        assert_eq!(&matched.line[matched.range_in_line.clone()], "match");
        assert!(!matched.line.contains('\u{FFFD}'));
    }

    #[test]
    fn a_zero_length_match_at_the_end_of_a_long_line_still_displays() {
        // Upstream's `zeroLengthMatchAtEndOfLongLine`.
        let text = "a".repeat(600);
        let matched = display_match(&text, 600..600);
        assert_eq!(matched.line.len(), 64);
        assert_eq!(matched.range_in_line, 64..64);
    }

    // ---- the walk -------------------------------------------------------

    /// A temporary directory that removes itself.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("subtext-folderfind-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("temp subdir");
            }
            std::fs::write(path, contents).expect("temp file");
        }

        fn write_bytes(&self, relative: &str, contents: &[u8]) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("temp subdir");
            }
            std::fs::write(path, contents).expect("temp file");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run(root: &Path, query: &Query, options: Options) -> Summary {
        let mut search = Search::new(root, query, options, None).expect("search");
        search.run(None, None).expect("run")
    }

    #[test]
    fn a_textual_search_groups_matches_by_file() {
        // Upstream's `textualSearchGroupsMatchesByFile`.
        let dir = TempDir::new("groups");
        dir.write("a.txt", "needle\nhay\nneedle\n");
        dir.write("Subfolder/b.txt", "hay\nneedle\n");
        dir.write("c.txt", "hay\n");

        let summary = run(&dir.0, &textual("needle"), Options::default());

        assert_eq!(summary.metrics.find_string, "needle");
        assert_eq!(summary.metrics.matched_file_count, 2);
        assert_eq!(summary.metrics.match_count, 3);
        let names: Vec<_> = summary.files.iter().map(FileResult::filename).collect();
        assert_eq!(names, ["b.txt", "a.txt"]);
        assert_eq!(summary.files[0].directory_path_components, ["Subfolder"]);
        assert!(summary.files[1].directory_path_components.is_empty());
        assert_eq!(summary.files[0].matches[0].range, 4..10);
        assert_eq!(summary.files[1].matches[0].range, 0..6);
        assert_eq!(summary.files[1].matches[1].range, 11..17);
    }

    #[test]
    fn the_walk_keeps_the_finders_directory_order() {
        // Upstream's `filteredSearchPreservesDirectoryTraversalAndOrdering`.
        let dir = TempDir::new("order");
        for name in ["Z10", "Z2", "Skip"] {
            dir.write(&format!("{name}/keep.txt"), "needle");
        }
        for name in ["a10.txt", "a2.txt", "ignore.txt", "note.md", "image.png"] {
            dir.write(name, "needle");
        }

        let options = Options {
            excluded_names: ["Skip".to_string(), "ignore.txt".to_string()]
                .into_iter()
                .collect(),
            file_scope: Some(FileScope::new(
                Conjunction::Any,
                vec![Rule::new(
                    Target::FileExtension,
                    Comparison::IsEqualTo,
                    "txt",
                )],
            )),
            ..Default::default()
        };
        let summary = run(&dir.0, &textual("needle"), options);

        let names: Vec<_> = summary.files.iter().map(FileResult::filename).collect();
        assert_eq!(names, ["keep.txt", "keep.txt", "a2.txt", "a10.txt"]);
        let paths: Vec<_> = summary
            .files
            .iter()
            .map(FileResult::directory_path)
            .collect();
        assert_eq!(paths, ["Z2", "Z10", "", ""]);
        assert_eq!(summary.metrics.match_count, 4);
    }

    #[test]
    fn a_file_listing_stops_at_its_own_limit() {
        // Upstream's `matchCountPerFileIsLimited`.
        let dir = TempDir::new("perfile");
        dir.write("a.txt", "needle needle needle needle");
        dir.write("b.txt", "needle");

        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                maximum_match_count_per_file: 3,
                ..Default::default()
            },
        );

        let names: Vec<_> = summary.files.iter().map(FileResult::filename).collect();
        assert_eq!(names, ["a.txt", "b.txt"]);
        assert_eq!(summary.files[0].matches.len(), 3);
        assert_eq!(summary.files[1].matches.len(), 1);
        assert!(summary.files[0].is_truncated);
        assert!(!summary.files[1].is_truncated);
        assert_eq!(summary.metrics.match_count, 4);
        assert_eq!(summary.metrics.matched_file_count, 2);
        assert!(summary.is_truncated());
        assert!(!summary.metrics.has_unsearched_files);
    }

    #[test]
    fn the_walk_stops_at_the_total_limit() {
        // Upstream's `totalMatchCountIsLimited`.
        let dir = TempDir::new("total");
        for name in ["a.txt", "b.txt", "c.txt"] {
            dir.write(name, "needle needle");
        }

        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                maximum_match_count: 3,
                ..Default::default()
            },
        );

        let names: Vec<_> = summary.files.iter().map(FileResult::filename).collect();
        assert_eq!(names, ["a.txt", "b.txt"]);
        assert_eq!(summary.files[0].matches.len(), 2);
        assert_eq!(summary.files[1].matches.len(), 1);
        assert!(!summary.files[0].is_truncated);
        assert!(summary.files[1].is_truncated);
        assert_eq!(summary.metrics.match_count, 3);
        assert!(summary.is_truncated());
        assert!(summary.metrics.has_unsearched_files);
    }

    #[test]
    fn reaching_the_total_limit_at_the_last_file_searches_everything() {
        // Upstream's `reachingTotalMatchCountAtLastFileSearchesAllFiles`.
        let dir = TempDir::new("last");
        dir.write("a.txt", "needle needle");
        dir.write("b.txt", "needle needle");

        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                maximum_match_count: 4,
                ..Default::default()
            },
        );

        assert_eq!(summary.files.len(), 2);
        assert_eq!(summary.metrics.match_count, 4);
        assert!(!summary.is_truncated());
    }

    #[test]
    fn the_progress_handle_tracks_the_final_metrics() {
        let dir = TempDir::new("progress");
        dir.write("a.txt", "needle\nneedle\n");
        dir.write("b.txt", "hay\nneedle\n");
        dir.write("c.txt", "hay\n");

        let progress = Progress::new("needle");
        let mut search =
            Search::new(&dir.0, &textual("needle"), Options::default(), None).expect("search");
        let summary = search.run(Some(&progress), None).expect("run");

        assert_eq!(progress.snapshot().find_string, "needle");
        assert_eq!(progress.snapshot(), summary.metrics);
    }

    #[test]
    fn a_cancelled_search_stops_without_a_summary() {
        let dir = TempDir::new("cancel");
        dir.write("a.txt", "needle");

        let cancellation = Cancellation::new();
        cancellation.cancel();
        let mut search =
            Search::new(&dir.0, &textual("needle"), Options::default(), None).expect("search");
        assert!(search.run(None, Some(&cancellation)).is_err());
    }

    #[test]
    fn hidden_files_and_excluded_names_are_not_searched() {
        // Upstream's `hiddenAndExcludedItemsAreNotSearched`.
        let dir = TempDir::new("hidden");
        dir.write("visible.txt", "needle");
        dir.write(".hidden.txt", "needle");
        dir.write(".git/config", "needle");

        let summary = run(&dir.0, &textual("needle"), Options::default());
        assert_eq!(summary.metrics.match_count, 1);

        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                includes_hidden_files: true,
                ..Default::default()
            },
        );
        assert_eq!(summary.metrics.match_count, 2);
    }

    #[test]
    fn a_binary_property_list_is_skipped() {
        // Upstream's `binaryPropertyListIsSkipped`.
        let dir = TempDir::new("bplist");
        dir.write_bytes("a.plist", b"bplist00needle");
        let summary = run(&dir.0, &textual("needle"), Options::default());
        assert_eq!(summary.metrics.match_count, 0);
    }

    #[test]
    fn a_binary_file_is_skipped() {
        // Upstream's `binaryFileIsSkipped`.
        let dir = TempDir::new("binary");
        dir.write_bytes("a.txt", b"\x00\x01needle");
        let summary = run(&dir.0, &textual("needle"), Options::default());
        assert_eq!(summary.metrics.match_count, 0);
    }

    #[test]
    fn utf16_without_a_nul_is_not_binary() {
        // Upstream's `URLTests` counterpart: a UTF-16 file has NUL bytes but is
        // still text, as long as it decodes without them.
        let dir = TempDir::new("utf16");
        let mut bytes = Vec::new();
        for unit in "needle".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        dir.write_bytes("a.txt", &bytes);

        assert!(!is_binary(&dir.0.join("a.txt")).unwrap());
        let summary = run(&dir.0, &textual("needle"), Options::default());
        assert_eq!(summary.metrics.match_count, 1);
    }

    #[test]
    fn a_file_larger_than_the_limit_is_skipped() {
        // Upstream's `fileLargerThanMaximumFileSizeIsSkipped`.
        let dir = TempDir::new("large");
        dir.write("small.txt", "needle");
        dir.write("large.txt", "needle needle");

        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                maximum_file_size: 8,
                ..Default::default()
            },
        );
        assert_eq!(summary.metrics.match_count, 1);
        let names: Vec<_> = summary.files.iter().map(FileResult::filename).collect();
        assert_eq!(names, ["small.txt"]);
    }

    #[test]
    fn a_file_that_does_not_decode_is_skipped() {
        // Upstream's `unreadableTextIsSkipped`. `0xFF` is neither valid UTF-8
        // nor a byte-order mark, and a one-byte file gives the detector nothing
        // to guess from, so SubText leaves it alone.
        let dir = TempDir::new("undecodable");
        dir.write_bytes("invalid.bin", &[0xFF]);
        dir.write("valid.txt", "needle");

        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                includes_other_file_types: true,
                ..Default::default()
            },
        );
        assert_eq!(summary.metrics.match_count, 1);
    }

    #[test]
    fn an_extra_inclusion_predicate_can_only_add_files() {
        // Upstream's `customInclusionCanSearchSyntaxMappedFiles` and
        // `customInclusionDoesNotExcludeSearchableFiles`.
        let dir = TempDir::new("included");
        dir.write("Custom.syntaxless", "needle");
        dir.write("a.txt", "needle");

        let mut search = Search::new(
            &dir.0,
            &textual("needle"),
            Options::default(),
            Some(Box::new(|path: &Path| {
                path.extension().is_some_and(|e| e == "syntaxless")
            })),
        )
        .expect("search");
        let summary = search.run(None, None).expect("run");
        assert_eq!(summary.metrics.match_count, 2);

        let mut search = Search::new(
            &dir.0,
            &textual("needle"),
            Options::default(),
            Some(Box::new(|_: &Path| false)),
        )
        .expect("search");
        let summary = search.run(None, None).expect("run");
        assert_eq!(summary.metrics.match_count, 1);
    }

    #[test]
    fn other_file_types_can_be_included() {
        // Upstream's `otherFileTypesOptionSearchesDecodableNonTextFiles`.
        let dir = TempDir::new("other");
        dir.write("a.weird", "needle");

        assert_eq!(
            run(&dir.0, &textual("needle"), Options::default())
                .metrics
                .match_count,
            0
        );
        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                includes_other_file_types: true,
                ..Default::default()
            },
        );
        assert_eq!(summary.metrics.match_count, 1);
    }

    #[test]
    fn a_regular_expression_query_is_honoured() {
        // Upstream's `regularExpressionSearch`.
        let dir = TempDir::new("regex");
        dir.write("regex.txt", "item-1\nitem-22\nitem-x\n");

        let query = Query::new(
            r"item-\d+",
            Mode {
                uses_regular_expression: true,
                ignores_case: false,
            },
        );
        let summary = run(&dir.0, &query, Options::default());
        assert_eq!(summary.metrics.match_count, 2);
        let lines: Vec<_> = summary.files[0]
            .matches
            .iter()
            .map(|m| m.line.as_str())
            .collect();
        assert_eq!(lines, ["item-1", "item-22"]);
    }

    #[test]
    fn a_zero_length_regular_expression_match_is_reported() {
        // Upstream's `zeroLengthMatchAtEndOfLongLine`, as a search rather than a
        // display question.
        let dir = TempDir::new("dollar");
        dir.write("a.txt", &"a".repeat(600));

        let query = Query::new(
            "$",
            Mode {
                uses_regular_expression: true,
                ignores_case: false,
            },
        );
        let summary = run(&dir.0, &query, Options::default());
        assert_eq!(summary.metrics.match_count, 1);
        assert_eq!(summary.files[0].matches[0].range, 600..600);
    }

    #[test]
    fn an_invalid_scope_fails_before_the_folder_is_read() {
        // Upstream's `invalidFileScopeThrowsBeforeScanning` and
        // `emptyFileScopeRuleValueThrowsBeforeScanning`.
        let broken = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(
                Target::Filename,
                Comparison::MatchesRegularExpression,
                "([",
            )],
        );
        assert!(matches!(
            scope_error(broken),
            SearchError::Scope(FileScopeError::InvalidRegularExpression(_))
        ));

        let empty = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(Target::Filename, Comparison::Contains, "")],
        );
        assert_eq!(
            scope_error(empty),
            SearchError::Scope(FileScopeError::EmptyValue)
        );
    }

    /// The error a scope raises, asserting that it raises one at all.
    ///
    /// `Search` holds a boxed predicate and so has no `Debug`, which is what
    /// `expect_err` would ask for.
    fn scope_error(scope: FileScope) -> SearchError {
        let options = Options {
            file_scope: Some(scope),
            ..Default::default()
        };
        match Search::new(Path::new("/nonexistent"), &textual("needle"), options, None) {
            Err(error) => error,
            Ok(_) => panic!("the scope should not have compiled"),
        }
    }

    #[test]
    fn an_empty_scope_does_not_filter_anything() {
        let dir = TempDir::new("emptyscope");
        dir.write("a.txt", "needle");
        let summary = run(
            &dir.0,
            &textual("needle"),
            Options {
                file_scope: Some(FileScope::default()),
                ..Default::default()
            },
        );
        assert_eq!(summary.metrics.match_count, 1);
    }

    // ---- file scopes ----------------------------------------------------

    fn scope_contains(scope: &FileScope, relative: &str, root: &Path) -> bool {
        let matcher = FileScopeMatcher::new(scope).expect("valid scope");
        let path = root.join(relative);
        let candidate = Candidate {
            path: path.clone(),
            is_directory: false,
            file_size: 0,
        };
        matcher.contains(&candidate, root)
    }

    #[test]
    fn every_scope_comparison_does_what_its_label_says() {
        // Upstream's `fileScopeRuleComparisons`.
        let root = Path::new("/root");
        let cases: [(Comparison, &str, bool); 7] = [
            (Comparison::Contains, "eed", true),
            (Comparison::Contains, "xyz", false),
            (Comparison::DoesNotContain, "xyz", true),
            (Comparison::IsEqualTo, "NEEDLE.TXT", true),
            (Comparison::IsNotEqualTo, "needle.txt", false),
            (Comparison::StartsWith, "NEED", true),
            (Comparison::EndsWith, ".TXT", true),
        ];
        for (comparison, value, expected) in cases {
            let scope = FileScope::new(
                Conjunction::Any,
                vec![Rule::new(Target::Filename, comparison, value)],
            );
            assert_eq!(
                scope_contains(&scope, "needle.txt", root),
                expected,
                "{comparison:?} {value}"
            );
        }
    }

    #[test]
    fn a_regular_expression_scope_matches_the_whole_value() {
        // Upstream's `fileScopeRegularExpressionRulesFilterCandidates` and the
        // "must match the whole value" note on `Comparison`.
        let root = Path::new("/root");
        let scope = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(
                Target::Filename,
                Comparison::MatchesRegularExpression,
                r"note.*\.md",
            )],
        );
        assert!(scope_contains(&scope, "note.md", root));
        assert!(scope_contains(&scope, "notes.md", root));
        assert!(!scope_contains(&scope, "note.md.bak", root));
    }

    #[test]
    fn a_regular_expression_scope_is_case_sensitive_unless_it_says_otherwise() {
        let root = Path::new("/root");
        let plain = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(
                Target::Filename,
                Comparison::MatchesRegularExpression,
                r"note\.md",
            )],
        );
        assert!(!scope_contains(&plain, "NOTE.md", root));

        let folded = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(
                Target::Filename,
                Comparison::MatchesRegularExpression,
                r"(?i)note\.md",
            )],
        );
        assert!(scope_contains(&folded, "NOTE.md", root));
    }

    #[test]
    fn a_regular_expression_scope_may_end_in_a_comment() {
        // The appended `(?x)` and newline are there so a pattern written in
        // extended mode can end in a comment: the newline closes the comment
        // before `\z`, which the comment would otherwise swallow.
        let root = Path::new("/root");
        let scope = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(
                Target::Filename,
                Comparison::MatchesRegularExpression,
                "(?x)note\\.md  # the notes file",
            )],
        );
        assert!(scope_contains(&scope, "note.md", root));
        assert!(!scope_contains(&scope, "other.md", root));
    }

    #[test]
    fn the_conjunction_decides_how_rules_combine() {
        // Upstream's `fileScopeConjunctionCombinesRules`.
        let root = Path::new("/root");
        let rules = vec![
            Rule::new(Target::FileExtension, Comparison::IsEqualTo, "txt"),
            Rule::new(Target::Filename, Comparison::StartsWith, "note"),
        ];

        let any = FileScope::new(Conjunction::Any, rules.clone());
        assert!(scope_contains(&any, "note.md", root));
        assert!(scope_contains(&any, "other.txt", root));
        assert!(!scope_contains(&any, "other.md", root));

        let all = FileScope::new(Conjunction::All, rules);
        assert!(scope_contains(&all, "note.txt", root));
        assert!(!scope_contains(&all, "note.md", root));
        assert!(!scope_contains(&all, "other.txt", root));
    }

    #[test]
    fn a_path_rule_reads_the_path_relative_to_the_root() {
        // Upstream's `fileScopePathTargetUsesRelativePath`.
        let root = Path::new("/root");
        let scope = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(
                Target::FilePath,
                Comparison::IsEqualTo,
                "sub/dir/note.md",
            )],
        );
        assert!(scope_contains(&scope, "sub/dir/note.md", root));
        assert!(!scope_contains(&scope, "note.md", root));
    }

    #[test]
    fn a_path_rule_falls_back_to_the_absolute_path() {
        // Upstream's `fileScopePathTargetFallsBackToAbsolutePath`.
        let root = Path::new("/root");
        let scope = FileScope::new(
            Conjunction::Any,
            vec![Rule::new(
                Target::FilePath,
                Comparison::Contains,
                "elsewhere",
            )],
        );
        assert!(scope_contains(&scope, "../elsewhere/note.md", root));
    }

    #[test]
    fn a_rule_without_a_value_is_not_valid() {
        // Upstream's `fileScopeMatcherRejectsInvalidRules`.
        assert!(!Rule::new(Target::Filename, Comparison::Contains, "").is_valid());
        assert!(Rule::new(Target::Filename, Comparison::Contains, "note").is_valid());
    }
}
