//! TUI tests: `TestBackend` rendering assertions per view over seeded fixture
//! data, plus state-transition tests that feed synthetic key events through
//! the same `handle_key` path the event loop uses. All I/O is confined to
//! `tempfile` temp dirs; nothing touches a real `~/.worklog`.

use super::app::{App, Mode, View};
use super::editor;
use super::views;
use crate::config::Config;
use crate::model::{Status, Task};
use crate::notes::NotesStore;
use crate::store::Store;
use chrono::{Duration, Local, NaiveDate};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;
use tempfile::TempDir;

// ---- fixtures & helpers ---------------------------------------------------

fn app_in(dir: &Path) -> App {
    let store = Store::new(dir);
    let notes = NotesStore::new(dir.join("notes"));
    App::new(store, notes, Config::default()).unwrap()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn task(text: &str, category: &str, status: Status, due: Option<NaiveDate>) -> Task {
    let mut t = Task::new(text, category, None, due);
    t.status = status;
    t
}

fn archived_on(text: &str, days_ago: i64) -> Task {
    let mut t = Task::new(text, "engineering", None, None);
    t.status = Status::Done;
    t.completed_at = Some((Local::now() - Duration::days(days_ago)).fixed_offset());
    t
}

fn render(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| views::draw(app, frame)).unwrap();
    terminal.backend().to_string()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(key(code)).unwrap();
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        app.handle_key(key(KeyCode::Char(c))).unwrap();
    }
}

// ---- rendering tests ------------------------------------------------------

#[test]
fn today_orders_overdue_first_and_dims_completions() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[
            task(
                "future item",
                "engineering",
                Status::Open,
                Some(today() + Duration::days(5)),
            ),
            task(
                "overdue item",
                "engineering",
                Status::Open,
                Some(today() - Duration::days(2)),
            ),
            task("due today item", "engineering", Status::Open, Some(today())),
        ])
        .unwrap();
    store
        .append_archive(&archived_on("finished today", 0))
        .unwrap();

    let app = app_in(dir.path());
    let out = render(&app);

    let overdue = out.find("overdue item").expect("overdue rendered");
    let due_today = out.find("due today item").expect("due-today rendered");
    let future = out.find("future item").expect("future rendered");
    assert!(overdue < due_today, "overdue should sort before due-today");
    assert!(due_today < future, "due-today should sort before future");

    assert!(
        out.contains("Completed today"),
        "completions header present"
    );
    assert!(out.contains("finished today"), "completion row present");
}

#[test]
fn standup_view_shows_three_groups() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[
            task("open work", "engineering", Status::Open, None),
            task("blocked work", "support", Status::Blocked, None),
        ])
        .unwrap();
    store
        .append_archive(&archived_on("shipped yesterday", 1))
        .unwrap();

    let mut app = app_in(dir.path());
    app.view = View::Standup;
    let out = render(&app);

    assert!(out.contains("Completed yesterday"));
    assert!(out.contains("shipped yesterday"));
    assert!(out.contains("Open"));
    assert!(out.contains("open work"));
    assert!(out.contains("Blocked"));
    assert!(out.contains("blocked work"));
}

#[test]
fn tasks_view_incremental_filter_narrows_list() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[
            task("fix login bug", "engineering", Status::Open, None),
            task("write docs", "intake", Status::Open, None),
        ])
        .unwrap();

    let mut app = app_in(dir.path());
    app.view = View::Tasks;

    let before = render(&app);
    assert!(before.contains("fix login bug"));
    assert!(before.contains("write docs"));

    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "login");
    let after = render(&app);
    assert!(after.contains("fix login bug"), "match kept");
    assert!(!after.contains("write docs"), "non-match filtered out");

    // esc clears the filter
    press(&mut app, KeyCode::Esc);
    let cleared = render(&app);
    assert!(cleared.contains("write docs"), "filter cleared");
}

#[test]
fn tasks_view_footer_shows_hints() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.view = View::Tasks;
    let out = render(&app);
    assert!(out.contains("filter"), "footer hint present");
    assert!(out.contains("cat["), "category filter indicator present");
}

#[test]
fn notes_list_and_detail_render_content() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let mut doc = notes.create("Long-term goals", None).unwrap();
    doc.body.add_item("Areas to grow into", "read DDIA ch. 8-9");
    notes.save(&mut doc).unwrap();

    let mut app = app_in(dir.path());
    app.view = View::NotesList;
    let list_out = render(&app);
    assert!(list_out.contains("Long-term goals"), "note title in list");
    assert!(list_out.contains("1 items"), "item count in list");

    // open the doc
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::NoteDetail);
    let detail_out = render(&app);
    assert!(
        detail_out.contains("Areas to grow into"),
        "heading rendered"
    );
    assert!(detail_out.contains("read DDIA ch. 8-9"), "item rendered");
    assert!(detail_out.contains("editor"), "detail footer hint present");
}

// ---- state-transition tests ----------------------------------------------

#[test]
fn add_task_parses_category_and_project_tokens() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.view = View::Tasks;

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "fix login @engineering #auth");
    press(&mut app, KeyCode::Enter);

    let saved = Store::new(dir.path()).load_tasks().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].text, "fix login");
    assert_eq!(saved[0].category, "engineering");
    assert_eq!(saved[0].project.as_deref(), Some("auth"));
}

#[test]
fn complete_moves_task_from_tasks_to_archive_on_disk() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("finish it", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    // Today view, first task selected by default.
    press(&mut app, KeyCode::Char(' '));

    let reader = Store::new(dir.path());
    assert!(
        reader.load_tasks().unwrap().is_empty(),
        "tasks.jsonl emptied"
    );
    let archived = reader.load_archive().unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].status, Status::Done);
    assert_eq!(archived[0].text, "finish it");
}

#[test]
fn block_toggle_persists() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("blockable", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('b'));
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].status,
        Status::Blocked
    );

    press(&mut app, KeyCode::Char('b'));
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].status,
        Status::Open
    );
}

#[test]
fn edit_task_text_persists() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("old text", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('e'));
    // clear the prefilled buffer, then type new text
    for _ in 0.."old text".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "new text");
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].text,
        "new text"
    );
}

#[test]
fn due_date_set_and_invalid_input_shows_footer_error() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("with due", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Char('d'));
    type_str(&mut app, "2026-08-01");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].due,
        Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap())
    );

    // invalid date: footer error, no crash, due unchanged
    press(&mut app, KeyCode::Char('d'));
    for _ in 0.."2026-08-01".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "notadate");
    press(&mut app, KeyCode::Enter);
    assert!(app.footer_msg.is_some(), "footer error set");
    assert_eq!(
        Store::new(dir.path()).load_tasks().unwrap()[0].due,
        Some(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()),
        "due unchanged on invalid input"
    );
}

#[test]
fn delete_confirm_and_cancel() {
    let dir = TempDir::new().unwrap();
    let store = Store::new(dir.path());
    store
        .save_tasks(&[task("deletable", "engineering", Status::Open, None)])
        .unwrap();

    let mut app = app_in(dir.path());

    // cancel keeps the task
    press(&mut app, KeyCode::Char('D'));
    assert!(matches!(app.mode, Mode::ConfirmDelete));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(Store::new(dir.path()).load_tasks().unwrap().len(), 1);

    // confirm removes it
    press(&mut app, KeyCode::Char('D'));
    press(&mut app, KeyCode::Char('y'));
    assert!(Store::new(dir.path()).load_tasks().unwrap().is_empty());
}

#[test]
fn new_note_creates_doc_and_opens_detail() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path());
    app.view = View::NotesList;

    press(&mut app, KeyCode::Char('N'));
    type_str(&mut app, "Scratchpad");
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.view, View::NoteDetail);
    let listed = NotesStore::new(dir.path().join("notes")).list().unwrap();
    assert!(listed.iter().any(|(_, title)| title == "Scratchpad"));
}

#[test]
fn add_note_item_persists_and_delete_removes_it() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let doc = notes.create("Ideas", None).unwrap();
    let slug = doc.slug.clone();

    let mut app = app_in(dir.path());
    app.view = View::NotesList;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::NoteDetail);

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "ship the thing");
    press(&mut app, KeyCode::Enter);

    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    assert_eq!(reloaded.body.items("Notes"), vec!["ship the thing"]);

    // delete it via confirm
    press(&mut app, KeyCode::Char('D'));
    press(&mut app, KeyCode::Char('y'));
    let after = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    assert!(after.body.items("Notes").is_empty());
}

// ---- editor escape hatch --------------------------------------------------

#[test]
fn editor_request_set_and_roundtrip_reloads_doc() {
    let dir = TempDir::new().unwrap();
    let notes = NotesStore::new(dir.path().join("notes"));
    let mut doc = notes.create("Editable", None).unwrap();
    doc.body.add_item("Notes", "original");
    notes.save(&mut doc).unwrap();
    let slug = doc.slug.clone();

    // a stub editor that appends a new list item line to the file
    let stub = dir.path().join("stub-editor.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nprintf -- '- edited by stub\\n' >> \"$1\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&stub, perms).unwrap();
    }

    let mut app = app_in(dir.path());
    app.view = View::NotesList;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::NoteDetail);

    press(&mut app, KeyCode::Char('E'));
    assert!(app.editor_request.is_some(), "editor request queued");

    // Run the editor half of the escape hatch (terminal suspend/resume is the
    // event loop's job and needs no TTY here).
    let editor_cmd = editor::resolve_editor(&app.config);
    let _ = editor_cmd; // resolve chain covered by editor unit tests
    app.run_editor(stub.to_str().unwrap()).unwrap();

    assert!(app.editor_request.is_none(), "request consumed");
    let reloaded = NotesStore::new(dir.path().join("notes"))
        .load(&slug)
        .unwrap();
    let items = reloaded.body.items("Notes");
    assert!(
        items.contains(&"edited by stub"),
        "doc reloaded with stub edit: {items:?}"
    );
}
