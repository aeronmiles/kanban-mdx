---
id: 78
title: Extract src/markdown/ to tui-md sub-crate
status: done
priority: high
created: 2026-03-13T10:33:47.773881Z
updated: 2026-03-13T12:04:58.24979Z
started: 2026-03-13T10:35:29.607202Z
completed: 2026-03-13T12:04:58.24979Z
tags:
    - layer-4
class: standard
---

## Context

The \`src/markdown/\` module is a self-contained markdown-to-ratatui renderer forked from tui-markdown with significant extensions: GFM table rendering with responsive column sizing, heading-level folding, syntect-based syntax highlighting with caching, and a pluggable \`StyleSheet\` trait. It has **zero domain coupling** — no imports from \`crate::model\`, \`crate::board\`, \`crate::io\`, or any kanban concept. It operates purely on \`&str\` → \`ratatui::text::Text\`.

Extracting it to a workspace sub-crate (mirroring the \`sembed/\` pattern) formalizes this clean boundary, makes the dependency surface explicit, and enables independent reuse.

## Source files (move to tui-md/src/)

- \`src/markdown/mod.rs\` → \`tui-md/src/lib.rs\` (1140 lines — Public API + TextWriter + TableBuilder + tests)
- \`src/markdown/highlight.rs\` → \`tui-md/src/highlight.rs\` (174 lines — Syntect integration + cache)
- \`src/markdown/options.rs\` → \`tui-md/src/options.rs\` (152 lines — Options builder + fold_events)
- \`src/markdown/style_sheet.rs\` → \`tui-md/src/style_sheet.rs\` (83 lines — StyleSheet trait + DefaultStyleSheet)

## Consumer files (update imports)

- \`src/lib.rs:6\` — Remove \`pub mod markdown;\`
- \`src/tui/theme.rs:14\` — \`use crate::markdown::StyleSheet\` → \`use tui_md::StyleSheet\`
- \`src/tui/render/detail.rs:11\` — \`use crate::markdown\` → \`use tui_md\`
- \`Cargo.toml\` — Add \`tui-md\` path dep; move \`pulldown-cmark\`, \`syntect\`, \`itertools\` to sub-crate (all exclusively used in markdown — verified by grep)

## Steps

1. Create \`tui-md/Cargo.toml\` following sembed pattern (explicit [lib] section, own deps only)
2. Move source files, fixing one internal path: \`use crate::markdown::style_sheet::\` → \`use crate::style_sheet::\` in options.rs
3. Update workspace Cargo.toml: add member, add path dep, remove 3 deps that moved
4. Update 3 consumer files (lib.rs, theme.rs, detail.rs) — import path changes only
5. Delete \`src/markdown/\` directory
6. Verify: \`cargo test --workspace\`, manual TUI check

## Detailed plan file

See \`.claude/plans/streamed-moseying-flame.md\` for the full implementation plan with exact code snippets.

## Non-Goals (Deferred)

- Splitting lib.rs further (TextWriter/TableBuilder into own files)
- LRU cache for highlight
- Moving fold_events out of options.rs
- Adding #![doc] / examples for publishing
