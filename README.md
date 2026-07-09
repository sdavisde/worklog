# wl

`wl` is a keyboard-first worklog: a global task list plus long-running note
documents, kept as plain JSONL and Markdown under `~/.worklog/`. There is no
daily file — "today" is only a *view* over timestamps, so completed tasks are
never deleted (they move to a permanent, append-only archive) and reference
notes are never copy-pasted forward day after day.

The storage format is deliberately plain and greppable so that an AI agent
(or you, with `grep`/`jq`) can read `~/.worklog/` directly — no export or
sync layer needed. See [`DESIGN.md`](./DESIGN.md) for the full design record
and rationale.

## Install

### Homebrew

```sh
brew install sdavisde/tap/wl
```

`brew` expands `sdavisde/tap` to the [`sdavisde/homebrew-tap`](https://github.com/sdavisde/homebrew-tap)
repo, which carries the canonical `Formula/wl.rb`. Both Apple Silicon
(`aarch64`) and Intel (`x86_64`) macOS builds are published.

The formula is regenerated automatically: tagging a `v*` release here runs
[`.github/workflows/release.yml`](./.github/workflows/release.yml), which
tests, builds both macOS binaries, publishes a GitHub release with the
tarballs and `.sha256` files, and then commits the updated formula (new
version + both sha256 values) to the tap repo.

**Migrating from the old `worklog-cli`?** That formula installs the same
`wl` binary name, so uninstall it first to avoid a conflict:

```sh
brew uninstall worklog-cli
brew install sdavisde/tap/wl
```

Your existing data is safe. Run `wl import-legacy` once to migrate old daily
notes into the current model — it renames `~/.worklog/daily_notes/` to
`~/.worklog/legacy/` and deletes nothing (see
[`wl import-legacy`](#wl-import-legacy) below).

### From source

```sh
cargo build --release
cp target/release/wl /usr/local/bin/   # or anywhere on $PATH
```

## Usage

```sh
wl                                                    # open the TUI
wl task "fix login @engineering #auth" --due 2026-07-10
wl standup                                            # print standup to stdout
wl import-legacy                                      # one-shot migration (see below)
```

`wl` with no subcommand opens the TUI on the **Today** view. `wl task` also
parses `@category`/`#project` tokens out of the text itself (see below), so
`--category`/`--project` flags and inline tokens both work.

### `wl import-legacy`

One-shot migration from the old daily-markdown workflow
(`<WORKLOG_DIR>/daily_notes/YYYY-MM-DD.md`) into the current model:

- The **most recent** daily note's unchecked (`- [ ]`) items become open
  tasks — items directly under `## Tasks` default to category `intake`;
  items under a `### Subsection` map to that category if it matches a
  configured category, else `intake`.
- The most recent daily note's `## Notes` content, and any `### Subsection`
  under `## Tasks` that holds non-checklist content, become note documents
  in `notes/`.
- Checked (`- [x]`) items across **all** historical daily notes are
  deduped by exact text and archived, using the earliest file's date as
  `completed_at`.
- On success, `daily_notes/` is renamed to `legacy/` and kept as read-only
  history — nothing is ever deleted.
- The importer refuses to run a second time: if `legacy/` already exists it
  aborts without modifying anything (idempotence guard).

It prints a summary of tasks imported, tasks archived, note docs created,
and files moved.

## Keybinds

Views: **Today** (home) → **Standup** (`s`) → **Tasks** (`t`) → **Notes**
(`n`); `g` returns to Today. View-switch keys and `g` only work from a
top-level view (not while inside a Notes document or an input box).

| Key | Action | Where |
|---|---|---|
| `g` | go to Today view | any top-level view |
| `s` | go to Standup view | any top-level view |
| `t` | go to Tasks view | any top-level view |
| `n` | go to Notes view | any top-level view |
| `j` / `↓` | move selection down | any view with a list |
| `k` / `↑` | move selection up | any view with a list |
| `q` / `esc` | quit | Today, Standup, Tasks, Notes list |
| `q` / `esc` | back to Notes list | Notes document view |

Task actions (Today and Tasks views):

| Key | Action |
|---|---|
| `a` | add a task (opens an input box; see token parsing below) |
| `space` / `x` | complete the selected task (moves it to the archive) |
| `b` | toggle blocked / open |
| `e` | edit the selected task's text |
| `d` | set/clear due date (`YYYY-MM-DD`; empty clears it) |
| `D` | delete the selected task (asks `y`/`n` to confirm) |

Tasks view only:

| Key | Action |
|---|---|
| `/` | incremental text filter (live as you type) |
| `c` | cycle the category filter (all → each configured category → all) |
| `p` | cycle the project filter (all → each distinct project in use → all) |

Notes views:

| Key | Action | Where |
|---|---|---|
| `enter` | open the selected document | Notes list |
| `N` | create a new note document (prompts for a title) | Notes list |
| `a` | add an item to the current/first section | Notes document |
| `e` | edit the selected item | Notes document |
| `D` | delete the selected item (asks `y`/`n` to confirm) | Notes document |
| `E` | suspend the TUI and open the document in `$EDITOR` (falling back to `editor_command` in `config.yaml`, then `vi`); resumes and reloads the document on return | Notes document |

Input boxes (add/edit/filter/due date/new note title) share one editing
mode: type to insert, `←`/`→` to move the cursor, `backspace` to delete,
`enter` to save, `esc` to cancel (an in-progress `/` filter is cleared on
cancel).

### Token parsing in the add-task box

Typing `a` on Today or Tasks opens an input box that parses inline tokens
out of the raw text as you commit it:

- `@category` — sets the task's category, **only if it matches a configured
  category** in `config.yaml`; an unrecognized `@token` is left in the task
  text as plain words. Only the first valid `@category` token is consumed.
- `#project` — sets the task's project tag. Only the first `#project` token
  is consumed.
- Anything left over (with the consumed tokens stripped) becomes the task
  text. Category defaults to `intake` if no valid `@category` token is
  present.

Example: `fix login @engineering #auth` → text `fix login`, category
`engineering`, project `auth`.

## Data model

```
~/.worklog/
  config.yaml          # categories, editor_command fallback
  tasks.jsonl          # active + blocked tasks (rewritten atomically on change)
  archive.jsonl        # completed tasks, append-only — the permanent record
  notes/
    long-term-goals.md # one markdown doc per note, YAML frontmatter
    auth-revamp.md      # project-scoped when frontmatter has `project:`
  legacy/               # daily_notes/, renamed here by `wl import-legacy`
```

`WORKLOG_DIR` overrides the storage root (used by tests and demos); it
defaults to `$HOME/.worklog`.

One `tasks.jsonl` line:

```json
{"id":"t_9f3k2m","text":"Fix login flow bug","category":"engineering","project":"auth-revamp","status":"open","due":"2026-07-10","created_at":"2026-07-07T09:14:00-05:00","completed_at":null}
```

Completing a task removes it from `tasks.jsonl` and appends the same record
(with `completed_at` set and `status` `"done"`) to `archive.jsonl`. Blocked
tasks stay in `tasks.jsonl` with `status: "blocked"`.

A note document (`notes/*.md`):

```markdown
---
title: Long-term goals
project: null
updated: 2026-07-07
---

## Areas to grow into

- Distributed systems depth — read DDIA ch. 8–9
- Public speaking: volunteer for next brown-bag
```

## Development

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Enable the local pre-commit hook (runs the three checks above):

```sh
git config core.hooksPath .githooks
```

The same three gates run in CI on every push/PR
(`.github/workflows/ci.yml`).

Tests never touch a real `~/.worklog`: every CLI/importer integration test
sets `WORKLOG_DIR` to a temporary directory, and ratatui `TestBackend` tests
drive `App::handle_key` directly. Any real-data testing (e.g. verifying the
importer against real daily notes) happens only against a gitignored local
copy, never against fixtures committed to git.
