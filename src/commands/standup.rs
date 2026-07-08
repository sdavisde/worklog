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
    print_tasks(&report.completed);
    println!();

    println!("Open");
    print_tasks(&report.open);
    println!();

    println!("Blocked");
    print_tasks(&report.blocked);
}

fn print_tasks(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("  (none)");
        return;
    }
    for task in tasks {
        let project = task
            .project
            .as_deref()
            .map(|p| format!(" #{p}"))
            .unwrap_or_default();
        let due = task.due.map(|d| format!(" (due {d})")).unwrap_or_default();
        println!("  - [{}] {}{project}{due}", task.category, task.text);
    }
}
