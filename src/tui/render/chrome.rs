//! Chrome rendering: status bar, search bar, suggestion dropdown, reader panel.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};

use crate::tui::app::{App, AppView, ViewMode};
use crate::tui::theme;
use super::detail::build_detail_lines;
use super::detail::render_scrolled_content;
use super::layout::{pad_right, truncate};

pub(super) fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let key = theme::help_key();
    let desc = theme::help_desc();
    let dim = theme::dim();

    let total_tasks: usize = app.columns.iter().map(|c| c.tasks.len()).sum();

    let view_label = match app.view_mode {
        ViewMode::Cards => "cards",
        ViewMode::List => "list",
    };

    // Left side: board info
    let mut spans: Vec<Span> = vec![
        Span::raw(" "),
        Span::styled(format!("{} tasks", total_tasks), dim),
        Span::styled(" | ", dim),
        Span::styled("s", key),
        Span::styled(format!(":{}", app.sort_mode.label()), desc),
        Span::raw("  "),
        Span::styled("a", key),
        Span::styled(format!(":{}", app.time_mode.label()), desc),
        Span::raw("  "),
        Span::styled("v", key),
        Span::styled(format!(":{}", view_label), desc),
    ];

    // Worktree filter indicator (#57).
    if app.worktree_filter_active {
        spans.push(Span::styled("  ", dim));
        spans.push(Span::styled(
            "w:worktree",
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(208))
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
    }

    // Context mode indicator.
    if app.picker.context_mode {
        spans.push(Span::styled("  ", dim));
        let ctx_label = if app.picker.context_label.is_empty() {
            "[ctx: auto]".to_string()
        } else {
            format!("[ctx: {}]", app.picker.context_label)
        };
        spans.push(Span::styled(
            ctx_label,
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(141))
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
    }

    // Select mode indicator.
    if app.select_mode {
        spans.push(Span::styled("  ", dim));
        spans.push(Span::styled(
            "[VISUAL]",
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(214))
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
    }

    // Right side: shortcuts
    if app.view == AppView::Board {
        spans.push(Span::styled(" | ", dim));
        spans.push(Span::styled("/", key));
        spans.push(Span::styled(":search  ", desc));
        spans.push(Span::styled("f", key));
        spans.push(Span::styled(":find  ", desc));
        spans.push(Span::styled("c", key));
        spans.push(Span::styled(":new  ", desc));
        spans.push(Span::styled("e", key));
        spans.push(Span::styled(":edit  ", desc));
        spans.push(Span::styled("r", key));
        spans.push(Span::styled(":reader  ", desc));
        spans.push(Span::styled("?", key));
        spans.push(Span::styled(":help  ", desc));
        spans.push(Span::styled("q", key));
        spans.push(Span::styled(":quit", desc));
    }

    // Status message (if any).
    if !app.status_message.is_empty() {
        spans.push(Span::styled(" | ", dim));
        spans.push(Span::styled(app.status_message.clone(), desc));
    }

    // FPS counter + perf mode + debug render stats (right-aligned).
    let perf_label = if app.debug.perf_mode { "perf:ON" } else { "perf:OFF" };
    let fps_text = format!(
        " {:.0}fps  {}  b:{}ms r:{}ms L:{} V:{} ",
        app.debug.fps, perf_label,
        app.debug.dbg_build_ms, app.debug.dbg_render_ms,
        app.debug.dbg_lines, app.debug.dbg_vrows,
    );
    let fps_len = fps_text.chars().count();

    // Pad to fill width, leaving room for the FPS suffix.
    let content_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (area.width as usize).saturating_sub(content_len + fps_len);
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(fps_text, dim));

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

pub(super) fn render_search_bar(app: &App, frame: &mut Frame, with_cursor: bool) {
    let area = frame.area();
    if area.height < 3 {
        return;
    }
    let bar_area = Rect::new(area.x, area.y + area.height - 2, area.width, 1);

    let style = theme::status_bar_style();
    let cursor = if with_cursor { "\u{2588}" } else { "" };

    // Show semantic mode suffix when ~ is present in query.
    let sem_suffix = if App::is_semantic_query(&app.search.query) {
        if let Some(ref err) = app.search.sem_error {
            let short = if err.len() > 30 { &err[..30] } else { err };
            format!("  [error: {}]", short)
        } else if app.search.sem_loading {
            "  [searching...]".to_string()
        } else if !app.search.sem_scores.is_empty() {
            format!("  [{} scored]", app.search.sem_scores.len())
        } else if app.search.sem_pending {
            "  [...]".to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let hint = if app.search.query.is_empty() && with_cursor {
        "  ?:syntax"
    } else {
        ""
    };
    let text = format!(" / {}{}{}{} ", app.search.query, cursor, sem_suffix, hint);
    let line = Line::from(Span::styled(
        pad_right(&text, bar_area.width as usize),
        style,
    ));
    frame.render_widget(Clear, bar_area);
    frame.render_widget(Paragraph::new(line), bar_area);

    // Suggestion dropdown (above the search bar).
    if with_cursor {
        let prefix = app
            .search.tab_prefix
            .as_deref()
            .unwrap_or(&app.search.query);
        let suggestions = app.search.history.completions(prefix);
        if !suggestions.is_empty() {
            render_suggestions(frame, &suggestions, bar_area, app.search.tab_idx);
        }
    }
}

/// Render a suggestion dropdown above the given bar area.
/// `tab_idx` indicates the currently highlighted suggestion (0 = most recent,
/// wraps around). When 0, the top entry is highlighted by default.
pub(super) fn render_suggestions(
    frame: &mut Frame,
    suggestions: &[&str],
    bar_area: Rect,
    tab_idx: usize,
) {
    let max_visible = 6.min(suggestions.len());
    let height = max_visible as u16;
    if bar_area.y < height + 1 {
        return;
    }

    // Show most-recent matches first (suggestions are oldest-first from completions()).
    let visible: Vec<&str> = suggestions.iter().rev().take(max_visible).copied().collect();

    // Which visible row is highlighted (tab_idx wraps through all completions,
    // but we only display max_visible so clamp within visible range).
    let highlight_row = if suggestions.is_empty() {
        0
    } else {
        (tab_idx % suggestions.len()).min(visible.len().saturating_sub(1))
    };

    // Dropdown appears directly above the bar.
    let dropdown_y = bar_area.y - height;
    // Fit dropdown width to longest entry (clamped to bar width).
    let max_len = visible.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let dropdown_w = (max_len + 4).min(bar_area.width as usize) as u16;
    let dropdown_area = Rect::new(bar_area.x + 1, dropdown_y, dropdown_w, height);

    frame.render_widget(Clear, dropdown_area);

    let dim = theme::dim();
    let highlight_style = theme::title_active();

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == highlight_row { highlight_style } else { dim };
            Line::from(Span::styled(format!("  {} ", entry), style))
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines).style(theme::status_bar_style()),
        dropdown_area,
    );
}

pub(super) fn render_reader_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let task = match app.active_task() {
        Some(t) => t.clone(),
        None => {
            let block = Block::new()
                .title(" Reader ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(theme::dialog_border());
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let msg = Paragraph::new("No task selected").style(theme::dim());
            frame.render_widget(msg, inner);
            return;
        }
    };

    let title = format!(" #{}: {} ", task.id, task.title);
    let block = Block::new()
        .title(truncate(&title, area.width.saturating_sub(2) as usize))
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme::dialog_border())
        .padding(Padding::new(1, 1, 1, 0));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let t0 = std::time::Instant::now();
    let content = build_detail_lines(app, &task, inner.width);
    let t1 = std::time::Instant::now();
    render_scrolled_content(&content, app.reader_scroll, inner, frame, None);
    let t2 = std::time::Instant::now();
    app.debug.dbg_build_ms = t1.duration_since(t0).as_millis();
    app.debug.dbg_render_ms = t2.duration_since(t1).as_millis();
    app.debug.dbg_lines = content.lines.len();
    app.debug.dbg_vrows = content.total_vrows();
}
