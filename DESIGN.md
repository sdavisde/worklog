# wl v2 — Design

A keyboard-driven Rust TUI for tasks and long-running notes. Replaces both the
Tauri desktop app (this repo's previous life) and `worklog-cli`. Ships as `wl`.

## Why

The daily-markdown workflow (`worklog-cli`) had two structural flaws:

1. **Completed tasks were deleted** on daily carry-over — destroying exactly the
   data needed for the AI/resume use case ("what did I actually do?").
2. **Notes sections were copied forward every day** — the same reference lists
   duplicated across hundreds of files, clouding context for any AI consumer.

The root cause: a *day* was the storage unit, but tasks and reference notes
aren't daily data. v2 removes the day as a file and keeps it only as a view.

## Decisions (settled)

| Question | Decision |
|---|---|
| Editing surface | TUI owns everything (ratatui). No editor shell-out. Notes are structured items under headings — short multi-line entries, not prose — so small TUI input boxes suffice. |
| Task lifecycle | Completing a task archives it forever with a timestamp. Nothing is ever deleted. The archive **is** the AI corpus. |
| Data spine | One global task list. Tasks carry a category (priority / support / engineering / …, configurable) and an optional project. Notes are named documents, optionally project-scoped. Capture is instant; triage happens later (the old "Intake" pattern, formalized). |
| Storage | Tasks + archive as JSONL; notes as markdown files with YAML frontmatter. Greppable, git-syncable, hand-editable, trivially AI-readable. |
| The day | No daily file. Home screen is a **Today** view; a **Standup** view (hotkey) shows yesterday's completions + today's open/blocked tasks. Timestamps reconstruct any day on demand. |
| Quick capture | CLI subcommands stay (`wl task "..."`) — the TUI is never a toll booth. Bare `wl` opens the TUI. |
| AI interface | None. Agents read `~/.worklog/` directly; the file format is the contract. If an agent can't make sense of the files, the format is wrong. |
| Codebase | This repo, wiped and rebuilt in Rust. Replaces `worklog-cli`; keeps the `wl` binary name. Port the markdown parser and config code from worklog-cli where useful. |
| Migration | One-shot `wl import-legacy`. Because old carry-over copied everything forward, the **most recent** daily note is a superset of current state — parse it for live tasks and note sections; move old files to `~/.worklog/legacy/` as read-only history. |
| Task fields | text, category, optional project, status (`open` \| `blocked` \| `done`), optional due date, created/completed timestamps. No ticket refs for now (JSONL makes adding fields painless later). |

## Data model

```
~/.worklog/
  config.yaml          # categories, keybind overrides, etc.
  tasks.jsonl          # active + blocked tasks (small, rewritten on change)
  archive.jsonl        # completed tasks (append-only, grows forever)
  notes/
    long-term-goals.md # one markdown doc per note, YAML frontmatter
    areas-to-grow.md
    auth-revamp.md     # project-scoped when frontmatter has `project:`
  legacy/              # old daily notes, read-only
```

Task line:

```json
{"id":"t_9f3k2m","text":"Fix login flow bug","category":"engineering","project":"auth-revamp","status":"open","due":"2026-07-10","created_at":"2026-07-07T09:14:00-05:00","completed_at":null}
```

Completing a task removes it from `tasks.jsonl` and appends it (with
`completed_at`) to `archive.jsonl`. Blocked tasks stay in `tasks.jsonl` with
`status: "blocked"`.

Note doc:

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

## TUI

Views (single window, one active view at a time):

- **Today** (home) — open/blocked tasks due or created recently, plus today's
  completions greyed at the bottom. Overdue items surfaced.
- **Standup** (`s`) — yesterday's completions / today's open / blocked & waiting.
- **Tasks** — full active list; filter by category, project, status.
- **Notes** — doc list → doc view; navigate headings/items, add/edit/delete
  items via an input box.

Keybind sketch (final bindings configurable in `config.yaml`):

| Key | Action |
|---|---|
| `a` | add task (input box; `#project` and `@category` tokens parsed inline) |
| `space`/`x` | complete task |
| `b` | toggle blocked |
| `e` | edit selected task/item |
| `d` | set/clear due date |
| `s` | standup view |
| `t` / `n` | tasks / notes view |
| `/` | filter |
| `j`/`k`, arrows | move |
| `q` / `esc` | back / quit |

## CLI

```
wl                    # open TUI
wl task "..."         # quick capture (flags: --category --project --due)
wl standup            # print standup to stdout (script/agent friendly)
wl import-legacy      # one-shot migration from daily notes
```

## Stack

Rust. `ratatui` + `crossterm` for the TUI; `clap`, `serde`/`serde_json`,
`serde_yaml`, `chrono` carried over from worklog-cli, plus its markdown block
parser (for note docs and the legacy importer).
