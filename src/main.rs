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
        }) => run_task(text, category, project, due),
        Some(Command::Standup) => run_standup(),
        Some(Command::ImportLegacy) => run_import_legacy(),
        None => run_tui(),
    }

    Ok(())
}

fn run_task(text: String, category: Option<String>, project: Option<String>, due: Option<String>) {
    println!("stub: wl task {text:?} (category={category:?}, project={project:?}, due={due:?})");
}

fn run_standup() {
    println!("stub: wl standup not yet implemented");
}

fn run_import_legacy() {
    println!("stub: wl import-legacy not yet implemented");
}

fn run_tui() {
    println!("TUI not yet implemented");
}
