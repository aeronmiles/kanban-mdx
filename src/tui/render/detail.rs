//! Detail view rendering: full-screen detail, scrolled content, find highlighting.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use tui_md as markdown;
use crate::model::task::Task;
use crate::tui::app::{self, App};
use crate::tui::theme;
use super::chrome::render_suggestions;

pub(super) fn render_detail(app: &mut App, frame: &mut Frame) {
    let task = match app.active_task() {
        Some(t) => t.clone(),
        None => {
            let msg = Paragraph::new("No task selected").style(theme::dim());
            frame.render_widget(msg, frame.area());
            return;
        }
    };

    let area = frame.area();

    // Split: pinned header + blank + content + blank + footer (+ find bar).
    let show_find_bar = app.detail.find_active || !app.detail.find_query.is_empty();
    let chunks = if show_find_bar {
        Layout::vertical([
            Constraint::Length(2), // header bar (2 lines)
            Constraint::Length(1), // blank separator
            Constraint::Min(0),    // scrollable content
            Constraint::Length(1), // find bar
            Constraint::Length(1), // blank separator
            Constraint::Length(1), // footer bar
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(2), // header bar (2 lines)
            Constraint::Length(1), // blank separator
            Constraint::Min(0),    // scrollable content
            Constraint::Length(0), // no find bar
            Constraint::Length(1), // blank separator
            Constraint::Length(1), // footer bar
        ])
        .split(area)
    };
    let header_area = chunks[0];
    let content_area = chunks[2];
    let find_bar_area = chunks[3];
    let status_area = chunks[5];

    // Borderless layout with center-aligned content (matches Go detail view).
    // Cap content width using reader_max_width for readability on wide terminals.
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

    // --- Pinned header bar ---
    let column_name = app
        .columns
        .iter()
        .find(|c| c.tasks.iter().any(|t| t.id == task.id))
        .map(|c| c.name.as_str())
        .unwrap_or("unknown");

    let mut header_spans: Vec<Span> = vec![
        Span::styled(
            format!("Task #{}: {}", task.id, task.title),
            theme::title_active(),
        ),
        Span::raw("  "),
        Span::styled(column_name.to_string(), theme::dim()),
    ];

    if !task.priority.is_empty() {
        header_spans.push(Span::raw("  "));
        header_spans.push(Span::styled(
            app::priority_label(&task.priority),
            theme::priority_style(&task.priority),
        ));
    }

    if !task.tags.is_empty() {
        header_spans.push(Span::raw("  "));
        header_spans.push(Span::styled(task.tags.join(", "), theme::tag_style()));
    }

    if !task.claimed_by.is_empty() {
        header_spans.push(Span::raw("  "));
        header_spans.push(Span::styled(
            format!("@{}", task.claimed_by),
            theme::claim_style(),
        ));
    }

    let header_line1 = Line::from(header_spans);

    let file_display = if task.file.is_empty() {
        format!("task #{}", task.id)
    } else {
        task.file.clone()
    };
    let header_line2 = Line::from(Span::styled(file_display, theme::dim()));

    let header_text = vec![header_line1.clone(), header_line2.clone()];
    let header_max_w =
        (header_line1.width().max(header_line2.width())) as u16;

    if content_width >= header_max_w {
        // Header fits within content width — left-align to content left edge.
        let ha = Rect {
            x: header_area.x + h_margin,
            y: header_area.y,
            width: content_width,
            height: header_area.height,
        };
        frame.render_widget(Paragraph::new(header_text), ha);
    } else {
        // Content narrower than header — center on full width.
        frame.render_widget(
            Paragraph::new(header_text).alignment(ratatui::layout::Alignment::Center),
            header_area,
        );
    }

    let content = build_detail_lines(app, &task, content_width);

    let find_ctx = if !app.detail.find_query.is_empty() {
        Some(FindContext {
            query: &app.detail.find_query,
            matches: &app.detail.find_matches,
            current: app.detail.find_current,
        })
    } else {
        None
    };

    render_scrolled_content(&content, app.detail.scroll, inner, frame, find_ctx);

    // Find bar (shown when find is active or matches still visible).
    if show_find_bar {
        let match_info = if app.detail.find_matches.is_empty() {
            "No matches".to_string()
        } else {
            format!("{}/{}", app.detail.find_current + 1, app.detail.find_matches.len())
        };

        let find_line = if app.detail.find_active {
            let hint = if app.detail.find_query.is_empty() { "  ?:syntax" } else { "" };
            Line::from(vec![
                Span::styled(" Find: ", theme::help_key()),
                Span::styled(format!("{}\u{2588}", app.detail.find_query), theme::title_active()),
                Span::styled(format!("  {}{}", match_info, hint), theme::dim()),
            ])
        } else {
            Line::from(vec![
                Span::styled(format!(" Find: {} ", app.detail.find_query), theme::dim()),
                Span::styled(
                    format!(" {}  n/N:next/prev", match_info),
                    theme::dim(),
                ),
            ])
        };

        let find_line_width = find_line.width() as u16;
        if content_width >= find_line_width {
            let find_area = Rect {
                x: find_bar_area.x + h_margin,
                y: find_bar_area.y,
                width: content_width,
                height: find_bar_area.height,
            };
            frame.render_widget(
                Paragraph::new(find_line).style(theme::status_bar_style()),
                find_area,
            );
        } else {
            frame.render_widget(
                Paragraph::new(find_line)
                    .style(theme::status_bar_style())
                    .alignment(ratatui::layout::Alignment::Center),
                find_bar_area,
            );
        }

        // Suggestion dropdown above the find bar.
        if app.detail.find_active {
            let prefix = app
                .detail.find_tab_prefix
                .as_deref()
                .unwrap_or(&app.detail.find_query);
            let suggestions = app.detail.find_history.completions(prefix);
            if !suggestions.is_empty() {
                render_suggestions(frame, &suggestions, find_bar_area, app.detail.find_tab_idx);
            }
        }
    }

    // Status bar with key hints (matches Go detail view format).
    let mut hints: Vec<Span> = vec![
        Span::styled("q/Esc", theme::help_key()),
        Span::styled(":back  ", theme::help_desc()),
        Span::styled("/", theme::help_key()),
        Span::styled(":find  ", theme::help_desc()),
        Span::styled("z/Z", theme::help_key()),
        Span::styled(":fold", theme::help_desc()),
    ];

    if app.fold_level() > 0 {
        hints.push(Span::styled(
            format!(" [h{}]", app.fold_level()),
            theme::dim(),
        ));
    }

    hints.push(Span::styled("  ", theme::help_desc()));
    hints.push(Span::styled(">/<", theme::help_key()));
    hints.push(Span::styled(":width  ", theme::help_desc()));
    hints.push(Span::styled("j/k", theme::help_key()));
    hints.push(Span::styled(":scroll  ", theme::help_desc()));
    hints.push(Span::styled("J/K", theme::help_key()));
    hints.push(Span::styled(":½pg  ", theme::help_desc()));
    hints.push(Span::styled("g/G", theme::help_key()));
    hints.push(Span::styled(":top/bottom  ", theme::help_desc()));
    hints.push(Span::styled("n/p", theme::help_key()));
    hints.push(Span::styled(":move  ", theme::help_desc()));
    hints.push(Span::styled("1-9", theme::help_key()));
    hints.push(Span::styled(":heading  ", theme::help_desc()));
    hints.push(Span::styled("y", theme::help_key()));
    hints.push(Span::styled(":copy  ", theme::help_desc()));
    hints.push(Span::styled("o", theme::help_key()));
    hints.push(Span::styled(":open", theme::help_desc()));

    let hint = Line::from(hints);
    let hint_width: u16 = hint.width() as u16;

    // Left-align footer to the content area's left edge, but fall back to
    // center-aligned on the full terminal when content is narrower than the
    // footer text.
    if content_width >= hint_width {
        // Footer fits within content width — left-align to content left edge.
        let footer_area = Rect {
            x: status_area.x + h_margin,
            y: status_area.y,
            width: content_width,
            height: status_area.height,
        };
        frame.render_widget(Paragraph::new(hint), footer_area);
    } else {
        // Content narrower than footer — center on full width.
        frame.render_widget(
            Paragraph::new(hint).alignment(ratatui::layout::Alignment::Center),
            status_area,
        );
    }
}

/// Build the content lines for a task detail view (shared by detail view
/// and reader panel).  Returns a `DetailContent` with precomputed visual-row
/// offsets so per-frame scroll arithmetic is O(1)/O(log n).
pub(crate) fn build_detail_lines(app: &mut App, task: &Task, width: u16) -> app::DetailContent {
    let bq = (app.brightness * app::THEME_QUANTIZE) as i32;
    let sq = (app.saturation * app::THEME_QUANTIZE) as i32;
    let updated_epoch = task.updated.timestamp();

    // Check cache — Rc::clone is a cheap pointer bump.
    if let Some(ref entry) = app.detail.cache {
        if entry.task_id == task.id as u32
            && entry.updated_epoch == updated_epoch
            && entry.body == task.body
            && entry.width == width
            && entry.theme == app.theme_kind
            && entry.brightness_q == bq
            && entry.saturation_q == sq
            && entry.fold_level == app.fold_level()
        {
            return app::DetailContent {
                lines: std::rc::Rc::clone(&entry.lines),
                vrow_offsets: std::rc::Rc::clone(&entry.vrow_offsets),
            };
        }
    }

    let column_name = app
        .columns
        .iter()
        .find(|c| c.tasks.iter().any(|t| t.id == task.id))
        .map(|c| c.name.as_str())
        .unwrap_or("unknown");

    let tags_display = if task.tags.is_empty() {
        "none".to_string()
    } else {
        task.tags.join(", ")
    };

    let claimed_display: String = if task.claimed_by.is_empty() {
        "none".to_string()
    } else {
        task.claimed_by.clone()
    };

    let assignee_display: String = if task.assignee.is_empty() {
        "none".to_string()
    } else {
        task.assignee.clone()
    };

    let blocked_display = if task.blocked {
        if task.block_reason.is_empty() {
            "yes".to_string()
        } else {
            format!("yes ({})", task.block_reason)
        }
    } else {
        "no".to_string()
    };

    let due_display = task
        .due
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "none".to_string());

    let created_display = task.created.format("%Y-%m-%d %H:%M").to_string();
    let updated_display = task.updated.format("%Y-%m-%d %H:%M").to_string();

    // Collect metadata as (label, value, style) triples.
    let mut fields: Vec<(&str, String, Style)> = vec![
        ("Status", column_name.to_string(), theme::title_inactive()),
        (
            "Priority",
            app::priority_label(&task.priority),
            theme::priority_style(&task.priority),
        ),
        ("Tags", tags_display, theme::tag_style()),
        (
            "Blocked",
            blocked_display,
            if task.blocked {
                theme::error_style()
            } else {
                theme::dim()
            },
        ),
        ("Claimed", claimed_display, theme::claim_style()),
        ("Assignee", assignee_display, theme::title_inactive()),
        ("Created", created_display, theme::dim()),
        ("Updated", updated_display, theme::dim()),
        ("Due", due_display, theme::dim()),
    ];

    if !task.branch.is_empty() {
        fields.push(("Branch", task.branch.clone(), theme::branch_style()));
    }
    if !task.class.is_empty() {
        fields.push(("Class", task.class.clone(), theme::dim()));
    }
    if !task.estimate.is_empty() {
        fields.push(("Estimate", task.estimate.clone(), theme::dim()));
    }
    if !task.depends_on.is_empty() {
        let deps_str = task
            .depends_on
            .iter()
            .map(|d| format!("#{}", d))
            .collect::<Vec<_>>()
            .join(", ");
        fields.push(("Depends", deps_str, theme::dim()));
    }

    // Lay out metadata in a two-column grid when width permits.
    // Both columns hug the left edge; remaining space is empty on the right.
    let w = width as usize;
    let use_two_cols = w >= 50;
    let gap = 4; // spaces between columns

    // Find the widest label in each column so values align.
    let max_label_left = fields
        .iter()
        .step_by(2)
        .map(|(l, _, _)| l.len())
        .max()
        .unwrap_or(0);
    let max_label_right = fields
        .iter()
        .skip(1)
        .step_by(2)
        .map(|(l, _, _)| l.len())
        .max()
        .unwrap_or(0);

    // Left column width = widest (padded-label + ": " + value) across all left entries.
    let left_col_w = fields
        .iter()
        .step_by(2)
        .map(|(_, v, _)| {
            // label is padded to max_label_left, then ": ", then value
            max_label_left + 2 + v.chars().count()
        })
        .max()
        .unwrap_or(0);

    // Right column width = widest (padded-label + ": " + value) across all right entries.
    let right_col_w = fields
        .iter()
        .skip(1)
        .step_by(2)
        .map(|(_, v, _)| max_label_right + 2 + v.chars().count())
        .max()
        .unwrap_or(0);

    // For single-column fallback.
    let max_label_all = fields
        .iter()
        .map(|(l, _, _)| l.len())
        .max()
        .unwrap_or(0);

    let mut lines: Vec<Line> = Vec::new();

    if use_two_cols {
        for row in fields.chunks(2) {
            let mut spans: Vec<Span> = Vec::new();

            let (l1, v1, s1) = &row[0];
            let label1 = format!("{:w$}: ", l1, w = max_label_left);
            let first_len = label1.chars().count() + v1.chars().count();
            spans.push(Span::styled(label1, theme::dim()));
            spans.push(Span::styled(v1.clone(), *s1));

            if row.len() > 1 {
                let (l2, v2, s2) = &row[1];
                let label2 = format!("{:w$}: ", l2, w = max_label_right);
                let pad = (left_col_w + gap).saturating_sub(first_len);
                spans.push(Span::raw(" ".repeat(pad.max(2))));
                spans.push(Span::styled(label2, theme::dim()));
                spans.push(Span::styled(v2.clone(), *s2));
            }

            lines.push(Line::from(spans));
        }
    } else {
        // Narrow fallback: single column, aligned labels.
        for (label, value, style) in &fields {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:w$}: ", label, w = max_label_all),
                    theme::dim(),
                ),
                Span::styled(value.clone(), *style),
            ]));
        }
    }

    let meta_width = if use_two_cols {
        left_col_w + gap + right_col_w
    } else {
        fields
            .iter()
            .map(|(_, v, _)| max_label_all + 2 + v.chars().count())
            .max()
            .unwrap_or(0)
    };

    lines.push(Line::from("".to_string()));
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(meta_width.min(w)),
        theme::dim(),
    )));
    lines.push(Line::from("".to_string()));

    // Render the body as styled markdown.
    let md_opts = markdown::Options::new(app.markdown_theme())
        .with_max_width(width as usize)
        .with_fold_level(app.fold_level())
        .with_adjust_color(theme::adjusted);
    let md_text = markdown::from_str_with_options(&task.body, &md_opts);
    let body_fg = theme::palette_text_normal();
    for line in md_text.lines {
        // Only fill in body_fg on spans whose line also has no fg color.
        // Lines with an explicit fg (headings, blockquotes) inherit their
        // color to child spans — forcing body_fg there would clobber them.
        let line_has_fg = line.style.fg.is_some();
        let owned_spans: Vec<Span<'static>> = line
            .spans
            .into_iter()
            .map(|s| {
                let mut style = s.style;
                if style.fg.is_none() && !line_has_fg {
                    style.fg = Some(body_fg);
                }
                Span::styled(s.content.into_owned(), style)
            })
            .collect();
        lines.push(Line::from(owned_spans).style(line.style));
    }

    // Precompute cumulative visual-row offsets (one-time O(n) at cache-build).
    let w = width.max(1);
    let mut offsets = Vec::with_capacity(lines.len() + 1);
    offsets.push(0usize);
    let mut cumulative = 0usize;
    for line in &lines {
        cumulative += Paragraph::new(vec![line.clone()])
            .wrap(Wrap { trim: false })
            .line_count(w); // requires ratatui "unstable-rendered-line-info" feature
        offsets.push(cumulative);
    }
    let rc_lines = std::rc::Rc::new(lines);
    let rc_offsets = std::rc::Rc::new(offsets);

    // Store in cache and return (Rc clones are cheap pointer bumps).
    app.detail.cache = Some(app::DetailLinesCache {
        task_id: task.id as u32,
        updated_epoch,
        body: task.body.clone(),
        width,
        theme: app.theme_kind,
        brightness_q: bq,
        saturation_q: sq,
        fold_level: app.fold_level(),
        lines: std::rc::Rc::clone(&rc_lines),
        vrow_offsets: std::rc::Rc::clone(&rc_offsets),
    });

    app::DetailContent {
        lines: rc_lines,
        vrow_offsets: rc_offsets,
    }
}

/// Highlight all case-insensitive occurrences of `query` in a Line's spans.
/// Uses `find_current_highlight` for the current-match line, `find_match_highlight`
/// for other matching lines.
fn highlight_find_in_line(line: &Line<'static>, query: &str, is_current: bool) -> Line<'static> {
    let hl_style = if is_current {
        theme::find_current_highlight()
    } else {
        theme::find_match_highlight()
    };
    let query_lower = query.to_lowercase();
    let query_len = query.chars().count();

    let mut result_spans: Vec<Span<'static>> = Vec::new();

    for span in &line.spans {
        let text = span.content.as_ref();
        let text_lower = text.to_lowercase();

        if !text_lower.contains(&query_lower) {
            // No matches in this span — keep as-is.
            result_spans.push(span.clone());
            continue;
        }

        // Split span at match boundaries.
        let chars: Vec<char> = text.chars().collect();
        let chars_lower: Vec<char> = text_lower.chars().collect();
        let mut pos = 0;

        while pos < chars.len() {
            // Try to find a match starting at or after `pos`.
            let remaining_lower: String = chars_lower[pos..].iter().collect();
            match remaining_lower.find(&query_lower) {
                Some(byte_offset) => {
                    // Convert byte offset in the remaining string to char offset.
                    let char_offset = remaining_lower[..byte_offset].chars().count();
                    let match_start = pos + char_offset;

                    // Emit pre-match text with original style.
                    if match_start > pos {
                        let pre: String = chars[pos..match_start].iter().collect();
                        result_spans.push(Span::styled(pre, span.style));
                    }

                    // Emit matched text with highlight style.
                    let match_end = (match_start + query_len).min(chars.len());
                    let matched: String = chars[match_start..match_end].iter().collect();
                    result_spans.push(Span::styled(matched, hl_style));

                    pos = match_end;
                }
                None => {
                    // No more matches — emit remainder.
                    let rest: String = chars[pos..].iter().collect();
                    result_spans.push(Span::styled(rest, span.style));
                    break;
                }
            }
        }
    }

    Line::from(result_spans).style(line.style)
}

/// Optional find-in-page context for highlighting matches.
pub(super) struct FindContext<'a> {
    pub query: &'a str,
    pub matches: &'a [usize],
    pub current: usize,
}

/// Render pre-built `DetailContent` into `area` with scroll and optional find
/// highlighting.  Both the detail view and the reader panel call this.
pub(super) fn render_scrolled_content(
    content: &app::DetailContent,
    raw_scroll: usize,
    area: Rect,
    frame: &mut Frame,
    find: Option<FindContext<'_>>,
) {
    let total_vrows = content.total_vrows();
    let scroll = raw_scroll.min(total_vrows.saturating_sub(1));

    let (mut visible, local_scroll) = content.viewport_slice(scroll, area.height);

    if let Some(ctx) = &find {
        if !ctx.query.is_empty() {
            let match_set: std::collections::HashSet<usize> =
                ctx.matches.iter().copied().collect();
            let current_match_line = ctx.matches.get(ctx.current).copied();
            let (slice_start, _) = content.viewport_range(scroll, area.height);
            for (vi, line) in visible.iter_mut().enumerate() {
                let global_i = slice_start + vi;
                if match_set.contains(&global_i) {
                    let is_current = current_match_line == Some(global_i);
                    *line = highlight_find_in_line(line, ctx.query, is_current);
                }
            }
        }
    }

    let paragraph = Paragraph::new(visible)
        .wrap(Wrap { trim: false })
        .scroll((local_scroll as u16, 0));
    frame.render_widget(paragraph, area);
}
