//! `wl import-legacy` — one-shot migration from the old daily-markdown
//! workflow (`daily_notes/YYYY-MM-DD.md`) into the v2 storage model.
//!
//! Import rules (see `docs/specs/01-spec-wl-v2-rebuild/01-spec-wl-v2-rebuild.md`,
//! Unit 3, and `01-tasks-wl-v2-rebuild.md` task 3.0):
//!
//! 1. Latest daily note only -> open tasks: every `- [ ]` item directly
//!    under `## Tasks` becomes an open task with category `intake`; every
//!    `- [ ]` item under a `###` subsection becomes an open task whose
//!    category is the slugified subsection name if it matches a configured
//!    category, else `intake`.
//! 2. Latest daily note only -> note docs: non-empty `## Notes` bullets
//!    become a doc titled "Imported notes"; each `###` subsection under
//!    `## Tasks` holding non-checklist content (bullets/paragraphs)
//!    becomes its own note doc titled with the subsection name, items
//!    preserved. Empty sections are skipped entirely.
//! 3. ALL daily notes -> archive: every `- [x]` item, deduped by exact
//!    text across files, becomes a done task with `completed_at`/
//!    `created_at` set to the EARLIEST file's date (midnight local time).
//! 4. Unchecked items in non-latest files are dropped — the old
//!    carry-over already copied them forward into the latest file.
//! 5. On success, `daily_notes/` is renamed to `legacy/`; if `legacy/`
//!    already exists the importer refuses to run (idempotence guard) and
//!    nothing is modified.

use crate::config;
use crate::markdown::{self, Block};
use crate::model::{Status, Task, generate_id};
use crate::notes::{self, NotesStore};
use crate::store::Store;
use chrono::{Local, NaiveDate, TimeZone};
use color_eyre::eyre::{Result, WrapErr, eyre};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub fn run() -> Result<()> {
    let store = Store::resolve()?;
    let summary = import(&store)?;
    print_summary(&summary);
    Ok(())
}

struct Summary {
    open_tasks: usize,
    archived_tasks: usize,
    note_docs: usize,
    files_moved: usize,
}

fn print_summary(s: &Summary) {
    println!(
        "Imported {} open task(s), archived {} task(s), created {} note doc(s).",
        s.open_tasks, s.archived_tasks, s.note_docs
    );
    println!(
        "Moved {} file(s) from daily_notes/ to legacy/.",
        s.files_moved
    );
}

fn import(store: &Store) -> Result<Summary> {
    let daily_notes_dir = store.daily_notes_dir();
    let legacy_dir = store.legacy_dir();

    if legacy_dir.exists() {
        return Err(eyre!(
            "{} already exists; wl import-legacy has already run and refuses to run twice \
             (idempotence guard). Move or remove it if you really intend to re-import.",
            legacy_dir.display()
        ));
    }
    if !daily_notes_dir.exists() {
        return Err(eyre!(
            "no daily_notes/ directory found at {}; nothing to import",
            daily_notes_dir.display()
        ));
    }

    let mut files = collect_daily_note_files(&daily_notes_dir)?;
    if files.is_empty() {
        return Err(eyre!(
            "{} contains no YYYY-MM-DD.md daily notes; nothing to import",
            daily_notes_dir.display()
        ));
    }
    files.sort_by_key(|f| f.date);

    let cfg = config::load_or_create(&store.config_path())?;

    // Rule 3: archive checked items across ALL files, deduped by exact
    // text. Files are processed in ascending date order, so the first time
    // a given text is seen is by construction its earliest occurrence.
    let mut seen_texts: HashSet<String> = HashSet::new();
    let mut archived: Vec<(String, NaiveDate)> = Vec::new();
    for file in &files {
        let note = parse_daily_note(&file.content);
        for item in note.all_checklist_items() {
            if item.checked && seen_texts.insert(item.text.clone()) {
                archived.push((item.text.clone(), file.date));
            }
        }
    }

    // Rules 1 & 2: latest file only -> open tasks + note docs.
    let latest = files.last().expect("checked non-empty above");
    let latest_note = parse_daily_note(&latest.content);

    let mut open_tasks: Vec<Task> = Vec::new();
    for item in &latest_note.direct_task_items {
        if !item.checked {
            open_tasks.push(Task::new(item.text.clone(), "intake", None, None));
        }
    }
    for subsection in &latest_note.subsections {
        let category = category_for(&subsection.name, &cfg.categories);
        for item in &subsection.checklist_items {
            if !item.checked {
                open_tasks.push(Task::new(item.text.clone(), category.clone(), None, None));
            }
        }
    }

    let notes_store = NotesStore::new(store.notes_dir());
    let mut note_docs_created = 0usize;

    if !latest_note.notes_items.is_empty() {
        create_note_doc(
            &notes_store,
            "Imported notes",
            "Notes",
            &latest_note.notes_items,
        )?;
        note_docs_created += 1;
    }
    for subsection in &latest_note.subsections {
        if !subsection.other_lines.is_empty() {
            create_note_doc(
                &notes_store,
                &subsection.name,
                &subsection.name,
                &subsection.other_lines,
            )?;
            note_docs_created += 1;
        }
    }

    // Persist open tasks (appended to whatever's already in tasks.jsonl).
    let mut existing_tasks = store.load_tasks()?;
    let open_count = open_tasks.len();
    existing_tasks.extend(open_tasks);
    store.save_tasks(&existing_tasks)?;

    // Persist archived tasks.
    let archived_count = archived.len();
    for (text, date) in &archived {
        let task = new_archived_task(text, *date)?;
        store.append_archive(&task)?;
    }

    // Rule 5: move daily_notes/ -> legacy/. This happens last so a failure
    // above never leaves the source files moved without their data
    // imported.
    let files_moved = files.len();
    fs::rename(&daily_notes_dir, &legacy_dir).wrap_err_with(|| {
        format!(
            "renaming {} to {}",
            daily_notes_dir.display(),
            legacy_dir.display()
        )
    })?;

    Ok(Summary {
        open_tasks: open_count,
        archived_tasks: archived_count,
        note_docs: note_docs_created,
        files_moved,
    })
}

struct DailyNoteFile {
    date: NaiveDate,
    content: String,
}

/// Read every `YYYY-MM-DD.md` file in `dir`. Files that don't match that
/// name are skipped with a warning rather than failing the whole import.
fn collect_daily_note_files(dir: &Path) -> Result<Vec<DailyNoteFile>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).wrap_err_with(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let date = match NaiveDate::parse_from_str(&stem, "%Y-%m-%d") {
            Ok(date) => date,
            Err(_) => {
                eprintln!(
                    "warning: skipping {} (name is not YYYY-MM-DD.md)",
                    path.display()
                );
                continue;
            }
        };
        let content =
            fs::read_to_string(&path).wrap_err_with(|| format!("reading {}", path.display()))?;
        out.push(DailyNoteFile { date, content });
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct ChecklistItem {
    checked: bool,
    text: String,
}

#[derive(Debug, Default)]
struct TaskSubsection {
    name: String,
    checklist_items: Vec<ChecklistItem>,
    /// Non-checklist content (plain bullets and stray paragraph lines) —
    /// the source material for a subsection's note doc.
    other_lines: Vec<String>,
}

#[derive(Debug, Default)]
struct DailyNote {
    /// `- [ ]`/`- [x]` items directly under `## Tasks`, not inside any
    /// `###` subsection.
    direct_task_items: Vec<ChecklistItem>,
    subsections: Vec<TaskSubsection>,
    /// Bullet/paragraph lines under `## Notes`.
    notes_items: Vec<String>,
}

impl DailyNote {
    fn all_checklist_items(&self) -> impl Iterator<Item = &ChecklistItem> {
        self.direct_task_items.iter().chain(
            self.subsections
                .iter()
                .flat_map(|s| s.checklist_items.iter()),
        )
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Section {
    None,
    Tasks,
    Notes,
    Other,
}

/// Walk a daily note's blocks, tracking the current `##` section and (when
/// inside `## Tasks`) the current `###` subsection.
fn parse_daily_note(text: &str) -> DailyNote {
    let mut note = DailyNote::default();
    let mut section = Section::None;
    let mut current_subsection: Option<usize> = None;

    for block in markdown::parse_blocks(text) {
        match block {
            Block::Heading { level: 1, .. } => {
                section = Section::None;
                current_subsection = None;
            }
            Block::Heading { level: 2, text } => {
                section = if text.eq_ignore_ascii_case("Tasks") {
                    Section::Tasks
                } else if text.eq_ignore_ascii_case("Notes") {
                    Section::Notes
                } else {
                    Section::Other
                };
                current_subsection = None;
            }
            Block::Heading { level: 3, text } if section == Section::Tasks => {
                note.subsections.push(TaskSubsection {
                    name: text,
                    ..Default::default()
                });
                current_subsection = Some(note.subsections.len() - 1);
            }
            Block::Heading { .. } => {
                current_subsection = None;
            }
            Block::Checklist { checked, text } => {
                if section == Section::Tasks {
                    let item = ChecklistItem { checked, text };
                    match current_subsection {
                        Some(idx) => note.subsections[idx].checklist_items.push(item),
                        None => note.direct_task_items.push(item),
                    }
                }
            }
            Block::Bullet { text } | Block::Paragraph { text } => match section {
                Section::Tasks => {
                    if let Some(idx) = current_subsection {
                        note.subsections[idx].other_lines.push(text);
                    }
                }
                Section::Notes => note.notes_items.push(text),
                _ => {}
            },
        }
    }

    note
}

/// Map a `###` subsection name to a task category: the slugified name if
/// it matches a configured category, else `intake`.
fn category_for(subsection_name: &str, categories: &[String]) -> String {
    let slug = notes::slugify(subsection_name);
    if categories.contains(&slug) {
        slug
    } else {
        "intake".to_string()
    }
}

fn create_note_doc(store: &NotesStore, title: &str, heading: &str, items: &[String]) -> Result<()> {
    let mut doc = store.create(title, None)?;
    for item in items {
        doc.body.add_item(heading, item.as_str());
    }
    store.save(&mut doc)?;
    Ok(())
}

/// Build a done `Task` for an archived (checked) legacy item, with
/// `created_at`/`completed_at` set to midnight local time on `date`.
fn new_archived_task(text: &str, date: NaiveDate) -> Result<Task> {
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| eyre!("invalid date {date}"))?;
    let local = Local
        .from_local_datetime(&naive)
        .earliest()
        .ok_or_else(|| eyre!("could not resolve local midnight for {date}"))?;
    let timestamp = local.fixed_offset();

    Ok(Task {
        id: generate_id(),
        text: text.to_string(),
        category: "intake".to_string(),
        project: None,
        status: Status::Done,
        due: None,
        created_at: timestamp,
        completed_at: Some(timestamp),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_daily_note_splits_direct_items_subsections_and_notes() {
        let text = "# 2024-01-05\n\n\
            ## Tasks\n\n\
            - [ ] Direct intake item\n\n\
            ### Priority\n\
            - [ ] Prioritized item alpha\n\n\
            ### Misc Reference\n\
            - reference bullet one\n\
            - reference bullet two\n\n\
            ## Notes\n\
            - freeform note one\n";

        let note = parse_daily_note(text);
        assert_eq!(note.direct_task_items.len(), 1);
        assert_eq!(note.direct_task_items[0].text, "Direct intake item");
        assert!(!note.direct_task_items[0].checked);

        assert_eq!(note.subsections.len(), 2);
        assert_eq!(note.subsections[0].name, "Priority");
        assert_eq!(note.subsections[0].checklist_items.len(), 1);
        assert_eq!(note.subsections[1].name, "Misc Reference");
        assert_eq!(note.subsections[1].other_lines.len(), 2);

        assert_eq!(note.notes_items, vec!["freeform note one".to_string()]);
    }

    #[test]
    fn category_for_maps_known_subsections_and_defaults_to_intake() {
        let categories = vec![
            "priority".to_string(),
            "support".to_string(),
            "project-management".to_string(),
            "engineering".to_string(),
            "intake".to_string(),
        ];
        assert_eq!(category_for("Priority", &categories), "priority");
        assert_eq!(
            category_for("Project Management", &categories),
            "project-management"
        );
        assert_eq!(category_for("Org admins", &categories), "intake");
    }
}
