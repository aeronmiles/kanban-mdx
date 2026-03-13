//! Dialog rendering: move, delete, create, goto dialogs.

use ratatui::{
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, CreateStep};
use crate::tui::theme;
use super::layout::{centered_fixed, truncate};

pub(super) fn render_move_dialog(app: &App, frame: &mut Frame) {
    let filtered_indices = app.filtered_columns();
    let visible_count = filtered_indices.len() as u16;
    let extra = if app.picker.move_filter_active { 1 } else { 0 };
    let dialog_height = 2 + visible_count + 2 + extra;
    let dialog_width: u16 = 40;

    let area = centered_fixed(dialog_width, dialog_height, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Move to... ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Show filter input when active.
    if app.picker.move_filter_active {
        lines.push(Line::from(vec![
            Span::styled("  / ", theme::help_key()),
            Span::styled(format!("{}_", app.picker.move_filter), theme::title_active()),
        ]));
    }

    for (cursor_pos, &col_idx) in filtered_indices.iter().enumerate() {
        let col = &app.columns[col_idx];
        let is_current = col_idx == app.active_col;
        let is_selected = cursor_pos == app.picker.move_cursor;

        let first_char = col
            .name
            .chars()
            .next()
            .map(|c| c.to_lowercase().to_string())
            .unwrap_or_default();

        let digit_hint = if col_idx < 9 {
            Span::styled(format!("{} ", col_idx + 1), theme::dim())
        } else {
            Span::raw("  ")
        };

        let mut spans = vec![
            Span::raw(" "),
            digit_hint,
            Span::styled(format!("[{}] ", first_char), theme::help_key()),
        ];

        let name_style = if is_selected {
            theme::list_active()
        } else {
            theme::list_inactive()
        };

        spans.push(Span::styled(col.name.clone(), name_style));

        if is_current {
            spans.push(Span::styled(" (current)", theme::dim()));
        }

        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("1-9", theme::help_key()),
        Span::styled(":move  ", theme::help_desc()),
        Span::styled("/", theme::help_key()),
        Span::styled(":filter  ", theme::help_desc()),
        Span::styled("[letter]", theme::help_key()),
        Span::styled(":jump  ", theme::help_desc()),
        Span::styled("enter", theme::help_key()),
        Span::styled(":select  ", theme::help_desc()),
        Span::styled("esc", theme::help_key()),
        Span::styled(":cancel", theme::help_desc()),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_delete_confirm(app: &App, frame: &mut Frame) {
    let task = match app.active_task() {
        Some(t) => t,
        None => return,
    };

    let title_display = truncate(&task.title, 24);
    let msg = format!("Delete #{} \"{}\"?", task.id, title_display);

    let dialog_width = (msg.len() as u16 + 6).max(30);
    let dialog_height: u16 = 6;

    let area = centered_fixed(dialog_width, dialog_height, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Delete Task ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let yes_style = if app.picker.delete_cursor == 0 {
        theme::error_style()
    } else {
        theme::dim()
    };
    let no_style = if app.picker.delete_cursor == 1 {
        theme::list_active()
    } else {
        theme::dim()
    };

    let lines = vec![
        Line::from(Span::raw(msg)),
        Line::from(""),
        Line::from(vec![
            Span::raw("       "),
            Span::styled("[Yes]", yes_style),
            Span::raw("  "),
            Span::styled("[No]", no_style),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_create_dialog(app: &App, frame: &mut Frame) {
    let dialog_width: u16 = 60;
    let dialog_height: u16 = 20;

    let area = centered_fixed(
        dialog_width.min(frame.area().width.saturating_sub(4)),
        dialog_height.min(frame.area().height.saturating_sub(4)),
        frame.area(),
    );
    frame.render_widget(Clear, area);

    let title_text = if app.create_state.is_edit {
        format!(" Edit task #{} ", app.create_state.edit_id)
    } else {
        format!(" Create task in {} ", app.create_state.status)
    };

    let block = Block::new()
        .title(title_text)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::new(2, 2, 1, 1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let step = &app.create_state.step;
    let step_label = format!(
        "  Step {}/{}: {}",
        step.index() + 1,
        CreateStep::count(),
        step.name()
    );

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(step_label, theme::dim())),
        Line::from(""),
    ];

    match app.create_state.step {
        CreateStep::Title => {
            lines.push(Line::from(vec![
                Span::styled("Title: ", theme::help_key()),
                Span::styled(
                    format!("{}\u{2588}", app.create_state.title),
                    theme::title_inactive(),
                ),
            ]));
        }
        CreateStep::Body => {
            lines.push(Line::from(Span::styled("Body:", theme::help_key())));
            // Show body text with cursor.
            let body_lines: Vec<&str> = app.create_state.body.split('\n').collect();
            let visible = 6.min(body_lines.len().max(1));
            for line in body_lines.iter().take(visible) {
                lines.push(Line::from(Span::raw(line.to_string())));
            }
            if body_lines.is_empty() || (body_lines.len() == 1 && body_lines[0].is_empty()) {
                lines.push(Line::from(Span::styled("\u{2588}", theme::dim())));
            } else {
                // Add cursor indicator to last line.
                let last_idx = lines.len() - 1;
                if let Some(last_line) = lines.get_mut(last_idx) {
                    let mut spans = last_line.spans.clone();
                    spans.push(Span::styled("\u{2588}", theme::dim()));
                    *last_line = Line::from(spans);
                }
            }
        }
        CreateStep::Priority => {
            lines.push(Line::from(Span::styled("Priority:", theme::help_key())));
            for (i, p) in app.cfg.priorities.iter().enumerate() {
                let cursor = if i == app.create_state.priority_index {
                    "> "
                } else {
                    "  "
                };
                let pstyle = if i == app.create_state.priority_index {
                    theme::list_active()
                } else {
                    theme::priority_style(p)
                };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", cursor, p),
                    pstyle,
                )));
            }
        }
        CreateStep::Tags => {
            lines.push(Line::from(vec![
                Span::styled("Tags ", theme::help_key()),
                Span::styled("(comma-separated)", theme::dim()),
                Span::styled(": ", theme::help_key()),
                Span::styled(
                    format!("{}\u{2588}", app.create_state.tags),
                    theme::title_inactive(),
                ),
            ]));
        }
    }

    // Hint line at bottom.
    lines.push(Line::from(""));
    let action = if app.create_state.is_edit {
        "save"
    } else {
        "create"
    };
    let hint = match app.create_state.step {
        CreateStep::Title => format!("tab:next  enter:{}  esc:cancel", action),
        CreateStep::Body => format!(
            "enter:newline  tab:next  shift+tab:back  ctrl+enter:{}  esc:cancel",
            action
        ),
        CreateStep::Priority => format!(
            "\u{2191}/\u{2193}:select  tab:next  shift+tab:back  enter:{}  esc:cancel",
            action
        ),
        CreateStep::Tags => format!("shift+tab:back  enter:{}  esc:cancel", action),
    };
    lines.push(Line::from(Span::styled(hint, theme::dim())));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_goto_dialog(app: &App, frame: &mut Frame) {
    let dialog_width: u16 = 30;
    let dialog_height: u16 = 5;

    let area = centered_fixed(dialog_width, dialog_height, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Go to task ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("Task #: ", theme::help_key()),
            Span::styled(
                format!("{}\u{2588}", app.goto_input),
                theme::title_inactive(),
            ),
        ]),
        Line::from(Span::styled(
            "enter:jump  esc:cancel",
            theme::dim(),
        )),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
