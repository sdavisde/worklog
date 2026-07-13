//! `wl standup` — print a plain-text standup report to stdout.

use crate::model::Task;
use crate::standup::{StandupReport, build_report};
use crate::store::Store;
use color_eyre::eyre::Result;

pub fn run() -> Result<()> {
    let store = Store::resolve()?;
    let report = build_report(&store)?;
    print_report(&report);
    Ok(())
}

fn print_report(report: &StandupReport) {
    println!("{}", report.completed_label);
    print_tasks(&report.completed, "-");
    println!();

    // Today: what's already finished today (done marker) followed by what's
    // still open.
    println!("Today");
    if report.completed_today.is_empty() && report.open.is_empty() {
        println!("  (none)");
    } else {
        print_present_tasks(&report.completed_today, "x");
        print_present_tasks(&report.open, "-");
    }
    println!();

    println!("Blocked");
    print_tasks(&report.blocked, "-");
}

/// Print a group, emitting `(none)` when empty. `bullet` marks each row (`-`
/// for pending, `x` for done).
fn print_tasks(tasks: &[Task], bullet: &str) {
    if tasks.is_empty() {
        println!("  (none)");
        return;
    }
    print_present_tasks(tasks, bullet);
}

/// Print the rows of a group without the empty-group `(none)` placeholder, so
/// callers can merge several groups under one heading.
fn print_present_tasks(tasks: &[Task], bullet: &str) {
    for task in tasks {
        let project = task
            .project
            .as_deref()
            .map(|p| format!(" #{p}"))
            .unwrap_or_default();
        let due = task.due.map(|d| format!(" (due {d})")).unwrap_or_default();
        println!("  {bullet} [{}] {}{project}{due}", task.category, task.text);
    }
}
