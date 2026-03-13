---
id: 77
title: Add text selection and copy support in kanban-mdx TUI
status: done
priority: medium
created: 2026-03-13T09:23:15.275358Z
updated: 2026-03-13T11:50:29.813436Z
started: 2026-03-13T11:42:17.783658Z
completed: 2026-03-13T11:42:17.783658Z
tags:
    - idea
class: standard
---

Implement text highlighting in the kanban-mdx TUI so users can select and copy text from the terminal. Currently the TUI captures all mouse/keyboard input, making it difficult to copy task titles, descriptions, or other content.

Scope:
- Enable mouse-based text selection in read-only views (detail view, reader panel)
- Allow copying selected text to system clipboard
- Consider a 'select mode' toggle that temporarily disables TUI keybindings to allow native terminal selection
- Investigate ratatui/crossterm mouse selection capabilities and clipboard crate integration

References:
- crossterm mouse capture modes (normal vs SGR)
- copypasta or arboard crate for clipboard access
- Some terminals support OSC 52 for clipboard writes without external crates
