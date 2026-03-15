---
id: 87
title: TUI guided documentation system (H key)
status: done
priority: medium
created: '2026-03-14T16:52:29.502051Z'
updated: '2026-03-14T17:45:55.922703Z'
tags:
- feature
- tui
- docs
---

Add an in-TUI guided documentation system accessible via `H` from board view. Renders structured markdown documentation pages using the existing tui-md pipeline, providing rich help content (diagrams, tables, code blocks) beyond the current `?` keybinding overlay.

## Three-Tier Help Model (Progressive Disclosure)

```
Friction ──────────────────────────────────► Depth

 Status bar hints          ? Quick Help          H Full Guide
 ┌──────────────┐     ┌──────────────────┐    ┌────────────────────┐
 │ one-line tip │     │ keybinding card  │    │ rich markdown docs │
 │ (contextual) │     │ (existing overlay│    │ with TOC, search,  │
 │              │     │  + "H: guide")   │    │ heading nav, fold  │
 └──────────────┘     └──────────────────┘    └────────────────────┘
  New user sees          Power user's           Reference & learning
  on first use           muscle memory          for all depths
```

1. **Status bar hints** — contextual one-liners triggered by heuristics (empty search results, first use of reader panel, etc.). Controlled by `tui.show_hints = true`.
2. **? Quick Help** — the existing keybinding overlay, unchanged, but with a new footer hint: "H: open full guide".
3. **H Full Guide** — the new system below.

## Navigation Model

Two layers with clear escape hierarchy:

```
  Board ──H──► Guide Index ──Enter──► Topic Page
    ▲              │                       │
    └──Esc─────────┘           ◄───Esc─────┘
```

### Guide Index
Filterable topic list, styled like the existing move/context pickers:

```
┌─ Guide ──────────────────────────────────────┐
│                                              │
│  Getting Started                             │
│    Board Basics                              │
│  ► Navigation                                │
│    Your First Task                           │
│                                              │
│  Board Views                                 │
│    Card vs List Mode                         │
│    Reader Panel                              │
│    Column Management                         │
│    Sorting & Filtering                       │
│                                              │
│  Search & Filter                             │
│    Search Syntax                             │
│    Semantic Search                           │
│    Context & Worktree Filtering              │
│                                              │
│  Task Management                             │
│    Creating & Editing                        │
│    Moving & Status Flow                      │
│    Priority & Classes of Service             │
│    Dependencies & Blocking                   │
│    Branches & Worktrees                      │
│                                              │
│  Multi-Agent Workflows                       │
│    Agent Session Lifecycle                   │
│    Claim Coordination                        │
│    Worktree Workflow                         │
│                                              │
│  Customization                               │
│    Themes & Display                          │
│    Configuration (config.toml)               │
│    Status Rules & Enforcement                │
│                                              │
│  /:filter  j/k:navigate  Enter:open  Esc:close
└──────────────────────────────────────────────┘
```

### Topic Page
Full-screen markdown rendering, identical UX to the Detail view:
- `j/k` scroll, `J/K` half-page, `g/G` top/bottom
- `(` `)` `'` `"` heading navigation
- `z/Z` heading folding
- `/` find-in-text with `n/N` match cycling
- `1-9` jump to Nth heading
- `Esc`/`q` back to index

## Content Architecture

Topics are markdown files embedded at compile time via `include_str!()`, organized into sections:

```
src/tui/guide/
├── mod.rs              # GuideTopic registry, state, key dispatch
├── content.rs          # Topic definitions with include_str!()
├── render.rs           # Index + page rendering
└── pages/
    ├── board-basics.md
    ├── navigation.md
    ├── first-task.md
    ├── views.md
    ├── reader-panel.md
    ├── column-management.md
    ├── search-syntax.md
    ├── semantic-search.md
    ├── context-filtering.md
    ├── task-lifecycle.md
    ├── priorities-classes.md
    ├── dependencies.md
    ├── branches-worktrees.md
    ├── agent-workflows.md
    ├── claim-coordination.md
    ├── themes.md
    ├── configuration.md
    └── status-rules.md
```

Each topic is a GuideTopic struct:

```rust
pub struct GuideTopic {
    pub title: &'static str,
    pub section: &'static str,       // group heading in the index
    pub body: &'static str,          // include_str!("pages/navigation.md")
    pub context_hint: Option<AppView>, // for context-sensitive entry
}
```

The `context_hint` field enables context-sensitive entry — pressing `H` from specific views could jump directly to the relevant topic (e.g., `H` during Search opens "Search Syntax", `H` in Detail opens "Reader Panel").

## State Model

```rust
// New variant in AppView enum
AppView::Guide

pub struct GuideState {
    pub mode: GuideMode,
    pub topic_cursor: usize,
    pub topic_filter: String,
    pub scroll: usize,
    pub cache: Option<DetailContent>,  // reuses existing cache type
    pub fold_level: usize,
    pub find_query: String,
    pub find_active: bool,
    pub find_matches: Vec<usize>,
    pub find_current: usize,
}

pub enum GuideMode {
    Index,  // browsing the topic list
    Page,   // reading a rendered topic
}
```

## Infrastructure Reuse

Nearly everything needed already exists:

| Component | Exists in | Reuse Strategy |
|-----------|-----------|----------------|
| Markdown rendering | tui-md crate | Direct — render guide pages identically to task bodies |
| Scroll + visual-row math | DetailContent | Direct — same caching, same O(1) scroll |
| Heading navigation | detail_nav.rs | Extract to shared helper or duplicate (small) |
| Find-in-text | DetailState find system | Mirror pattern in GuideState |
| Heading folding | fold_level + tui-md | Direct reuse |
| Filterable list | help overlay filter, move dialog filter | Same pattern |
| Overlay layout | layout::centered_rect | Direct for index; full-screen for page |
| Key dispatch | keys/ module system | New keys/guide.rs following same pattern |
| Content embedding | include_str! for SKILL.md | Same pattern |

## Content Sources

Most content already exists and just needs restructuring:

| Topic | Source |
|-------|--------|
| Task Lifecycle | Task #81 section 1 (state machine diagram) |
| Agent Workflows | Task #81 section 2 (session lifecycle) |
| Worktree Workflow | Task #81 section 3 |
| Claim Coordination | Task #81 section 4 |
| Context Resolution | Task #81 section 5 |
| TUI Navigation | Task #81 section 6 (keybinding tables) |
| Search Syntax | render_search_help() content, expanded |
| Configuration | Task #81 section 8 (enforcement rules) |
| Priority/Classes | Task #81 section 9 (sorting algorithm) |
| Board Basics | New — intro for first-time users |
| First Task | New — walkthrough tutorial |
| Reader Panel | New — feature deep-dive |

## Context-Sensitive Hints (Status Bar)

The status bar already shows mode, sort, and filter info. Hints are appended as a dimmed suffix when heuristics fire:

```
Triggers:
- Empty search results           → "Tip: ? for search syntax"
- First column collapse           → "Tip: X expands all columns"
- First reader panel open         → "Tip: z/Z folds headings"
- First context picker open       → "Tip: H for context guide"
- Board opened for first time     → "Tip: ? for shortcuts, H for guide"
```

First-seen state tracked via a small bitfield in config (persisted to config.toml):

```toml
[tui]
show_hints = true
hints_seen = 0   # bitfield, each bit = one hint type
```

## Alternatives Considered and Rejected

**Interactive tutorial mode** (step-by-step "press j now... good!"): High coupling to keybindings, fragile, patronizing for non-beginners. The guide approach serves both tutorial and reference needs without the maintenance burden.

**Man-page / external viewer**: Breaks flow — the user leaves the TUI. The whole point is in-TUI discoverability.

**Expanding the existing ? overlay**: The two-column keybinding layout doesn't support rich content (diagrams, tables, code blocks). Better to keep ? as a quick card and have the guide for depth.

## Files Touched

- `src/tui/types.rs` — new `AppView::Guide` variant + `GuideState` + `GuideMode`
- `src/tui/app.rs` — add `guide: GuideState` field to App
- `src/tui/keys/mod.rs` — dispatch to guide key handler
- `src/tui/keys/guide.rs` — new: guide-specific key handling (index + page modes)
- `src/tui/render/mod.rs` — dispatch to guide renderer
- `src/tui/render/guide.rs` — new: guide index + page rendering
- `src/tui/guide/` — new module: content registry, topic definitions, embedded pages
- `src/tui/render/overlays.rs` — add "H: guide" hint to help footer
- `src/tui/keys/board.rs` — bind `H` to open guide
- `src/tui/keys/detail.rs` — bind `H` to open guide (context-sensitive)

## Estimated Scope

- ~400-500 lines new Rust (state, rendering, key handling)
- ~200-300 lines new markdown (basics/tutorial topics); rest adapted from existing task #81
- Zero new dependencies
