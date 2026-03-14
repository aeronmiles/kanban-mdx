---
id: 32
title: Config migration system (automatic schema version upgrades)
status: done
priority: critical
created: 2026-03-11T08:20:43.619393Z
updated: 2026-03-11T08:26:10.226125Z
started: 2026-03-11T08:26:10.226125Z
completed: 2026-03-11T08:26:10.226125Z
tags:
    - parity
class: standard
---

Go version has a full config migration system: bumps CurrentVersion, migration functions in migrate.go registered in a migrations map, each migration increments cfg.Version, with fixture directories and compat tests for every version. kanban-mdx has a version field but no automatic migration pipeline.
