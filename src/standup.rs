//! Shared standup report logic, consumed by `wl standup` (and, later, the
//! TUI's Standup view).

use crate::model::{Status, Task};
use crate::store::Store;
use chrono::{Local, NaiveDate};
use color_eyre::eyre::Result;

/// A built standup report, in three sections:
///
/// - **Completed (yesterday / most-recent):** work finished since the last
///   day with completions, *excluding today*. `completed_label` describes
///   which day it is, since "yesterday" falls back to the most recent
///   earlier day that has any completions.
/// - **Today:** what's finished *today* (`completed_today`, pulled from the
///   archive by completion date) plus what's still open (`open`).
/// - **Blocked:** blocked tasks (`blocked`).
///
/// `completed` and `completed_today` are disjoint by construction: the
/// "yesterday / most-recent" window only ever looks at days strictly before
/// today, so a task finished today appears in `completed_today` alone.
pub struct StandupReport {
    pub completed_label: String,
    pub completed: Vec<Task>,
    pub completed_today: Vec<Task>,
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
    let completed_today = completions_on(&archive, today);

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
        completed_today,
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

    fn archived_on(text: &str, date: NaiveDate) -> Task {
        let mut task = Task::new(text, "engineering", None, None);
        task.status = Status::Done;
        task.completed_at = Some(date.and_hms_opt(9, 0, 0).unwrap().and_utc().fixed_offset());
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
    fn todays_completions_excluded_from_yesterday_section() {
        let today = Local::now().date_naive();
        let archive = vec![archived("did today", 0), archived("did yesterday", 1)];

        // The "yesterday / most-recent" section only sees strictly-earlier days.
        let (label, completed) = completions_for(&archive, today);
        assert_eq!(label, "Completed yesterday");
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].text, "did yesterday");

        // Today's completion is surfaced separately, so it can't duplicate.
        let today_done = completions_on(&archive, today);
        assert_eq!(today_done.len(), 1);
        assert_eq!(today_done[0].text, "did today");
    }

    #[test]
    fn monday_shows_friday_completions_and_todays_separately() {
        let monday = NaiveDate::from_ymd_opt(2026, 7, 13).unwrap();
        let friday = NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        let archive = vec![
            archived_on("friday work", friday),
            archived_on("monday work", monday),
        ];

        // Sunday (yesterday) has nothing, so the section falls back to Friday
        // and never reaches forward into today's completions.
        let (label, completed) = completions_for(&archive, monday);
        assert!(label.contains("Friday"), "label was: {label}");
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].text, "friday work");

        let today_done = completions_on(&archive, monday);
        assert_eq!(today_done.len(), 1);
        assert_eq!(today_done[0].text, "monday work");
    }

    #[test]
    fn build_report_splits_today_from_earlier_completions() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());

        let mut open_task = Task::new("open one", "intake", None, None);
        open_task.status = Status::Open;
        store.save_tasks(&[open_task]).unwrap();
        store.append_archive(&archived("done today", 0)).unwrap();
        store
            .append_archive(&archived("done yesterday", 1))
            .unwrap();

        let report = build_report(&store).unwrap();
        assert_eq!(report.completed.len(), 1);
        assert_eq!(report.completed[0].text, "done yesterday");
        assert_eq!(report.completed_today.len(), 1);
        assert_eq!(report.completed_today[0].text, "done today");
        assert_eq!(report.open.len(), 1);
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
