---
id: 40
title: Environment variable support ($KANBAN_AGENT fallback for --claim)
status: done
priority: critical
created: 2026-03-11T08:21:05.581531Z
updated: 2026-03-11T08:36:42.585653Z
started: 2026-03-11T08:36:42.585652Z
completed: 2026-03-11T08:36:42.585652Z
tags:
    - parity
class: standard
---

Go version supports $KANBAN_AGENT env var as fallback for --claim flags (pick, move, edit, handoff). Also supports $KANBAN_EMBED_API_KEY for semantic search API key. kanban-mdx may support $KANBAN_OUTPUT and $KANBAN_DIR via clap env but lacks $KANBAN_AGENT fallback.
