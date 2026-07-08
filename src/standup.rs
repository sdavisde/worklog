//! Shared standup report logic, consumed by `wl standup` (and, later, the
//! TUI's Standup view).

use crate::model::{Status, Task};
use crate::store::Store;
use chrono::{Local, NaiveDate};
use color_eyre::eyre::Result;

/// A built standup report: what was completed (with a label describing
/// which day, since "yesterday" falls back to the most recent day with any
/// completions), what's open, and what's blocked.
pub struct StandupReport {
    pub completed_label: String,
    pub completed: Vec<Task>,
    pub open: Vec<Task>,
    pub blocked: Vec<Task>,
}

/// Build a standup report from the store's current `tasks.jsonl` and
/// `archive.jsonl`.
pub fn build_report(store: &Store) -> Result<StandupReport> {
    let archive = store.load_archive()?;
    let tasks = store.load_tasks()?;

    let today = Local::now().date_naive();
    let (completed_label, completed) = completions_for(&archive, today);

    let open = tasks
        .iter()
        .filter(|t| t.status == Status::Open)
        .cloned()
        .collect();
    let blocked = tasks
        .iter()
        .filter(|t| t.status == Status::Blocked)
        .cloned()
        .collect();

    Ok(StandupReport {
        completed_label,
        completed,
        open,
        blocked,
    })
}

/// Find the completions to show: calendar yesterday if any exist, otherwise
/// the most recent earlier day that has completions (labeled as a
/// fallback).
fn completions_for(archive: &[Task], today: NaiveDate) -> (String, Vec<Task>) {
    let yesterday = today.pred_opt().unwrap_or(today);

    let yesterday_completions = completions_on(archive, yesterday);
    if !yesterday_completions.is_empty() {
        return ("Completed yesterday".to_string(), yesterday_completions);
    }

    let most_recent = archive
        .iter()
        .filter_map(|t| t.completed_at.map(|c| c.date_naive()))
        .filter(|d| *d < today)
        .max();

    match most_recent {
        Some(date) => {
            let items = completions_on(archive, date);
            let label = format!("Completed {} (most recent)", format_day_header(date));
            (label, items)
        }
        None => ("Completed yesterday".to_string(), Vec::new()),
    }
}

fn completions_on(archive: &[Task], date: NaiveDate) -> Vec<Task> {
    archive
        .iter()
        .filter(|t| t.completed_at.map(|c| c.date_naive()) == Some(date))
        .cloned()
        .collect()
}

fn format_day_header(date: NaiveDate) -> String {
    format!("{} {}", date.format("%A"), date.format("%Y-%m-%d"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Task;
    use chrono::Duration;
    use tempfile::tempdir;

    fn archived(text: &str, days_ago: i64) -> Task {
        let mut task = Task::new(text, "engineering", None, None);
        task.status = Status::Done;
        task.completed_at = Some((Local::now() - Duration::days(days_ago)).fixed_offset());
        task
    }

    #[test]
    fn uses_yesterday_when_present() {
        let today = Local::now().date_naive();
        let archive = vec![archived("did a thing", 1)];
        let (label, completed) = completions_for(&archive, today);
        assert_eq!(label, "Completed yesterday");
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn falls_back_to_most_recent_day_with_completions() {
        let today = Local::now().date_naive();
        let archive = vec![archived("three days back", 3)];
        let (label, completed) = completions_for(&archive, today);
        assert!(label.contains("most recent"), "label was: {label}");
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].text, "three days back");
    }

    #[test]
    fn no_completions_at_all_yields_empty_yesterday() {
        let today = Local::now().date_naive();
        let (label, completed) = completions_for(&[], today);
        assert_eq!(label, "Completed yesterday");
        assert!(completed.is_empty());
    }

    #[test]
    fn build_report_groups_open_and_blocked() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());

        let mut open_task = Task::new("open one", "intake", None, None);
        open_task.status = Status::Open;
        let mut blocked_task = Task::new("blocked one", "support", None, None);
        blocked_task.status = Status::Blocked;
        store.save_tasks(&[open_task, blocked_task]).unwrap();
        store
            .append_archive(&archived("done yesterday", 1))
            .unwrap();

        let report = build_report(&store).unwrap();
        assert_eq!(report.completed_label, "Completed yesterday");
        assert_eq!(report.completed.len(), 1);
        assert_eq!(report.open.len(), 1);
        assert_eq!(report.blocked.len(), 1);
    }
}
