//! On-disk storage: `tasks.jsonl` (active/blocked), `archive.jsonl`
//! (completed, append-only), and directory resolution under `~/.worklog`
//! (overridable via `WORKLOG_DIR`).

use crate::model::{Status, Task};
use chrono::Local;
use color_eyre::eyre::{Result, WrapErr, eyre};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Handle to a resolved `~/.worklog`-shaped directory.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// Resolve the data directory from `WORKLOG_DIR` (if set and non-empty)
    /// or `$HOME/.worklog`, creating it if it doesn't exist yet.
    pub fn resolve() -> Result<Self> {
        let dir = resolve_dir()?;
        fs::create_dir_all(&dir)
            .wrap_err_with(|| format!("creating worklog dir {}", dir.display()))?;
        Ok(Self { dir })
    }

    /// Build a `Store` pointed at an explicit directory. Used by tests today;
    /// will also back a future `--dir` override / TUI wiring.
    #[allow(dead_code)]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    #[allow(dead_code)]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn tasks_path(&self) -> PathBuf {
        self.dir.join("tasks.jsonl")
    }

    pub fn archive_path(&self) -> PathBuf {
        self.dir.join("archive.jsonl")
    }

    /// Used starting with the Notes module (Unit 4 TUI / notes wiring).
    #[allow(dead_code)]
    pub fn notes_dir(&self) -> PathBuf {
        self.dir.join("notes")
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.yaml")
    }

    /// Load all active/blocked tasks from `tasks.jsonl`.
    pub fn load_tasks(&self) -> Result<Vec<Task>> {
        load_jsonl(&self.tasks_path())
    }

    /// Atomically rewrite `tasks.jsonl` (temp file + rename).
    pub fn save_tasks(&self, tasks: &[Task]) -> Result<()> {
        save_jsonl_atomic(&self.dir, &self.tasks_path(), tasks)
    }

    /// Load all archived (completed) tasks from `archive.jsonl`.
    pub fn load_archive(&self) -> Result<Vec<Task>> {
        load_jsonl(&self.archive_path())
    }

    /// Append a single task record to `archive.jsonl`.
    pub fn append_archive(&self, task: &Task) -> Result<()> {
        append_jsonl(&self.archive_path(), task)
    }

    /// Append a new task to `tasks.jsonl`.
    pub fn add_task(&self, task: Task) -> Result<()> {
        let mut tasks = self.load_tasks()?;
        tasks.push(task);
        self.save_tasks(&tasks)
    }

    /// Mark the task with the given id as done: set `status`/`completed_at`,
    /// remove it from `tasks.jsonl`, and append it to `archive.jsonl`.
    ///
    /// Not yet wired into a CLI command in this unit (the TUI's `space`/`x`
    /// complete action lands in a later unit), but implemented now per the
    /// storage-layer spec.
    #[allow(dead_code)]
    pub fn complete_task(&self, id: &str) -> Result<Task> {
        let mut tasks = self.load_tasks()?;
        let idx = tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| eyre!("no task with id {id}"))?;
        let mut task = tasks.remove(idx);
        task.status = Status::Done;
        task.completed_at = Some(Local::now().fixed_offset());

        self.save_tasks(&tasks)?;
        self.append_archive(&task)?;
        Ok(task)
    }
}

fn resolve_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("WORKLOG_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let home = std::env::var("HOME").wrap_err("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(".worklog"))
}

/// Read a JSONL file into a `Vec<Task>`, skipping (and warning to stderr
/// about) blank or corrupt lines instead of failing the whole read.
fn load_jsonl(path: &Path) -> Result<Vec<Task>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content =
        fs::read_to_string(path).wrap_err_with(|| format!("reading {}", path.display()))?;

    let mut tasks = Vec::new();
    for (n, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Task>(trimmed) {
            Ok(task) => tasks.push(task),
            Err(err) => {
                eprintln!(
                    "warning: skipping corrupt line {} in {}: {err}",
                    n + 1,
                    path.display()
                );
            }
        }
    }
    Ok(tasks)
}

/// Rewrite `path` atomically: write to a sibling temp file in the same
/// directory, then rename over the destination.
fn save_jsonl_atomic(dir: &Path, path: &Path, tasks: &[Task]) -> Result<()> {
    fs::create_dir_all(dir).wrap_err_with(|| format!("creating {}", dir.display()))?;

    let mut buf = String::new();
    for task in tasks {
        buf.push_str(&serde_json::to_string(task).wrap_err("serializing task")?);
        buf.push('\n');
    }

    let file_name = path
        .file_name()
        .ok_or_else(|| eyre!("path {} has no file name", path.display()))?
        .to_string_lossy();
    let tmp_path = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));

    fs::write(&tmp_path, &buf)
        .wrap_err_with(|| format!("writing temp file {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .wrap_err_with(|| format!("renaming {} into {}", tmp_path.display(), path.display()))?;
    Ok(())
}

/// Append a single task as a JSON line to `path`, creating the file (and its
/// parent directory) if needed.
fn append_jsonl(path: &Path, task: &Task) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).wrap_err_with(|| format!("creating {}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .wrap_err_with(|| format!("opening {}", path.display()))?;

    let line = serde_json::to_string(task).wrap_err("serializing task")?;
    writeln!(file, "{line}").wrap_err_with(|| format!("appending to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Task;
    use tempfile::tempdir;

    #[test]
    fn save_and_load_tasks_round_trip() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());

        let t1 = Task::new("first task", "engineering", None, None);
        let t2 = Task::new("second task", "support", Some("proj".to_string()), None);
        store.save_tasks(&[t1.clone(), t2.clone()]).unwrap();

        let loaded = store.load_tasks().unwrap();
        assert_eq!(loaded, vec![t1, t2]);
    }

    #[test]
    fn load_tasks_missing_file_returns_empty() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        assert_eq!(store.load_tasks().unwrap(), Vec::new());
    }

    #[test]
    fn load_tasks_skips_corrupt_and_blank_lines() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let good = Task::new("good task", "intake", None, None);
        let good_json = serde_json::to_string(&good).unwrap();

        let content = format!("{good_json}\n\nnot json at all\n   \n");
        fs::write(store.tasks_path(), content).unwrap();

        let loaded = store.load_tasks().unwrap();
        assert_eq!(loaded, vec![good]);
    }

    #[test]
    fn append_archive_adds_lines() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());

        let t1 = Task::new("archived one", "engineering", None, None);
        let t2 = Task::new("archived two", "engineering", None, None);
        store.append_archive(&t1).unwrap();
        store.append_archive(&t2).unwrap();

        let loaded = store.load_archive().unwrap();
        assert_eq!(loaded, vec![t1, t2]);
    }

    #[test]
    fn complete_task_moves_record_to_archive() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());

        let task = Task::new("finish the thing", "engineering", None, None);
        let id = task.id.clone();
        store.add_task(task).unwrap();

        let completed = store.complete_task(&id).unwrap();
        assert_eq!(completed.status, Status::Done);
        assert!(completed.completed_at.is_some());

        assert_eq!(store.load_tasks().unwrap(), Vec::new());
        let archived = store.load_archive().unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, id);
        assert_eq!(archived[0].status, Status::Done);
    }

    #[test]
    fn complete_task_missing_id_errors() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        assert!(store.complete_task("t_nope00").is_err());
    }

    #[test]
    fn save_tasks_is_atomic_no_leftover_temp_file() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store
            .save_tasks(&[Task::new("a", "intake", None, None)])
            .unwrap();

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert!(entries.iter().all(|e| !e.ends_with(".tmp")));
        assert!(entries.contains(&"tasks.jsonl".to_string()));
    }
}
