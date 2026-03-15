//! Guide view rendering: topic index overlay and full-screen page reader.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
    Frame,
};

use tui_md as markdown;
use crate::tui::app::{self, App, GuideContentCache, GuideMode, THEME_QUANTIZE};
use crate::tui::guide::content;
use crate::tui::theme;
use super::detail::{render_scrolled_content, FindContext};
use super::layout::centered_rect;

pub(super) fn render_guide(app: &mut App, frame: &mut Frame) {
    match app.guide.mode {
        GuideMode::Index => render_guide_index(app, frame),
        GuideMode::Page => render_guide_page(app, frame),
    }
}

// ── Index view ───────────────────────────────────────────────────────

fn render_guide_index(app: &App, frame: &mut Frame) {
    let area = centered_rect(60, 80, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Guide ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::uniform(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let topics = content::topics();
    let filtered = content::filtered_indices(&app.guide.topic_filter);

    let mut lines: Vec<Line> = Vec::new();

    // Filter bar.
    if app.guide.topic_filter_active {
        lines.push(Line::from(vec![
            Span::styled("/ ", theme::help_key()),
            Span::styled(
                format!("{}\u{2588}", app.guide.topic_filter),
                theme::title_inactive(),
            ),
        ]));
        lines.push(Line::from(""));
    } else if !app.guide.topic_filter.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Filter: ", theme::help_key()),
            Span::styled(app.guide.topic_filter.clone(), theme::title_inactive()),
        ]));
        lines.push(Line::from(""));
    }

    // Group topics by section.
    let mut last_section = "";
    for (list_idx, &topic_idx) in filtered.iter().enumerate() {
        let topic = &topics[topic_idx];

        if topic.section != last_section {
            if !last_section.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                topic.section.to_string(),
                theme::help_key(),
            )));
            last_section = topic.section;
        }

        let is_cursor = list_idx == app.guide.topic_cursor;
        let prefix = if is_cursor { "  \u{25b8} " } else { "    " };
        let style = if is_cursor {
            theme::title_active()
        } else {
            theme::help_desc()
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, topic.title),
            style,
        )));
    }

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No matching topics",
            theme::dim(),
        )));
    }

    // Footer.
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "/:filter  j/k:navigate  Enter:open  Esc:close",
        theme::dim(),
    )));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

// ── Page view ────────────────────────────────────────────────────────

fn render_guide_page(app: &mut App, frame: &mut Frame) {
    let topics = content::topics();
    let topic_idx = app.guide.topic_cursor;
    let topic = match topics.get(topic_idx) {
        Some(t) => t,
        None => {
            app.guide.mode = GuideMode::Index;
            return;
        }
    };

    let area = frame.area();

    let show_find_bar = app.guide.find_active || !app.guide.find_query.is_empty();
    let chunks = if show_find_bar {
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area)
    };
    let header_area = chunks[0];
    let content_area = chunks[2];
    let find_bar_area = chunks[3];
    let status_area = chunks[5];

    let content_width = if app.reader_max_width > 0 {
        (app.reader_max_width as u16).min(content_area.width)
    } else {
        content_area.width
    };
    let h_margin = content_area.width.saturating_sub(content_width) / 2;
    let inner = Rect {
        x: content_area.x + h_margin,
        y: content_area.y,
        width: content_width,
        height: content_area.height,
    };

    // Header.
    let header_line1 = Line::from(vec![
        Span::styled(format!("Guide: {}", topic.title), theme::title_active()),
        Span::raw("  "),
        Span::styled(topic.section.to_string(), theme::dim()),
    ]);
    let header_line2 = Line::from(Span::styled(
        format!("{}/{}", topic.section, topic.title),
        theme::dim(),
    ));
    let header_text = vec![header_line1, header_line2];
    let ha = Rect {
        x: header_area.x + h_margin,
        y: header_area.y,
        width: content_width,
        height: header_area.height,
    };
    frame.render_widget(Paragraph::new(header_text), ha);

    // Build guide content.
    let guide_content = build_guide_lines(app, topic_idx, content_width);

    let find_ctx = if !app.guide.find_query.is_empty() {
        Some(FindContext {
            query: &app.guide.find_query,
            matches: &app.guide.find_matches,
            current: app.guide.find_current,
        })
    } else {
        None
    };

    // Clamp scroll to document bounds so subsequent saturating_sub works.
    let max_scroll = guide_content.total_vrows().saturating_sub(1);
    app.guide.scroll = app.guide.scroll.min(max_scroll);

    render_scrolled_content(&guide_content, app.guide.scroll, inner, frame, find_ctx);

    // Find bar.
    if show_find_bar {
        let match_info = if app.guide.find_matches.is_empty() {
            "No matches".to_string()
        } else {
            format!(
                "{}/{}",
                app.guide.find_current + 1,
                app.guide.find_matches.len()
            )
        };

        let find_line = if app.guide.find_active {
            Line::from(vec![
                Span::styled(" Find: ", theme::help_key()),
                Span::styled(
                    format!("{}\u{2588}", app.guide.find_query),
                    theme::title_active(),
                ),
                Span::styled(format!("  {}", match_info), theme::dim()),
            ])
        } else {
            Line::from(vec![
                Span::styled(format!(" Find: {} ", app.guide.find_query), theme::dim()),
                Span::styled(
                    format!(" {}  n/N:next/prev", match_info),
                    theme::dim(),
                ),
            ])
        };

        let fa = Rect {
            x: find_bar_area.x + h_margin,
            y: find_bar_area.y,
            width: content_width,
            height: find_bar_area.height,
        };
        frame.render_widget(
            Paragraph::new(find_line).style(theme::status_bar_style()),
            fa,
        );
    }

    // Footer.
    let mut hints: Vec<Span> = vec![
        Span::styled("q/Esc", theme::help_key()),
        Span::styled(":index  ", theme::help_desc()),
        Span::styled("/", theme::help_key()),
        Span::styled(":find  ", theme::help_desc()),
        Span::styled("z/Z", theme::help_key()),
        Span::styled(":fold", theme::help_desc()),
    ];

    if app.guide.fold_level > 0 {
        hints.push(Span::styled(
            format!(" [h{}]", app.guide.fold_level),
            theme::dim(),
        ));
    }

    hints.push(Span::styled("  ", theme::help_desc()));
    hints.push(Span::styled(">/<", theme::help_key()));
    hints.push(Span::styled(":width  ", theme::help_desc()));
    hints.push(Span::styled("j/k", theme::help_key()));
    hints.push(Span::styled(":scroll  ", theme::help_desc()));
    hints.push(Span::styled("J/K", theme::help_key()));
    hints.push(Span::styled(":halfpg  ", theme::help_desc()));
    hints.push(Span::styled("g/G", theme::help_key()));
    hints.push(Span::styled(":top/bottom  ", theme::help_desc()));
    hints.push(Span::styled("1-9", theme::help_key()));
    hints.push(Span::styled(":heading", theme::help_desc()));

    let hint = Line::from(hints);
    let footer_area = Rect {
        x: status_area.x + h_margin,
        y: status_area.y,
        width: content_width,
        height: status_area.height,
    };
    frame.render_widget(Paragraph::new(hint), footer_area);
}

// ── Content building ─────────────────────────────────────────────────

fn build_guide_lines(
    app: &mut App,
    topic_index: usize,
    width: u16,
) -> app::DetailContent {
    let bq = (app.brightness * THEME_QUANTIZE) as i32;
    let sq = (app.saturation * THEME_QUANTIZE) as i32;

    // Check cache.
    if let Some(ref entry) = app.guide.cache {
        if entry.topic_index == topic_index
            && entry.width == width
            && entry.theme == app.theme_kind
            && entry.brightness_q == bq
            && entry.saturation_q == sq
            && entry.fold_level == app.guide.fold_level
        {
            return entry.content.clone();
        }
    }

    let topics = content::topics();
    let body = topics[topic_index].body;

    // Render markdown.
    let md_opts = markdown::Options::new(app.markdown_theme())
        .with_max_width(width as usize)
        .with_fold_level(app.guide.fold_level)
        .with_adjust_color(theme::adjusted);
    let md_text = markdown::from_str_with_options(body, &md_opts);
    let body_fg = theme::palette_text_normal();

    let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
    let mut line_texts: Vec<String> = Vec::new();
    let mut all_headings: Vec<usize> = Vec::new();
    let mut l2_headings: Vec<usize> = Vec::new();

    for md_line in md_text.lines {
        let line_has_fg = md_line.style.fg.is_some();
        let owned_spans: Vec<ratatui::text::Span<'static>> = md_line
            .spans
            .into_iter()
            .map(|s| {
                let mut style = s.style;
                if style.fg.is_none() && !line_has_fg {
                    style.fg = Some(body_fg);
                }
                ratatui::text::Span::styled(s.content.into_owned(), style)
            })
            .collect();

        let text: String = owned_spans.iter().map(|s| s.content.as_ref()).collect();
        let idx = lines.len();
        let is_code = md_line.style.bg.is_some();

        if !is_code {
            if let Some(first) = owned_spans.first() {
                let content = first.content.as_ref();
                if content.starts_with('#') {
                    all_headings.push(idx);
                    if !content.starts_with("###") {
                        l2_headings.push(idx);
                    }
                }
            }
        }

        line_texts.push(text);
        lines.push(ratatui::text::Line::from(owned_spans).style(md_line.style));
    }

    // Collapse stacked headings.
    let collapsed = App::collapse_rendered_headings(&all_headings, &line_texts);

    // Compute visual-row offsets.
    let w = width.max(1);
    let mut offsets = Vec::with_capacity(lines.len() + 1);
    offsets.push(0usize);
    let mut cumulative = 0usize;
    for line in &lines {
        cumulative += ratatui::widgets::Paragraph::new(vec![line.clone()])
            .wrap(ratatui::widgets::Wrap { trim: false })
            .line_count(w);
        offsets.push(cumulative);
    }

    let rc_lines = std::rc::Rc::new(lines);
    let rc_offsets = std::rc::Rc::new(offsets);

    // Convert heading line indices to visual-row offsets.
    let content = app::DetailContent {
        lines: std::rc::Rc::clone(&rc_lines),
        vrow_offsets: std::rc::Rc::clone(&rc_offsets),
    };

    let offsets_any: Vec<usize> = collapsed
        .iter()
        .map(|&idx| content.line_to_vrow(idx))
        .collect();
    let offsets_l2: Vec<usize> = l2_headings
        .iter()
        .map(|&idx| content.line_to_vrow(idx))
        .collect();

    app.guide.cache = Some(GuideContentCache {
        topic_index,
        width,
        theme: app.theme_kind,
        brightness_q: bq,
        saturation_q: sq,
        fold_level: app.guide.fold_level,
        content: content.clone(),
        heading_offsets_any: offsets_any,
        heading_offsets_l2: offsets_l2,
        line_texts,
    });

    content
}
