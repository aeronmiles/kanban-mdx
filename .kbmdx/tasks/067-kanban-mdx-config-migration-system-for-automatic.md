---
id: 67
title: 'kanban-mdx: Config migration system for automatic schema upgrades'
status: done
priority: critical
created: 2026-03-12T09:00:55.474597Z
updated: 2026-03-12T09:08:34.8031Z
started: 2026-03-12T09:08:34.803099Z
completed: 2026-03-12T09:08:34.803099Z
tags:
    - kanban-mdx
class: standard
---

Go has a versioned config migration system that automatically upgrades old config.yml schemas to the current version. kanban-mdx treats version mismatches as a fatal error with no auto-upgrade path.

## What Go does
- `internal/config/migrate.go` has a `migrations` map of version→function
- Each migration transforms config from version N to N+1
- Migrations run automatically on load when `cfg.Version < CurrentVersion`
- Bump `CurrentVersion` in defaults.go when schema changes

## What kanban-mdx does
- Config validation rejects mismatched versions as a fatal error
- Users must manually update their config.yml to match the expected version

## What to implement
- Add a migration registry (version → transform function)
- On load, if version < current, run migrations sequentially
- Each migration increments the version number
- Write updated config back to disk after successful migration
- Add tests with fixture configs for each historical version

[[2026-03-12]] Thu 09:08
[2026-03-12 12:00] Already implemented — kanban-mdx/src/io/config_file.rs has full migration system (v1→v14) called from load() before validation. Task description was based on incorrect analysis.
