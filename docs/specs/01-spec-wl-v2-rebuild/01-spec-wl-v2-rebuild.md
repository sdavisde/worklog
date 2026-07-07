# 01-spec-wl-v2-rebuild.md

## Introduction/Overview

Rebuild this repository as **wl v2**: a keyboard-driven Rust terminal application
(CLI + ratatui TUI) for task tracking and long-running notes, replacing both the
old Tauri desktop app (this repo's previous contents) and the separate
`worklog-cli` project. The primary goal is to eliminate the two structural flaws
of the daily-markdown workflow — completed tasks being deleted and notes being
duplicated across daily files — by removing "the day" as a storage unit while
keeping capture lightning fast and all data plainly readable by AI agents.

`DESIGN.md` at the repo root is the authoritative design record (agreed with the
user through a full design interview). This spec operationalizes it.

## Goals

- Replace daily markdown files with a global task list (JSONL) and named
  long-running note documents (markdown + YAML frontmatter) under `~/.worklog/`.
- Preserve completed tasks forever in an append-only archive that serves as the
  AI/resume corpus.
- Keep capture instant: `wl task "..."` from any shell; bare `wl` opens the TUI.
- Provide Today (home), Standup, Tasks, and Notes views in a fully
  keyboard-driven TUI, including an `$EDITOR` escape hatch for note documents.
- Migrate existing `~/.worklog/daily_notes/` data via a one-shot importer,
  verified against a copy of the user's real data.
- Ship an AI-native repository: minimal-correct `CLAUDE.md`, strict lint gates
  (rustfmt + clippy `-D warnings`), a pre-commit hook enforcing them, and CI.
- Be installable via Homebrew (personal tap) or ship exact instructions for it.

## User Stories

- **As a developer living in the terminal**, I want to add a task in one shell
  command without opening anything, so that capturing work never interrupts flow.
- **As a daily standup participant**, I want one keystroke (or `wl standup`) to
  show what I completed yesterday, what's open today, and what's blocked, so I
  can read it straight into the meeting.
- **As someone maintaining a resume**, I want every completed task permanently
  archived with timestamps, so an AI agent can later answer "what did I do last
  quarter?" from `~/.worklog/` directly.
- **As a note-keeper**, I want long-running reference documents ("Long-term
  goals", per-project notes) that exist once instead of being copied into every
  daily file, so my notes stay deduplicated and AI-consumable.
- **As a Vim user**, I want to press one key to open a note document in my
  `$EDITOR` from the TUI and return where I left off, so structured tooling
  never blocks freeform writing.
- **As the repo owner**, I want lint/format/test gates enforced by pre-commit
  hook and CI, so AI agents contributing here cannot land broken code.

## Demoable Units of Work

### Unit 1: Repo reset + Rust scaffold + AI-native tooling

**Purpose:** Clear out the obsolete Tauri app (preserving git history) and stand
up a compiling, lint-clean Rust workspace with quality gates, so all later work
lands on solid rails.

**Functional Requirements:**
- The repository shall contain no Tauri/React application files after the reset;
  `DESIGN.md`, `docs/specs/`, and git history are preserved.
- The system shall build a binary named `wl` via `cargo build` from a single
  crate at the repo root.
- The repository shall enforce `cargo fmt --check`, `cargo clippy --all-targets
  -- -D warnings`, and `cargo test` via a `.githooks/pre-commit` hook wired with
  `git config core.hooksPath .githooks`, plus a GitHub Actions CI workflow
  running the same three gates.
- The repository shall contain a rewritten `CLAUDE.md` with minimal, correct
  instructions (build/test/lint commands, architecture map, data-model summary,
  keyboard-first philosophy) and a rewritten `README.md`.
- Personal worklog data shall never be committed: test fixtures use synthetic
  data; any real-data directory used locally is gitignored.

**Proof Artifacts:**
- CLI: `cargo build && cargo clippy --all-targets -- -D warnings && cargo fmt --check` exiting 0 demonstrates a clean scaffold.
- CLI: `git log --oneline | head` showing prior Tauri history above the reset commit demonstrates history preservation.
- File: `.githooks/pre-commit` + CI workflow file demonstrate enforced gates.

### Unit 2: Storage layer + CLI capture + standup

**Purpose:** The data model on disk plus the two highest-value non-TUI commands,
proving the format end-to-end before any UI exists.

**Functional Requirements:**
- The system shall store active/blocked tasks in `~/.worklog/tasks.jsonl` (one
  JSON object per line) and completed tasks appended to `~/.worklog/archive.jsonl`.
- A task shall carry: `id`, `text`, `category` (string, from a configurable
  list), optional `project`, `status` (`open` | `blocked` | `done`), optional
  `due` (date), `created_at`, and `completed_at` (null until done), all
  serialized in RFC 3339 timestamps.
- `wl task "<text>"` shall append an open task, supporting `--category`,
  `--project`, and `--due YYYY-MM-DD` flags, defaulting category to `intake`.
- `wl standup` shall print to stdout: tasks completed yesterday (falling back to
  the most recent day with completions when yesterday has none, labeled as such),
  open tasks, and blocked tasks, grouped with headers.
- The system shall read/write `~/.worklog/config.yaml` (categories list, editor
  command fallback), creating a commented default on first run.
- Notes shall live as individual markdown files in `~/.worklog/notes/` with YAML
  frontmatter (`title`, optional `project`, `updated`).
- All storage paths shall be overridable via a `WORKLOG_DIR` environment
  variable so tests and demos never touch real data.

**Proof Artifacts:**
- CLI transcript: `WORKLOG_DIR=<tmp> wl task "demo" --category engineering` followed by `cat <tmp>/tasks.jsonl` demonstrates capture and format.
- CLI transcript: `wl standup` output over seeded fixture data demonstrates the standup report.
- Test: storage round-trip unit tests (`cargo test`) pass, demonstrating serialization stability.

### Unit 3: Legacy migration

**Purpose:** One-shot import of the old daily-note world into the new model,
validated against a copy of the user's real data (kept outside git).

**Functional Requirements:**
- `wl import-legacy` shall parse the most recent daily note in
  `<WORKLOG_DIR>/daily_notes/` and convert unchecked checklist items under
  `## Tasks` (including items under `###` subsections, mapped to categories)
  into open tasks in `tasks.jsonl`.
- The importer shall convert content under `## Notes` (and non-checklist
  content under `## Tasks` subsections) into note documents in `notes/`.
- The importer shall convert checked items (`- [x]`) from **all** historical
  daily notes into archived tasks, using the file's date as `completed_at`.
- The importer shall move `daily_notes/` to `legacy/` after a successful import
  and shall refuse to run twice (idempotence guard: `legacy/` already exists).
- The importer shall print a summary (N tasks imported, N archived, N note docs
  created, files moved) and never delete file contents.

**Proof Artifacts:**
- CLI transcript: running `WORKLOG_DIR=<copy-of-real-data> wl import-legacy` against the scratchpad copy of `~/.worklog` demonstrates correct counts and output on real data.
- Test: importer integration test over a synthetic fixture tree (`cargo test`) demonstrates parsing rules.

### Unit 4: TUI — Today, Standup, Tasks, Notes + $EDITOR

**Purpose:** The daily driver: bare `wl` opens a keyboard-only ratatui interface
over the same storage.

**Functional Requirements:**
- Bare `wl` shall open the TUI on the **Today** view: open/blocked tasks
  (overdue and due-today surfaced first), with today's completions shown
  dimmed at the bottom.
- `s` shall show the **Standup** view (same content as `wl standup`); `t` the
  full **Tasks** view with filtering (`/` incremental filter; category/project
  cycling); `n` the **Notes** view (document list → document detail).
- The user shall add a task from any task view via `a` (input box; supports
  `@category` and `#project` tokens inline), complete with `space`/`x`, toggle
  blocked with `b`, edit text with `e`, set/clear due date with `d`, and delete
  with `D` (with confirm).
- In the Notes document view, the user shall add/edit/delete list items via
  input boxes, and `E` shall suspend the TUI, open the document in `$EDITOR`
  (falling back to config `editor_command`, then `vi`), and redraw on return —
  the lazygit pattern (leave alternate screen/raw mode, inherit stdio, restore).
- Completing a task in the TUI shall move it from `tasks.jsonl` to
  `archive.jsonl` immediately (write-through; no unsaved state on quit).
- `q`/`esc` shall navigate back/quit; `j`/`k` and arrows shall move selection;
  every action shall be reachable without a mouse; a footer shall show
  context-relevant keybinds.

**Proof Artifacts:**
- Test: ratatui `TestBackend` rendering tests for each view over fixture data demonstrate layout and content.
- CLI transcript: scripted TUI session (or captured frames) showing add → block → complete flow, with before/after `tasks.jsonl`/`archive.jsonl` diffs demonstrating write-through.
- CLI transcript: `E` opening `$EDITOR` (using a stub editor script in tests) and the TUI resuming demonstrates the escape hatch.

### Unit 5: Homebrew installability

**Purpose:** `brew install`-able releases, or failing that, exact repeatable
instructions.

**Functional Requirements:**
- The repository shall contain a release GitHub Actions workflow building an
  `aarch64-apple-darwin` binary tarball on tag push.
- The repository shall contain a ready-to-use Homebrew formula (`Formula/wl.rb`
  or docs equivalent) pointing at the release tarball, plus README instructions
  for creating `sdavisde/homebrew-tap` and installing via
  `brew install sdavisde/tap/wl`.
- `wl --version` shall report the crate version.

**Proof Artifacts:**
- File: release workflow + formula committed demonstrates the pipeline.
- CLI: `wl --version` output demonstrates version reporting.
- Docs: README "Install" section demonstrates the user-facing path.

## Non-Goals (Out of Scope)

1. **No ticket references** (Azure DevOps ids) on tasks — deliberately deferred;
   JSONL makes the field addition trivial later.
2. **No AI integration layer** (no MCP server, no export/summary commands beyond
   `wl standup`) — agents read `~/.worklog/` directly; the format is the contract.
3. **No sync/cloud** — local-first only; users may git-init `~/.worklog` themselves.
4. **No daily note files** — the day exists only as views over timestamps.
5. **No prose editor inside the TUI** — multi-line editing happens via `$EDITOR`;
   TUI input boxes are single-line (with token parsing).
6. **No Windows/Linux release packaging** in this pass (code stays portable, but
   only the macOS aarch64 release path is delivered).
7. **No historical reconstruction** of completed tasks by diffing old dailies
   beyond the direct `- [x]` import described in Unit 3.

## Design Considerations

Keyboard-first is absolute: every interaction reachable without a mouse, footer
hints on every view, auto-focus semantics equivalent for the TUI (initial
selection always set). Visual style follows lazygit/gitui conventions: list
panes, dim-styled completed items, minimal chrome. Category and project render
as compact tags on task rows; overdue dates render highlighted.

## Repository Standards

This is a repo reset; standards are established by this spec:

- Rust 2024 edition, rustfmt defaults, clippy with `-D warnings`.
- Conventional Commits messages (matching the user's `commit` skill).
- Tests colocated (`#[cfg(test)]`) for units; `tests/` for CLI/importer
  integration tests using `WORKLOG_DIR` temp dirs.
- `CLAUDE.md` kept minimal and correct; `DESIGN.md` is the design record.
- No personal data in git: fixtures are synthetic; real-data testing happens in
  the session scratchpad only.

## Technical Considerations

- **Crates:** `ratatui` + `crossterm` (TUI), `clap` derive (CLI), `serde` +
  `serde_json` (tasks), `chrono` (dates), a maintained YAML crate for
  config/frontmatter (chosen from current-standards research at implementation
  time; `serde_yaml` proper is unmaintained), `anyhow`-style error handling.
  Exact versions pinned from the research pass.
- **$EDITOR suspension:** restore terminal (leave alternate screen + raw mode),
  spawn `$EDITOR <file>` inheriting stdio, wait, re-enter TUI and force a full
  redraw; install panic hooks that restore the terminal (ratatui's current
  init/restore helpers).
- **Write strategy:** `tasks.jsonl` is small — rewrite atomically (temp file +
  rename) on every mutation; `archive.jsonl` is append-only.
- **IDs:** short random ids (e.g. `t_` + 6 base36 chars) generated at capture.
- **Old-code reuse:** the markdown block parser and config patterns from
  `worklog-cli` (cloned in session scratchpad) may be ported where they help the
  importer/notes layer; being Rust 2024-clean matters more than reuse.
- **Standup "yesterday":** calendar yesterday, falling back to the most recent
  day with completions (labeled), so Monday standups show Friday's work.

## Security Considerations

- All data is local under `~/.worklog/`; no network calls, no telemetry.
- Real personal notes must never enter git or test fixtures; the migration is
  verified against a scratchpad copy only. Proof artifacts committed to the repo
  must use synthetic data.
- The pre-commit hook must not read or print worklog data.

## Success Metrics

1. **Capture latency**: `wl task "x"` completes in under ~50ms perceived (single
   process, no daemon).
2. **Data integrity**: zero task loss across complete/block/edit operations —
   proven by round-trip tests; completed tasks always land in `archive.jsonl`.
3. **Migration correctness**: import of the real-data copy produces counts the
   user can eyeball-verify, with original files preserved under `legacy/`.
4. **Gate enforcement**: pre-commit and CI both fail on any clippy warning,
   format drift, or test failure.
5. **Install path**: `brew install sdavisde/tap/wl` works once the tap repo
   exists (or README instructions reproduce it exactly).

## Open Questions

Non-blocking; defaults chosen so implementation can proceed:

1. Default category list is `priority, support, project-management, engineering,
   intake` (mirrors the old template); user can edit `config.yaml`.
2. Whether `wl note "..."` quick-capture (append to an inbox note doc) is wanted
   — deferred; not in scope unless trivially cheap during Unit 2.
3. Exact color theme — implementer's judgment following lazygit-like conventions.
