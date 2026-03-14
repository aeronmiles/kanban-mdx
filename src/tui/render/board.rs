//! Board rendering: columns, cards, and list view.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::model::task::Task;
use crate::tui::app::{self, App, AppView, Column, ViewMode};
use crate::tui::theme;
use super::chrome::{render_reader_panel, render_search_bar, render_status_bar};
use super::layout::{pad_right, truncate};

pub(super) fn render_board(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    let board_area = chunks[0];
    let status_area = chunks[1];

    if app.reader_open {
        // Split board area: board columns | reader panel.
        let board_w = app.board_width();
        let reader_w = area.width.saturating_sub(board_w);

        let h_chunks = Layout::horizontal([
            Constraint::Length(board_w),
            Constraint::Length(reader_w),
        ])
        .split(board_area);

        render_columns(app, frame, h_chunks[0]);
        render_reader_panel(app, frame, h_chunks[1]);
    } else {
        render_columns(app, frame, board_area);
    }

    render_status_bar(app, frame, status_area);

    if !app.search.query.is_empty() && app.view == AppView::Board {
        render_search_bar(app, frame, false);
    }
}

fn render_columns(app: &App, frame: &mut Frame, area: Rect) {
    if app.columns.is_empty() {
        let msg = Paragraph::new("No columns").style(theme::dim());
        frame.render_widget(msg, area);
        return;
    }

    // Collect only expanded (non-collapsed) columns with their original indices.
    let expanded: Vec<(usize, &Column)> = app
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.collapsed)
        .collect();

    if expanded.is_empty() {
        return;
    }

    // When hide_empty_columns is active, filter out columns with zero visible
    // (filtered) tasks — unless ALL columns are empty (fallback: show all).
    let expanded: Vec<(usize, &Column)> = if app.hide_empty_columns {
        let filtering = !app.search.query.is_empty()
            || app.worktree_filter_active
            || app.picker.context_mode;
        let context_ids = app.compute_context_ids();
        let non_empty: Vec<(usize, &Column)> = expanded
            .iter()
            .filter(|(_, col)| {
                let count = if filtering {
                    App::filtered_tasks(
                        col,
                        &app.search.query,
                        app.worktree_filter_active,
                        &app.search.sem_scores,
                        &context_ids,
                        app.time_mode.label(),
                    )
                    .len()
                } else {
                    col.tasks.len()
                };
                count > 0
            })
            .copied()
            .collect();
        // Fallback: if all columns would be hidden, show them all.
        if non_empty.is_empty() { expanded } else { non_empty }
    } else {
        expanded
    };

    let col_width = area.width / expanded.len() as u16;
    let constraints: Vec<Constraint> = expanded
        .iter()
        .map(|_| Constraint::Length(col_width))
        .collect();

    let col_areas = Layout::horizontal(constraints).split(area);

    for (slot, &(orig_idx, col)) in expanded.iter().enumerate() {
        let is_active = orig_idx == app.active_col;
        render_column(app, frame, col, col_areas[slot], is_active);
    }
}

fn render_column(
    app: &App,
    frame: &mut Frame,
    col: &Column,
    area: Rect,
    is_active: bool,
) {
    if area.height < 1 || area.width < 2 {
        return;
    }

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    let header_area = chunks[0];
    let tasks_area = chunks[1];

    // -- Header (with WIP indicator #36) --
    let wip_limit = app.cfg.wip_limit(&col.name);
    let total_count = col.tasks.len();
    let context_ids = app.compute_context_ids();
    let filtering = !app.search.query.is_empty()
        || app.worktree_filter_active
        || app.picker.context_mode;
    let tasks: Vec<&Task> = if filtering {
        App::filtered_tasks(
            col,
            &app.search.query,
            app.worktree_filter_active,
            &app.search.sem_scores,
            &context_ids,
            app.time_mode.label(),
        )
    } else {
        col.tasks.iter().collect()
    };
    let filtered_count = tasks.len();
    let header_text = if wip_limit > 0 {
        if filtering {
            format!(" {} [{}/{}/{}] ", col.name, filtered_count, total_count, wip_limit)
        } else {
            format!(" {} [{}/{}] ", col.name, total_count, wip_limit)
        }
    } else if filtering {
        format!(" {} ({}/{}) ", col.name, filtered_count, total_count)
    } else {
        format!(" {} ({}) ", col.name, total_count)
    };

    let wip_exceeded = wip_limit > 0 && total_count as i32 > wip_limit;

    let base_style = if is_active {
        theme::header_active()
    } else {
        theme::header_inactive()
    };

    let mut spans: Vec<Span> = Vec::new();
    if wip_exceeded {
        // Show header with warning when WIP exceeded.
        let padded = pad_right(&header_text, header_area.width.saturating_sub(2) as usize);
        spans.push(Span::styled(padded, base_style));
        spans.push(Span::styled(
            "\u{26a0} ",
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(196))
                .bg(if is_active {
                    ratatui::style::Color::Indexed(130)
                } else {
                    ratatui::style::Color::Indexed(236)
                })
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(
            pad_right(&header_text, header_area.width as usize),
            base_style,
        ));
    }

    let header = Paragraph::new(Line::from(spans));
    frame.render_widget(header, header_area);

    // -- Tasks --
    if tasks.is_empty() {
        if tasks_area.height > 0 {
            let empty = Paragraph::new(Line::from(Span::styled("  (empty)", theme::dim())));
            frame.render_widget(empty, tasks_area);
        }
        return;
    }

    // Map active_row (unfiltered) to a position in the filtered list by task ID.
    let active_id = app.active_task().map(|t| t.id);
    let active_filtered_row = active_id
        .and_then(|id| tasks.iter().position(|t| t.id == id))
        .unwrap_or(0);

    match app.view_mode {
        ViewMode::Cards => {
            render_cards(
                frame,
                app,
                &tasks,
                tasks_area,
                is_active,
                col.scroll_offset,
                active_filtered_row,
            );
        }
        ViewMode::List => {
            render_list(
                frame,
                app,
                &tasks,
                tasks_area,
                is_active,
                col.scroll_offset,
                active_filtered_row,
            );
        }
    }
}

/// Compute card height: borders(2) + title(1) + metadata(1) = 4.
/// Multi-line titles wrap within the card via the Paragraph widget;
/// the card height stays fixed so there is no empty padding line.
fn card_height(_app: &App) -> u16 {
    4 // top border(1) + title(1) + metadata(1) + bottom border(1)
}

fn render_cards(
    frame: &mut Frame,
    app: &App,
    tasks: &[&Task],
    area: Rect,
    is_active_col: bool,
    scroll_offset: usize,
    active_row: usize,
) {
    let ch = card_height(app);
    let max_slots = (area.height / ch) as usize;
    if max_slots == 0 {
        return;
    }

    // Two-pass auto-scroll: first pass computes offset assuming no indicators,
    // second pass adjusts for indicator rows that steal card space.
    let compute_offset = |slots: usize| -> usize {
        if is_active_col {
            let mut off = scroll_offset;
            if active_row >= off + slots {
                off = active_row + 1 - slots;
            }
            if active_row < off {
                off = active_row;
            }
            off.min(tasks.len().saturating_sub(slots))
        } else {
            scroll_offset.min(tasks.len().saturating_sub(slots))
        }
    };

    // First pass: compute offset with full slots.
    let mut offset = compute_offset(max_slots);

    // Account for indicator rows stealing card space:
    // top indicator appears when offset > 0, bottom when tasks extend past view.
    let has_top = offset > 0;
    let has_bottom = tasks.len().saturating_sub(offset + max_slots) > 0;
    let indicator_rows = (has_top as u16) + (has_bottom as u16);
    let effective_slots = ((area.height - indicator_rows) / ch) as usize;
    let effective_slots = effective_slots.max(1);

    // Second pass: recompute offset with reduced slots.
    if effective_slots < max_slots {
        offset = compute_offset(effective_slots);
    }

    let top_hidden = offset;
    let bottom_hidden = tasks.len().saturating_sub(offset + effective_slots);

    let mut y = area.y;

    if top_hidden > 0 && area.height > 0 {
        let indicator = Line::from(Span::styled(
            format!("  \u{2191} {} more", top_hidden),
            theme::dim(),
        ));
        frame.render_widget(Paragraph::new(indicator), Rect::new(area.x, y, area.width, 1));
        y += 1;
    }

    let end = (offset + effective_slots).min(tasks.len());
    for (slot, task_idx) in (offset..end).enumerate() {
        let task = tasks[task_idx];
        let card_y = y + slot as u16 * ch;
        if card_y + ch > area.y + area.height {
            break;
        }
        let card_area = Rect::new(area.x, card_y, area.width, ch);
        let is_selected = is_active_col && task_idx == active_row;
        render_card(frame, app, task, card_area, is_selected);
    }

    if bottom_hidden > 0 {
        let indicator_y = area.y + area.height - 1;
        if indicator_y > y {
            let indicator = Line::from(Span::styled(
                format!("  \u{2193} {} more", bottom_hidden),
                theme::dim(),
            ));
            frame.render_widget(
                Paragraph::new(indicator),
                Rect::new(area.x, indicator_y, area.width, 1),
            );
        }
    }
}

fn render_card(frame: &mut Frame, app: &App, task: &Task, area: Rect, is_selected: bool) {
    let border_style = if task.blocked {
        theme::card_border_blocked()
    } else if is_selected {
        theme::card_border_active()
    } else {
        theme::card_border_inactive()
    };

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 || inner.width < 2 {
        return;
    }

    let title_style = if is_selected {
        theme::title_active()
    } else {
        theme::title_inactive()
    };

    let id_str = format!("#{}", task.id);
    let line_width = inner.width as usize;
    let first_line_max = line_width.saturating_sub(id_str.len() + 1);

    let mut text: Vec<Line> = Vec::new();

    // Always truncate title to a single line.
    let title_display = truncate(&task.title, first_line_max);
    text.push(Line::from(vec![
        Span::styled(id_str.clone(), title_style),
        Span::raw(" "),
        Span::styled(title_display, title_style),
    ]));

    // Metadata line: AGE PRIORITY [score%] due branch [tags] @claimed
    let (_, freshness_color) = app::task_freshness_dot(task);
    let age_text = app::task_age_display(task, app.time_mode);
    let pri_label = app::priority_label(&task.priority);
    let mut spans: Vec<Span> = vec![
        Span::styled(age_text, theme::freshness_style(freshness_color)),
        Span::raw(" "),
        Span::styled(pri_label, theme::priority_style(&task.priority)),
    ];

    // Semantic match score: show percentage when semantic search is active.
    if let Some(&score) = app.search.sem_scores.get(&task.id) {
        let pct = (score * 100.0).round() as u8;
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("{}%", pct),
            theme::sem_score_style(score),
        ));
    }

    // Due date warning (#39): overdue = red, due within 24h = yellow.
    if let Some(due) = task.due {
        let today = chrono::Utc::now().date_naive();
        let due_style = if due < today {
            // Overdue: red.
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(196))
                .add_modifier(ratatui::style::Modifier::BOLD)
        } else if due <= today + chrono::Duration::days(1) {
            // Due within 24h: yellow/orange.
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(214))
                .add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            theme::dim()
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(due.format("%m/%d").to_string(), due_style));
    }

    if !task.branch.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(task.branch.clone(), theme::branch_style()));
    }

    for tag in &task.tags {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("[{}]", tag), theme::tag_style()));
    }

    if !task.claimed_by.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("@{}", task.claimed_by),
            theme::claim_style(),
        ));
    }

    text.push(Line::from(spans));
    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, inner);
}

fn render_list(
    frame: &mut Frame,
    app: &App,
    tasks: &[&Task],
    area: Rect,
    is_active_col: bool,
    scroll_offset: usize,
    active_row: usize,
) {
    let visible_slots = area.height as usize;
    if visible_slots == 0 {
        return;
    }

    // Auto-scroll to keep active row visible.
    let offset = if is_active_col {
        let mut off = scroll_offset;
        if active_row >= off + visible_slots {
            off = active_row + 1 - visible_slots;
        }
        if active_row < off {
            off = active_row;
        }
        off.min(tasks.len().saturating_sub(visible_slots))
    } else {
        scroll_offset.min(tasks.len().saturating_sub(visible_slots))
    };

    let end = (offset + visible_slots).min(tasks.len());

    for (slot, task_idx) in (offset..end).enumerate() {
        let task = tasks[task_idx];
        let is_selected = is_active_col && task_idx == active_row;

        let style = if is_selected {
            theme::list_active()
        } else {
            theme::list_inactive()
        };

        let (_, freshness_color) = app::task_freshness_dot(task);
        let id_str = format!("#{}", task.id);
        let age = app::task_age_display(task, app.time_mode);
        let score_str = app.search.sem_scores.get(&task.id).map(|&s| {
            format!(" {}%", (s * 100.0).round() as u8)
        });

        let prefix_len = id_str.len() + 1;
        let suffix_len = 1 + age.len() + score_str.as_ref().map_or(0, |s| s.len());
        let max_title = (area.width as usize)
            .saturating_sub(prefix_len)
            .saturating_sub(suffix_len);
        let title_text = truncate(&task.title, max_title);
        let padding = max_title.saturating_sub(title_text.len());

        let mut spans = vec![
            Span::styled(id_str, style),
            Span::raw(" "),
            Span::styled(title_text, style),
            Span::styled(" ".repeat(padding), style),
        ];
        if let Some((score_label, &score)) = score_str.as_ref().zip(app.search.sem_scores.get(&task.id)) {
            spans.push(Span::styled(score_label.clone(), theme::sem_score_style(score)));
        }
        spans.push(Span::styled(format!(" {}", age), theme::freshness_style(freshness_color)));

        let line = Line::from(spans);

        let row_area = Rect::new(area.x, area.y + slot as u16, area.width, 1);
        frame.render_widget(Paragraph::new(line), row_area);
    }
}
