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
    /// TUI color theme: `default` (built purely from named ANSI colors, so
    /// it inherits the terminal palette), a built-in preset name, or the
    /// name of a file under `<worklog-dir>/themes/<name>.yaml`.
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            categories: default_categories(),
            editor_command: default_editor_command(),
            notes_pane: NotesPane::default(),
            theme: default_theme(),
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

fn default_theme() -> String {
    "default".to_string()
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

# theme: the TUI's color theme. One of:
#   default            - built purely from named ANSI colors, so the TUI
#                         inherits your terminal's own palette (default)
#   catppuccin-mocha    \
#   gruvbox-dark         \
#   dracula               > built-in presets, no extra files needed
#   vesper               /
#   <anything else>    - loaded from ~/.worklog/themes/<name>.yaml
#
# A theme file sets any of 13 style slots (accent, selection, muted, hint,
# error, category, project, due, due_overdue, insert_mode, normal_mode,
# md_code, md_link); slots it omits keep the default theme's value, so a
# two-line file that only overrides `accent` is perfectly valid. Each slot is
# a compact style string: <fg-color>? (on <bg-color>)? <modifier>*, e.g.
# cyan bold, black on yellow bold, a hex color like #89b4fa, or reversed.
# Colors may be named (cyan, darkgray, ...), hex (#rrggbb), or an indexed
# terminal color (0-255); modifiers are bold, dim, italic, underlined,
# reversed, crossedout. See themes/*.yaml in the wl repo for full examples.
theme: default
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

/// Persist a new `theme:` value to `config.yaml` while preserving every other
/// line and comment verbatim. Replaces the first uncommented `theme:` line if
/// one exists, otherwise appends a fresh `theme: {name}` line (ensuring exactly
/// one trailing newline).
pub fn set_theme(path: &Path, name: &str) -> Result<()> {
    let content =
        fs::read_to_string(path).wrap_err_with(|| format!("reading {}", path.display()))?;

    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let replaced = lines.iter_mut().find(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("theme:") && !trimmed.starts_with('#')
    });

    match replaced {
        Some(line) => *line = format!("theme: {name}"),
        None => lines.push(format!("theme: {name}")),
    }

    let mut out = lines.join("\n");
    out.push('\n');
    fs::write(path, out).wrap_err_with(|| format!("writing {}", path.display()))?;
    Ok(())
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
    fn missing_theme_defaults_to_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "categories:\n  - solo\n").unwrap();

        let config = load_or_create(&path).unwrap();
        assert_eq!(config.theme, "default");
    }

    #[test]
    fn set_theme_replaces_existing_line_preserving_comments() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            "# leading comment\ncategories:\n  - solo\n# theme choice below\ntheme: default\n# trailing comment\n",
        )
        .unwrap();

        set_theme(&path, "dracula").unwrap();

        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("# leading comment"));
        assert!(on_disk.contains("# theme choice below"));
        assert!(on_disk.contains("# trailing comment"));
        assert!(on_disk.contains("theme: dracula"));
        assert!(!on_disk.contains("theme: default"));

        let config = load_or_create(&path).unwrap();
        assert_eq!(config.theme, "dracula");
    }

    #[test]
    fn set_theme_appends_when_no_theme_line_present() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "categories:\n  - solo\n").unwrap();

        set_theme(&path, "gruvbox-dark").unwrap();

        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.ends_with("theme: gruvbox-dark\n"));
        assert!(!on_disk.ends_with("\n\n"));

        let config = load_or_create(&path).unwrap();
        assert_eq!(config.theme, "gruvbox-dark");
    }

    #[test]
    fn set_theme_ignores_commented_theme_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(&path, "categories:\n  - solo\n# theme: default\n").unwrap();

        set_theme(&path, "catppuccin-mocha").unwrap();

        let on_disk = fs::read_to_string(&path).unwrap();
        // the comment survives untouched...
        assert!(on_disk.contains("# theme: default"));
        // ...and a real theme line was appended, not the comment edited.
        assert!(on_disk.contains("\ntheme: catppuccin-mocha\n"));

        let config = load_or_create(&path).unwrap();
        assert_eq!(config.theme, "catppuccin-mocha");
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
