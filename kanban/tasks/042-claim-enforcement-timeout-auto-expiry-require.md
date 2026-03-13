---
id: 42
title: Claim enforcement (timeout auto-expiry, require_claim on statuses)
status: done
priority: critical
created: 2026-03-11T08:21:10.03459Z
updated: 2026-03-11T08:36:43.216893Z
started: 2026-03-11T08:36:43.216892Z
completed: 2026-03-11T08:36:43.216892Z
tags:
    - parity
class: standard
---

Go version enforces: claim timeout auto-expires stale claims (configurable, default 1h), require_claim on statuses blocks unclaimed tasks from entering, rejects edits/moves on tasks claimed by different agents. kanban-mdx has claim_timeout in config but enforcement behavior is unclear.
