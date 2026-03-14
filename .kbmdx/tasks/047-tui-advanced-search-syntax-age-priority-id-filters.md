---
id: 47
title: 'TUI: Advanced search syntax (age, priority, ID filters)'
status: done
priority: critical
created: 2026-03-11T09:10:50.914991Z
updated: 2026-03-11T09:30:44.152365Z
started: 2026-03-11T09:30:44.152365Z
completed: 2026-03-11T09:30:44.152365Z
tags:
    - parity
class: standard
---

Go TUI search supports rich filter syntax: @48h (updated within 48h), @>2w (older than 2 weeks), @today, created:3d, p:high, p:medium+ (medium or higher), p:high- (high or lower), p:c (critical), id:5, id:1,3,7, id:5-10, #5. kanban-mdx search only does basic substring matching. Implement the full search DSL.
