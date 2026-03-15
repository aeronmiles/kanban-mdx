//! Picker rendering: branch picker, context picker, confirm branch.

use ratatui::{
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, ContextKind, ContextPickerMode};
use crate::tui::theme;
use super::layout::centered_fixed;

pub(super) fn render_branch_picker(app: &App, frame: &mut Frame) {
    let filtered = app.filtered_branches();
    let visible_count = filtered.len().min(15) as u16;
    let title = if app.picker.branch_worktree_only {
        " Assign Worktree Branch "
    } else {
        " Assign Branch "
    };

    let dialog_height = (4 + visible_count + 2).max(8);
    let dialog_width: u16 = 50;

    let area = centered_fixed(
        dialog_width.min(frame.area().width.saturating_sub(4)),
        dialog_height.min(frame.area().height.saturating_sub(4)),
        frame.area(),
    );
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Filter input.
    let filter_display = if app.picker.branch_filter.is_empty() {
        "type to filter...".to_string()
    } else {
        format!("{}\u{2588}", app.picker.branch_filter)
    };
    lines.push(Line::from(vec![
        Span::styled("  / ", theme::help_key()),
        Span::styled(
            filter_display,
            if app.picker.branch_filter.is_empty() {
                theme::dim()
            } else {
                theme::title_active()
            },
        ),
    ]));
    lines.push(Line::from(""));

    // Branch list (scrolled to show cursor).
    let scroll_offset = if app.picker.branch_cursor >= visible_count as usize {
        app.picker.branch_cursor - visible_count as usize + 1
    } else {
        0
    };

    let current_branch = app
        .active_task()
        .map(|t| t.branch.as_str())
        .unwrap_or("");

    for (i, branch) in filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_count as usize)
    {
        let is_selected = i == app.picker.branch_cursor;
        let is_assigned = branch == current_branch;

        let mut spans = vec![Span::raw("  ")];

        if is_assigned {
            spans.push(Span::styled("\u{25cf} ", theme::branch_style()));
        } else {
            spans.push(Span::raw("  "));
        }

        let name_style = if is_selected {
            theme::list_active()
        } else {
            theme::branch_style()
        };
        spans.push(Span::styled(branch.clone(), name_style));

        lines.push(Line::from(spans));
    }

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No matching branches",
            theme::dim(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("enter", theme::help_key()),
        Span::styled(":assign  ", theme::help_desc()),
        Span::styled("backspace", theme::help_key()),
        Span::styled(":clear  ", theme::help_desc()),
        Span::styled("esc", theme::help_key()),
        Span::styled(":cancel", theme::help_desc()),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_context_picker(app: &App, frame: &mut Frame) {
    let filtered = app.filtered_context_items();
    let visible_count = filtered.len().min(15) as u16;

    let title = match (&app.picker.context_picker_mode, app.picker.context_worktree_only) {
        (ContextPickerMode::SwitchContext, false) => " Switch Context ",
        (ContextPickerMode::SwitchContext, true) => " Switch Context (worktree) ",
        (ContextPickerMode::AssignBranch, false) => " Assign Branch ",
        (ContextPickerMode::AssignBranch, true) => " Assign Worktree Branch ",
    };

    let dialog_height = (4 + visible_count + 2).max(8);
    let dialog_width: u16 = 55;

    let area = centered_fixed(
        dialog_width.min(frame.area().width.saturating_sub(4)),
        dialog_height.min(frame.area().height.saturating_sub(4)),
        frame.area(),
    );
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Filter input line.
    let filter_display = if app.picker.context_filter.is_empty() {
        "type to filter...".to_string()
    } else {
        format!("{}\u{2588}", app.picker.context_filter)
    };
    lines.push(Line::from(vec![
        Span::styled("  / ", theme::help_key()),
        Span::styled(
            filter_display,
            if app.picker.context_filter.is_empty() {
                theme::dim()
            } else {
                theme::title_active()
            },
        ),
    ]));
    lines.push(Line::from(""));

    // Item list (scrolled to show cursor).
    let max_visible = visible_count as usize;
    let scroll_offset = if app.picker.context_cursor >= max_visible {
        app.picker.context_cursor - max_visible + 1
    } else {
        0
    };

    for (i, item) in filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
    {
        let selected = i == app.picker.context_cursor;
        let prefix = if selected { "\u{25b8} " } else { "  " };

        let mut label = item.label.clone();
        if item.missing {
            label = format!("{} (missing)", label);
        }

        // Check if this is the active context.
        if app.picker.context_mode && !item.branch.is_empty() && item.branch == app.picker.context_label {
            label = format!("{} (active)", label);
        }

        let style = match (&item.kind, selected, item.missing) {
            (_, _, true) => ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(196)),
            (ContextKind::Auto | ContextKind::Clear, true, _) => theme::list_active(),
            (ContextKind::Auto | ContextKind::Clear, false, _) => theme::dim(),
            (ContextKind::New, true, _) => ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(34))
                .add_modifier(ratatui::style::Modifier::BOLD),
            (ContextKind::New, false, _) => ratatui::style::Style::default()
                .fg(ratatui::style::Color::Indexed(34)),
            (_, true, _) => theme::list_active(),
            (_, false, _) => theme::branch_style(),
        };

        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, label),
            style,
        )));
    }

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No matching items",
            theme::dim(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("enter", theme::help_key()),
        Span::styled(":select  ", theme::help_desc()),
        Span::styled("type", theme::help_key()),
        Span::styled(":filter  ", theme::help_desc()),
        Span::styled("esc", theme::help_key()),
        Span::styled(":cancel", theme::help_desc()),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_confirm_branch(app: &App, frame: &mut Frame) {
    let msg = if app.picker.context_worktree_only {
        format!(
            "Create worktree for branch '{}'?\n  path: ../kb-{}\n\n  y/n",
            app.picker.confirm_branch_name, app.picker.confirm_branch_name
        )
    } else {
        format!("Create branch '{}'?\n\n  y/n", app.picker.confirm_branch_name)
    };

    let dialog_width: u16 = 50;
    let dialog_height: u16 = 7;

    let area = centered_fixed(
        dialog_width.min(frame.area().width.saturating_sub(4)),
        dialog_height.min(frame.area().height.saturating_sub(4)),
        frame.area(),
    );
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Confirm Branch ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::Indexed(214)))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let paragraph = Paragraph::new(msg)
        .style(theme::title_inactive())
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_file_picker(app: &App, frame: &mut Frame) {
    let max_visible: usize = 18;
    let dialog_width: u16 = 60;

    let filtered: Vec<&crate::tui::app::FilePickerEntry> = if app.file_picker.filter.is_empty() {
        app.file_picker.entries.iter().collect()
    } else {
        let q = app.file_picker.filter.to_lowercase();
        app.file_picker
            .entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&q))
            .collect()
    };

    // In path-input mode, show completions instead of directory entries.
    let show_completions = app.file_picker.path_input_active
        && !app.file_picker.tab_completions.is_empty();

    let list_count = if show_completions {
        app.file_picker.tab_completions.len().min(max_visible)
    } else {
        filtered.len().min(max_visible)
    };

    // 2 (input + blank) + list + 2 (blank + footer) + 2 (border) + 2 (padding)
    let dialog_height = (list_count as u16 + 8).max(10);

    let area = centered_fixed(
        dialog_width.min(frame.area().width.saturating_sub(4)),
        dialog_height.min(frame.area().height.saturating_sub(4)),
        frame.area(),
    );
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Open File ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Header: path input or filter bar or cwd.
    if app.file_picker.path_input_active {
        lines.push(Line::from(vec![
            Span::styled(" Path: ", theme::help_key()),
            Span::styled(
                format!("{}\u{2588}", app.file_picker.path_input),
                theme::title_active(),
            ),
        ]));
    } else if !app.file_picker.filter.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(" Filter: ", theme::help_key()),
            Span::styled(
                format!("{}\u{2588}", app.file_picker.filter),
                theme::title_active(),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            format!(" {}", app.file_picker.cwd.display()),
            theme::dim(),
        )));
    }
    lines.push(Line::from(""));

    if show_completions {
        // Show tab-completion candidates.
        let comps = &app.file_picker.tab_completions;
        let active_idx = if comps.is_empty() {
            0
        } else {
            app.file_picker.tab_idx % comps.len()
        };
        // Scroll to keep the active completion visible.
        let scroll_offset = if active_idx >= max_visible {
            active_idx - max_visible + 1
        } else {
            0
        };
        for (i, comp) in comps.iter().enumerate().skip(scroll_offset).take(max_visible) {
            let is_active = i == active_idx;
            let prefix = if is_active { "\u{25b8} " } else { "  " };
            // Show just the filename portion for readability.
            let display = std::path::Path::new(comp)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(comp);
            let suffix = if comp.ends_with('/') { "/" } else { "" };
            let style = if is_active {
                theme::list_active()
            } else if comp.ends_with('/') {
                theme::branch_style()
            } else {
                theme::title_inactive()
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}{}", prefix, display, suffix),
                style,
            )));
        }
        if comps.len() > max_visible {
            lines.push(Line::from(Span::styled(
                format!("  ({} more...)", comps.len() - max_visible),
                theme::dim(),
            )));
        }
    } else {
        // Directory entries.
        let scroll_offset = if app.file_picker.cursor >= max_visible {
            app.file_picker.cursor - max_visible + 1
        } else {
            0
        };
        for (i, entry) in filtered
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(max_visible)
        {
            let is_cursor = i == app.file_picker.cursor;
            let prefix = if is_cursor { "\u{25b8} " } else { "  " };
            let style = if is_cursor {
                theme::list_active()
            } else if entry.is_dir {
                theme::branch_style()
            } else {
                theme::title_inactive()
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, entry.name),
                style,
            )));
        }
        if filtered.is_empty() {
            lines.push(Line::from(Span::styled("  (empty)", theme::dim())));
        }
    }

    lines.push(Line::from(""));

    // Footer hints.
    if app.file_picker.path_input_active {
        lines.push(Line::from(vec![
            Span::styled("tab", theme::help_key()),
            Span::styled(":complete  ", theme::help_desc()),
            Span::styled("enter", theme::help_key()),
            Span::styled(":open  ", theme::help_desc()),
            Span::styled("esc", theme::help_key()),
            Span::styled(":cancel", theme::help_desc()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("enter", theme::help_key()),
            Span::styled(":open  ", theme::help_desc()),
            Span::styled("h", theme::help_key()),
            Span::styled(":up  ", theme::help_desc()),
            Span::styled("/", theme::help_key()),
            Span::styled(":path  ", theme::help_desc()),
            Span::styled("esc", theme::help_key()),
            Span::styled(":cancel", theme::help_desc()),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
