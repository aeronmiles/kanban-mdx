---
id: 84
title: Migrate config format from YAML to TOML
status: done
priority: low
created: 2026-03-13T17:19:26.149388Z
updated: 2026-03-13T19:57:02.063875Z
started: 2026-03-13T19:57:02.063875Z
completed: 2026-03-13T19:57:02.063875Z
tags:
    - idea
claimed_by: grison-upgo
claimed_at: 2026-03-13T19:57:02.063875Z
class: standard
---

Switch kanban-mdx config from YAML (serde_yml) to TOML (toml crate).

**Why:**
- TOML has stronger typing (no implicit yes→true, Norway problem)
- Better Rust ecosystem alignment (Cargo.toml, rustfmt.toml, etc.)
- The `toml` crate is more mature and better maintained than `serde_yml`

**When:**
- Only once kanban-mdx is the canonical version and Go compatibility is no longer a concern
- Bundle with a `kanban migrate-config` one-time conversion command

**What's involved:**
- Replace `serde_yml` with `toml` crate in Cargo.toml
- Update config_file.rs load/save to use TOML
- Add a one-time YAML→TOML migration command
- Port or drop the 14 existing YAML migrations (new format starts fresh at current schema version)
- Update all tests and fixtures
- Note: `[[statuses]]` / `[[classes]]` array-of-tables syntax is more verbose than YAML lists — accept the tradeoff
