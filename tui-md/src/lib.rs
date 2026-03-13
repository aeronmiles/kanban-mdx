// Forked from tui-markdown (https://github.com/joshka/tui-markdown)
// Original: MIT / Apache-2.0 — Copyright Josh McKinney 2024
//
// Extensions over upstream:
//   - Table rendering (GFM tables → bordered ratatui Text)
//   - Improved link display

pub mod highlight;
pub mod options;
pub mod style_sheet;

pub use options::Options;
pub use style_sheet::{DefaultStyleSheet, StyleSheet};

use itertools::{Itertools, Position};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel,
    Options as ParseOptions, Parser, Tag, TagEnd,
};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};

/// Render Markdown into a [`Text`] using the default [`Options`].
pub fn from_str(input: &str) -> Text<'_> {
    from_str_with_options(input, &Options::default())
}

/// Render Markdown into a [`Text`] using custom [`Options`].
pub fn from_str_with_options<'a, S>(input: &'a str, options: &Options<S>) -> Text<'a>
where
    S: StyleSheet,
{
    let mut parse_opts = ParseOptions::empty();
    parse_opts.insert(ParseOptions::ENABLE_STRIKETHROUGH);
    parse_opts.insert(ParseOptions::ENABLE_TASKLISTS);
    parse_opts.insert(ParseOptions::ENABLE_HEADING_ATTRIBUTES);
    parse_opts.insert(ParseOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    parse_opts.insert(ParseOptions::ENABLE_TABLES);
    let parser = Parser::new_ext(input, parse_opts);

    if options.fold_level >= 2 {
        let events = options::fold_events(parser.collect(), options.fold_level);
        let mut writer = TextWriter::new(events.into_iter(), options.styles.clone(), options.max_width, options.adjust_color);
        writer.run();
        writer.text
    } else {
        let mut writer = TextWriter::new(parser, options.styles.clone(), options.max_width, options.adjust_color);
        writer.run();
        writer.text
    }
}

// ---------------------------------------------------------------------------
// Heading attribute metadata
// ---------------------------------------------------------------------------

struct HeadingMeta<'a> {
    id: Option<CowStr<'a>>,
    classes: Vec<CowStr<'a>>,
    attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
}

impl<'a> HeadingMeta<'a> {
    fn into_option(self) -> Option<Self> {
        if self.id.is_some() || !self.classes.is_empty() || !self.attrs.is_empty() {
            Some(self)
        } else {
            None
        }
    }

    fn to_suffix(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(id) = &self.id {
            parts.push(format!("#{id}"));
        }
        for class in &self.classes {
            parts.push(format!(".{class}"));
        }
        for (key, value) in &self.attrs {
            match value {
                Some(v) => parts.push(format!("{key}={v}")),
                None => parts.push(key.to_string()),
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(format!(" {{{}}}", parts.join(" ")))
        }
    }
}

// ---------------------------------------------------------------------------
// Table builder — collects cells, then flushes a bordered table
// ---------------------------------------------------------------------------

struct TableBuilder<'a> {
    alignments: Vec<Alignment>,
    header_row: Vec<Vec<Span<'a>>>,
    rows: Vec<Vec<Vec<Span<'a>>>>,
    current_cell: Vec<Span<'a>>,
    in_header: bool,
    current_row: Vec<Vec<Span<'a>>>,
}

impl<'a> TableBuilder<'a> {
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
            header_row: Vec::new(),
            rows: Vec::new(),
            current_cell: Vec::new(),
            in_header: false,
            current_row: Vec::new(),
        }
    }

    fn start_head(&mut self) {
        self.in_header = true;
        self.current_row.clear();
    }

    fn end_head(&mut self) {
        self.header_row = std::mem::take(&mut self.current_row);
        self.in_header = false;
    }

    fn start_row(&mut self) {
        self.current_row.clear();
    }

    fn end_row(&mut self) {
        if !self.in_header {
            self.rows.push(std::mem::take(&mut self.current_row));
        }
    }

    fn start_cell(&mut self) {
        self.current_cell.clear();
    }

    fn push_span(&mut self, span: Span<'a>) {
        self.current_cell.push(span);
    }

    fn end_cell(&mut self) {
        self.current_row.push(std::mem::take(&mut self.current_cell));
    }

    /// Compute the display width of a cell's spans (character count).
    fn cell_width(cell: &[Span<'_>]) -> usize {
        cell.iter().map(|s| s.content.chars().count()).sum()
    }

    /// Flush the table to a series of ratatui `Line`s.
    fn flush<S: StyleSheet>(self, styles: &S, max_width: Option<usize>) -> Vec<Line<'a>> {
        let num_cols = self.alignments.len().max(
            self.header_row
                .len()
                .max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0)),
        );

        // Compute column widths.
        let mut col_widths = vec![0usize; num_cols];
        for (i, cell) in self.header_row.iter().enumerate() {
            col_widths[i] = col_widths[i].max(Self::cell_width(cell));
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    col_widths[i] = col_widths[i].max(Self::cell_width(cell));
                }
            }
        }
        // Minimum column width of 3 for the separator.
        for w in &mut col_widths {
            *w = (*w).max(3);
        }

        // Cap column widths to fit within max_width if set.
        // Total width = 1(left border) + sum(col_width + 2(padding)) + (num_cols-1)*1(separator) + 1(right border)
        //             = 2 + num_cols*2 + sum(col_widths) + (num_cols-1)
        if let Some(max_w) = max_width {
            if num_cols > 0 {
                let overhead = 2 + num_cols * 2 + num_cols.saturating_sub(1); // borders + padding + separators
                let available = max_w.saturating_sub(overhead);
                let total: usize = col_widths.iter().sum();
                if total > available && available > 0 {
                    // Shrink only the columns that exceed an equal share.
                    // Columns that fit within the share keep their natural
                    // width; the excess budget is redistributed to wider ones.
                    let mut budget = available;
                    let mut flexible = num_cols; // columns still subject to shrinking
                    let mut frozen = vec![false; num_cols];

                    // Iteratively freeze columns whose natural width fits
                    // within the per-column share of remaining budget.
                    loop {
                        if flexible == 0 {
                            break;
                        }
                        let share = budget / flexible;
                        let mut changed = false;
                        for (i, w) in col_widths.iter().enumerate() {
                            if !frozen[i] && *w <= share {
                                frozen[i] = true;
                                budget -= *w;
                                flexible -= 1;
                                changed = true;
                            }
                        }
                        if !changed {
                            break;
                        }
                    }

                    // Distribute remaining budget equally among unfrozen columns.
                    if flexible > 0 {
                        let share = budget / flexible;
                        let mut extra = budget % flexible;
                        for (i, w) in col_widths.iter_mut().enumerate() {
                            if !frozen[i] {
                                *w = share.max(3);
                                if extra > 0 {
                                    *w += 1;
                                    extra -= 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        let border_style = styles.table_border();
        let header_style = styles.table_header();

        let mut lines: Vec<Line<'a>> = Vec::new();

        // Top border: ┌───┬───┐
        lines.push(Self::border_line(
            &col_widths,
            "┌",
            "┬",
            "┐",
            border_style,
        ));

        // Header row.
        if !self.header_row.is_empty() {
            lines.extend(Self::data_lines(
                &self.header_row,
                &col_widths,
                &self.alignments,
                border_style,
                Some(header_style),
            ));
            // Header separator: ├───┼───┤
            lines.push(Self::border_line(
                &col_widths,
                "├",
                "┼",
                "┤",
                border_style,
            ));
        }

        // Data rows.
        for row in &self.rows {
            lines.extend(Self::data_lines(
                row,
                &col_widths,
                &self.alignments,
                border_style,
                None,
            ));
        }

        // Bottom border: └───┴───┘
        lines.push(Self::border_line(
            &col_widths,
            "└",
            "┴",
            "┘",
            border_style,
        ));

        lines
    }

    fn border_line<'b>(
        col_widths: &[usize],
        left: &'b str,
        mid: &'b str,
        right: &'b str,
        style: Style,
    ) -> Line<'b> {
        let mut spans: Vec<Span<'b>> = Vec::new();
        spans.push(Span::styled(left.to_owned(), style));
        for (i, &w) in col_widths.iter().enumerate() {
            spans.push(Span::styled("─".repeat(w + 2), style));
            if i + 1 < col_widths.len() {
                spans.push(Span::styled(mid.to_owned(), style));
            }
        }
        spans.push(Span::styled(right.to_owned(), style));
        Line::from(spans)
    }

    /// Render a logical row as one or more `Line`s, wrapping cell content
    /// across multiple lines when it exceeds the column width.
    fn data_lines<'b>(
        row: &[Vec<Span<'b>>],
        col_widths: &[usize],
        alignments: &[Alignment],
        border_style: Style,
        cell_style_override: Option<Style>,
    ) -> Vec<Line<'b>> {
        // Wrap each cell into lines of at most col_width characters.
        let wrapped: Vec<Vec<Vec<Span<'b>>>> = col_widths
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                let cell = row.get(i).cloned().unwrap_or_default();
                Self::wrap_cell(&cell, w)
            })
            .collect();

        // Row height = max wrapped lines across all cells.
        let row_height = wrapped.iter().map(|c| c.len()).max().unwrap_or(1);

        let mut output: Vec<Line<'b>> = Vec::new();
        for line_idx in 0..row_height {
            let mut spans: Vec<Span<'b>> = Vec::new();
            spans.push(Span::styled("│ ".to_owned(), border_style));

            for (i, &w) in col_widths.iter().enumerate() {
                let cell_line = wrapped
                    .get(i)
                    .and_then(|lines| lines.get(line_idx));

                let content_width = cell_line
                    .map(|cl| Self::cell_width(cl))
                    .unwrap_or(0);
                let padding = w.saturating_sub(content_width);

                let alignment = alignments.get(i).copied().unwrap_or(Alignment::None);
                let (pad_left, pad_right) = match alignment {
                    Alignment::Center => (padding / 2, padding - padding / 2),
                    Alignment::Right => (padding, 0),
                    Alignment::Left | Alignment::None => (0, padding),
                };

                if pad_left > 0 {
                    spans.push(Span::raw(" ".repeat(pad_left)));
                }
                if let Some(cell_spans) = cell_line {
                    for s in cell_spans {
                        let styled = if let Some(override_style) = cell_style_override {
                            Span::styled(s.content.clone(), s.style.patch(override_style))
                        } else {
                            s.clone()
                        };
                        spans.push(styled);
                    }
                }
                if pad_right > 0 {
                    spans.push(Span::raw(" ".repeat(pad_right)));
                }

                if i + 1 < col_widths.len() {
                    spans.push(Span::styled(" │ ".to_owned(), border_style));
                }
            }
            spans.push(Span::styled(" │".to_owned(), border_style));
            output.push(Line::from(spans));
        }
        output
    }

    /// Wrap a cell's spans into lines of at most `max_w` characters.
    fn wrap_cell<'b>(cell: &[Span<'b>], max_w: usize) -> Vec<Vec<Span<'b>>> {
        if max_w == 0 {
            return vec![vec![]];
        }

        let total = Self::cell_width(cell);
        if total <= max_w {
            return vec![cell.to_vec()];
        }

        // Flatten all characters with their styles, then chunk by max_w.
        let mut chars_with_style: Vec<(char, Style)> = Vec::with_capacity(total);
        for span in cell {
            for ch in span.content.chars() {
                chars_with_style.push((ch, span.style));
            }
        }

        let mut lines: Vec<Vec<Span<'b>>> = Vec::new();
        for chunk in chars_with_style.chunks(max_w) {
            // Group consecutive chars with the same style into spans.
            let mut line_spans: Vec<Span<'b>> = Vec::new();
            let mut current_str = String::new();
            let mut current_style = chunk[0].1;

            for &(ch, style) in chunk {
                if style == current_style {
                    current_str.push(ch);
                } else {
                    if !current_str.is_empty() {
                        line_spans.push(Span::styled(
                            std::mem::take(&mut current_str),
                            current_style,
                        ));
                    }
                    current_style = style;
                    current_str.push(ch);
                }
            }
            if !current_str.is_empty() {
                line_spans.push(Span::styled(current_str, current_style));
            }
            lines.push(line_spans);
        }

        if lines.is_empty() {
            lines.push(vec![]);
        }
        lines
    }
}

// ---------------------------------------------------------------------------
// Core writer
// ---------------------------------------------------------------------------

struct TextWriter<'a, I, S: StyleSheet> {
    iter: I,
    text: Text<'a>,
    inline_styles: Vec<Style>,
    line_prefixes: Vec<Span<'a>>,
    line_styles: Vec<Style>,
    list_indices: Vec<Option<u64>>,
    link: Option<CowStr<'a>>,
    styles: S,
    heading_meta: Option<HeadingMeta<'a>>,
    in_metadata_block: bool,
    needs_newline: bool,
    table: Option<TableBuilder<'a>>,
    max_width: Option<usize>,
    /// Language hint for the current fenced code block (if any).
    code_lang: Option<String>,
    /// Buffered code block content for syntax highlighting.
    code_buffer: Option<String>,
    /// Color adjustment function for syntect output (brightness/saturation).
    adjust_color: fn(ratatui::style::Color) -> ratatui::style::Color,
}

impl<'a, I, S> TextWriter<'a, I, S>
where
    I: Iterator<Item = Event<'a>>,
    S: StyleSheet,
{
    fn new(
        iter: I,
        styles: S,
        max_width: Option<usize>,
        adjust_color: fn(ratatui::style::Color) -> ratatui::style::Color,
    ) -> Self {
        Self {
            iter,
            text: Text::default(),
            inline_styles: vec![],
            line_styles: vec![],
            line_prefixes: vec![],
            list_indices: vec![],
            needs_newline: false,
            link: None,
            styles,
            heading_meta: None,
            in_metadata_block: false,
            table: None,
            max_width,
            code_lang: None,
            code_buffer: None,
            adjust_color,
        }
    }

    fn run(&mut self) {
        while let Some(event) = self.iter.next() {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: Event<'a>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(text),
            Event::Code(code) => self.code(code),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.rule(),
            Event::TaskListMarker(checked) => self.task_list_marker(checked),
            _ => {} // Html, FootnoteReference, Math — silently ignored
        }
    }

    // ── Tag dispatch ──────────────────────────────────────────────────

    fn start_tag(&mut self, tag: Tag<'a>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => self.start_heading(level, HeadingMeta { id, classes, attrs }),
            Tag::BlockQuote(kind) => self.start_blockquote(kind),
            Tag::CodeBlock(kind) => self.start_codeblock(kind),
            Tag::List(start_index) => self.start_list(start_index),
            Tag::Item => self.start_item(),
            Tag::Table(alignments) => self.start_table(alignments),
            Tag::TableHead => self.start_table_head(),
            Tag::TableRow => self.start_table_row(),
            Tag::TableCell => self.start_table_cell(),
            Tag::Emphasis => self.push_inline_style(Style::new().italic()),
            Tag::Strong => self.push_inline_style(Style::new().bold()),
            Tag::Strikethrough => self.push_inline_style(Style::new().crossed_out()),
            Tag::Link { dest_url, .. } => self.push_link(dest_url),
            Tag::MetadataBlock(_) => self.start_metadata_block(),
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.end_paragraph(),
            TagEnd::Heading(_) => self.end_heading(),
            TagEnd::BlockQuote(_) => self.end_blockquote(),
            TagEnd::CodeBlock => self.end_codeblock(),
            TagEnd::List(_) => self.end_list(),
            TagEnd::Item => {}
            TagEnd::Table => self.end_table(),
            TagEnd::TableHead => self.end_table_head(),
            TagEnd::TableRow => self.end_table_row(),
            TagEnd::TableCell => self.end_table_cell(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_inline_style(),
            TagEnd::Link => self.pop_link(),
            TagEnd::MetadataBlock(_) => self.end_metadata_block(),
            _ => {}
        }
    }

    // ── Block-level elements ──────────────────────────────────────────

    fn start_paragraph(&mut self) {
        if self.needs_newline {
            self.push_line(Line::default());
        }
        self.push_line(Line::default());
        self.needs_newline = false;
    }

    fn end_paragraph(&mut self) {
        self.needs_newline = true;
    }

    fn start_heading(&mut self, level: HeadingLevel, meta: HeadingMeta<'a>) {
        if self.needs_newline {
            self.push_line(Line::default());
        }
        let n = match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        };
        let style = self.styles.heading(n);
        let prefix = format!("{} ", "#".repeat(n as usize));
        self.push_line(Line::styled(prefix, style));
        self.heading_meta = meta.into_option();
        self.needs_newline = false;
    }

    fn end_heading(&mut self) {
        if let Some(meta) = self.heading_meta.take() {
            if let Some(suffix) = meta.to_suffix() {
                self.push_span(Span::styled(suffix, self.styles.heading_meta()));
            }
        }
        self.needs_newline = true;
    }

    fn start_blockquote(&mut self, _kind: Option<BlockQuoteKind>) {
        if self.needs_newline {
            self.push_line(Line::default());
            self.needs_newline = false;
        }
        self.line_prefixes.push(Span::from(">"));
        self.line_styles.push(self.styles.blockquote());
    }

    fn end_blockquote(&mut self) {
        self.line_prefixes.pop();
        self.line_styles.pop();
        self.needs_newline = true;
    }

    fn start_codeblock(&mut self, kind: CodeBlockKind<'_>) {
        if !self.text.lines.is_empty() {
            self.push_line(Line::default());
        }
        let lang = match kind {
            CodeBlockKind::Fenced(ref lang) => lang.as_ref(),
            CodeBlockKind::Indented => "",
        };
        self.line_styles.push(self.styles.code());
        self.push_line(Line::from(format!("```{lang}")));
        self.needs_newline = true;

        // Set up buffering for syntax highlighting.
        self.code_lang = if lang.is_empty() {
            None
        } else {
            Some(lang.to_string())
        };
        self.code_buffer = Some(String::new());
    }

    fn end_codeblock(&mut self) {
        if let Some(buffer) = self.code_buffer.take() {
            let theme_name = self.styles.code_theme();

            // Try syntax highlighting if we have a language and a theme.
            let highlighted = if !theme_name.is_empty() {
                self.code_lang
                    .as_deref()
                    .and_then(|lang| highlight::highlight_code(&buffer, lang, theme_name))
            } else {
                None
            };

            if let Some(lines) = highlighted {
                for line in lines {
                    // Apply brightness/saturation adjustment to syntect colors.
                    let adjusted_spans: Vec<Span> = line
                        .spans
                        .into_iter()
                        .map(|s| {
                            let mut style = s.style;
                            if let Some(color) = style.fg {
                                style.fg = Some((self.adjust_color)(color));
                            }
                            if let Some(color) = style.bg {
                                style.bg = Some((self.adjust_color)(color));
                            }
                            Span::styled(s.content, style)
                        })
                        .collect();
                    // push_line applies the code line_style (bg) and any
                    // line_prefixes (blockquote/list). Span fg from syntect
                    // overrides the line_style fg, so highlighting is preserved.
                    self.push_line(Line::from(adjusted_spans));
                }
            } else {
                // Fallback: plain code style, line by line.
                for text_line in buffer.lines() {
                    self.push_line(Line::from(text_line.to_string()));
                }
            }

            self.code_lang = None;
        }

        self.push_line(Line::from("```"));
        self.needs_newline = true;
        self.line_styles.pop();
    }

    fn start_metadata_block(&mut self) {
        if self.needs_newline {
            self.push_line(Line::default());
        }
        self.line_styles.push(self.styles.metadata_block());
        self.push_line(Line::from("---"));
        self.push_line(Line::default());
        self.in_metadata_block = true;
    }

    fn end_metadata_block(&mut self) {
        if self.in_metadata_block {
            self.push_line(Line::from("---"));
            self.line_styles.pop();
            self.in_metadata_block = false;
            self.needs_newline = true;
        }
    }

    fn rule(&mut self) {
        if self.needs_newline {
            self.push_line(Line::default());
        }
        self.push_line(Line::from("---"));
        self.needs_newline = true;
    }

    // ── Lists ─────────────────────────────────────────────────────────

    fn start_list(&mut self, index: Option<u64>) {
        if self.list_indices.is_empty() && self.needs_newline {
            self.push_line(Line::default());
        }
        self.list_indices.push(index);
    }

    fn end_list(&mut self) {
        self.list_indices.pop();
        self.needs_newline = true;
    }

    fn start_item(&mut self) {
        self.push_line(Line::default());
        let width = self.list_indices.len() * 4 - 3;
        if let Some(last_index) = self.list_indices.last_mut() {
            let span = match last_index {
                None => Span::from(" ".repeat(width - 1) + "- "),
                Some(index) => {
                    *index += 1;
                    format!("{:width$}. ", *index - 1).light_blue()
                }
            };
            self.push_span(span);
        }
        self.needs_newline = false;
    }

    fn task_list_marker(&mut self, checked: bool) {
        let marker = if checked { 'x' } else { ' ' };
        let marker_span = Span::from(format!("[{marker}] "));
        if let Some(line) = self.text.lines.last_mut() {
            if let Some(first_span) = line.spans.first_mut() {
                let content = first_span.content.to_mut();
                if content.ends_with("- ") {
                    let len = content.len();
                    content.truncate(len - 2);
                    content.push_str("- [");
                    content.push(marker);
                    content.push_str("] ");
                    return;
                }
            }
            line.spans.insert(1, marker_span);
        } else {
            self.push_span(marker_span);
        }
    }

    // ── Tables ────────────────────────────────────────────────────────

    fn start_table(&mut self, alignments: Vec<Alignment>) {
        if self.needs_newline {
            self.push_line(Line::default());
            self.needs_newline = false;
        }
        self.table = Some(TableBuilder::new(alignments));
    }

    fn end_table(&mut self) {
        if let Some(table) = self.table.take() {
            let lines = table.flush(&self.styles, self.max_width);
            for line in lines {
                self.text.lines.push(line);
            }
            self.needs_newline = true;
        }
    }

    fn start_table_head(&mut self) {
        if let Some(t) = &mut self.table {
            t.start_head();
        }
    }

    fn end_table_head(&mut self) {
        if let Some(t) = &mut self.table {
            t.end_head();
        }
    }

    fn start_table_row(&mut self) {
        if let Some(t) = &mut self.table {
            t.start_row();
        }
    }

    fn end_table_row(&mut self) {
        if let Some(t) = &mut self.table {
            t.end_row();
        }
    }

    fn start_table_cell(&mut self) {
        if let Some(t) = &mut self.table {
            t.start_cell();
        }
    }

    fn end_table_cell(&mut self) {
        if let Some(t) = &mut self.table {
            t.end_cell();
        }
    }

    // ── Inline elements ───────────────────────────────────────────────

    fn text(&mut self, text: CowStr<'a>) {
        // If we're inside a table, push spans into the table builder.
        if let Some(table) = &mut self.table {
            let style = self.inline_styles.last().copied().unwrap_or_default();
            table.push_span(Span::styled(text.into_string(), style));
            return;
        }

        // If we're inside a code block, buffer for syntax highlighting.
        if let Some(ref mut buf) = self.code_buffer {
            buf.push_str(&text);
            return;
        }

        for (position, line) in text.lines().with_position() {
            if self.needs_newline {
                self.push_line(Line::default());
                self.needs_newline = false;
            }
            if matches!(position, Position::Middle | Position::Last) {
                self.push_line(Line::default());
            }
            let style = self.inline_styles.last().copied().unwrap_or_default();
            self.push_span(Span::styled(line.to_owned(), style));
        }
        self.needs_newline = false;
    }

    fn code(&mut self, code: CowStr<'a>) {
        if let Some(table) = &mut self.table {
            table.push_span(Span::styled(code.into_string(), self.styles.code()));
            return;
        }
        self.push_span(Span::styled(code, self.styles.code()));
    }

    fn soft_break(&mut self) {
        if self.code_buffer.is_some() {
            if let Some(ref mut buf) = self.code_buffer {
                buf.push('\n');
            }
            return;
        }
        if self.in_metadata_block {
            self.hard_break();
        } else {
            self.push_span(Span::raw(" "));
        }
    }

    fn hard_break(&mut self) {
        if self.code_buffer.is_some() {
            if let Some(ref mut buf) = self.code_buffer {
                buf.push('\n');
            }
            return;
        }
        self.push_line(Line::default());
    }

    // ── Style stack ───────────────────────────────────────────────────

    fn push_inline_style(&mut self, style: Style) {
        let current = self.inline_styles.last().copied().unwrap_or_default();
        self.inline_styles.push(current.patch(style));
    }

    fn pop_inline_style(&mut self) {
        self.inline_styles.pop();
    }

    fn push_link(&mut self, dest_url: CowStr<'a>) {
        self.link = Some(dest_url);
    }

    fn pop_link(&mut self) {
        if let Some(link) = self.link.take() {
            self.push_span(" (".into());
            self.push_span(Span::styled(link, self.styles.link()));
            self.push_span(")".into());
        }
    }

    // ── Output helpers ────────────────────────────────────────────────

    fn push_line(&mut self, line: Line<'a>) {
        let style = self.line_styles.last().copied().unwrap_or_default();
        let mut line = line.patch_style(style);

        let line_prefixes = self.line_prefixes.iter().cloned().collect_vec();
        if !line_prefixes.is_empty() {
            line.spans.insert(0, " ".into());
        }
        for prefix in line_prefixes.iter().rev().cloned() {
            line.spans.insert(0, prefix);
        }

        // Pad lines with a background colour to fill max_width so the bg
        // extends across the full rendering area (e.g. code blocks).
        if let Some(max_w) = self.max_width {
            if style.bg.is_some() {
                let content_width: usize =
                    line.spans.iter().map(|s| s.content.chars().count()).sum();
                if content_width < max_w {
                    line.spans
                        .push(Span::styled(" ".repeat(max_w - content_width), style));
                }
            }
        }

        self.text.lines.push(line);
    }

    fn push_span(&mut self, span: Span<'a>) {
        if let Some(line) = self.text.lines.last_mut() {
            line.push_span(span);
        } else {
            self.push_line(Line::from(vec![span]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(from_str(""), Text::default());
    }

    #[test]
    fn paragraph() {
        let text = from_str("Hello, world!");
        assert_eq!(text, Text::from("Hello, world!"));
    }

    #[test]
    fn heading_h1() {
        let text = from_str("# Title");
        assert!(!text.lines.is_empty());
        // First line should contain the "# " prefix.
        let first = &text.lines[0];
        let joined: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.contains("# "));
        assert!(joined.contains("Title"));
    }

    #[test]
    fn bold_and_italic() {
        let text = from_str("**bold** and *italic*");
        // Should produce at least one line.
        assert!(!text.lines.is_empty());
    }

    #[test]
    fn simple_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
        let text = from_str(md);
        let rendered: String = text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        // Should contain box-drawing characters.
        assert!(rendered.contains('┌'));
        assert!(rendered.contains('┘'));
        assert!(rendered.contains('│'));
        // Should contain the cell data.
        assert!(rendered.contains('A'));
        assert!(rendered.contains('4'));
    }

    #[test]
    fn table_alignment() {
        let md = "| Left | Center | Right |\n|:-----|:------:|------:|\n| a | b | c |";
        let text = from_str(md);
        // Just verify it doesn't panic and produces output.
        assert!(text.lines.len() >= 4); // top border + header + separator + data + bottom border
    }

    #[test]
    fn link() {
        let text = from_str("[click](https://example.com)");
        let joined: String = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(joined.contains("click"));
        assert!(joined.contains("https://example.com"));
    }

    #[test]
    fn code_block() {
        let text = from_str("```rust\nfn main() {}\n```");
        let joined: String = text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("```rust"));
        assert!(joined.contains("fn main()"));
    }

    #[test]
    fn task_list() {
        let text = from_str("- [ ] todo\n- [x] done");
        let joined: String = text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("[ ]"));
        assert!(joined.contains("[x]"));
    }

    #[test]
    fn blockquote() {
        let text = from_str("> quoted text");
        let first = &text.lines[0];
        let joined: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.contains('>'));
        assert!(joined.contains("quoted text"));
    }

    /// Helper: render markdown to a flat string for assertion convenience.
    fn render_to_string<S: StyleSheet>(input: &str, opts: &Options<S>) -> String {
        from_str_with_options(input, opts)
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn fold_level_0_shows_everything() {
        let md = "## Section\nBody text\n### Sub\nSub body";
        let opts = Options::default(); // fold_level = 0
        let out = render_to_string(md, &opts);
        assert!(out.contains("Section"));
        assert!(out.contains("Body text"));
        assert!(out.contains("Sub"));
        assert!(out.contains("Sub body"));
    }

    #[test]
    fn fold_level_3_folds_h3() {
        let md = "## Overview\nKeep this.\n### Details\nHide this.";
        // fold_level 3 → fold headings at depth >= 3 (i.e. ### and deeper).
        let opts = Options::default().with_fold_level(3);
        let out = render_to_string(md, &opts);
        assert!(out.contains("Overview"));
        assert!(out.contains("Keep this"));
        assert!(out.contains("Details")); // heading preserved
        assert!(out.contains("▸")); // fold indicator present
        assert!(!out.contains("Hide this")); // body folded
    }

    #[test]
    fn fold_level_1_is_noop() {
        let md = "# Top\nTop body\n## Mid\nMid body\n### Deep\nDeep body";
        // fold_level < 2 is a no-op.
        let opts = Options::default().with_fold_level(1);
        let out = render_to_string(md, &opts);
        assert!(out.contains("Top body"));
        assert!(out.contains("Mid body"));
        assert!(out.contains("Deep body"));
    }

    #[test]
    fn fold_level_2_folds_h2_and_deeper() {
        let md = "# H1\nH1 body\n## H2\nH2 body\n### H3\nH3 body";
        // fold_level 2 → fold headings at depth >= 2 (## and ###).
        let opts = Options::default().with_fold_level(2);
        let out = render_to_string(md, &opts);
        assert!(out.contains("# ")); // h1 preserved
        assert!(out.contains("H1 body")); // h1 content preserved
        assert!(out.contains("## ")); // h2 heading preserved
        assert!(out.contains("▸")); // fold indicator
        assert!(!out.contains("H2 body")); // h2 body folded
        assert!(!out.contains("H3 body")); // h3 inside h2 also folded
    }

    #[test]
    fn fold_sibling_headings_each_get_indicator() {
        let md = "## A\nA body\n## B\nB body";
        // fold_level 2 → fold h2+.
        let opts = Options::default().with_fold_level(2);
        let out = render_to_string(md, &opts);
        assert!(out.contains("A"));
        assert!(out.contains("B"));
        assert!(!out.contains("A body"));
        assert!(!out.contains("B body"));
        // Both headings should have fold indicators.
        assert_eq!(out.matches('▸').count(), 2);
    }
}
