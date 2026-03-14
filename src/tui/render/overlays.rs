//! Overlay rendering: help screen, search DSL help, debug info.

use ratatui::{
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme;
use super::layout::{centered_fixed, centered_rect, pad_right};

pub(super) fn render_help(app: &App, frame: &mut Frame) {
    let area = centered_rect(60, 80, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Keyboard Shortcuts ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::uniform(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Two-column help entries: (key_l, desc_l, key_r, desc_r).
    // Section headers have desc_l empty, key_l = section name.
    let entries: &[(&str, &str, &str, &str)] = &[
        ("Navigation", "", "Actions", ""),
        ("h/{/\u{2190}", "Prev column (cycle)", "c", "Create task"),
        ("l/}/\u{2192}", "Next column (cycle)", "e", "Edit task"),
        ("j/]/\u{2193}", "Next task", "d", "Delete task"),
        ("k/[/\u{2191}", "Prev task", "m", "Move task"),
        ("g/Home", "Top of column", "+/-", "Raise/lower priority"),
        ("G/End", "Bottom of column", "y", "Yank task content"),
        ("J/K", "Half-page scroll", "Y", "Yank task path"),
        ("PgUp/Dn", "Page scroll", "o", "Open in $EDITOR"),
        ("^J/^K", "Full page scroll", "Enter", "Open detail view"),
        ("", "", "", ""),
        ("Display", "", "Reader", ""),
        ("V", "Toggle card/list view", "R", "Toggle reader panel"),
        ("s/S", "Cycle sort mode", "z/Z", "Fold deeper/shallower"),
        ("t", "Cycle theme", "</>", "Narrow/widen panel"),
        (",/.", "Brightness -/+", "'/\"", "Next/prev ## heading"),
        ("M-,/M-.", "Saturation -/+", "(/)", "Next/prev heading"),
        ("T", "Reset adjustments", "", ""),
        ("a", "Cycle time mode", "", ""),
        ("x", "Collapse column", "", ""),
        ("X", "Expand all columns", "", ""),
        ("1-9", "Solo column", "", ""),
        ("!-*", "Toggle column", "", ""),
        ("", "", "", ""),
        ("Search & Filter", "", "Other", ""),
        ("/", "Search filter (? syntax)", "r", "Reload board"),
        ("f", "Find tasks", "u/^Z", "Undo"),
        ("^F", "Open search", "^R", "Redo"),
        ("w", "Toggle worktree filter", ":/^G", "Go to task #"),
        ("b", "Assign branch", "^D", "Debug info"),
        ("C", "Switch context", "?", "This help"),
        ("W", "Context (worktree)", "q/Esc", "Quit"),
        ("v", "Visual mode (select)", "^C", "Force quit"),
        ("", "", "", ""),
        ("Detail View", "", "", ""),
        ("j/k", "Scroll up/down", "m", "Move task"),
        ("]/[", "Scroll 3 lines", "y", "Yank task content"),
        ("J/K", "Half-page scroll", "Y", "Yank task path"),
        ("d/u", "Half-page (vim)", "o", "Open in $EDITOR"),
        ("^J/^K", "Full page scroll", "v", "Visual mode (select)"),
        ("g/G", "Top / bottom", "t", "Cycle theme"),
        ("({/})", "Prev/next heading", ",/.", "Brightness -/+"),
        ("'/\"", "Next/prev ## heading", "M-,/M-.", "Saturation -/+"),
        ("1-9", "Jump to heading", "T", "Reset adjustments"),
        ("/", "Find in text", "z/Z", "Fold deeper/shallower"),
        ("n/N", "Find next/prev", "</>", "Narrow/widen width"),
        ("", "", ":/^G", "Go to task #"),
        ("", "", "q/Esc", "Back to board"),
    ];

    let filter = app.help_filter.to_lowercase();
    let has_filter = !filter.is_empty();

    // Build all lines, filtering as we go.
    let mut all_lines: Vec<Line> = Vec::new();

    // If filter is active, show filter bar first.
    if app.help_filter_active {
        all_lines.push(Line::from(vec![
            Span::styled("/ ", theme::help_key()),
            Span::styled(
                format!("{}\u{2588}", app.help_filter),
                theme::title_inactive(),
            ),
        ]));
        all_lines.push(Line::from(""));
    } else if has_filter {
        all_lines.push(Line::from(vec![
            Span::styled("Filter: ", theme::help_key()),
            Span::styled(app.help_filter.clone(), theme::title_inactive()),
        ]));
        all_lines.push(Line::from(""));
    }

    // Track current section entries for filtering.
    let mut i = 0;
    while i < entries.len() {
        let &(key_l, desc_l, key_r, desc_r) = &entries[i];

        // Blank separator row.
        if key_l.is_empty() && desc_l.is_empty() && key_r.is_empty() && desc_r.is_empty() {
            if !has_filter {
                all_lines.push(Line::from(""));
            }
            i += 1;
            continue;
        }

        // Section header row (desc_l is empty, key_l is section name).
        let is_header = desc_l.is_empty() && !key_l.is_empty();
        if is_header {
            if has_filter {
                // Only show header if any subsequent entries in this section match.
                let mut has_match = false;
                let mut j = i + 1;
                while j < entries.len() {
                    let &(kl, dl, kr, dr) = &entries[j];
                    // Stop at next separator or next header.
                    if (kl.is_empty() && dl.is_empty() && kr.is_empty() && dr.is_empty())
                        || (dl.is_empty() && !kl.is_empty())
                    {
                        break;
                    }
                    if kl.to_lowercase().contains(&filter)
                        || dl.to_lowercase().contains(&filter)
                        || kr.to_lowercase().contains(&filter)
                        || dr.to_lowercase().contains(&filter)
                    {
                        has_match = true;
                        break;
                    }
                    j += 1;
                }
                // Also check right-side header.
                if !has_match && !key_r.is_empty() {
                    let mut j = i + 1;
                    while j < entries.len() {
                        let &(_, _, kr, dr) = &entries[j];
                        if kr.is_empty() && dr.is_empty() {
                            j += 1;
                            continue;
                        }
                        if kr.to_lowercase().contains(&filter)
                            || dr.to_lowercase().contains(&filter)
                        {
                            has_match = true;
                            break;
                        }
                        j += 1;
                    }
                }
                if !has_match {
                    i += 1;
                    continue;
                }
            }

            let mut spans = Vec::new();
            spans.push(Span::styled(pad_right(key_l, 36), theme::help_key()));
            if !key_r.is_empty() {
                spans.push(Span::styled(key_r.to_string(), theme::help_key()));
            }
            all_lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // Regular entry row — apply filter.
        if has_filter {
            let matches_left = key_l.to_lowercase().contains(&filter)
                || desc_l.to_lowercase().contains(&filter);
            let matches_right = key_r.to_lowercase().contains(&filter)
                || desc_r.to_lowercase().contains(&filter);
            if !matches_left && !matches_right {
                i += 1;
                continue;
            }
        }

        let mut spans = Vec::new();
        spans.push(Span::styled(pad_right(key_l, 8), theme::help_key()));
        spans.push(Span::styled(pad_right(desc_l, 28), theme::help_desc()));

        if desc_r.is_empty() && !key_r.is_empty() {
            spans.push(Span::styled(key_r.to_string(), theme::help_key()));
        } else if !key_r.is_empty() {
            spans.push(Span::styled(pad_right(key_r, 8), theme::help_key()));
            spans.push(Span::styled(desc_r.to_string(), theme::help_desc()));
        }

        all_lines.push(Line::from(spans));
        i += 1;
    }

    // Footer hint.
    all_lines.push(Line::from(""));
    all_lines.push(Line::from(Span::styled(
        "/:filter  j/k:scroll  g/G:top/bottom  ?/esc:close",
        theme::dim(),
    )));

    // Apply scroll offset.
    let max_scroll = all_lines.len().saturating_sub(1);
    let scroll = app.help_scroll.min(max_scroll);

    let paragraph = Paragraph::new(all_lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_search_help(app: &App, frame: &mut Frame) {
    let area = centered_fixed(56, 33, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Search Syntax ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let k = |s: &str| Span::styled(s.to_string(), theme::help_key());
    let d = |s: &str| Span::styled(s.to_string(), theme::help_desc());
    let dim = |s: &str| Span::styled(s.to_string(), theme::dim());

    let mut lines: Vec<Line> = Vec::new();

    // -- Time filters
    lines.push(Line::from(k("Time")));
    lines.push(Line::from(vec![
        k("  @48h  @3d  @2w  @1mo  @today"),
    ]));
    lines.push(Line::from(vec![
        d("  "),
        dim("within duration (follows age mode)"),
    ]));
    lines.push(Line::from(vec![
        k("  @>2w"),
        d("              "),
        dim("older than duration"),
    ]));
    lines.push(Line::from(vec![
        k("  created:3d  updated:>1w"),
    ]));
    lines.push(Line::from(vec![
        d("  "),
        dim("explicit field (ignores age mode)"),
    ]));
    lines.push(Line::from(vec![
        d("  "),
        dim("units: m h d w mo  (30m 48h 3d 2w 1mo)"),
    ]));
    lines.push(Line::from(""));

    // -- Priority filters
    lines.push(Line::from(k("Priority")));
    lines.push(Line::from(vec![
        k("  p:high"),
        d("            "),
        dim("exact match"),
    ]));
    lines.push(Line::from(vec![
        k("  p:medium+"),
        d("         "),
        dim("at or above"),
    ]));
    lines.push(Line::from(vec![
        k("  p:high-"),
        d("           "),
        dim("at or below"),
    ]));
    lines.push(Line::from(vec![
        d("  "),
        dim("prefixes: c h m l  (e.g. p:c = critical)"),
    ]));
    lines.push(Line::from(""));

    // -- ID filters
    lines.push(Line::from(k("ID")));
    lines.push(Line::from(vec![
        k("  #5  id:5  id:1,3,7  id:5-10"),
    ]));
    lines.push(Line::from(""));

    // -- Semantic search
    lines.push(Line::from(k("Semantic")));
    lines.push(Line::from(vec![
        k("  ~query"),
        d("            "),
        dim("embedding search"),
    ]));
    lines.push(Line::from(vec![
        d("  "),
        dim("combine: p:high ~error handling"),
    ]));
    lines.push(Line::from(""));

    // -- Free text
    lines.push(Line::from(k("Text")));
    lines.push(Line::from(vec![
        d("  "),
        dim("any other token: substring in title/body/tags"),
    ]));
    lines.push(Line::from(""));

    // -- Combination
    lines.push(Line::from(k("Combining")));
    lines.push(Line::from(vec![
        d("  "),
        dim("all filters AND together"),
    ]));
    lines.push(Line::from(vec![
        k("  @24h p:h+ ~perf fix"),
        d("  "),
        dim("\u{2190} all apply"),
    ]));
    lines.push(Line::from(""));

    // -- Footer
    lines.push(Line::from(dim(
        "j/k:scroll  ?/esc:close",
    )));

    // Apply scroll offset.
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let scroll = app.search_help_scroll.min(max_scroll);

    let paragraph = Paragraph::new(lines).scroll((scroll as u16, 0));
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_debug(app: &App, frame: &mut Frame) {
    let area = centered_rect(70, 80, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::new()
        .title(" Debug Info ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::dialog_border())
        .padding(Padding::uniform(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let expanded = app.columns.iter().filter(|c| !c.collapsed).count();
    let collapsed = app.columns.iter().filter(|c| c.collapsed).count();
    let total_tasks: usize = app.columns.iter().map(|c| c.tasks.len()).sum();

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled("General", theme::help_key())));
    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        "  Terminal size:  {}x{}",
        app.terminal_width, app.terminal_height
    )));
    lines.push(Line::from(format!(
        "  Columns:        {} ({} expanded, {} collapsed)",
        app.columns.len(),
        expanded,
        collapsed
    )));
    lines.push(Line::from(format!(
        "  Total tasks:    {}",
        total_tasks
    )));
    lines.push(Line::from(format!(
        "  Active pos:     col={}, row={}",
        app.active_col, app.active_row
    )));
    lines.push(Line::from(format!(
        "  View mode:      {:?}",
        app.view_mode
    )));
    lines.push(Line::from(format!(
        "  Sort mode:      {:?}",
        app.sort_mode
    )));
    lines.push(Line::from(format!(
        "  Time mode:      {:?}",
        app.time_mode
    )));
    lines.push(Line::from(format!(
        "  Theme:          {:?}",
        app.theme_kind
    )));
    lines.push(Line::from(format!(
        "  Brightness:     {:+.0}%",
        app.brightness * 100.0
    )));
    lines.push(Line::from(format!(
        "  Saturation:     {:+.0}%",
        app.saturation * 100.0
    )));
    lines.push(Line::from(""));

    // Per-column task counts.
    lines.push(Line::from(Span::styled("Columns", theme::help_key())));
    lines.push(Line::from(""));
    for (i, col) in app.columns.iter().enumerate() {
        let state = if col.collapsed { " [collapsed]" } else { "" };
        let active = if i == app.active_col { " <-" } else { "" };
        lines.push(Line::from(format!(
            "  {:2}. {:<16} {:>3} tasks{}{}",
            i,
            col.name,
            col.tasks.len(),
            state,
            active
        )));
    }
    lines.push(Line::from(""));

    // Search / reader state.
    lines.push(Line::from(Span::styled("State", theme::help_key())));
    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        "  Search query:   \"{}\"",
        app.search.query
    )));
    lines.push(Line::from(format!(
        "  Reader:         {}  pct={}%  detail_max={}",
        if app.reader_open { "open" } else { "closed" },
        app.reader_width_pct,
        app.reader_max_width
    )));
    lines.push(Line::from(format!(
        "  Fold level:     {}",
        app.fold_level()
    )));
    lines.push(Line::from(format!(
        "  Config dir:     {}",
        app.cfg.dir().display()
    )));
    lines.push(Line::from(""));

    // Footer.
    lines.push(Line::from(Span::styled(
        "j/k:scroll  g/G:top/bottom  q/esc/^D:close",
        theme::dim(),
    )));

    // Apply scroll offset.
    let max_scroll = lines.len().saturating_sub(1);
    let scroll = app.debug.scroll.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, inner);
}
