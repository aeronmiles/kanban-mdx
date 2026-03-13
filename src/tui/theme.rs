//! TUI theme and color definitions.
//!
//! All style functions read the active [`ThemeKind`] from a thread-local cell
//! that is set once per render frame via [`set_active`]. This keeps call-site
//! signatures unchanged (`theme::dim()`, `theme::help_key()`, etc.) while
//! making every visual element theme-aware.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};

use ratatui::style::{Color, Modifier, Style};

use tui_md::StyleSheet;

// ---------------------------------------------------------------------------
// Active theme (thread-local)
// ---------------------------------------------------------------------------

/// Available colour themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
    Dracula,
    TokyoNight,
    Pink,
    Ascii,
}

impl ThemeKind {
    /// Advance to the next theme in the cycle.
    pub fn next(&self) -> ThemeKind {
        match self {
            ThemeKind::Dark => ThemeKind::Light,
            ThemeKind::Light => ThemeKind::Dracula,
            ThemeKind::Dracula => ThemeKind::TokyoNight,
            ThemeKind::TokyoNight => ThemeKind::Pink,
            ThemeKind::Pink => ThemeKind::Ascii,
            ThemeKind::Ascii => ThemeKind::Dark,
        }
    }

    /// Human-readable label for the status bar.
    pub fn label(&self) -> &str {
        match self {
            ThemeKind::Dark => "Dark",
            ThemeKind::Light => "Light",
            ThemeKind::Dracula => "Dracula",
            ThemeKind::TokyoNight => "Tokyo Night",
            ThemeKind::Pink => "Pink",
            ThemeKind::Ascii => "Ascii",
        }
    }

    /// Total number of themes in the cycle.
    pub fn count() -> usize {
        6
    }

    /// Config string representation (matches Go glamour/styles constants).
    pub fn as_config_str(&self) -> &str {
        match self {
            ThemeKind::Dark => "dark",
            ThemeKind::Light => "light",
            ThemeKind::Dracula => "dracula",
            ThemeKind::TokyoNight => "tokyo-night",
            ThemeKind::Pink => "pink",
            ThemeKind::Ascii => "ascii",
        }
    }

    /// Parse from config string. Returns `Dark` for unknown values.
    pub fn from_config_str(s: &str) -> ThemeKind {
        match s {
            "dark" => ThemeKind::Dark,
            "light" => ThemeKind::Light,
            "dracula" => ThemeKind::Dracula,
            "tokyo-night" => ThemeKind::TokyoNight,
            "pink" => ThemeKind::Pink,
            "ascii" => ThemeKind::Ascii,
            _ => ThemeKind::Dark,
        }
    }
}

thread_local! {
    static ACTIVE_THEME: Cell<ThemeKind> = const { Cell::new(ThemeKind::Dark) };
    static BRIGHTNESS: Cell<f32> = const { Cell::new(0.0) };
    static SATURATION: Cell<f32> = const { Cell::new(0.0) };
    static PALETTE_CACHE: RefCell<Option<Palette>> = const { RefCell::new(None) };
}

/// Set the active theme and adjustments for this render frame.
pub fn set_active(kind: ThemeKind) {
    ACTIVE_THEME.with(|c| c.set(kind));
    PALETTE_CACHE.with(|c| *c.borrow_mut() = None);
}

/// Set brightness and saturation adjustments (-1.0 to 1.0).
pub fn set_adjustments(brightness: f32, saturation: f32) {
    BRIGHTNESS.with(|c| c.set(brightness));
    SATURATION.with(|c| c.set(saturation));
    PALETTE_CACHE.with(|c| *c.borrow_mut() = None);
}

fn active() -> ThemeKind {
    ACTIVE_THEME.with(|c| c.get())
}

fn brightness() -> f32 {
    BRIGHTNESS.with(|c| c.get())
}

fn saturation_adj() -> f32 {
    SATURATION.with(|c| c.get())
}

// ---------------------------------------------------------------------------
// Color adjustment (brightness / saturation via HSL)
// ---------------------------------------------------------------------------

/// ANSI 256-color cube level → 8-bit RGB value.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Convert an ANSI 256-color index to (R, G, B).
fn indexed_to_rgb(idx: u8) -> (u8, u8, u8) {
    match idx {
        // Standard and bright colours (approximate).
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        // 6×6×6 colour cube.
        16..=231 => {
            let n = idx - 16;
            let r = CUBE_LEVELS[(n / 36) as usize];
            let g = CUBE_LEVELS[((n % 36) / 6) as usize];
            let b = CUBE_LEVELS[(n % 6) as usize];
            (r, g, b)
        }
        // Grayscale ramp.
        232..=255 => {
            let v = 8 + (idx - 232) * 10;
            (v, v, v)
        }
    }
}

/// Convert (R, G, B) to (H, S, L) where H is 0..360, S and L are 0..1.
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - rf).abs() < f32::EPSILON {
        let mut h = (gf - bf) / d;
        if gf < bf {
            h += 6.0;
        }
        h
    } else if (max - gf).abs() < f32::EPSILON {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

/// Convert (H, S, L) back to (R, G, B).
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < f32::EPSILON {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hn = h / 360.0;
    let r = (hue_to_rgb(p, q, hn + 1.0 / 3.0) * 255.0).round() as u8;
    let g = (hue_to_rgb(p, q, hn) * 255.0).round() as u8;
    let b = (hue_to_rgb(p, q, hn - 1.0 / 3.0) * 255.0).round() as u8;
    (r, g, b)
}

/// Adjust a colour's brightness and saturation.
/// `bright` and `sat` are offsets in -1.0..1.0.
pub fn adjust_color(color: Color, bright: f32, sat: f32) -> Color {
    if bright.abs() < f32::EPSILON && sat.abs() < f32::EPSILON {
        return color;
    }
    let (r, g, b) = match color {
        Color::Indexed(idx) => indexed_to_rgb(idx),
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return color, // Reset, Black, etc. — leave untouched
    };
    let (h, s, l) = rgb_to_hsl(r, g, b);
    let new_l = (l + bright).clamp(0.0, 1.0);
    let new_s = (s + sat).clamp(0.0, 1.0);
    let (nr, ng, nb) = hsl_to_rgb(h, new_s, new_l);
    Color::Rgb(nr, ng, nb)
}

// ---------------------------------------------------------------------------
// Internal palette — one struct holds every colour slot for a theme
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Palette {
    // Accent colours
    accent: Color,        // primary accent (borders, highlights)
    accent_bright: Color, // bright variant (search bg, secondary accent)
    text_on_accent: Color, // text rendered on accent bg

    // Text hierarchy
    text_bright: Color, // brightest text (active titles)
    text_normal: Color, // standard body text
    text_dim: Color,    // secondary / labels

    // Backgrounds
    bg_bar: Color, // status bar, header bar bg

    // Semantic colours
    error: Color,
    claim: Color,
    tag: Color,
    branch: Color,

    // Search / find
    search_bg: Color,
    find_current_bg: Color,
    find_match_bg: Color,

    // Priority palette
    priority_critical: Color,
    priority_high: Color,
    priority_medium: Color,
    priority_low: Color,

    // Card borders
    card_border: Color, // inactive card border

    // Markdown-specific
    md_heading_primary: Color,
    md_heading_secondary: Color,
    md_code_fg: Color,
    md_code_bg: Color,
    md_link: Color,
    md_blockquote: Color,
    md_table_border: Color,
}

impl Palette {
    /// Apply brightness/saturation adjustments to all colour fields.
    fn adjusted(self) -> Self {
        let b = brightness();
        let s = saturation_adj();
        if b.abs() < f32::EPSILON && s.abs() < f32::EPSILON {
            return self;
        }
        let a = |c: Color| adjust_color(c, b, s);
        Palette {
            accent: a(self.accent),
            accent_bright: a(self.accent_bright),
            text_on_accent: a(self.text_on_accent),
            text_bright: a(self.text_bright),
            text_normal: a(self.text_normal),
            text_dim: a(self.text_dim),
            bg_bar: a(self.bg_bar),
            error: a(self.error),
            claim: a(self.claim),
            tag: a(self.tag),
            branch: a(self.branch),
            search_bg: a(self.search_bg),
            find_current_bg: a(self.find_current_bg),
            find_match_bg: a(self.find_match_bg),
            priority_critical: a(self.priority_critical),
            priority_high: a(self.priority_high),
            priority_medium: a(self.priority_medium),
            priority_low: a(self.priority_low),
            card_border: a(self.card_border),
            md_heading_primary: a(self.md_heading_primary),
            md_heading_secondary: a(self.md_heading_secondary),
            md_code_fg: a(self.md_code_fg),
            md_code_bg: a(self.md_code_bg),
            md_link: a(self.md_link),
            md_blockquote: a(self.md_blockquote),
            md_table_border: a(self.md_table_border),
        }
    }
}

fn palette() -> Palette {
    PALETTE_CACHE.with(|cell| {
        if let Some(p) = *cell.borrow() {
            return p;
        }
        let p = build_palette();
        *cell.borrow_mut() = Some(p);
        p
    })
}

fn build_palette() -> Palette {
    match active() {
        ThemeKind::Dark => Palette {
            accent: Color::Indexed(130),        // copper-orange
            accent_bright: Color::Indexed(214),  // gold
            text_on_accent: Color::Indexed(230), // cream
            text_bright: Color::Indexed(255),    // bright white
            text_normal: Color::Indexed(252),    // light gray
            text_dim: Color::Indexed(241),       // mid gray
            bg_bar: Color::Indexed(235),         // dimmed 30% (was 236)
            error: Color::Indexed(196),          // red
            claim: Color::Indexed(36),           // teal-green
            tag: Color::Indexed(65),             // dark sage
            branch: Color::Indexed(108),         // sage green
            search_bg: Color::Indexed(214),      // gold
            find_current_bg: Color::Indexed(226), // bright yellow
            find_match_bg: Color::Indexed(178),  // dim yellow
            priority_critical: Color::Indexed(196),
            priority_high: Color::Indexed(208),   // orange
            priority_medium: Color::Indexed(214), // gold
            priority_low: Color::Indexed(243),    // muted gray
            card_border: Color::Indexed(240),
            md_heading_primary: Color::Indexed(130),
            md_heading_secondary: Color::Indexed(214),
            md_code_fg: Color::Indexed(252),
            md_code_bg: Color::Indexed(233),     // dimmed 55% (was 236)
            md_link: Color::Indexed(108),
            md_blockquote: Color::Indexed(246),
            md_table_border: Color::Indexed(240),
        }
        .adjusted(),
        ThemeKind::Light => Palette {
            accent: Color::Indexed(25),          // deep blue
            accent_bright: Color::Indexed(33),   // bright blue
            text_on_accent: Color::Indexed(231), // white
            text_bright: Color::Indexed(232),    // near-black
            text_normal: Color::Indexed(238),    // dark gray
            text_dim: Color::Indexed(245),       // mid gray
            bg_bar: Color::Indexed(247),         // dimmed 30% (was 254)
            error: Color::Indexed(160),          // dark red
            claim: Color::Indexed(30),           // dark teal
            tag: Color::Indexed(22),             // dark green
            branch: Color::Indexed(27),          // blue
            search_bg: Color::Indexed(220),      // amber
            find_current_bg: Color::Indexed(226), // bright yellow
            find_match_bg: Color::Indexed(229),  // pale yellow
            priority_critical: Color::Indexed(160),
            priority_high: Color::Indexed(172),   // orange
            priority_medium: Color::Indexed(172), // dark gold
            priority_low: Color::Indexed(245),
            card_border: Color::Indexed(250),
            md_heading_primary: Color::Indexed(25),
            md_heading_secondary: Color::Indexed(130),
            md_code_fg: Color::Indexed(238),
            md_code_bg: Color::Indexed(243),     // dimmed 55% (was 254)
            md_link: Color::Indexed(27),
            md_blockquote: Color::Indexed(243),
            md_table_border: Color::Indexed(250),
        }
        .adjusted(),
        ThemeKind::Dracula => Palette {
            accent: Color::Indexed(141),         // purple
            accent_bright: Color::Indexed(117),  // cyan
            text_on_accent: Color::Indexed(232), // near-black
            text_bright: Color::Indexed(231),    // white
            text_normal: Color::Indexed(253),    // light gray
            text_dim: Color::Indexed(245),       // mid gray
            bg_bar: Color::Indexed(235),         // dimmed 30% (was 236)
            error: Color::Indexed(203),          // pink-red
            claim: Color::Indexed(84),           // green
            tag: Color::Indexed(228),            // yellow
            branch: Color::Indexed(117),         // cyan
            search_bg: Color::Indexed(141),      // purple
            find_current_bg: Color::Indexed(228), // yellow
            find_match_bg: Color::Indexed(61),   // muted purple
            priority_critical: Color::Indexed(203),
            priority_high: Color::Indexed(208),   // orange
            priority_medium: Color::Indexed(228), // yellow
            priority_low: Color::Indexed(245),
            card_border: Color::Indexed(61),      // muted purple
            md_heading_primary: Color::Indexed(141),
            md_heading_secondary: Color::Indexed(117),
            md_code_fg: Color::Indexed(231),
            md_code_bg: Color::Indexed(233),     // dimmed 55% (was 236)
            md_link: Color::Indexed(84),
            md_blockquote: Color::Indexed(228),
            md_table_border: Color::Indexed(61),
        }
        .adjusted(),
        ThemeKind::TokyoNight => Palette {
            accent: Color::Indexed(111),         // blue
            accent_bright: Color::Indexed(180),  // amber
            text_on_accent: Color::Indexed(232), // near-black
            text_bright: Color::Indexed(189),    // pale blue-white
            text_normal: Color::Indexed(252),    // light gray
            text_dim: Color::Indexed(243),       // mid gray
            bg_bar: Color::Indexed(234),         // dimmed 30% (was 235)
            error: Color::Indexed(167),          // muted red
            claim: Color::Indexed(73),           // teal
            tag: Color::Indexed(150),            // sage
            branch: Color::Indexed(109),         // muted blue
            search_bg: Color::Indexed(111),      // blue
            find_current_bg: Color::Indexed(180), // amber
            find_match_bg: Color::Indexed(59),   // muted
            priority_critical: Color::Indexed(167),
            priority_high: Color::Indexed(208),   // orange
            priority_medium: Color::Indexed(180), // amber
            priority_low: Color::Indexed(243),
            card_border: Color::Indexed(59),
            md_heading_primary: Color::Indexed(111),
            md_heading_secondary: Color::Indexed(180),
            md_code_fg: Color::Indexed(189),
            md_code_bg: Color::Indexed(234),     // dimmed 35% (was 235)
            md_link: Color::Indexed(73),
            md_blockquote: Color::Indexed(109),
            md_table_border: Color::Indexed(59),
        }
        .adjusted(),
        ThemeKind::Pink => Palette {
            accent: Color::Indexed(198),         // hot pink
            accent_bright: Color::Indexed(213),  // light pink
            text_on_accent: Color::Indexed(232), // near-black
            text_bright: Color::Indexed(231),    // white
            text_normal: Color::Indexed(252),    // light gray
            text_dim: Color::Indexed(243),       // mid gray
            bg_bar: Color::Indexed(235),         // dimmed 30% (was 236)
            error: Color::Indexed(196),          // red
            claim: Color::Indexed(219),          // pastel pink
            tag: Color::Indexed(207),            // magenta
            branch: Color::Indexed(183),         // lavender
            search_bg: Color::Indexed(198),      // hot pink
            find_current_bg: Color::Indexed(213), // light pink
            find_match_bg: Color::Indexed(162),  // dark magenta
            priority_critical: Color::Indexed(196),
            priority_high: Color::Indexed(208),   // orange
            priority_medium: Color::Indexed(213), // light pink
            priority_low: Color::Indexed(243),
            card_border: Color::Indexed(238),
            md_heading_primary: Color::Indexed(198),
            md_heading_secondary: Color::Indexed(213),
            md_code_fg: Color::Indexed(231),
            md_code_bg: Color::Indexed(233),     // dimmed 55% (was 236)
            md_link: Color::Indexed(207),
            md_blockquote: Color::Indexed(183),
            md_table_border: Color::Indexed(162),
        }
        .adjusted(),
        ThemeKind::Ascii => Palette {
            // Grayscale hierarchy: 255=white → 232=near-black
            accent: Color::Indexed(255),         // white — strongest
            accent_bright: Color::Indexed(253),  // near-white
            text_on_accent: Color::Indexed(232), // near-black on white bg
            text_bright: Color::Indexed(255),    // white — titles
            text_normal: Color::Indexed(252),    // light — body text
            text_dim: Color::Indexed(245),       // mid — labels, secondary
            bg_bar: Color::Indexed(235),         // dimmed 30% (was 236)
            error: Color::Indexed(255),          // bright (bold modifier adds emphasis)
            claim: Color::Indexed(250),          // light-mid
            tag: Color::Indexed(248),            // mid-light
            branch: Color::Indexed(250),         // light-mid
            search_bg: Color::Indexed(255),      // white bg for contrast
            find_current_bg: Color::Indexed(253),
            find_match_bg: Color::Indexed(248),
            priority_critical: Color::Indexed(255), // brightest
            priority_high: Color::Indexed(252),     // bright
            priority_medium: Color::Indexed(248),   // mid
            priority_low: Color::Indexed(242),      // dim
            card_border: Color::Indexed(243),       // mid-dim
            md_heading_primary: Color::Indexed(255),
            md_heading_secondary: Color::Indexed(250),
            md_code_fg: Color::Indexed(252),
            md_code_bg: Color::Indexed(233),     // dimmed 55% (was 236)
            md_link: Color::Indexed(252),
            md_blockquote: Color::Indexed(246),
            md_table_border: Color::Indexed(243),
        }
        .adjusted(),
    }
}

// ---------------------------------------------------------------------------
// Public style functions — signatures unchanged, now palette-driven
// ---------------------------------------------------------------------------

/// Active column header: bright text on accent background, bold.
pub fn header_active() -> Style {
    let p = palette();
    Style::default()
        .fg(p.text_on_accent)
        .bg(p.accent)
        .add_modifier(Modifier::BOLD)
}

/// Inactive column header: normal text on bar background, bold.
pub fn header_inactive() -> Style {
    let p = palette();
    Style::default()
        .fg(p.text_normal)
        .bg(p.bg_bar)
        .add_modifier(Modifier::BOLD)
}

/// Active card border: accent colour.
pub fn card_border_active() -> Style {
    let p = palette();
    Style::default()
        .fg(p.accent)
        .add_modifier(Modifier::BOLD)
}

/// Inactive card border.
pub fn card_border_inactive() -> Style {
    Style::default().fg(palette().card_border)
}

/// Blocked card border: error colour.
pub fn card_border_blocked() -> Style {
    Style::default()
        .fg(palette().error)
        .add_modifier(Modifier::BOLD)
}

/// Priority-specific style.
pub fn priority_style(priority: &str) -> Style {
    let p = palette();
    match priority.to_lowercase().as_str() {
        "critical" => Style::default()
            .fg(p.priority_critical)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        "high" => Style::default()
            .fg(p.priority_high)
            .add_modifier(Modifier::BOLD),
        "medium" => Style::default().fg(p.priority_medium),
        "low" => Style::default().fg(p.priority_low),
        _ => Style::default().fg(p.priority_low),
    }
}

/// Status bar: normal text on bar background.
pub fn status_bar_style() -> Style {
    let p = palette();
    Style::default().fg(p.text_normal).bg(p.bg_bar)
}

/// Search match highlight.
pub fn search_highlight() -> Style {
    let p = palette();
    Style::default()
        .fg(Color::Indexed(0))
        .bg(p.search_bg)
        .add_modifier(Modifier::BOLD)
}

/// Current find match highlight.
pub fn find_current_highlight() -> Style {
    let p = palette();
    Style::default()
        .fg(Color::Indexed(0))
        .bg(p.find_current_bg)
        .add_modifier(Modifier::BOLD)
}

/// Non-current find match highlight.
pub fn find_match_highlight() -> Style {
    let p = palette();
    Style::default().fg(Color::Indexed(0)).bg(p.find_match_bg)
}

/// Dim text for secondary information.
pub fn dim() -> Style {
    Style::default().fg(palette().text_dim)
}

/// Dialog/modal border: accent colour.
pub fn dialog_border() -> Style {
    Style::default().fg(palette().accent)
}

/// Error text: bold.
pub fn error_style() -> Style {
    Style::default()
        .fg(palette().error)
        .add_modifier(Modifier::BOLD)
}

/// Claimed-by agent label: bold.
pub fn claim_style() -> Style {
    Style::default()
        .fg(palette().claim)
        .add_modifier(Modifier::BOLD)
}

/// Tag label.
pub fn tag_style() -> Style {
    Style::default().fg(palette().tag)
}

/// Branch label.
pub fn branch_style() -> Style {
    Style::default()
        .fg(palette().branch)
        .add_modifier(Modifier::ITALIC)
}

/// Semantic similarity score badge. Color intensity scales with score:
/// green for high relevance, yellow for moderate, dim for low.
pub fn sem_score_style(score: f32) -> Style {
    let color = if score >= 0.8 {
        Color::Indexed(48) // bright green
    } else if score >= 0.6 {
        Color::Indexed(114) // green
    } else if score >= 0.4 {
        Color::Indexed(178) // yellow
    } else {
        Color::Indexed(245) // dim grey
    };
    Style::default().fg(adjusted(color)).add_modifier(Modifier::BOLD)
}

/// Apply current brightness/saturation adjustments to an arbitrary color.
pub fn adjusted(color: Color) -> Color {
    adjust_color(color, brightness(), saturation_adj())
}

/// The palette's normal body-text color (already brightness/saturation adjusted).
pub fn palette_text_normal() -> Color {
    palette().text_normal
}

/// Freshness dot style using the color from freshness computation.
pub fn freshness_style(color: Color) -> Style {
    Style::default().fg(adjusted(color))
}

/// Active task/card title: bright, bold.
pub fn title_active() -> Style {
    Style::default()
        .fg(palette().text_bright)
        .add_modifier(Modifier::BOLD)
}

/// Inactive task/card title: normal text.
pub fn title_inactive() -> Style {
    Style::default().fg(palette().text_normal)
}

/// Active list item: reverse-video with accent background.
pub fn list_active() -> Style {
    let p = palette();
    Style::default()
        .fg(p.text_on_accent)
        .bg(p.accent)
        .add_modifier(Modifier::BOLD)
}

/// Inactive list item.
pub fn list_inactive() -> Style {
    Style::default().fg(palette().text_normal)
}

/// Help dialog key binding: accent, bold.
pub fn help_key() -> Style {
    Style::default()
        .fg(palette().accent)
        .add_modifier(Modifier::BOLD)
}

/// Help dialog description text.
pub fn help_desc() -> Style {
    Style::default().fg(palette().text_normal)
}

// ---------------------------------------------------------------------------
// Markdown StyleSheet — uses the same palette
// ---------------------------------------------------------------------------

/// A `StyleSheet` implementation that dispatches to the active [`ThemeKind`].
#[derive(Clone, Copy, Debug)]
pub struct ThemedStyleSheet(pub ThemeKind);

impl ThemedStyleSheet {
    /// Fetch the palette for this stylesheet's theme, temporarily
    /// swapping the thread-local active theme to read it.
    fn pal(&self) -> Palette {
        let prev = active();
        set_active(self.0);
        let p = palette();
        set_active(prev);
        p
    }
}

impl StyleSheet for ThemedStyleSheet {
    fn heading(&self, level: u8) -> Style {
        let p = self.pal();
        match level {
            1 => Style::default()
                .fg(p.md_heading_primary)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            2 => Style::default()
                .fg(p.md_heading_secondary)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
            3 => Style::default()
                .fg(p.md_heading_primary)
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .fg(p.md_heading_secondary)
                .add_modifier(Modifier::ITALIC),
        }
    }

    fn code(&self) -> Style {
        let p = self.pal();
        Style::default().fg(p.md_code_fg).bg(p.md_code_bg)
    }

    fn link(&self) -> Style {
        let p = self.pal();
        Style::default()
            .fg(p.md_link)
            .add_modifier(Modifier::UNDERLINED)
    }

    fn blockquote(&self) -> Style {
        Style::default().fg(self.pal().md_blockquote)
    }

    fn heading_meta(&self) -> Style {
        dim()
    }

    fn metadata_block(&self) -> Style {
        dim()
    }

    fn table_border(&self) -> Style {
        Style::default().fg(self.pal().md_table_border)
    }

    fn table_header(&self) -> Style {
        Style::default()
            .fg(self.pal().text_normal)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    }

    fn code_theme(&self) -> &str {
        match self.0 {
            ThemeKind::Dark => "base16-ocean.dark",
            ThemeKind::Light => "InspiredGitHub",
            ThemeKind::Dracula => "base16-eighties.dark",
            ThemeKind::TokyoNight => "base16-ocean.dark",
            ThemeKind::Pink => "base16-mocha.dark",
            ThemeKind::Ascii => "",
        }
    }
}
