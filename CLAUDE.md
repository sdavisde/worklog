# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Commands

- Build: `cargo build`
- Run: `cargo run --bin wl -- <args>`
- Format check: `cargo fmt --check` (fix with `cargo fmt`)
- Lint: `cargo clippy --all-targets -- -D warnings`
- Test: `cargo test`

## Quality Gates

Every commit must pass `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`, and `cargo test`. These run automatically via
`.githooks/pre-commit` (enabled locally with
`git config core.hooksPath .githooks`) and again in CI
(`.github/workflows/ci.yml`) on every push/PR.

## Architecture

Single crate at repo root, binary `wl` (`src/main.rs`, clap derive CLI; bare
`wl` opens the TUI, subcommands are `task`, `standup`, `import-legacy`).
Modules (see `docs/specs/01-spec-wl-v2-rebuild/01-tasks-wl-v2-rebuild.md`
Relevant Files table for the authoritative list):

- `src/model.rs` — `Task`/`Status` types, id generation.
- `src/store.rs` — JSONL read/write, `WORKLOG_DIR` resolution.
- `src/config.rs` — `config.yaml` load/create.
- `src/notes.rs` — note document + frontmatter handling.
- `src/markdown.rs` — minimal markdown block parser (importer/notes).
- `src/standup.rs` — shared standup-report builder used by both `wl standup`
  and the TUI's Standup view.
- `src/commands/` — `task`, `standup`, `import_legacy` subcommand handlers.
- `src/tui/` — ratatui views (Today/Standup/Tasks/Notes), event loop, `$EDITOR`
  escape hatch (`src/tui/app.rs` is the state machine; `src/tui/views/` is
  pure rendering).

## Data Model

All data lives under `~/.worklog/` (override with `WORKLOG_DIR` — tests and
demos must always set this and must never touch a real `~/.worklog`):

- `config.yaml` — categories, editor fallback.
- `tasks.jsonl` — active/blocked tasks, rewritten atomically on change.
- `archive.jsonl` — completed tasks, append-only (the permanent record).
- `notes/*.md` — long-running note documents with YAML frontmatter.
- `legacy/` — old `daily_notes/`, renamed here by the one-shot
  `wl import-legacy` migration (refuses to re-run once this exists).

See `DESIGN.md` for the full rationale and `docs/specs/` for the spec/task
breakdown driving implementation.

## Philosophy

Keyboard-first, always: every feature must be usable without a mouse. No
personal worklog data is ever committed — fixtures are synthetic, and any
real-data testing happens only in a gitignored local/scratch directory.
