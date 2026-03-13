---
id: 44
title: Structured error output with exit codes (0/1/2)
status: done
priority: critical
created: 2026-03-11T08:21:15.355614Z
updated: 2026-03-11T08:26:11.004572Z
started: 2026-03-11T08:26:11.004571Z
completed: 2026-03-11T08:26:11.004571Z
tags:
    - parity
class: standard
---

Go version uses specific exit codes: 0 (success), 1 (general error), 2 (internal error). Errors output as structured JSON with code and details. Warnings go to stderr, data to stdout. Silent errors for no-op operations (idempotent). kanban-mdx error handling should match this contract.
