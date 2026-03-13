//! Layout utility functions: centering, truncation, padding.

use ratatui::layout::{Constraint, Layout, Rect};

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

pub(super) fn centered_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

pub(super) fn truncate(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else if max_len <= 1 {
        "\u{2026}".to_string()
    } else {
        let mut result: String = chars[..max_len - 1].iter().collect();
        result.push('\u{2026}');
        result
    }
}

pub(super) fn pad_right(s: &str, width: usize) -> String {
    let char_len = s.chars().count();
    if char_len >= width {
        s.chars().take(width).collect()
    } else {
        format!("{}{}", s, " ".repeat(width - char_len))
    }
}
