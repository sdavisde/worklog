//! Integration tests for `wl import-legacy`, run against synthetic
//! `daily_notes/` fixture trees built entirely inside a tempdir (no
//! personal data — see `WORKLOG_DIR` note in `tests/cli.rs`).
//!
//! Fixture shape deliberately mirrors the real daily-note structures
//! catalogued from the user's actual `~/.worklog/daily_notes/` (file per
//! day, `## Tasks` with direct items and/or `###` subsections, `## Notes`,
//! checked-item carry-over duplication across consecutive files) without
//! reusing any real task/note text.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_wl")
}

fn wl(dir: &Path) -> Command {
    let mut cmd = Command::new(bin());
    cmd.env("WORKLOG_DIR", dir);
    cmd
}

/// Write the synthetic fixture tree used by most tests below:
///
/// - `2024-01-01.md`: earliest file; one checked item directly under
///   `## Tasks` (no subsection) that reappears later — exercises cross-file
///   dedupe with the EARLIEST date winning.
/// - `2024-01-02.md`: a completely empty file — exercises empty-file
///   handling.
/// - `2024-01-03.md`: a non-latest file with unchecked items (direct and
///   under a subsection) that must NOT be imported (superseded by
///   carry-over), plus a duplicate of the 2024-01-01 checked item and a
///   `## Notes` bullet that must NOT be imported (non-latest).
/// - `2024-01-05.md`: the latest file — direct unchecked item (-> intake),
///   a `### Priority` subsection unchecked item (-> priority category), a
///   `### Engineering` subsection with only a checked duplicate item, a
///   `### Support` subsection with a checked item carrying a timer suffix,
///   a `### Misc Reference` subsection of plain bullets (-> note doc,
///   category doesn't match so would've defaulted to intake had it had
///   checklist items), and `## Notes` bullets (-> "Imported notes" doc).
fn write_fixture_tree(worklog_dir: &Path) {
    let daily_notes = worklog_dir.join("daily_notes");
    fs::create_dir_all(&daily_notes).unwrap();

    fs::write(
        daily_notes.join("2024-01-01.md"),
        "# 2024-01-01\n\n\
         ## Tasks\n\n\
         ### Priority\n\n\
         - [x] Ship the widget report\n\n\
         ## Notes\n",
    )
    .unwrap();

    fs::write(daily_notes.join("2024-01-02.md"), "").unwrap();

    fs::write(
        daily_notes.join("2024-01-03.md"),
        "# 2024-01-03\n\n\
         ## Tasks\n\n\
         - [x] Ship the widget report\n\
         - [ ] Old dangling item not carried forward\n\n\
         ### Engineering\n\
         - [ ] Refactor the parser (superseded by latest)\n\n\
         ## Notes\n\
         - an old note bullet that must not be imported\n",
    )
    .unwrap();

    fs::write(
        daily_notes.join("2024-01-05.md"),
        "# 2024-01-05\n\n\
         ## Tasks\n\n\
         - [ ] Direct intake item\n\n\
         ### Priority\n\
         - [ ] Prioritized item alpha\n\n\
         ### Engineering\n\
         - [x] Ship the widget report\n\n\
         ### Support\n\
         - [x] Reply to customer ticket  [01.04.2024@10:00 - 01.04.2024@10:30]\n\n\
         ### Misc Reference\n\
         - reference bullet one\n\
         - reference bullet two\n\
         - reference bullet three\n\n\
         ## Notes\n\
         - freeform note one\n\
         - freeform note two\n",
    )
    .unwrap();
}

#[test]
fn import_legacy_produces_expected_counts_and_summary() {
    let dir = tempdir().unwrap();
    write_fixture_tree(dir.path());

    let output = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 2 open tasks: "Direct intake item" + "Prioritized item alpha".
    assert!(
        stdout.contains("Imported 2 open task(s)"),
        "stdout: {stdout}"
    );
    // 2 unique archived tasks: "Ship the widget report" (deduped across
    // three files) + "Reply to customer ticket ...".
    assert!(stdout.contains("archived 2 task(s)"), "stdout: {stdout}");
    // 2 note docs: "Imported notes" (from ## Notes) + "Misc Reference".
    assert!(stdout.contains("created 2 note doc(s)"), "stdout: {stdout}");
    assert!(
        stdout.contains("Moved 4 file(s) from daily_notes/ to legacy/"),
        "stdout: {stdout}"
    );
}

#[test]
fn open_tasks_have_correct_text_and_category() {
    let dir = tempdir().unwrap();
    write_fixture_tree(dir.path());

    let output = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(output.status.success());

    let content = fs::read_to_string(dir.path().join("tasks.jsonl")).unwrap();
    let tasks: Vec<serde_json::Value> = content
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(tasks.len(), 2);

    let direct = tasks
        .iter()
        .find(|t| t["text"] == "Direct intake item")
        .expect("direct intake item present");
    assert_eq!(direct["category"], "intake");
    assert_eq!(direct["status"], "open");

    let priority = tasks
        .iter()
        .find(|t| t["text"] == "Prioritized item alpha")
        .expect("subsection-mapped item present");
    assert_eq!(priority["category"], "priority");
    assert_eq!(priority["status"], "open");

    // Items unchecked in non-latest files must not appear at all.
    assert!(
        !tasks
            .iter()
            .any(|t| t["text"] == "Old dangling item not carried forward"),
        "non-latest unchecked item leaked into tasks.jsonl: {tasks:?}"
    );
    assert!(
        !tasks
            .iter()
            .any(|t| t["text"] == "Refactor the parser (superseded by latest)"),
        "non-latest unchecked subsection item leaked into tasks.jsonl: {tasks:?}"
    );
}

#[test]
fn archived_tasks_deduped_with_earliest_date_and_verbatim_timer_suffix() {
    let dir = tempdir().unwrap();
    write_fixture_tree(dir.path());

    let output = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(output.status.success());

    let content = fs::read_to_string(dir.path().join("archive.jsonl")).unwrap();
    let tasks: Vec<serde_json::Value> = content
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(tasks.len(), 2);

    // "Ship the widget report" is checked in 2024-01-01, 2024-01-03, and
    // 2024-01-05 — must appear exactly once, dated to the EARLIEST file.
    let shipped: Vec<_> = tasks
        .iter()
        .filter(|t| t["text"] == "Ship the widget report")
        .collect();
    assert_eq!(
        shipped.len(),
        1,
        "duplicate checked item must be deduped: {tasks:?}"
    );
    assert_eq!(shipped[0]["status"], "done");
    assert_eq!(shipped[0]["category"], "intake");
    let completed_at = shipped[0]["completed_at"].as_str().unwrap();
    assert!(
        completed_at.starts_with("2024-01-01"),
        "expected earliest date 2024-01-01, got {completed_at}"
    );
    let created_at = shipped[0]["created_at"].as_str().unwrap();
    assert!(
        created_at.starts_with("2024-01-01"),
        "expected earliest date 2024-01-01, got {created_at}"
    );

    // Timer-suffix text preserved verbatim, dated to its own (only) file.
    let ticket = tasks
        .iter()
        .find(|t| t["text"] == "Reply to customer ticket  [01.04.2024@10:00 - 01.04.2024@10:30]")
        .expect("timer-suffix task present with verbatim text");
    assert_eq!(ticket["status"], "done");
    let ticket_completed = ticket["completed_at"].as_str().unwrap();
    assert!(
        ticket_completed.starts_with("2024-01-05"),
        "got {ticket_completed}"
    );
}

#[test]
fn note_docs_created_from_notes_section_and_non_checklist_subsection() {
    let dir = tempdir().unwrap();
    write_fixture_tree(dir.path());

    let output = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(output.status.success());

    let notes_dir = dir.path().join("notes");
    let mut slugs: Vec<String> = fs::read_dir(&notes_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    slugs.sort();
    assert_eq!(
        slugs,
        vec![
            "imported-notes.md".to_string(),
            "misc-reference.md".to_string()
        ]
    );

    let imported_notes = fs::read_to_string(notes_dir.join("imported-notes.md")).unwrap();
    assert!(imported_notes.contains("title: Imported notes"));
    assert!(imported_notes.contains("- freeform note one"));
    assert!(imported_notes.contains("- freeform note two"));
    // The non-latest ## Notes bullet must not have been imported.
    assert!(!imported_notes.contains("an old note bullet"));

    let misc_reference = fs::read_to_string(notes_dir.join("misc-reference.md")).unwrap();
    assert!(misc_reference.contains("title: Misc Reference"));
    assert!(misc_reference.contains("- reference bullet one"));
    assert!(misc_reference.contains("- reference bullet two"));
    assert!(misc_reference.contains("- reference bullet three"));
}

#[test]
fn daily_notes_moved_to_legacy_with_all_files_intact() {
    let dir = tempdir().unwrap();
    write_fixture_tree(dir.path());

    let before: std::collections::BTreeMap<String, String> =
        fs::read_dir(dir.path().join("daily_notes"))
            .unwrap()
            .map(|e| {
                let path = e.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                let content = fs::read_to_string(&path).unwrap();
                (name, content)
            })
            .collect();

    let output = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(output.status.success());

    assert!(!dir.path().join("daily_notes").exists());
    let legacy_dir = dir.path().join("legacy");
    assert!(legacy_dir.exists());

    let after: std::collections::BTreeMap<String, String> = fs::read_dir(&legacy_dir)
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let content = fs::read_to_string(&path).unwrap();
            (name, content)
        })
        .collect();

    assert_eq!(
        before, after,
        "file contents must be byte-identical after move"
    );
}

#[test]
fn idempotence_guard_refuses_second_run() {
    let dir = tempdir().unwrap();
    write_fixture_tree(dir.path());

    let first = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(first.status.success());

    let tasks_before = fs::read_to_string(dir.path().join("tasks.jsonl")).unwrap();
    let archive_before = fs::read_to_string(dir.path().join("archive.jsonl")).unwrap();

    let second = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(
        !second.status.success(),
        "second import-legacy run must fail"
    );
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("legacy"), "stderr: {stderr}");

    // Nothing should have changed on the refused second run.
    let tasks_after = fs::read_to_string(dir.path().join("tasks.jsonl")).unwrap();
    let archive_after = fs::read_to_string(dir.path().join("archive.jsonl")).unwrap();
    assert_eq!(tasks_before, tasks_after);
    assert_eq!(archive_before, archive_after);
}

#[test]
fn missing_daily_notes_dir_is_a_clear_error() {
    let dir = tempdir().unwrap();
    // No daily_notes/ at all.
    let output = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("daily_notes"), "stderr: {stderr}");
}

#[test]
fn empty_daily_note_files_do_not_crash_and_contribute_nothing() {
    let dir = tempdir().unwrap();
    let daily_notes = dir.path().join("daily_notes");
    fs::create_dir_all(&daily_notes).unwrap();
    // Every file is empty, including the "latest" one.
    fs::write(daily_notes.join("2024-02-01.md"), "").unwrap();
    fs::write(daily_notes.join("2024-02-02.md"), "").unwrap();

    let output = wl(dir.path()).arg("import-legacy").output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Imported 0 open task(s)"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("archived 0 task(s)"), "stdout: {stdout}");
    assert!(stdout.contains("created 0 note doc(s)"), "stdout: {stdout}");
    assert!(
        stdout.contains("Moved 2 file(s) from daily_notes/ to legacy/"),
        "stdout: {stdout}"
    );

    assert!(dir.path().join("legacy").exists());
    assert!(
        !dir.path().join("tasks.jsonl").exists() || {
            fs::read_to_string(dir.path().join("tasks.jsonl"))
                .unwrap()
                .trim()
                .is_empty()
        }
    );
}
