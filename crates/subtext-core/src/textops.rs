//! Text operations behind CotEditor's **Text** menu.
//!
//! Every function here takes the buffer and the current selection and returns
//! the replacement buffer plus the selection that should follow the edit, so
//! the caller can push one undoable change. Offsets are byte offsets into the
//! buffer and are always kept on `char` boundaries.

use std::ops::Range;

use unicode_normalization::UnicodeNormalization;

/// A buffer replacement: the new text and where the selection lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    pub selection: Range<usize>,
}

impl Edit {
    /// A no-op, used when an operation does not apply.
    pub fn unchanged(text: impl Into<String>, selection: Range<usize>) -> Self {
        Self { text: text.into(), selection }
    }
}

/// Comment markers for the document's syntax, used by [`toggle_comment`].
#[derive(Debug, Clone, Default)]
pub struct CommentDelimiters {
    /// Line comment prefix, e.g. `//` or `#`.
    pub inline: Option<String>,
    /// Block comment pair, e.g. `/*` and `*/`.
    pub block: Option<(String, String)>,
}

/// The lines touched by `selection`, as byte ranges.
///
/// A selection that ends exactly at the start of a line does not include that
/// line — the same rule NSTextView and CotEditor use.
pub fn selected_line_ranges(text: &str, selection: &Range<usize>) -> Vec<Range<usize>> {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let mut line_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
    if line_end < text.len() {
        // include the newline itself so line moves keep their structure
        line_end += 1;
    }
    std::iter::once(line_start..line_end).collect()
}

/// Index of the first line wholly or partially covered by `offset`.
fn line_index_at(text: &str, offset: usize) -> usize {
    text[..floor_char_boundary(text, offset)].matches('\n').count()
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Shift every selected line right by one indentation step.
pub fn shift_right(text: &str, selection: &Range<usize>, indent: &str) -> Edit {
    indent_lines(text, selection, indent)
}

/// Remove one indentation step from every selected line.
pub fn shift_left(text: &str, selection: &Range<usize>, indent: &str) -> Edit {
    let range = selection.clone();
    let (start, end) = (range.start.min(text.len()), range.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    for line in split_inclusive_newline(&text[block_start..block_end]) {
        let body = line.trim_end_matches('\n');
        let newline = &line[body.len()..];
        match strip_one_indent(body, indent) {
            Some(rest) => out.push_str(rest),
            None => out.push_str(body),
        }
        out.push_str(newline);
    }
    out.push_str(&text[block_end..]);

    // Keep the selection over the same characters where possible.
    let removed_before = removed_before(&text[..start], &text[block_start..start], indent);
    let new_start = start.saturating_sub(removed_before).max(block_start);
    let new_end = (end.saturating_sub(removed_before)).min(out.len());
    Edit { text: out, selection: new_start..new_end.max(new_start) }
}

fn removed_before(_prefix: &str, before_cursor: &str, indent: &str) -> usize {
    strip_one_indent(before_cursor, indent).map_or(0, |rest| before_cursor.len() - rest.len())
}

fn strip_one_indent<'a>(line: &'a str, indent: &'a str) -> Option<&'a str> {
    if line.is_empty() {
        return None;
    }
    if let Some(rest) = line.strip_prefix(indent) {
        return Some(rest);
    }
    // A partial indent made of the same unit still counts as one step, which
    // is what makes Shift Left feel right on a half-indented line: four spaces
    // of indent strips at most four leading spaces, one tab strips one tab.
    let unit = indent.chars().next()?;
    let mut taken = 0;
    for c in line.chars() {
        if c == unit && taken < indent.len() {
            taken += c.len_utf8();
        } else {
            break;
        }
    }
    if taken > 0 { Some(&line[taken..]) } else { None }
}

fn indent_lines(text: &str, selection: &Range<usize>, indent: &str) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let mut out = String::with_capacity(text.len() + indent.len() * 4);
    out.push_str(&text[..block_start]);
    let mut count = 0;
    for line in split_inclusive_newline(&text[block_start..block_end]) {
        // A blank line is left alone: indenting it would only create trailing
        // whitespace the user then has to clean up.
        if !line.trim_end_matches('\n').is_empty() {
            out.push_str(indent);
            count += indent.len();
        }
        out.push_str(line);
    }
    out.push_str(&text[block_end..]);

    let extra = if start == block_start { 0 } else { indent.len() };
    let new_start = start + extra;
    let new_end = (end + count).min(out.len());
    Edit { text: out, selection: new_start..new_end }
}

fn block_end_of(text: &str, end: usize) -> usize {
    let end = floor_char_boundary(text, end);
    let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
    line_end.min(text.len())
}

fn split_inclusive_newline(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            parts.push(&text[start..=i]);
            start = i + 1;
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

/// Convert the leading whitespace of the selection to spaces.
pub fn convert_indentation_to_spaces(text: &str, selection: &Range<usize>, tab_width: usize) -> Edit {
    map_selected_lines(text, selection, |line| {
        let leading = line.len() - line.trim_start_matches([' ', '\t']).len();
        let mut column = 0;
        for c in line[..leading].chars() {
            if c == '\t' {
                column += tab_width - (column % tab_width);
            } else {
                column += 1;
            }
        }
        format!("{}{}", " ".repeat(column), &line[leading..])
    })
}

/// Convert the leading whitespace of the selection to tabs.
pub fn convert_indentation_to_tabs(text: &str, selection: &Range<usize>, tab_width: usize) -> Edit {
    map_selected_lines(text, selection, |line| {
        let leading = line.len() - line.trim_start_matches([' ', '\t']).len();
        let mut column = 0;
        for c in line[..leading].chars() {
            if c == '\t' {
                column += tab_width - (column % tab_width);
            } else {
                column += 1;
            }
        }
        let tabs = column / tab_width;
        let spaces = column % tab_width;
        format!("{}{}{}", "\t".repeat(tabs), " ".repeat(spaces), &line[leading..])
    })
}

/// Wrap each selected line in `prefix` / `suffix`.
pub fn surround_lines(
    text: &str,
    selection: &Range<usize>,
    prefix: &str,
    suffix: &str,
) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let mut out = String::with_capacity(text.len() + 8);
    out.push_str(&text[..block_start]);
    for line in split_inclusive_newline(&text[block_start..block_end]) {
        let body = line.trim_end_matches('\n');
        let newline = &line[body.len()..];
        if body.is_empty() {
            out.push_str(line);
            continue;
        }
        out.push_str(prefix);
        out.push_str(body);
        out.push_str(suffix);
        out.push_str(newline);
    }
    out.push_str(&text[block_end..]);
    let new_start = start + prefix.len();
    Edit { text: out, selection: new_start..(end + prefix.len()) }
}

/// Trim whitespace at the end of every selected line.
pub fn trim_trailing_whitespace(text: &str, selection: &Range<usize>) -> Edit {
    map_selected_lines(text, selection, |line| line.trim_end().to_string())
}

fn map_selected_lines(
    text: &str,
    selection: &Range<usize>,
    mut map: impl FnMut(&str) -> String,
) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    for line in split_inclusive_newline(&text[block_start..block_end]) {
        let body = line.trim_end_matches('\n');
        let newline = &line[body.len()..];
        out.push_str(&map(body));
        out.push_str(newline);
    }
    out.push_str(&text[block_end..]);
    let delta = out.len() as isize - text.len() as isize;
    let new_end = (end as isize + delta).max(start as isize) as usize;
    let new_end = new_end.min(out.len());
    Edit { text: out, selection: start..new_end }
}

/// Toggle line or block comments over the selection.
///
/// CotEditor's rule: if every selected line already carries the inline
/// prefix, remove it; otherwise add it. When the syntax has no inline
/// comment, the block delimiters are wrapped around the whole selection
/// instead, and unwrapped if they are already there.
pub fn toggle_comment(
    text: &str,
    selection: &Range<usize>,
    delimiters: &CommentDelimiters,
) -> Edit {
    if let Some(inline) = &delimiters.inline {
        let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
        let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
        let block_end = block_end_of(text, end);
        let lines = split_inclusive_newline(&text[block_start..block_end]);

        let all_commented = !lines.is_empty()
            && lines.iter().all(|line| {
                let body = line.trim_end_matches('\n');
                body.trim_start().is_empty() || body.trim_start().starts_with(inline.as_str())
            });

        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..block_start]);
        for line in &lines {
            let body = line.trim_end_matches('\n');
            let newline = &line[body.len()..];
            if body.trim_start().is_empty() {
                out.push_str(line);
                continue;
            }
            if all_commented {
                let indent = body.len() - body.trim_start().len();
                let stripped = body[indent..].strip_prefix(inline.as_str()).unwrap_or(&body[indent..]);
                // also eat the single space CotEditor inserts after the marker
                let stripped = stripped.strip_prefix(' ').unwrap_or(stripped);
                out.push_str(&body[..indent]);
                out.push_str(stripped);
            } else {
                let indent = body.len() - body.trim_start().len();
                out.push_str(&body[..indent]);
                out.push_str(inline);
                out.push(' ');
                out.push_str(&body[indent..]);
            }
            out.push_str(newline);
        }
        out.push_str(&text[block_end..]);
        let new_end = out.len().min(text.len() + lines.len() * (inline.len() + 1));
        return Edit { text: out, selection: start..new_end };
    }

    if let Some((open, close)) = &delimiters.block {
        let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
        let selected = &text[start..end];
        let trimmed = selected.trim();
        let (new_text, delta) = if trimmed.starts_with(open.as_str())
            && trimmed.ends_with(close.as_str())
            && trimmed.len() >= open.len() + close.len()
        {
            // Strip the outer delimiters, keeping whatever padding sat
            // between them and the text.
            let inner_start = selected.find(open.as_str()).unwrap_or(0);
            let inner_end = selected.rfind(close.as_str()).unwrap_or(selected.len());
            let mut replaced = String::with_capacity(selected.len());
            replaced.push_str(&selected[..inner_start]);
            replaced.push_str(&selected[inner_start + open.len()..inner_end]);
            replaced.push_str(&selected[inner_end + close.len()..]);
            let delta = -(open.len() as isize + close.len() as isize);
            (replaced, delta)
        } else {
            (format!("{open}{selected}{close}"), (open.len() + close.len()) as isize)
        };
        let mut out = String::with_capacity(text.len() + 4);
        out.push_str(&text[..start]);
        out.push_str(&new_text);
        out.push_str(&text[end..]);
        let new_end = (end as isize + delta).max(start as isize) as usize;
        return Edit { text: out, selection: start..new_end };
    }

    Edit::unchanged(text.to_string(), selection.clone())
}

/// Move the selected lines up one line, or down one line.
pub fn move_lines(text: &str, selection: &Range<usize>, up: bool) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    if up {
        if block_start == 0 {
            return Edit::unchanged(text.to_string(), selection.clone());
        }
        let previous_start = text[..block_start - 1].rfind('\n').map_or(0, |i| i + 1);
        let previous = &text[previous_start..block_start];
        let block = &text[block_start..block_end];
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..previous_start]);
        out.push_str(block);
        out.push_str(previous);
        out.push_str(&text[block_end..]);
        let shift = block.len() as isize - previous.len() as isize;
        let new_start = (start as isize + shift) as usize;
        let new_end = (end as isize + shift) as usize;
        Edit { text: out, selection: new_start..new_end }
    } else {
        if block_end >= text.len() {
            return Edit::unchanged(text.to_string(), selection.clone());
        }
        let next_end = text[block_end..].find('\n').map_or(text.len(), |i| block_end + i + 1);
        let next = &text[block_end..next_end];
        let block = &text[block_start..block_end];
        let mut out = String::with_capacity(text.len());
        out.push_str(&text[..block_start]);
        out.push_str(next);
        out.push_str(block);
        out.push_str(&text[next_end..]);
        let shift = next.len() as isize;
        let new_start = (start as isize + shift) as usize;
        let new_end = (end as isize + shift) as usize;
        Edit { text: out, selection: new_start..new_end }
    }
}

/// Duplicate every selected line.
pub fn duplicate_lines(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = text[block_start..block_end].to_string();
    let needs_newline = !block.ends_with('\n');
    let mut inserted = block;
    if needs_newline {
        inserted.push('\n');
    }

    let mut out = String::with_capacity(text.len() + inserted.len());
    out.push_str(&text[..block_end]);
    out.push_str(&inserted);
    out.push_str(&text[block_end..]);
    let new_start = start + inserted.len();
    let new_end = end + inserted.len();
    Edit { text: out, selection: new_start..new_end }
}

/// Delete every selected line.
pub fn delete_lines(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&text[block_end..]);
    let caret = block_start.min(out.len());
    Edit { text: out, selection: caret..caret }
}

/// Join the selected lines into one, collapsing the whitespace at each break.
pub fn join_lines(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = text[block_start..block_end].trim_end_matches('\n');
    let joined = block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&joined);
    out.push_str(&text[block_end..]);
    Edit { text: out, selection: block_start..block_start + joined.len() }
}

/// Options for [`sort_lines`], mirroring CotEditor's sort sheet.
#[derive(Debug, Clone, Default)]
pub struct SortOptions {
    pub descending: bool,
    pub case_insensitive: bool,
    pub numeric: bool,
    /// A regular expression; when set, the capture group `group` is the key.
    pub pattern: Option<String>,
    pub group: usize,
}

/// Sort the selected lines.
pub fn sort_lines(text: &str, selection: &Range<usize>, options: &SortOptions) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);

    let block = &text[block_start..block_end];
    let trailing_newline = block.ends_with('\n');
    let body = block.trim_end_matches('\n');
    let mut lines: Vec<&str> = if body.is_empty() { Vec::new() } else { body.split('\n').collect() };

    let key = |line: &str| -> String {
        let mut value = line.to_string();
        if let Some(pattern) = &options.pattern
            && let Ok(re) = regex::Regex::new(pattern)
                && let Some(captures) = re.captures(line) {
                    value = captures.get(options.group).map_or(String::new(), |m| m.as_str().to_string());
                }
        if options.case_insensitive {
            value = value.to_lowercase();
        }
        value
    };

    lines.sort_by(|a, b| {
        let (ka, kb) = (key(a), key(b));
        let ordering = if options.numeric {
            let na = ka.trim().parse::<f64>().ok();
            let nb = kb.trim().parse::<f64>().ok();
            match (na, nb) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => ka.cmp(&kb),
            }
        } else {
            ka.cmp(&kb)
        };
        if options.descending { ordering.reverse() } else { ordering }
    });

    let mut joined = lines.join("\n");
    if trailing_newline && !joined.is_empty() {
        joined.push('\n');
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&joined);
    out.push_str(&text[block_end..]);
    Edit { text: out, selection: block_start..block_start + joined.len() }
}

/// Reverse the order of the selected lines.
pub fn reverse_lines(text: &str, selection: &Range<usize>) -> Edit {
    let options = SortOptions::default();
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = &text[block_start..block_end];
    let trailing_newline = block.ends_with('\n');
    let body = block.trim_end_matches('\n');
    let mut lines: Vec<&str> = if body.is_empty() { Vec::new() } else { body.split('\n').collect() };
    lines.reverse();
    let mut joined = lines.join("\n");
    if trailing_newline && !joined.is_empty() {
        joined.push('\n');
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&joined);
    out.push_str(&text[block_end..]);
    let _ = options;
    Edit { text: out, selection: block_start..block_start + joined.len() }
}

/// Shuffle the selected lines with a caller-supplied random source.
pub fn shuffle_lines(text: &str, selection: &Range<usize>, mut random: impl FnMut(usize) -> usize) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = &text[block_start..block_end];
    let trailing_newline = block.ends_with('\n');
    let body = block.trim_end_matches('\n');
    let mut lines: Vec<&str> = if body.is_empty() { Vec::new() } else { body.split('\n').collect() };

    // Fisher-Yates.
    for i in (1..lines.len()).rev() {
        let j = random(i + 1) % (i + 1);
        lines.swap(i, j);
    }

    let mut joined = lines.join("\n");
    if trailing_newline && !joined.is_empty() {
        joined.push('\n');
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&joined);
    out.push_str(&text[block_end..]);
    Edit { text: out, selection: block_start..block_start + joined.len() }
}

/// Remove duplicate lines, keeping the first occurrence.
pub fn delete_duplicate_lines(text: &str, selection: &Range<usize>, case_insensitive: bool) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let block_start = text[..floor_char_boundary(text, start)].rfind('\n').map_or(0, |i| i + 1);
    let block_end = block_end_of(text, end);
    let block = &text[block_start..block_end];
    let trailing_newline = block.ends_with('\n');
    let body = block.trim_end_matches('\n');
    let lines: Vec<&str> = if body.is_empty() { Vec::new() } else { body.split('\n').collect() };

    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::with_capacity(lines.len());
    for line in lines {
        let key = if case_insensitive { line.to_lowercase() } else { line.to_string() };
        if seen.insert(key) {
            kept.push(line);
        }
    }

    let mut joined = kept.join("\n");
    if trailing_newline && !joined.is_empty() {
        joined.push('\n');
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..block_start]);
    out.push_str(&joined);
    out.push_str(&text[block_end..]);
    Edit { text: out, selection: block_start..block_start + joined.len() }
}

/// Which case transformation to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseTransform {
    Upper,
    Lower,
    Capitalize,
}

/// Change the case of the selection.
pub fn transform_case(text: &str, selection: &Range<usize>, transform: CaseTransform) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let selected = &text[start..end];
    let replaced = match transform {
        CaseTransform::Upper => selected.to_uppercase(),
        CaseTransform::Lower => selected.to_lowercase(),
        CaseTransform::Capitalize => capitalize_words(selected),
    };
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// Uppercase the first letter of each word, leaving the rest untouched —
/// CotEditor's "Capitalize", which is *not* title-casing.
fn capitalize_words(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut at_word_start = true;
    for c in input.chars() {
        if c.is_alphanumeric() {
            if at_word_start {
                out.extend(c.to_uppercase());
                at_word_start = false;
            } else {
                out.push(c);
            }
        } else {
            at_word_start = true;
            out.push(c);
        }
    }
    out
}

/// Which width conversion to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthTransform {
    /// Every character, including punctuation and katakana.
    Full,
    /// Only ASCII letters and digits.
    FullAlphanumeric,
    Half,
    HalfAlphanumeric,
}

/// Convert between full-width and half-width forms.
pub fn transform_width(text: &str, selection: &Range<usize>, transform: WidthTransform) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let selected = &text[start..end];
    let replaced = match transform {
        WidthTransform::Full => to_full_width(selected, true),
        WidthTransform::FullAlphanumeric => to_full_width(selected, false),
        WidthTransform::Half => to_half_width(selected, true),
        WidthTransform::HalfAlphanumeric => to_half_width(selected, false),
    };
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

fn to_full_width(input: &str, include_kana: bool) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for c in input.chars() {
        match c {
            ' ' => out.push('\u{3000}'),
            '\u{21}'..='\u{7E}' => {
                out.push(char::from_u32(c as u32 + 0xFEE0).unwrap_or(c));
            }
            '\u{FF61}'..='\u{FF9F}' if include_kana => {
                out.push_str(&half_kana_to_full(c));
            }
            _ => out.push(c),
        }
    }
    out
}

fn to_half_width(input: &str, include_kana: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\u{3000}' => out.push(' '),
            '\u{FF01}'..='\u{FF5E}' => {
                out.push(char::from_u32(c as u32 - 0xFEE0).unwrap_or(c));
            }
            '\u{3041}'..='\u{30FF}' if include_kana => {
                out.push_str(&full_kana_to_half(c));
            }
            _ => out.push(c),
        }
    }
    out
}

/// Half-width katakana, with the dakuten/handakuten forms expanded.
fn half_kana_to_full(c: char) -> String {
    const TABLE: [(char, &str); 63] = [
        ('\u{FF61}', "。"), ('\u{FF62}', "「"), ('\u{FF63}', "」"), ('\u{FF64}', "、"),
        ('\u{FF65}', "・"), ('\u{FF66}', "ヲ"), ('\u{FF67}', "ァ"), ('\u{FF68}', "ィ"),
        ('\u{FF69}', "ゥ"), ('\u{FF6A}', "ェ"), ('\u{FF6B}', "ォ"), ('\u{FF6C}', "ャ"),
        ('\u{FF6D}', "ュ"), ('\u{FF6E}', "ョ"), ('\u{FF6F}', "ッ"), ('\u{FF70}', "ー"),
        ('\u{FF71}', "ア"), ('\u{FF72}', "イ"), ('\u{FF73}', "ウ"), ('\u{FF74}', "エ"),
        ('\u{FF75}', "オ"), ('\u{FF76}', "カ"), ('\u{FF77}', "キ"), ('\u{FF78}', "ク"),
        ('\u{FF79}', "ケ"), ('\u{FF7A}', "コ"), ('\u{FF7B}', "サ"), ('\u{FF7C}', "シ"),
        ('\u{FF7D}', "ス"), ('\u{FF7E}', "セ"), ('\u{FF7F}', "ソ"), ('\u{FF80}', "タ"),
        ('\u{FF81}', "チ"), ('\u{FF82}', "ツ"), ('\u{FF83}', "テ"), ('\u{FF84}', "ト"),
        ('\u{FF85}', "ナ"), ('\u{FF86}', "ニ"), ('\u{FF87}', "ヌ"), ('\u{FF88}', "ネ"),
        ('\u{FF89}', "ノ"), ('\u{FF8A}', "ハ"), ('\u{FF8B}', "ヒ"), ('\u{FF8C}', "フ"),
        ('\u{FF8D}', "ヘ"), ('\u{FF8E}', "ホ"), ('\u{FF8F}', "マ"), ('\u{FF90}', "ミ"),
        ('\u{FF91}', "ム"), ('\u{FF92}', "メ"), ('\u{FF93}', "モ"), ('\u{FF94}', "ヤ"),
        ('\u{FF95}', "ユ"), ('\u{FF96}', "ヨ"), ('\u{FF97}', "ラ"), ('\u{FF98}', "リ"),
        ('\u{FF99}', "ル"), ('\u{FF9A}', "レ"), ('\u{FF9B}', "ロ"), ('\u{FF9C}', "ワ"),
        ('\u{FF9D}', "ン"), ('\u{FF9E}', "\u{3099}"), ('\u{FF9F}', "\u{309A}"),
    ];
    for (from, to) in TABLE {
        if from == c {
            return to.to_string();
        }
    }
    c.to_string()
}

fn full_kana_to_half(c: char) -> String {
    // Reverse of the table above, applied to the voiced forms first.
    const VOICED: [(char, &str); 21] = [
        ('ガ', "ｶﾞ"), ('ギ', "ｷﾞ"), ('グ', "ｸﾞ"), ('ゲ', "ｹﾞ"), ('ゴ', "ｺﾞ"),
        ('ザ', "ｻﾞ"), ('ジ', "ｼﾞ"), ('ズ', "ｽﾞ"), ('ゼ', "ｾﾞ"), ('ゾ', "ｿﾞ"),
        ('ダ', "ﾀﾞ"), ('ヂ', "ﾁﾞ"), ('ヅ', "ﾂﾞ"), ('デ', "ﾃﾞ"), ('ド', "ﾄﾞ"),
        ('バ', "ﾊﾞ"), ('ビ', "ﾋﾞ"), ('ブ', "ﾌﾞ"), ('ベ', "ﾍﾞ"), ('ボ', "ﾎﾞ"),
        ('ヴ', "ｳﾞ"),
    ];
    const SEMI: [(char, &str); 5] =
        [('パ', "ﾊﾟ"), ('ピ', "ﾋﾟ"), ('プ', "ﾌﾟ"), ('ペ', "ﾍﾟ"), ('ポ', "ﾎﾟ")];
    const PLAIN: [(char, &str); 56] = [
        ('。', "｡"), ('「', "｢"), ('」', "｣"), ('、', "､"), ('・', "･"), ('ヲ', "ｦ"),
        ('ァ', "ｧ"), ('ィ', "ｨ"), ('ゥ', "ｩ"), ('ェ', "ｪ"), ('ォ', "ｫ"), ('ャ', "ｬ"),
        ('ュ', "ｭ"), ('ョ', "ｮ"), ('ッ', "ｯ"), ('ー', "ｰ"), ('ア', "ｱ"), ('イ', "ｲ"),
        ('ウ', "ｳ"), ('エ', "ｴ"), ('オ', "ｵ"), ('カ', "ｶ"), ('キ', "ｷ"), ('ク', "ｸ"),
        ('ケ', "ｹ"), ('コ', "ｺ"), ('サ', "ｻ"), ('シ', "ｼ"), ('ス', "ｽ"), ('セ', "ｾ"),
        ('ソ', "ｿ"), ('タ', "ﾀ"), ('チ', "ﾁ"), ('ツ', "ﾂ"), ('テ', "ﾃ"), ('ト', "ﾄ"),
        ('ナ', "ﾅ"), ('ニ', "ﾆ"), ('ヌ', "ﾇ"), ('ネ', "ﾈ"), ('ノ', "ﾉ"), ('ハ', "ﾊ"),
        ('ヒ', "ﾋ"), ('フ', "ﾌ"), ('ヘ', "ﾍ"), ('ホ', "ﾎ"), ('マ', "ﾏ"), ('ミ', "ﾐ"),
        ('ム', "ﾑ"), ('メ', "ﾒ"), ('モ', "ﾓ"), ('ヤ', "ﾔ"), ('ユ', "ﾕ"), ('ヨ', "ﾖ"),
        ('ラ', "ﾗ"), ('リ', "ﾘ"),
    ];
    for (from, to) in VOICED.iter().chain(SEMI.iter()).chain(PLAIN.iter()) {
        if *from == c {
            return (*to).to_string();
        }
    }
    // Hiragana shares its codepoints' shapes; map the common range by offset.
    if ('\u{3041}'..='\u{3093}').contains(&c) {
        let katakana = char::from_u32(c as u32 + 0x60).unwrap_or(c);
        if katakana != c {
            return full_kana_to_half(katakana);
        }
    }
    // The tail of the katakana block that the table above omits.
    const TAIL: [(char, &str); 13] = [
        ('ル', "ﾙ"), ('レ', "ﾚ"), ('ロ', "ﾛ"), ('ワ', "ﾜ"), ('ン', "ﾝ"), ('゛', "ﾞ"),
        ('゜', "ﾟ"), ('ヮ', "ﾜ"), ('ヰ', "ｲ"), ('ヱ', "ｴ"), ('ヵ', "ｶ"), ('ヶ', "ｹ"),
        ('ヽ', "ﾂ"),
    ];
    for (from, to) in TAIL {
        if from == c {
            return to.to_string();
        }
    }
    c.to_string()
}

/// Percent-encode the selection (RFC 3986 unreserved characters are kept).
pub fn url_encode(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let mut replaced = String::with_capacity((end - start) * 3);
    for byte in &text.as_bytes()[start..end] {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                replaced.push(*byte as char);
            }
            _ => replaced.push_str(&format!("%{byte:02X}")),
        }
    }
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// Decode percent-escapes in the selection.
pub fn url_decode(text: &str, selection: &Range<usize>) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let bytes = &text.as_bytes()[start..end];
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                decoded.push(value);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let replaced = String::from_utf8_lossy(&decoded).into_owned();
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// Which Unicode normalization form to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Normalization {
    /// Canonical decomposition.
    Nfd,
    /// Canonical composition.
    Nfc,
    /// Compatibility decomposition.
    Nfkd,
    /// Compatibility composition.
    Nfkc,
}

pub fn normalize(text: &str, selection: &Range<usize>, form: Normalization) -> Edit {
    let (start, end) = (selection.start.min(text.len()), selection.end.min(text.len()));
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end);
    if start == end {
        return Edit::unchanged(text.to_string(), selection.clone());
    }
    let selected = &text[start..end];
    let replaced: String = match form {
        Normalization::Nfd => selected.nfd().collect(),
        Normalization::Nfc => selected.nfc().collect(),
        Normalization::Nfkd => selected.nfkd().collect(),
        Normalization::Nfkc => selected.nfkc().collect(),
    };
    let mut out = String::with_capacity(text.len() + replaced.len());
    out.push_str(&text[..start]);
    out.push_str(&replaced);
    out.push_str(&text[end..]);
    Edit { text: out, selection: start..start + replaced.len() }
}

/// The line number (1-based) containing `offset`, and the column (1-based).
pub fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let line = line_index_at(text, offset) + 1;
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let column = text[line_start..offset].chars().count() + 1;
    (line, column)
}

/// Byte offset of the start of 1-based `line`.
pub fn offset_of_line(text: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut current = 1;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            current += 1;
            if current == line {
                return i + 1;
            }
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole document selected — what the Text-menu commands operate on
    /// when the user has picked everything.
    fn all(text: &str) -> Range<usize> {
        0..text.len()
    }

    #[test]
    fn shift_right_then_left_is_a_round_trip() {
        let text = "one\ntwo\n";
        let right = shift_right(text, &all(text), "    ");
        assert_eq!(right.text, "    one\n    two\n");
        let left = shift_left(&right.text, &right.selection, "    ");
        assert_eq!(left.text, text);
    }

    #[test]
    fn shift_left_strips_a_partial_indent() {
        let text = "  one\n";
        assert_eq!(shift_left(text, &all(text), "    ").text, "one\n");
    }

    #[test]
    fn shift_right_skips_blank_lines() {
        let text = "one\n\ntwo";
        assert_eq!(shift_right(text, &all(text), "  ").text, "  one\n\n  two");
    }

    #[test]
    fn comment_toggle_adds_then_removes() {
        let delimiters = CommentDelimiters { inline: Some("//".into()), block: None };
        let text = "let a = 1;\nlet b = 2;";
        let commented = toggle_comment(text, &all(text), &delimiters);
        assert_eq!(commented.text, "// let a = 1;\n// let b = 2;");
        let restored = toggle_comment(&commented.text, &all(&commented.text), &delimiters);
        assert_eq!(restored.text, text);
    }

    #[test]
    fn comment_toggle_preserves_indentation() {
        let delimiters = CommentDelimiters { inline: Some("#".into()), block: None };
        let text = "def f():\n    return 1";
        assert_eq!(
            toggle_comment(text, &all(text), &delimiters).text,
            "# def f():\n    # return 1"
        );
    }

    #[test]
    fn block_comment_wraps_and_unwraps() {
        let delimiters = CommentDelimiters { inline: None, block: Some(("/*".into(), "*/".into())) };
        let text = "abc";
        let wrapped = toggle_comment(text, &(0..3), &delimiters);
        assert_eq!(wrapped.text, "/*abc*/");
        let unwrapped = toggle_comment(&wrapped.text, &(0..wrapped.text.len()), &delimiters);
        assert_eq!(unwrapped.text, "abc");
    }

    #[test]
    fn move_lines_up_and_down_are_inverses() {
        let text = "a\nb\nc";
        let down = move_lines(text, &(0..1), false);
        assert_eq!(down.text, "b\na\nc");
        let up = move_lines(&down.text, &(2..3), true);
        assert_eq!(up.text, text);
    }

    #[test]
    fn duplicate_and_delete_line() {
        let text = "a\nb";
        let dup = duplicate_lines(text, &(0..1));
        assert_eq!(dup.text, "a\na\nb");
        let del = delete_lines(text, &(0..1));
        assert_eq!(del.text, "b");
    }

    #[test]
    fn join_lines_collapses_breaks() {
        let text = "one\n   two\nthree";
        assert_eq!(join_lines(text, &all(text)).text, "one two three");
    }

    #[test]
    fn sort_lines_ascending_and_descending() {
        let text = "banana\napple\ncherry";
        let options = SortOptions::default();
        assert_eq!(sort_lines(text, &all(text), &options).text, "apple\nbanana\ncherry");
        let desc = SortOptions { descending: true, ..Default::default() };
        assert_eq!(sort_lines(text, &all(text), &desc).text, "cherry\nbanana\napple");
    }

    #[test]
    fn sort_lines_numeric_orders_by_value() {
        let text = "10\n9\n100";
        let options = SortOptions { numeric: true, ..Default::default() };
        assert_eq!(sort_lines(text, &all(text), &options).text, "9\n10\n100");
    }

    #[test]
    fn delete_duplicates_keeps_first() {
        let text = "a\nb\na\nB";
        assert_eq!(delete_duplicate_lines(text, &all(text), false).text, "a\nb\nB");
        assert_eq!(delete_duplicate_lines(text, &all(text), true).text, "a\nb");
    }

    #[test]
    fn case_transforms() {
        let text = "hello WORLD";
        assert_eq!(
            transform_case(text, &(0..5), CaseTransform::Upper).text,
            "HELLO WORLD"
        );
        assert_eq!(
            transform_case(text, &(0..11), CaseTransform::Capitalize).text,
            "Hello WORLD"
        );
    }

    #[test]
    fn width_transforms_round_trip_ascii() {
        let text = "ABC 123";
        let full = transform_width(text, &(0..text.len()), WidthTransform::Full);
        assert_eq!(full.text, "ＡＢＣ　１２３");
        let half = transform_width(&full.text, &(0..full.text.len()), WidthTransform::Half);
        assert_eq!(half.text, text);
    }

    #[test]
    fn half_width_katakana_becomes_full_width() {
        let text = "ｶﾀｶﾅ";
        let full = transform_width(text, &(0..text.len()), WidthTransform::Full);
        assert_eq!(full.text, "カタカナ");
    }

    #[test]
    fn url_encode_decode_round_trip() {
        let text = "a b&c";
        let encoded = url_encode(text, &(0..text.len()));
        assert_eq!(encoded.text, "a%20b%26c");
        let decoded = url_decode(&encoded.text, &(0..encoded.text.len()));
        assert_eq!(decoded.text, text);
    }

    #[test]
    fn normalize_nfc_composes() {
        let text = "e\u{301}";
        let nfc = normalize(text, &(0..text.len()), Normalization::Nfc);
        assert_eq!(nfc.text, "é");
    }

    #[test]
    fn surround_and_trim() {
        let text = "a  \nb  ";
        assert_eq!(trim_trailing_whitespace(text, &all(text)).text, "a\nb");
        assert_eq!(surround_lines(text, &all(text), "\"", "\"").text, "\"a  \"\n\"b  \"");
    }

    #[test]
    fn line_and_column_round_trip() {
        let text = "ab\ncde\nf";
        assert_eq!(line_and_column(text, 0), (1, 1));
        assert_eq!(line_and_column(text, 3), (2, 1));
        assert_eq!(line_and_column(text, 5), (2, 3));
        assert_eq!(offset_of_line(text, 2), 3);
        assert_eq!(offset_of_line(text, 3), 7);
    }

    #[test]
    fn indentation_conversion_respects_tab_width() {
        let text = "\tx\n  y";
        assert_eq!(
            convert_indentation_to_spaces(text, &all(text), 4).text,
            "    x\n  y"
        );
        assert_eq!(
            convert_indentation_to_tabs("        x", &all("        x"), 4).text,
            "\t\tx"
        );
    }
}
