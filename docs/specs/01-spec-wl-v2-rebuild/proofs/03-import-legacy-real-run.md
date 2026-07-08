# Unit 3 proof: `wl import-legacy` against real data (sanitized)

This is a sanitized transcript of running `wl import-legacy` against a copy
of the user's real `~/.worklog` (18 files under `daily_notes/`). It contains
**only commands, counts, and structural verification checks** — no task or
note text/content, per the security rule in
`docs/specs/01-spec-wl-v2-rebuild/01-spec-wl-v2-rebuild.md` (migration is
verified against a scratchpad copy only; proof artifacts committed to the
repo use no real content).

## Setup

The real `~/.worklog` was pre-copied by the operator to a pristine,
untouched backup directory outside the repo (session scratchpad). A fresh
working copy was made from that backup so the backup itself was never
modified:

```
cp -R <scratchpad>/worklog-data-backup <scratchpad>/import-run-1
```

Backup contents: `config.yaml`, `daily_notes/` (18 `.md` files),
`summaries/`, `tasks.csv`.

## Run

```
$ WORKLOG_DIR=<scratchpad>/import-run-1 wl import-legacy
Imported 1 open task(s), archived 7 task(s), created 1 note doc(s).
Moved 18 file(s) from daily_notes/ to legacy/.
```

Exit code: `0`.

## Verification checks performed

All checks below were run against the *working copy* only; the pristine
backup was never modified.

1. **Archived task count** — `archive.jsonl` contains exactly **7** records,
   all `status: done`, `category: intake`. `completed_at`/`created_at` dates
   (no text) observed: `2025-07-20`, `2025-08-13`, `2025-08-17` (×2),
   `2025-08-30` (×2), `2025-12-05` — consistent with deduping repeated
   checked items across consecutive daily notes and keeping each one's
   earliest occurrence.
2. **Open task count** — `tasks.jsonl` contains exactly **1** record,
   `status: open`, `category: intake`.
3. **Note doc count** — `notes/` contains exactly **1** file
   (`org-admins.md`), frontmatter `title: Org admins`, with **4** bullet
   items in its body.
4. **File move** — `daily_notes/` no longer exists in the working copy;
   `legacy/` exists and contains exactly **18** files.
5. **Byte-for-byte integrity** — `diff -rq <backup>/daily_notes
   <working-copy>/legacy` reported **no differences**: all 18 files are
   byte-identical to the pristine backup after the move.
6. **Idempotence guard** — running `wl import-legacy` a second time against
   the same working copy exits non-zero with an error naming the existing
   `legacy/` path and refusing to re-import; no files were modified by the
   second run.

## Result

All counts matched the hand-derived expectation exactly (7 archived, 1
open, 1 note doc, 18 files moved, byte-identical `legacy/`); no
discrepancies were found.
