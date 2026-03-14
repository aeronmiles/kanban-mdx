---
id: 75
title: kanban-mdx Rust Codebase Evaluation Report
status: references
priority: low
created: 2026-03-13T06:57:46.858799Z
updated: 2026-03-13T07:48:17.522633Z
tags:
    - reference
class: standard
---

# kanban-mdx Rust Codebase Evaluation Report

**Date:** 2026-03-13
**Scope:** Full architectural and code quality review of the \`kanban-mdx/\` Rust workspace
**Total source lines:** ~27,750 (excluding \`target/\`)
**Files reviewed:** 86 \`.rs\` source files across 2 workspace members

---

## Executive Summary

kbmdx is a well-structured Rust rewrite/extension of the Go kanban-mdx tool, comprising a CLI, TUI, semantic embedding system, and supporting libraries. The codebase demonstrates solid Rust idioms, comprehensive error handling, and good test coverage in most modules. However, it has one critical structural problem — a 4,657-line god object — and several moderate issues around consistency, missing abstractions, and hardcoded values.

**Overall grade: B+** — Strong foundation with clear areas for improvement.

---

## 1. Architecture & Module Organization

### Strengths
- **Clean layered separation:** \`model\` → \`io\` → \`board\` → \`cli\` → \`tui\` follows a unidirectional dependency flow. No circular imports.
- **Workspace member isolation:** \`sembed\` is a standalone crate with its own Cargo.toml, clean trait boundary (\`Embedder\`), and zero knowledge of kanban domain types. Excellent composability.
- **31 CLI subcommands** are each in their own file with a single \`run()\` entry point — easy to find, easy to maintain.
- **Board operations** (\`board/\`) are pure functions over \`&[Task]\` + \`&Config\` — no side effects, no I/O, highly testable.

### Issues
1. **\`tui/app.rs\` is a 4,657-line god object.** The \`App\` struct has **147 fields** spanning 6 concerns: view state, navigation, search, semantic search, context picking, caching/perf. The \`handle_key()\` method alone is likely 2,000+ lines. This is the single biggest structural problem in the codebase.
   - **Recommendation:** Extract into sub-structs (\`SearchState\`, \`SemanticState\`, \`ContextPickerState\`, \`DetailViewState\`, \`CreateWizardState\`) that \`App\` composes. Factor \`handle_key()\` into per-view dispatch methods.
2. **\`tui/ui.rs\` at 2,610 lines** is large but better structured — 28 independent render functions with a clean \`render()\` dispatcher. Less urgent but could benefit from splitting into files per view.
3. **No \`board\` module re-export of \`branch_context\`** — it's the only board submodule not publicly re-exported from \`board/mod.rs\`, despite being used by CLI commands.

---

## 2. Error Handling

### Strengths
- **Machine-readable error codes:** \`ErrorCode\` enum with 21 variants and \`as_str()\` → \`"TASK_NOT_FOUND"\` etc. Designed for agent consumption. Excellent design.
- **\`CliError\`** carries code + message + optional details map (\`serde_json::Value\`), with deterministic exit codes (1 for user errors, 2 for internal).
- **\`SilentError\`** for batch operations — exits with code, no stderr output. Clean UX.
- **Lenient reads** (\`read_all_lenient()\`) collect warnings without aborting — critical for robustness on real-world messy task files.

### Issues
1. **Inconsistent error types in CLI layer.** Most commands return \`Result<(), CliError>\`, but \`edit_one()\` returns \`Result<(), String>\`. This forces string-to-CliError conversions at call boundaries.
   - **Recommendation:** Standardize on \`CliError\` everywhere. Use \`impl Into<CliError>\` where bridge types are needed.
2. **\`log_mutation()\` silently swallows all errors.** Logging should indeed not fail commands, but the complete silence means write failures (disk full, permissions) are invisible.
   - **Recommendation:** Log to stderr with a \`tracing::warn!\` or similar — don't panic, but don't hide either.
3. **\`RwLock::unwrap()\` in \`sembed::Index\`** — all 8 lock acquisitions use \`.unwrap()\`, which panics on lock poisoning. Unlikely in practice, but violates the otherwise careful error handling.
   - **Recommendation:** Use \`.expect("index lock poisoned")\` at minimum, or handle gracefully.

---

## 3. Code Smells — Item by Item

### 3.1 God Object: \`App\` struct (CRITICAL)
- **File:** \`src/tui/app.rs:608-754\`
- **147 public fields**, most with \`pub\` visibility
- Mixes: view state, sort/filter, search input, semantic search channels, context picker, branch picker, create wizard, undo snapshots, render caches, perf counters, FPS tracking
- **Why it matters:** Every new TUI feature adds more fields here. Testing requires constructing the entire App. Render cache invalidation must reason about 147 fields. New contributors face a wall of state.
- **Recommendation:** Decompose into:
  - \`BoardState\` (columns, active_col/row, sort_mode, view_mode, hide_empty)
  - \`SearchState\` (query, history, tab completion, matches)
  - \`SemanticState\` (channels, debounce, results, scores)
  - \`PickerState\` (branch/context/move/delete pickers)
  - \`CreateState\` (already partially extracted)
  - \`DetailState\` (scroll, find, fold, caches)
  - \`DebugState\` (fps, render stats, perf mode)

### 3.2 Double directory scan in \`find_by_id()\` (MODERATE)
- **File:** \`src/model/task.rs\`
- Scans all files by filename prefix, then if no match, scans all files reading frontmatter YAML
- **Why it matters:** O(2n) for every task lookup in a directory with n files
- **Recommendation:** Single-pass scan: parse filename first (cheap), fall back to frontmatter only for ambiguous matches in the same iteration.

### 3.3 Hardcoded magic numbers scattered across modules (MODERATE)
| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| Slug max length | 50 | \`model/task.rs\` | Truncate slugs |
| Log max lines | 10,000 | \`board/log.rs\` | Truncate activity.jsonl |
| Undo max entries | 100 | \`board/undo.rs\` | Stack depth |
| Highlight cache max | 256 | \`markdown/highlight.rs\` | Syntax cache entries |
| Debounce delay | 100ms | \`watcher/watcher.rs\` | FS watcher |
| Semantic debounce | 300ms | \`tui/app.rs\` | Search debounce |
| Status expiry | 2s | \`tui/mod.rs\` | Status bar auto-clear |
| History max | 100 | \`tui/app.rs\` | Search history |

- **Recommendation:** Extract to named constants at module level, or collocate in config. Currently some are \`const\` and some are inline literals — inconsistent.

### 3.4 Repetitive theme palette construction (MODERATE)
- **File:** \`src/tui/theme.rs\`
- \`build_palette()\` repeats nearly identical color definitions for 6 themes
- Each theme is ~25 lines of color constants; pattern is identical, only values differ
- **Recommendation:** Define themes as data (array/map of color values), not as code. A single \`Palette::from_colors()\` factory with a theme-specific color array.

### 3.5 Deprecated dependency: \`serde_yaml = "0.9.34-deprecated"\` (MODERATE)
- **File:** \`Cargo.toml:24\`
- The crate is explicitly deprecated by its maintainer. The replacement is \`serde_yml\` or an alternative.
- **Why it matters:** No future security patches, potential API breakage in transitive updates.
- **Recommendation:** Migrate to \`serde_yml\` or consider using \`toml\` for config (YAML is only used for task frontmatter and config — both could be TOML).

### 3.6 Blocking HTTP in sembed (MODERATE)
- **File:** \`sembed/src/openai.rs\`, \`sembed/src/ollama.rs\`
- Uses \`reqwest::blocking::Client\` — cannot be used in async contexts, no timeouts, no retries
- **Why it matters:** The TUI already uses async channels for semantic search (\`mpsc\`), but the actual API calls block a thread. No timeout means a hung API call blocks forever.
- **Recommendation:** Add configurable timeout to \`reqwest::blocking::ClientBuilder\`. Consider async migration if the TUI grows more concurrent features.

### 3.7 No input validation in sembed API clients (LOW)
- **File:** \`sembed/src/openai.rs\`, \`sembed/src/ollama.rs\`
- Empty API keys, invalid URLs, and non-existent models are accepted at construction time; they fail at request time with opaque HTTP errors.
- **Recommendation:** Validate at \`new()\` — fail fast with \`EmbedError::Config\`.

### 3.8 Config post-construction mutation via \`set_dir()\` (LOW)
- **File:** \`src/model/config.rs\`
- Config is constructed, then \`dir\` is set later. This means path accessors (\`tasks_path()\`, \`config_path()\`) panic or return wrong paths if called before \`set_dir()\`.
- **Recommendation:** Builder pattern or require \`dir\` at construction.

### 3.9 Terminal status logic assumes column ordering (LOW)
- **File:** \`src/model/config.rs\`
- \`is_terminal_status()\` assumes the last non-archived status is "done". If someone reorders statuses in config, this breaks silently.
- **Recommendation:** Explicit \`terminal: true\` flag in StatusConfig, or document the ordering constraint.

### 3.10 Duplicate WIP check logic (LOW)
- **File:** \`src/cli/wip.rs\`, \`src/cli/create.rs\`, \`src/cli/move_cmd.rs\`, \`src/cli/edit.rs\`
- Three commands independently call WIP checking with slightly different exclude-self semantics.
- **Recommendation:** The \`wip.rs\` helper exists but isn't used consistently. Refactor all callers to use it.

### 3.11 Git operations via subprocess with no error context (LOW)
- **File:** \`src/util/git.rs\`
- Functions return \`Option<String>\` or \`Vec<String>\` — failure is indistinguishable from "no git repo" or "no branches". No error messages.
- **Recommendation:** Return \`Result\` with context, or at minimum log failures.

### 3.12 No-op config migrations (LOW)
- **File:** \`src/io/config_file.rs\`
- Migrations v6→v7, v9→v10, v13→v14 are no-ops that only bump the version number.
- **Why it matters:** 3 of 13 migrations do nothing — confusing for anyone reading the migration chain. The version number jumps without visible changes.
- **Recommendation:** Document why each no-op exists (e.g., "deserialization compat handled elsewhere") as a code comment.

---

## 4. Testing

### Strengths
- **Excellent test coverage in:** \`tui/search.rs\` (100+ tests), \`embed/mod.rs\` (50+), \`cli/import.rs\` (18), \`model/task.rs\` (22), \`model/config.rs\` (23), \`io/config_file.rs\` (24), \`board/deps.rs\` (9), \`cli/gitignore.rs\` (8), \`cli/branch_check.rs\` (6), \`sembed/*\` (comprehensive per-module).
- **Good patterns:** Table-driven tests, golden file comparisons, fixture data, round-trip serialization tests.

### Gaps
- **Zero tests for \`tui/app.rs\`** — the largest file in the codebase (4,657 lines) has no unit tests. Key handler logic, column building, filtered tasks, context computation — all untested.
- **Zero tests for \`tui/ui.rs\`** (2,610 lines) — no snapshot tests for rendered output.
- **Most CLI commands lack tests** — only \`import\`, \`gitignore\`, \`branch_check\`, \`embed\`, and \`find\` have tests. The remaining 26 commands rely entirely on manual testing or implicit coverage via the Go e2e suite.
- **No integration tests** — no test exercises the full pipeline (load config → read tasks → filter → sort → render).
- **No tests for \`board/undo.rs\`** or \`board/branch_context.rs\`.

### Recommendations
- Priority 1: Add behavioral tests for \`App::handle_key()\` — simulate keypresses and assert state changes. The Go TUI has this pattern (\`sendKey\` helper).
- Priority 2: Snapshot tests for \`ui::render()\` using ratatui's \`TestBackend\`.
- Priority 3: Add at least one test per CLI command covering the happy path.

---

## 5. Dependency Analysis

### Well-chosen dependencies
| Crate | Purpose | Assessment |
|-------|---------|------------|
| \`clap\` 4 | CLI parsing | Industry standard, derive macros reduce boilerplate |
| \`ratatui\` 0.30 | TUI framework | Active maintenance, good API |
| \`crossterm\` 0.29 | Terminal backend | Standard pairing with ratatui |
| \`chrono\` 0.4 | Date/time | Mature, serde support |
| \`thiserror\` 2 | Error derives | Clean, zero-cost |
| \`color-eyre\` 0.6 | Error reporting | Good for CLI tools |
| \`pulldown-cmark\` 0.13 | Markdown parsing | Reference implementation |
| \`syntect\` 5 | Syntax highlighting | Heavy but comprehensive |
| \`sembed\` (local) | Embeddings | Clean trait boundary |

### Concerns
| Crate | Issue |
|-------|-------|
| \`serde_yaml = "0.9.34-deprecated"\` | Explicitly deprecated. Migrate to \`serde_yml\` or alternative. |
| \`reqwest\` 0.13.2 with \`blocking\` | Pulls in tokio for blocking client; heavy for sync-only use. Consider \`ureq\` for simpler blocking HTTP. |
| \`tokio\` 1 with \`rt + macros\` | Only used to power reqwest's blocking client. If reqwest is replaced with \`ureq\`, tokio can be dropped entirely — significant compile time savings. |
| \`rand\` 0.10 | Only used for agent name generation (pick random word). Could use \`fastrand\` (no-std, much lighter) or even simple hash-based selection. |
| \`fs2\` 0.4 | File locking. Last release 2016. Consider \`fd-lock\` or \`file-guard\` for active maintenance. |

---

## 6. Idiomatic Rust Assessment

### Good patterns
- **\`thiserror\` + enum-based errors** — consistent across all modules
- **Serde skip_serializing_if** — clean optional field handling in YAML/JSON
- **Iterator chains** over explicit loops in board operations
- **\`cfg(unix)\` conditional compilation** for file permissions
- **\`OnceLock\` / \`LazyLock\`** for expensive one-time initialization
- **\`BTreeMap\`** for sorted group output (deterministic ordering)

### Anti-patterns
- **\`pub\` on 147 struct fields** in \`App\` — breaks encapsulation, makes refactoring impossible without touching every caller
- **\`RefCell\` in \`App\`** for caches — interior mutability in a non-shared type suggests the ownership model is wrong. The caches should be computed and stored in mutable borrows, not smuggled through RefCell.
- **\`isize\` for history cursor** (\`InputHistory::cursor: isize\`) — using -1 as sentinel is a C idiom. Use \`Option<usize>\` instead.
- **String-typed field names** for sort/group-by — \`sort_by: &str\` with runtime validation. These should be enums (\`SortField::Id | Status | Priority | ...\`).
- **Thread-local state** in \`tui/theme.rs\` — global mutable state via \`thread_local!\` makes testing difficult and is fragile (must call \`set_active()\` before any style function).

---

## 7. Performance Considerations

### Current design
- **Search is O(n)** over all tasks — fine for expected scale (hundreds of tasks).
- **Semantic search is O(n)** brute-force cosine similarity — fine for small indexes, won't scale past ~10K documents.
- **Syntax highlighting cache** (256 entries, cleared on overflow) — adequate but could use LRU.
- **File watcher debouncing** (100ms) — good balance between responsiveness and CPU.
- **Detail view caches** (\`RefCell<Option<...>>\`) — avoids re-rendering markdown on every frame.

### Bottlenecks (if scale increases)
1. \`find_by_id()\` double scan
2. \`ensure_consistency()\` has O(n²) duplicate checking
3. Log loading reads all entries before filtering (O(n) memory)
4. \`Index::search()\` clones all documents on every search

---

## 8. sembed Workspace — Dedicated Assessment

### Strengths
- **Minimal trait contract:** Single \`embed()\` method — perfect abstraction boundary
- **Three implementations:** OpenAI-compatible (covers OpenAI + Voyage AI + custom), Ollama, Mock
- **Deterministic mock** with call recording — excellent for testing
- **Content-hash staleness detection** — only re-embeds changed documents
- **Well-tested:** Every module has unit tests; mock validates determinism and normalization

### Issues
- No request timeouts (API calls can hang indefinitely)
- No retry logic for transient failures
- \`RwLock::unwrap()\` on all lock acquisitions (8 sites)
- Document cloning in search results — unnecessary allocation
- \`partial_cmp().unwrap_or(Equal)\` silently swallows NaN in similarity sorting

---

## 9. Actionable Recommendations (Priority-Ordered)

### P0 — Architectural (do before adding more TUI features)
1. **Decompose \`App\` struct** into composed sub-states
2. **Factor \`handle_key()\`** into per-view dispatch methods
3. **Add TUI behavioral tests** (keypress → state assertion)

### P1 — Correctness & Maintenance
4. **Replace \`serde_yaml\`** with non-deprecated alternative
5. **Standardize error types** (\`CliError\` everywhere, not \`String\`)
6. **Add request timeouts** to sembed HTTP clients
7. **Use \`Option<usize>\`** instead of \`isize\` sentinel for history cursor
8. **Validate inputs at construction** in sembed (\`new()\` not \`embed()\`)

### P2 — Code Quality
9. **Extract hardcoded constants** to named module-level \`const\`
10. **Data-driven theme palettes** instead of repetitive code
11. **Enum-typed sort/group fields** instead of string + runtime validation
12. **Document no-op migrations** with explanatory comments
13. **Unify WIP check call sites** through \`wip.rs\` helper

### P3 — Performance (defer until scale demands)
14. **Single-pass \`find_by_id()\`**
15. **LRU cache** for syntax highlighting
16. **Streaming log filter** instead of load-all
17. **Consider \`ureq\`** to drop tokio dependency

---

## 10. Summary Statistics

| Metric | Value |
|--------|-------|
| Total source LOC | 27,748 |
| Source files | 86 |
| Workspace members | 2 (\`kanban-mdx\`, \`sembed\`) |
| CLI subcommands | 31 |
| Largest file | \`tui/app.rs\` (4,657 LOC) |
| Next largest | \`tui/ui.rs\` (2,610 LOC) |
| Config version | 14 (13 migrations) |
| Test coverage | Strong in model/io/board/embed/search; absent in TUI/most CLI |
| Dependencies | 18 direct, 1 deprecated (\`serde_yaml\`) |
| God objects | 1 (\`App\` — 147 fields) |
| Critical issues | 1 (App decomposition) |
| Moderate issues | 6 |
| Low issues | 5 |
