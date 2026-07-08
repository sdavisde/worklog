# Proof: manual TUI smoke test (real pty)

Run by the orchestrator after task 4.0, closing both FLAG risks from the
planning audit. All runs used `WORKLOG_DIR` pointed at a scratch dir; the real
`~/.worklog` was never touched. Content shown is synthetic.

## 1. CLI seed + standup

```
$ wl task "overdue thing @engineering #wlv2" --due 2026-07-01   # captured t_gaqo4v
$ wl task "due today" --due 2026-07-07                          # captured t_6tlxqj
$ wl task "plain intake task"                                   # captured t_x84nzv
$ wl standup
Completed yesterday
  (none)
Open
  - [intake] overdue thing @engineering #wlv2 (due 2026-07-01)
  - [intake] due today (due 2026-07-07)
  - [intake] plain intake task
Blocked
  (none)
```

## 2. TUI keystroke drive (script(1) pty, piped keys `s t n g x q`)

- Exit code 0; alternate screen entered (`?1049h`) and cleanly restored
  (`?1049l`, cursor shown).
- The `x` press completed the **overdue** task — confirming Today's
  overdue-first ordering selected it — and write-through moved it from
  `tasks.jsonl` to `archive.jsonl` with `status: "done"` + `completed_at` set.

## 3. Rendered frames (expect(1), 30x100 pty, keys `s t n g q`)

Frame text captured from the pty log confirms all views drew real content:
`Today`, `Standup`, `Tasks`, notes list (incl. `N new` footer hint), task text
`due today`, and `Completed` group headers.

## 4. $EDITOR suspend/resume (expect(1), stub editor)

```
EDITOR=stub-editor.sh   # appends "- edited via stub editor" to $1
keys: n  N  "smoke doc" <enter>  E  (editor runs)  q  q
```

- `notes/smoke-doc.md` created via the TUI with correct frontmatter.
- Stub editor ran during suspension and appended its line.
- Resumed TUI **rendered the reloaded doc** ("edited via stub editor" appears
  in post-`E` frames); clean quit, exit 0.

Result: interactive behavior, alternate-screen handling, write-through
persistence, and the $EDITOR escape hatch all verified in a real terminal.
