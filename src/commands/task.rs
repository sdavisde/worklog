//! `wl task "<text>"` quick-capture subcommand.

use crate::config;
use crate::model::Task;
use crate::store::Store;
use chrono::NaiveDate;
use color_eyre::eyre::{Result, eyre};

pub fn run(
    text: String,
    category: Option<String>,
    project: Option<String>,
    due: Option<String>,
) -> Result<()> {
    let store = Store::resolve()?;
    let cfg = config::load_or_create(&store.config_path())?;

    let category = category.unwrap_or_else(|| "intake".to_string());
    if !cfg.categories.iter().any(|c| c == &category) {
        return Err(eyre!(
            "invalid category {category:?}; valid categories: {}",
            cfg.categories.join(", ")
        ));
    }

    let due = due
        .map(|raw| {
            NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
                .map_err(|err| eyre!("invalid --due date {raw:?} (expected YYYY-MM-DD): {err}"))
        })
        .transpose()?;

    let task = Task::new(text, category, project, due);
    let id = task.id.clone();
    store.add_task(task)?;
    println!("captured {id}");
    Ok(())
}
