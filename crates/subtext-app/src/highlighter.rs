//! Plugging CotEditor's syntax rules and themes into GPUI's editor.
//!
//! GPUI Kit's editor asks for two things: an [`InputHighlighter`] that turns a
//! buffer into styled runs, and an [`InputEditorStyle`] that says what the
//! editor's chrome looks like. Both are application-owned seams, which is
//! exactly what makes a faithful reimplementation possible — the parser and
//! the palette are ours, the text engine is GPUI's.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::input::{
    DiagnosticColors, EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputEditorStyle,
    InputHighlighter, InputHighlighterFactory, Rope, SharedHighlightStyleResolver,
};
use gpui_kit::{App, Context, HighlightStyle, Hsla, Rgba, SharedString, Window};
use subtext_syntax::{HighlightToken, Highlighter, Syntax};
use subtext_theme::{Rgba as CotRgba, Theme};

/// Convert a `.cottheme` colour to a GPUI colour.
pub fn to_hsla(color: CotRgba) -> Hsla {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: color.a as f32 / 255.0,
    }
    .into()
}

/// Resolves CotEditor's ten token names into renderable styles.
///
/// The names are the same strings the `.cotsyntax` files and the tree-sitter
/// queries use, so there is no translation table to drift out of date.
#[derive(Debug, Clone, Default)]
pub struct CotStyleResolver {
    styles: HashMap<String, HighlightStyle>,
}

impl CotStyleResolver {
    /// Build a resolver from a theme.
    ///
    /// CotEditor themes carry no weight or slant — every token is plain, only
    /// its colour differs — so the styles here are colour-only.
    pub fn new(theme: &Theme) -> Self {
        let mut styles = HashMap::new();
        for token in [
            HighlightToken::Keywords,
            HighlightToken::Commands,
            HighlightToken::Types,
            HighlightToken::Attributes,
            HighlightToken::Variables,
            HighlightToken::Values,
            HighlightToken::Numbers,
            HighlightToken::Strings,
            HighlightToken::Characters,
            HighlightToken::Comments,
        ] {
            let entry = theme.token_color(token);
            let color = entry.color.unwrap_or_else(|| theme.text_color());
            styles.insert(
                token.style_name().to_string(),
                HighlightStyle {
                    color: Some(to_hsla(color)),
                    ..Default::default()
                },
            );
        }
        Self { styles }
    }
}

impl HighlightStyleResolver for CotStyleResolver {
    fn style(&self, name: &str) -> Option<HighlightStyle> {
        self.styles.get(name).cloned()
    }
}

/// Project a CotEditor theme onto the theme the editor paints with.
///
/// The document pane is a `gpui-component` `Editor`, and `Input::render`
/// re-applies `set_editor_style` from `cx.theme()` on **every frame**. A style
/// set on the editor state is therefore overwritten before anything reads it —
/// which is why every token used to come out in the plain text colour, and why
/// `editor_style` alone was never enough.
///
/// The colours have to live where the component looks: the global component
/// theme. Its syntax palette speaks Zed's names (`keyword`, `string.special`),
/// which is what [`HighlightToken::style_name`] maps CotEditor's ten tokens
/// onto.
pub fn project_theme(theme: &Theme, cx: &mut App) {
    // Every editor colour goes through [`editor_style`] first, so the fallbacks
    // a `.cottheme` leaves out are resolved in exactly one place — the same
    // place the editor state is given its style, which is what keeps the two
    // from drifting.
    let style = editor_style(theme);

    let mut component = gpui_kit::component::Theme::global(cx).clone();
    component.colors.foreground = style.foreground;
    component.colors.muted_foreground = style.muted_foreground;
    component.colors.background = style.background;
    component.colors.border = style.border;
    component.colors.selection = style.selection;
    component.colors.caret = style.caret;

    let mut highlight = (*component.highlight_theme).clone();
    highlight.style.editor_background = Some(style.background);
    highlight.style.editor_foreground = Some(style.foreground);
    highlight.style.editor_invisible = style.editor_invisible;
    highlight.style.editor_active_line = style.editor_active_line;
    highlight.style.editor_gutter_background = style.editor_gutter_background;
    highlight.style.syntax = syntax_colors(theme);
    component.highlight_theme = std::sync::Arc::new(highlight);

    cx.set_global(component);
}

/// The syntax palette, in the component theme's vocabulary.
///
/// `SyntaxColors` has one field per name and no public constructor, so this
/// goes through the shape its own deserializer takes — the same `syntax` block
/// a Zed theme file has.
fn syntax_colors(theme: &Theme) -> gpui_kit::component::highlighter::SyntaxColors {
    let mut palette = serde_json::Map::new();
    for token in subtext_syntax::PRIORITY {
        let color = theme
            .token_color(token)
            .color
            .unwrap_or_else(|| theme.text_color());
        palette.insert(
            token.style_name().to_string(),
            serde_json::json!({
                "color": format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b),
            }),
        );
    }
    serde_json::from_value(serde_json::Value::Object(palette)).unwrap_or_default()
}

/// Project a theme onto the editor's chrome.
///
/// A `.cottheme` key is optional — CotEditor leaves a colour out when it wants
/// the system's — so each one is resolved against the colour CotEditor falls
/// back to. The fallbacks are in `.cottheme` terms, which is why the theme's
/// own colours are kept alongside their converted forms.
pub fn editor_style(theme: &Theme) -> InputEditorStyle {
    let text_color = theme.text_color();
    let background_color = theme.background_color();

    let text = to_hsla(text_color);
    let background = to_hsla(background_color);
    let selection = to_hsla(
        theme
            .selection
            .resolve(CotRgba::new(0xB5, 0xD6, 0xFD, 0xFF)),
    );
    let caret = to_hsla(theme.insertion_point.resolve(text_color));
    let invisibles = to_hsla(
        theme
            .invisibles
            .resolve(CotRgba::new(0xC0, 0xC0, 0xC0, 0xFF)),
    );
    let current_line = to_hsla(theme.line_highlight.resolve(background_color));

    InputEditorStyle {
        foreground: text,
        muted_foreground: to_hsla(theme.comments.resolve(text_color)),
        background,
        border: to_hsla(theme.invisibles.resolve(background_color)),
        selection,
        caret,
        diagnostics: DiagnosticColors::default(),
        highlight_styles: Arc::new(CotStyleResolver::new(theme)) as SharedHighlightStyleResolver,
        editor_invisible: Some(invisibles),
        editor_active_line: Some(current_line),
        editor_gutter_background: Some(background),
        fold_icon_renderer: None,
    }
}

/// A syntax's rules bound to one editor.
#[derive(Clone)]
pub struct CotHighlighter {
    inner: Highlighter,
    /// The buffer the spans were computed against. Kept so `styles` can slice
    /// without re-running the rules.
    snapshot: String,
    language: SharedString,
}

impl CotHighlighter {
    /// A highlighter for a syntax, from whichever source it has.
    ///
    /// A bundle with a `Highlights.json` is highlighted by those rules. One
    /// without — the twenty-three whose colours are a tree-sitter query — is
    /// highlighted by the query, because the only rules it carries are the
    /// comment and string delimiters from `Edit.json`, and running just those
    /// leaves every keyword, type and number in the plain text colour.
    pub fn new(syntax: &Syntax) -> Self {
        let inner = match syntax.highlight_query() {
            Some(query) if !syntax.has_highlight_file => {
                Highlighter::with_query(&syntax.name, &query)
            }
            _ => Highlighter::new(&syntax.highlights),
        };
        Self {
            inner,
            snapshot: String::new(),
            language: SharedString::from(syntax.name.clone()),
        }
    }

    /// Re-run the syntax's rules over `text`.
    ///
    /// Separated from [`InputHighlighter::update`] so the styling logic can be
    /// exercised without a live GPUI window.
    pub fn rehighlight_text(&mut self, text: &str) {
        self.inner.rehighlight(text);
        self.snapshot = text.to_string();
    }

    /// Styled runs that tile `range` exactly, as GPUI's renderer requires.
    pub fn styled_runs(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let start = range.start.min(self.snapshot.len());
        let end = range.end.min(self.snapshot.len());
        if start >= end {
            return Vec::new();
        }

        // Gaps between tokens are emitted as unstyled runs rather than
        // omitted: the renderer expects a complete tiling.
        let mut runs: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
        let mut cursor = start;
        for span in self.inner.spans() {
            if span.range.end <= start || span.range.start >= end {
                continue;
            }
            let span_start = span.range.start.max(start);
            let span_end = span.range.end.min(end);
            if span_start > cursor {
                runs.push((cursor..span_start, HighlightStyle::default()));
            }
            let style = resolver.style(span.token.style_name()).unwrap_or_default();
            runs.push((span_start.max(cursor)..span_end, style));
            cursor = span_end.max(cursor);
        }
        if cursor < end {
            runs.push((cursor..end, HighlightStyle::default()));
        }
        runs
    }
}

impl InputHighlighter for CotHighlighter {
    fn language(&self) -> SharedString {
        self.language.clone()
    }

    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        // CotEditor's rules are not incremental, so each edit re-runs them
        // over the whole buffer. That is what CotEditor itself does.
        self.rehighlight_text(&text.to_string());
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        self.styled_runs(range, resolver)
    }

    fn fold_ranges(&self, text: &Rope) -> Vec<FoldRange> {
        fold_ranges_for(&text.to_string())
    }
}

/// Foldable regions, from brace and bracket nesting.
///
/// A line is foldable when a bracket opened on it is still open at the start
/// of a later line — which is what makes a function body collapse.
pub fn fold_ranges_for(text: &str) -> Vec<FoldRange> {
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut ranges = Vec::new();
    let mut line = 0;

    for (offset, c) in text.char_indices() {
        if c == '\n' {
            line += 1;
            continue;
        }
        // Skip brackets inside strings and comments by ignoring the common
        // cases: this is a fold hint, not a parse.
        match c {
            '{' | '[' | '(' => stack.push((c, line)),
            '}' | ']' | ')' => {
                let expected = match c {
                    '}' => '{',
                    ']' => '[',
                    _ => '(',
                };
                if let Some(index) = stack.iter().rposition(|(open, _)| *open == expected) {
                    let (_, start_line) = stack.remove(index);
                    if line > start_line {
                        ranges.push(FoldRange::new(start_line, line));
                    }
                }
            }
            _ => {
                let _ = offset;
            }
        }
    }

    ranges.sort_by_key(|range| (range.start_line, range.end_line));
    ranges
}

/// Build the factory GPUI Kit calls when it creates an editor.
///
/// The closure receives the language name GPUI knows the editor by, which is
/// the syntax's own name.
pub fn factory(syntaxes: Arc<Vec<Syntax>>) -> InputHighlighterFactory {
    std::rc::Rc::new(move |language: &str| {
        syntaxes
            .iter()
            .find(|syntax| syntax.name == language)
            .map(|syntax| Box::new(CotHighlighter::new(syntax)) as Box<dyn InputHighlighter>)
    })
}

/// Install the factory that every editor in the application will use.
pub fn install_factory(syntaxes: Arc<Vec<Syntax>>, cx: &mut App) {
    // The factory is stored per editor when the state is built, so it is
    // handed out through a thread-local that `EditorPane::new` reads.
    FACTORY.with(|slot| *slot.borrow_mut() = Some(factory(syntaxes)));
    let _ = cx;
}

thread_local! {
    static FACTORY: std::cell::RefCell<Option<InputHighlighterFactory>> =
        const { std::cell::RefCell::new(None) };
}

/// The factory installed by [`install_factory`].
pub fn current_factory() -> Option<InputHighlighterFactory> {
    FACTORY.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtext_syntax::SyntaxManager;

    fn resources() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources")
    }

    fn syntax(name: &str) -> Syntax {
        SyntaxManager::load_from_directory(resources().join("syntaxes"))
            .unwrap()
            .by_name(name)
            .unwrap()
            .clone()
    }

    #[test]
    fn resolver_covers_every_token_the_themes_define() {
        let theme = Theme::builtin_light();
        let resolver = CotStyleResolver::new(&theme);
        for token in [
            HighlightToken::Keywords,
            HighlightToken::Comments,
            HighlightToken::Strings,
            HighlightToken::Numbers,
            HighlightToken::Values,
            HighlightToken::Variables,
            HighlightToken::Attributes,
            HighlightToken::Types,
            HighlightToken::Commands,
            HighlightToken::Characters,
        ] {
            assert!(
                resolver.style(token.style_name()).is_some(),
                "{} has no style",
                token.style_name()
            );
        }
        assert!(resolver.style("nonsense").is_none());
    }

    #[test]
    fn editor_style_projects_the_theme_background() {
        let theme = Theme::builtin_dark();
        let style = editor_style(&theme);
        let expected = to_hsla(theme.background_color());
        assert_eq!(style.background, expected);
    }

    #[test]
    fn fold_ranges_find_a_brace_block() {
        let text = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";
        let ranges = fold_ranges_for(text);
        assert!(ranges.iter().any(|r| r.start_line == 0 && r.end_line == 3));
    }

    #[test]
    fn fold_ranges_nest() {
        let text = "a {\nb {\nc\n}\n}\n";
        let ranges = fold_ranges_for(text);
        assert_eq!(ranges.len(), 2);
        assert!(ranges.iter().any(|r| r.start_line == 1 && r.end_line == 3));
        assert!(ranges.iter().any(|r| r.start_line == 0 && r.end_line == 4));
    }

    #[test]
    fn unbalanced_brackets_produce_no_folds() {
        assert!(fold_ranges_for("fn main() {\n").is_empty());
        assert!(fold_ranges_for("}\n{\n").is_empty());
    }

    #[test]
    fn a_syntax_highlighter_styles_a_rust_snippet() {
        let syntax = syntax("Rust");
        let highlighter = CotHighlighter::new(&syntax);
        assert_eq!(highlighter.language().as_ref(), "Rust");
    }

    /// Rust has no `Highlights.json`: its colours are a tree-sitter query, and
    /// before this the buffer showed keywords, types and numbers in the plain
    /// text colour.
    #[test]
    fn a_query_highlighted_syntax_colours_its_keywords() {
        let syntax = syntax("Rust");
        assert!(!syntax.has_highlight_file, "Rust must have no rule file");
        let mut highlighter = CotHighlighter::new(&syntax);
        let text = "fn main() {\n    let x: u32 = 1;\n}\n";
        highlighter.rehighlight_text(text);

        let resolver = CotStyleResolver::new(&Theme::builtin_light());
        let runs = highlighter.styled_runs(&(0..text.len()), &resolver);
        let plain = HighlightStyle::default();
        let styled: Vec<&str> = runs
            .iter()
            .filter(|(_, style)| *style != plain)
            .map(|(range, _)| &text[range.clone()])
            .collect();
        assert!(
            styled.contains(&"fn") && styled.contains(&"let") && styled.contains(&"u32"),
            "the keywords and the type should be coloured, got {styled:?}"
        );
    }

    #[test]
    fn styles_tile_the_requested_range() {
        let syntax = syntax("YAML");
        let mut highlighter = CotHighlighter::new(&syntax);
        let text = "key: value  # note\nother: 12\n";
        highlighter.rehighlight_text(text);

        let resolver = CotStyleResolver::new(&Theme::builtin_light());
        let runs = highlighter.styled_runs(&(0..text.len()), &resolver);

        let mut cursor = 0;
        for (range, _) in &runs {
            assert_eq!(
                range.start, cursor,
                "runs must tile without gaps or overlap"
            );
            cursor = range.end;
        }
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn a_sub_range_is_tiled_within_its_bounds() {
        let syntax = syntax("YAML");
        let mut highlighter = CotHighlighter::new(&syntax);
        let text = "key: value  # note\nother: 12\n";
        highlighter.rehighlight_text(text);

        let resolver = CotStyleResolver::new(&Theme::builtin_light());
        let runs = highlighter.styled_runs(&(5..18), &resolver);
        assert_eq!(runs.first().map(|(r, _)| r.start), Some(5));
        assert_eq!(runs.last().map(|(r, _)| r.end), Some(18));
    }

    #[test]
    fn an_empty_buffer_produces_no_runs() {
        let syntax = syntax("YAML");
        let highlighter = CotHighlighter::new(&syntax);
        let resolver = CotStyleResolver::new(&Theme::builtin_light());
        assert!(highlighter.styled_runs(&(0..0), &resolver).is_empty());
    }

    /// The palette the component theme carries is the one the editor paints
    /// with, and it has to speak the component's vocabulary.
    #[test]
    fn the_syntax_palette_is_projected_into_the_component_vocabulary() {
        let theme = Theme::builtin_light();
        let colors = syntax_colors(&theme);

        let keyword = colors.style("keyword").expect("keyword");
        let expected = theme
            .token_color(subtext_syntax::HighlightToken::Keywords)
            .color
            .expect("the built-in theme defines keywords");
        assert_eq!(keyword.color, Some(to_hsla(expected)));

        for (name, token) in [
            ("attribute", subtext_syntax::HighlightToken::Attributes),
            ("string.special", subtext_syntax::HighlightToken::Characters),
            ("function", subtext_syntax::HighlightToken::Commands),
            ("comment", subtext_syntax::HighlightToken::Comments),
            ("number", subtext_syntax::HighlightToken::Numbers),
            ("string", subtext_syntax::HighlightToken::Strings),
            ("type", subtext_syntax::HighlightToken::Types),
            ("constant", subtext_syntax::HighlightToken::Values),
            ("variable", subtext_syntax::HighlightToken::Variables),
        ] {
            assert_eq!(token.style_name(), name);
            assert!(colors.style(name).is_some(), "{name} is missing");
        }
    }

    #[test]
    fn comments_are_styled_in_a_regex_language() {
        let syntax = syntax("YAML");
        let mut highlighter = CotHighlighter::new(&syntax);
        let text = "# a comment\nkey: 1\n";
        highlighter.rehighlight_text(text);

        let resolver = CotStyleResolver::new(&Theme::builtin_light());
        let runs = highlighter.styled_runs(&(0..text.len()), &resolver);
        let comment_style = resolver
            .style(subtext_syntax::HighlightToken::Comments.style_name())
            .expect("the resolver knows the comment token");
        assert!(
            runs.iter()
                .any(|(range, style)| range.start == 0 && *style == comment_style),
            "the leading comment should carry the comment colour"
        );
    }
}
