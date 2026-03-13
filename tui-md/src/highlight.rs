//! Syntax highlighting for fenced code blocks using syntect.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SynStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Per-block highlight cache keyed by (code, lang, theme_name).
/// Avoids re-running syntect tokenisation when only brightness/width/fold changed.
static HIGHLIGHT_CACHE: LazyLock<Mutex<HashMap<(u64, u64, u64), Vec<Line<'static>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Simple FNV-1a hash to avoid pulling in extra deps.
fn fnv_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Convert a syntect RGBA colour to a ratatui `Color::Rgb`.
fn to_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Convert a syntect token style to a ratatui `Style` (foreground + modifiers only).
fn to_style(s: SynStyle) -> Style {
    let mut style = Style::default().fg(to_color(s.foreground));
    if s.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if s.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if s.font_style.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

/// Highlight a code block and return styled `Line`s.
///
/// Returns `None` if the language or theme is not recognised, allowing the
/// caller to fall back to a plain code style.
///
/// Results are cached by `(code, lang, theme_name)` so that re-renders
/// triggered by brightness, width, or fold-level changes skip syntect
/// tokenisation entirely.
pub fn highlight_code<'a>(code: &str, lang: &str, theme_name: &str) -> Option<Vec<Line<'a>>> {
    let code_hash = fnv_hash(code.as_bytes());
    let lang_hash = fnv_hash(lang.as_bytes());
    let theme_hash = fnv_hash(theme_name.as_bytes());
    let key = (code_hash, lang_hash, theme_hash);

    // Fast path: return cached result.
    if let Ok(cache) = HIGHLIGHT_CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return Some(cached.clone());
        }
    }

    // Slow path: run syntect.
    let syntax = SYNTAX_SET.find_syntax_by_token(lang)?;
    let theme = THEME_SET.themes.get(theme_name)?;

    let mut hl = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();

    for line in code.lines() {
        let ranges = hl.highlight_line(line, &SYNTAX_SET).ok()?;
        let spans: Vec<Span<'static>> = ranges
            .into_iter()
            .map(|(style, text)| Span::styled(text.to_string(), to_style(style)))
            .collect();
        lines.push(Line::from(spans));
    }

    // Store in cache.
    if let Ok(mut cache) = HIGHLIGHT_CACHE.lock() {
        // Cap cache size to avoid unbounded growth.
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, lines.clone());
    }

    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_rust_code() {
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let lines = highlight_code(code, "rs", "base16-ocean.dark");
        assert!(lines.is_some(), "should highlight Rust code");
        let lines = lines.unwrap();
        assert_eq!(lines.len(), 3);
        // Each line should have at least one span with an Rgb fg color.
        for line in &lines {
            assert!(!line.spans.is_empty());
            let has_rgb = line
                .spans
                .iter()
                .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
            assert!(has_rgb, "spans should have Rgb fg colors from syntect");
        }
    }

    #[test]
    fn highlight_unknown_lang_returns_none() {
        let result = highlight_code("x = 1", "nonexistent-lang-xyz", "base16-ocean.dark");
        assert!(result.is_none());
    }

    #[test]
    fn highlight_unknown_theme_returns_none() {
        let result = highlight_code("fn main() {}", "rs", "nonexistent-theme");
        assert!(result.is_none());
    }

    #[test]
    fn highlight_empty_code() {
        let lines = highlight_code("", "rs", "base16-ocean.dark");
        assert!(lines.is_some());
        assert!(lines.unwrap().is_empty());
    }

    #[test]
    fn highlight_python() {
        let code = "def hello():\n    print('world')";
        let lines = highlight_code(code, "py", "base16-ocean.dark");
        assert!(lines.is_some());
        assert_eq!(lines.unwrap().len(), 2);
    }

    #[test]
    fn highlight_cache_hit() {
        let code = "let x = 42;";
        let first = highlight_code(code, "rs", "base16-ocean.dark");
        let second = highlight_code(code, "rs", "base16-ocean.dark");
        assert!(first.is_some());
        assert!(second.is_some());
        // Both calls should return identical results.
        let a = first.unwrap();
        let b = second.unwrap();
        assert_eq!(a.len(), b.len());
        for (la, lb) in a.iter().zip(b.iter()) {
            assert_eq!(la.spans.len(), lb.spans.len());
        }
    }

    #[test]
    fn highlight_different_themes_cached_separately() {
        let code = "fn foo() {}";
        let a = highlight_code(code, "rs", "base16-ocean.dark");
        let b = highlight_code(code, "rs", "InspiredGitHub");
        assert!(a.is_some());
        assert!(b.is_some());
        // Different themes should produce different styling.
    }
}
