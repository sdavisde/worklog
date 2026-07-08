# 01-tasks-wl-v2-rebuild.md

Execution blueprint for `01-spec-wl-v2-rebuild.md`. User pre-approved all
planning gates ("run without questions"); parent tasks and sub-tasks were
generated in a single pass.

## Relevant Files

| File | Why It Is Relevant |
| --- | --- |
| `Cargo.toml` | Crate manifest; binary target `wl`; pinned dependencies. |
| `src/main.rs` | clap entry point; routes subcommands vs bare `wl` → TUI. |
| `src/config.rs` | `~/.worklog/config.yaml` load/create (categories, editor fallback). |
| `src/model.rs` | `Task`, `Status`, id generation; serde serialization. |
| `src/store.rs` | JSONL read/write, atomic rewrite of `tasks.jsonl`, append to `archive.jsonl`, `WORKLOG_DIR` resolution. |
| `src/notes.rs` | Note document listing, frontmatter parse/write, item add/edit/delete. |
| `src/commands/task.rs` | `wl task` quick capture. |
| `src/commands/standup.rs` | `wl standup` stdout report (shared logic with TUI Standup view). |
| `src/commands/import_legacy.rs` | One-shot daily-notes importer. |
| `src/markdown.rs` | Minimal markdown block parsing for importer + note docs (ported/simplified from worklog-cli). |
| `src/tui/mod.rs` | Terminal init/restore, panic hook, event loop. |
| `src/tui/app.rs` | App state machine: views, selection, input modes. |
| `src/tui/views/*.rs` | Today, Standup, Tasks, Notes renderers. |
| `src/tui/editor.rs` | $EDITOR suspend/resume escape hatch. |
| `tests/cli.rs` | Integration tests for capture/standup via `WORKLOG_DIR` temp dirs. |
| `tests/import_legacy.rs` | Importer integration tests over synthetic fixture tree. |
| `tests/fixtures/` | Synthetic daily-note fixtures (no personal data). |
| `src/tui/` unit tests | ratatui `TestBackend` rendering tests colocated per view. |
| `CLAUDE.md` | Rewritten minimal AI instructions. |
| `README.md` | Rewritten user-facing docs incl. Homebrew install. |
| `DESIGN.md` | Already-written design record (kept). |
| `.githooks/pre-commit` | fmt + clippy + test gate. |
| `.github/workflows/ci.yml` | Same gates on push/PR. |
| `.github/workflows/release.yml` | Tag-triggered aarch64-apple-darwin tarball release. |
| `Formula/wl.rb` | Homebrew formula template for personal tap. |
| `.gitignore` | Rust ignores + local real-data test dirs. |

### Notes

- Testing command: `cargo test`. Lint: `cargo clippy --all-targets -- -D warnings`. Format: `cargo fmt --check`.
- All tests must set `WORKLOG_DIR` to a temp dir; never touch `~/.worklog`.
- Real-data migration verification runs against the scratchpad copy only and its transcript (sanitized counts, no content) becomes a proof artifact.
- Commit at each parent-task boundary using Conventional Commits.

## Tasks

### [x] 1.0 Repo reset + Rust scaffold + AI-native tooling

#### 1.0 Proof Artifact(s)

- CLI: `cargo build && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` exits 0 demonstrates clean scaffold and gates.
- CLI: `git log --oneline | head -5` shows reset commit atop Tauri history demonstrates history preservation.
- Diff: `.githooks/pre-commit` and `.github/workflows/ci.yml` exist; `git config core.hooksPath` returns `.githooks` demonstrates enforced gates.

#### 1.0 Tasks

- [x] 1.1 `git rm` all Tauri/React-era files (src/, src-tauri/, public/, package.json, configs, old CLAUDE.md/README.md, project-plan.md, .cursor/, .vscode/); keep `DESIGN.md`, `docs/`, `.git`.
- [x] 1.2 `cargo init` (edition 2024) with `[[bin]] name = "wl"`; add pinned deps from research: ratatui, crossterm, clap (derive), serde, serde_json, chrono, YAML crate, anyhow.
- [x] 1.3 Write `.gitignore` (target/, local test-data dirs), `rustfmt.toml` only if deviating (default: none), clippy config via Cargo.toml lints table.
- [x] 1.4 Write `.githooks/pre-commit` (fmt --check, clippy -D warnings, test) and document `git config core.hooksPath .githooks` in README + CLAUDE.md; set the config locally.
- [x] 1.5 Write `.github/workflows/ci.yml` running the same three gates on push.
- [x] 1.6 Write minimal-correct `CLAUDE.md` (commands, architecture map, data model, keyboard-first philosophy, no-personal-data rule) and stub `README.md`.
- [x] 1.7 Commit: `chore!: reset repo for wl v2 rust rebuild`.

### [x] 2.0 Storage layer + CLI capture + standup

#### 2.0 Proof Artifact(s)

- CLI transcript: `WORKLOG_DIR=$(mktemp -d) wl task "demo" --category engineering && cat $WORKLOG_DIR/tasks.jsonl` demonstrates capture + JSONL format (FR: capture, task fields).
- CLI transcript: `wl standup` over seeded fixture data shows yesterday/open/blocked groups (FR: standup fallback labeling).
- Test: `tests/cli.rs` + `src/store.rs`/`src/model.rs` unit tests pass via `cargo test` demonstrates round-trip integrity and config creation.

#### 2.0 Tasks

- [x] 2.1 Implement `src/model.rs`: `Task` struct (id, text, category, project, status, due, created_at, completed_at), `Status` enum, `t_`+base36 id gen, serde round-trip unit tests.
- [x] 2.2 Implement `src/store.rs`: `WORKLOG_DIR`/`~/.worklog` resolution, load/save `tasks.jsonl` (atomic temp+rename), append `archive.jsonl`, `complete_task` moves record; unit tests.
- [x] 2.3 Implement `src/config.rs`: load-or-create `config.yaml` with commented defaults (categories: priority, support, project-management, engineering, intake; editor_command).
- [x] 2.4 Implement `src/notes.rs`: list note docs, parse/write YAML frontmatter, create doc, add/edit/delete top-level list items under headings; unit tests.
- [x] 2.5 Implement `wl task` subcommand with `--category/--project/--due` (validate category against config; default `intake`).
- [x] 2.6 Implement standup report logic (shared module) + `wl standup` printer: completed-yesterday with most-recent-day fallback (labeled), open, blocked.
- [x] 2.7 Integration tests in `tests/cli.rs` using `assert_cmd`-style invocation with `WORKLOG_DIR` temp dirs.
- [x] 2.8 Commit: `feat: storage layer, task capture, standup command`.

### [x] 3.0 Legacy migration

#### 3.0 Proof Artifact(s)

- Test: `tests/import_legacy.rs` over synthetic fixtures (subsection categories, checked/unchecked items, notes sections, idempotence guard) passes.
- CLI transcript (sanitized, counts only): `WORKLOG_DIR=<scratchpad copy> wl import-legacy` run against the copy of real data demonstrates correct behavior on real files; original files intact under `legacy/`.

#### 3.0 Tasks

- [x] 3.1 Implement `src/markdown.rs`: minimal block model (headings, checklist items, list items, paragraphs) sufficient for daily-note parsing; unit tests.
- [x] 3.2 Implement importer: latest note → open tasks (### subsection → category mapping, unknown → intake) + note docs from `## Notes` and non-checklist subsection content.
- [x] 3.3 Implement historical pass: `- [x]` items across all dailies → archive records; dedupe checked items by exact text across files (old carry-over duplicated them), using the EARLIEST file date as `completed_at`. Real-data expectation: 7 unique archived tasks.
- [x] 3.4 Implement move `daily_notes/` → `legacy/`, idempotence refusal, and summary output (counts).
- [x] 3.5 Run against scratchpad copy of real `~/.worklog`; eyeball output; capture sanitized transcript as proof artifact in spec dir.
- [x] 3.6 Commit: `feat: legacy daily-notes importer`.

### [x] 4.0 TUI — Today, Standup, Tasks, Notes + $EDITOR

#### 4.0 Proof Artifact(s)

- Test: `TestBackend` rendering tests per view (Today ordering incl. overdue-first and dimmed completions; Standup groups; Tasks filter; Notes list/detail) pass via `cargo test`.
- Test/transcript: editor escape hatch test using a stub `$EDITOR` script proves suspend→edit→resume and file mtime change.
- CLI transcript: scripted session diffing `tasks.jsonl`/`archive.jsonl` before/after add/block/complete demonstrates write-through persistence.

#### 4.0 Tasks

- [x] 4.1 Implement `src/tui/mod.rs`: ratatui init/restore with panic hook, event loop, tick/draw cycle.
- [x] 4.2 Implement `src/tui/app.rs`: view enum (Today/Standup/Tasks/Notes list/Notes detail), selection state, input mode (normal vs editing buffer), confirm-dialog state.
- [x] 4.3 Implement Today view: open/blocked with overdue → due-today → rest ordering, today's completions dimmed at bottom.
- [x] 4.4 Implement Standup view reusing the shared standup module.
- [x] 4.5 Implement Tasks view: full list, `/` incremental filter, category/project cycle filters.
- [x] 4.6 Implement task actions across views: `a` add (with `@category` `#project` token parsing), `space`/`x` complete, `b` block toggle, `e` edit, `d` due date, `D` delete w/ confirm — all write-through to store.
- [x] 4.7 Implement Notes views: doc list, doc detail (headings + items), item add/edit/delete via input box, `N` new doc.
- [x] 4.8 Implement `src/tui/editor.rs`: `E` suspends TUI (leave alt screen + raw mode), spawns `$EDITOR`/config fallback/`vi` inheriting stdio, waits, restores + full redraw; reload doc from disk.
- [x] 4.9 Footer keybind hints per view; `j/k`/arrows navigation; `q`/`esc` back/quit semantics.
- [x] 4.10 `TestBackend` rendering tests per view + editor stub test.
- [x] 4.11 Commit: `feat: ratatui TUI with today/standup/tasks/notes views`.

### [x] 5.0 Homebrew installability + docs polish

#### 5.0 Proof Artifact(s)

- CLI: `wl --version` prints crate version demonstrates version reporting.
- Diff: `.github/workflows/release.yml` + `Formula/wl.rb` committed demonstrates the release pipeline.
- Docs: README "Install" section with exact tap-creation + `brew install sdavisde/tap/wl` steps demonstrates the user path.

#### 5.0 Tasks

- [x] 5.1 Write `.github/workflows/release.yml`: on tag `v*`, build aarch64-apple-darwin, tar + shasum, create GitHub release (mirror worklog-cli's pipeline where sane).
- [x] 5.2 Write `Formula/wl.rb` template (url to release tarball, sha256 placeholder documented) + README instructions for `sdavisde/homebrew-tap`.
- [x] 5.3 Finalize README (usage, keybinds table, data model, migration guide) and CLAUDE.md accuracy pass.
- [x] 5.4 Commit: `feat: release workflow and homebrew formula` and run full gate suite.
