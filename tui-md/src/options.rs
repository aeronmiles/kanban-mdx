// Forked from tui-markdown (https://github.com/joshka/tui-markdown)
// Original: MIT / Apache-2.0 — Copyright Josh McKinney 2024

use crate::style_sheet::{DefaultStyleSheet, StyleSheet};

/// Identity color transform (no adjustment).
fn identity_color(c: ratatui::style::Color) -> ratatui::style::Color {
    c
}

/// Rendering options for the markdown converter.
#[derive(Clone)]
#[non_exhaustive]
pub struct Options<S: StyleSheet = DefaultStyleSheet> {
    pub(crate) styles: S,
    /// Maximum width for block elements like tables. `None` means unlimited.
    pub(crate) max_width: Option<usize>,
    /// Heading fold level. `0` means show everything. Level `N` (>= 2)
    /// collapses content under headings of depth `>= N` (e.g. `3` folds
    /// `###` sections, `2` folds `##` and deeper). Values < 2 are no-ops.
    pub(crate) fold_level: usize,
    /// Color adjustment function applied to syntax-highlighted code spans.
    pub(crate) adjust_color: fn(ratatui::style::Color) -> ratatui::style::Color,
}

impl<S: StyleSheet> Options<S> {
    pub fn new(styles: S) -> Self {
        Self {
            styles,
            max_width: None,
            fold_level: 0,
            adjust_color: identity_color,
        }
    }

    /// Set the maximum width for block elements (tables).
    pub fn with_max_width(mut self, width: usize) -> Self {
        self.max_width = Some(width);
        self
    }

    /// Set the heading fold level. See [`Options::fold_level`].
    pub fn with_fold_level(mut self, level: usize) -> Self {
        self.fold_level = level;
        self
    }

    /// Set a color adjustment function for syntax-highlighted code.
    pub fn with_adjust_color(mut self, f: fn(ratatui::style::Color) -> ratatui::style::Color) -> Self {
        self.adjust_color = f;
        self
    }
}

impl Default for Options<DefaultStyleSheet> {
    fn default() -> Self {
        Self::new(DefaultStyleSheet)
    }
}

// ---------------------------------------------------------------------------
// Heading-fold event filter
// ---------------------------------------------------------------------------

use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};

/// Heading depth as a `u8` (1–6).
pub(crate) fn heading_depth(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Filters a collected event stream to collapse content under headings
/// whose depth is `>= fold_level`. `fold_level` is the heading level
/// (e.g. 2 = fold `##`, 3 = fold `###`). Values < 2 are a no-op.
///
/// The heading itself is preserved, followed by a fold indicator
/// (`" ▸ …"`). All content between the heading and the next heading at
/// the same-or-shallower level is removed.
pub(crate) fn fold_events<'a>(events: Vec<Event<'a>>, fold_level: usize) -> Vec<Event<'a>> {
    if fold_level < 2 {
        return events;
    }
    let threshold = fold_level as u8;
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut suppressing: Option<u8> = None;
    // Track whether we're inside a heading tag (between Start and End).
    // Events inside the heading itself must always pass through so the
    // heading text is rendered even when folded.
    let mut in_heading: bool = false;
    // The depth of the heading that should start suppression once it ends.
    let mut pending_fold: Option<u8> = None;

    for event in events {
        // Heading start: may lift suppression or prepare a new fold.
        if let Event::Start(Tag::Heading { level, .. }) = &event {
            let depth = heading_depth(*level);
            in_heading = true;

            // A heading at or above the suppressed depth lifts suppression.
            if let Some(sup_depth) = suppressing {
                if depth <= sup_depth {
                    suppressing = None;
                }
            }

            // Mark this heading for folding (suppression begins at its End).
            if depth >= threshold {
                pending_fold = Some(depth);
            }

            out.push(event);
            continue;
        }

        // Heading end: if marked for folding, start suppression and inject indicator.
        if let Event::End(TagEnd::Heading(_)) = &event {
            in_heading = false;
            out.push(event);

            if let Some(depth) = pending_fold.take() {
                suppressing = Some(depth);
                out.push(Event::Start(Tag::Paragraph));
                out.push(Event::Text(" ▸ …".into()));
                out.push(Event::End(TagEnd::Paragraph));
            }
            continue;
        }

        // Always let events inside a heading tag through.
        if in_heading {
            out.push(event);
            continue;
        }

        // While suppressing, drop non-heading events.
        if suppressing.is_some() {
            continue;
        }

        out.push(event);
    }

    out
}
