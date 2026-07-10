mod commands;
mod config;
mod markdown;
mod model;
mod notes;
mod standup;
mod store;
mod theme;
mod tui;

use clap::{Parser, Subcommand};

/// wl - a keyboard-first worklog TUI and CLI.
#[derive(Parser, Debug)]
#[command(name = "wl", version, about = "Keyboard-first worklog TUI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Quick-capture a task.
    Task {
        /// The task text.
        text: String,

        /// Task category (defaults to "intake").
        #[arg(long)]
        category: Option<String>,

        /// Optional project tag.
        #[arg(long)]
        project: Option<String>,

        /// Optional due date (YYYY-MM-DD).
        #[arg(long)]
        due: Option<String>,
    },

    /// Print a standup report (yesterday's completions, open, blocked).
    Standup,

    /// One-shot migration from the legacy daily-notes format.
    ImportLegacy,
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Task {
            text,
            category,
            project,
            due,
        }) => commands::task::run(text, category, project, due)?,
        Some(Command::Standup) => commands::standup::run()?,
        Some(Command::ImportLegacy) => commands::import_legacy::run()?,
        None => tui::run()?,
    }

    Ok(())
}
