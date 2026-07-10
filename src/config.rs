//! `~/.worklog/config.yaml` load-or-create.

use color_eyre::eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Where the TUI renders the notes detail pane relative to the active tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotesPane {
    /// Sidebar in wide terminals, bottom pane in narrow ones (default).
    #[default]
    Auto,
    /// Always alongside the active tab, on the right.
    Right,
    /// Always below the active tab.
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_categories")]
    pub categories: Vec<String>,
    /// Fallback editor for the TUI's `$EDITOR` escape hatch, used when the
    /// `$EDITOR` environment variable is unset.
    #[serde(default = "default_editor_command")]
    pub editor_command: String,
    /// Position of the notes detail pane: `auto`, `right`, or `bottom`.
    #[serde(default)]
    pub notes_pane: NotesPane,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            categories: default_categories(),
            editor_command: default_editor_command(),
            notes_pane: NotesPane::default(),
        }
    }
}

fn default_categories() -> Vec<String> {
    vec![
        "priority".to_string(),
        "support".to_string(),
        "project-management".to_string(),
        "engineering".to_string(),
        "intake".to_string(),
    ]
}

fn default_editor_command() -> String {
    "nvim".to_string()
}

/// The literal file written on first run. Kept as a plain commented string
/// (rather than generated from `Config`) so the on-disk defaults carry
/// human-readable explanations.
const DEFAULT_CONFIG_YAML: &str = r#"# wl configuration file.

# categories: valid task categories. `wl task --category <c>` is validated
# against this list; the default category (when --category is omitted) is
# "intake".
categories:
  - priority
  - support
  - project-management
  - engineering
  - intake

# editor_command: fallback editor used by the TUI's $EDITOR escape hatch
# when the $EDITOR environment variable is not set.
editor_command: nvim

# notes_pane: where the TUI draws the notes detail pane. One of:
#   auto   - sidebar in wide terminals, bottom pane in narrow ones (default)
#   right  - always alongside the active tab, on the right
#   bottom - always below the active tab
notes_pane: auto
"#;

/// Load `config.yaml` from `path`, or write the commented default file and
/// parse it back if it doesn't exist yet.
pub fn load_or_create(path: &Path) -> Result<Config> {
    if path.exists() {
        let content =
            fs::read_to_string(path).wrap_err_with(|| format!("reading {}", path.display()))?;
        let config: Config = serde_norway::from_str(&content)
            .wrap_err_with(|| format!("parsing {}", path.display()))?;
        return Ok(config);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).wrap_err_with(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, DEFAULT_CONFIG_YAML).wrap_err_with(|| format!("writing {}", path.display()))?;

    let config: Config = serde_norway::from_str(DEFAULT_CONFIG_YAML)
        .wrap_err("parsing freshly written default config.yaml")?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_default_config_on_first_run() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        assert!(!path.exists());

        let config = load_or_create(&path).unwrap();
        assert!(path.exists());
        assert_eq!(config.categories, default_categories());
        assert_eq!(config.editor_command, "nvim");

        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("# wl configuration file"));
        assert!(on_disk.contains("categories:"));
    }

    #[test]
    fn loads_existing_config_without_overwriting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            "categories:\n  - one\n  - two\neditor_command: vim\n",
        )
        .unwrap();

        let config = load_or_create(&path).unwrap();
        assert_eq!(
            config.categories,
            vec!["one".to_string(), "two".to_string()]
        );
        assert_eq!(config.editor_command, "vim");
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "categories:\n  - solo\n").unwrap();

        let config = load_or_create(&path).unwrap();
        assert_eq!(config.categories, vec!["solo".to_string()]);
        assert_eq!(config.editor_command, "nvim");
    }

    #[test]
    fn missing_notes_pane_defaults_to_auto() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "categories:\n  - solo\n").unwrap();

        let config = load_or_create(&path).unwrap();
        assert_eq!(config.notes_pane, NotesPane::Auto);
    }

    #[test]
    fn notes_pane_values_parse() {
        for (value, expected) in [
            ("auto", NotesPane::Auto),
            ("right", NotesPane::Right),
            ("bottom", NotesPane::Bottom),
        ] {
            let dir = tempdir().unwrap();
            let path = dir.path().join("config.yaml");
            fs::write(&path, format!("notes_pane: {value}\n")).unwrap();

            let config = load_or_create(&path).unwrap();
            assert_eq!(config.notes_pane, expected);
        }
    }
}
