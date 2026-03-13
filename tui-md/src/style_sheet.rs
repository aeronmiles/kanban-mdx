// Forked from tui-markdown (https://github.com/joshka/tui-markdown)
// Original: MIT / Apache-2.0 — Copyright Josh McKinney 2024

use ratatui::style::Style;

/// A collection of styles consumed by the markdown renderer.
///
/// Implement this trait to provide a custom theme. The default implementation
/// ([`DefaultStyleSheet`]) matches the stock tui-markdown appearance.
pub trait StyleSheet: Clone + Send + Sync + 'static {
    /// Style for a Markdown heading. `level` is 1-based (1 = `# H1`).
    fn heading(&self, level: u8) -> Style;

    /// Style for inline `code` spans and fenced code blocks.
    fn code(&self) -> Style;

    /// Style for link text.
    fn link(&self) -> Style;

    /// Base style applied to blockquotes.
    fn blockquote(&self) -> Style;

    /// Style for heading attribute metadata.
    fn heading_meta(&self) -> Style;

    /// Style for YAML front-matter blocks.
    fn metadata_block(&self) -> Style;

    /// Style for table borders and header separator.
    fn table_border(&self) -> Style {
        Style::default()
    }

    /// Style for table header cells.
    fn table_header(&self) -> Style {
        Style::new().bold().underlined()
    }

    /// Syntect theme name for syntax highlighting in fenced code blocks.
    /// Return an empty string to disable syntax highlighting.
    fn code_theme(&self) -> &str {
        "base16-ocean.dark"
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultStyleSheet;

impl StyleSheet for DefaultStyleSheet {
    fn heading(&self, level: u8) -> Style {
        match level {
            1 => Style::new().on_cyan().bold().underlined(),
            2 => Style::new().cyan().bold(),
            3 => Style::new().cyan().bold().italic(),
            _ => Style::new().light_cyan().italic(),
        }
    }

    fn code(&self) -> Style {
        Style::new().white().on_black()
    }

    fn link(&self) -> Style {
        Style::new().blue().underlined()
    }

    fn blockquote(&self) -> Style {
        Style::new().green()
    }

    fn heading_meta(&self) -> Style {
        Style::new().dim()
    }

    fn metadata_block(&self) -> Style {
        Style::new().light_yellow()
    }

    fn code_theme(&self) -> &str {
        "base16-ocean.dark"
    }
}
