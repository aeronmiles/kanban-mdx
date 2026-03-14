---
id: 79
title: Add text selection & copy support to kanban-mdx TUI
status: done
priority: medium
created: '2026-03-13T10:54:54.437468Z'
updated: '2026-03-13T12:05:27.959195Z'
started: '2026-03-13T11:16:42.190724Z'
completed: '2026-03-13T11:16:42.190724Z'
tags:
- layer-4
claimed_by: sotol-gypsine
claimed_at: '2026-03-13T11:16:42.190725Z'
class: standard
---

crossterm's EnableMouseCapture intercepts all mouse events and prevents native terminal text selection. Users cannot copy task titles, descriptions, or body content from the TUI.

## Prerequisite: Remove Duplicate Key Handlers

The codebase has 11 key/mouse handler methods duplicated in both src/tui/app.rs (old, lines ~1121–2340) and src/tui/keys/*.rs (new). The old duplicates must be deleted first so the project compiles.

## Feature 1: Y to Copy Task Content

- Extract a reusable copy_to_clipboard(text: &str) -> bool helper from the existing copy_task_path() method in app.rs
- Add copy_task_content() that copies title + body
- Bind Y in keys/board.rs and keys/detail.rs

## Feature 2: Select Mode Toggle (Ctrl+S)

- Add select_mode: bool to App, toggle via Ctrl+S
- toggle_select_mode() calls crossterm DisableMouseCapture / EnableMouseCapture at runtime
- Intercept Ctrl+S in handle_key() before view dispatch
- Early return in handle_mouse() if select_mode
- Esc exits select mode before other Esc actions
- Show [SELECT] indicator in status bar (render/chrome.rs)
- Add help entries in render/overlays.rs

## Files to modify

- src/tui/app.rs (remove duplicates, add helper + select_mode field)
- src/tui/keys/board.rs (Y binding, Esc select mode exit)
- src/tui/keys/detail.rs (Y binding, Esc select mode exit)
- src/tui/render/chrome.rs (status bar indicator)
- src/tui/render/overlays.rs (help entries)
