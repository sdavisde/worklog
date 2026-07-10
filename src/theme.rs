//! TUI theming: a small set of semantic style "slots" (`Theme`), a compact
//! lazygit-flavored style-string grammar for describing them in YAML, and
//! lookup across the default theme, user theme files under
//! `<worklog-dir>/themes/`, and a handful of embedded presets.
//!
//! Out of scope (see the design doc): light/dark auto-detection, an in-app
//! theme picker, per-status colors, live reload.

use crate::store::Store;
use color_eyre::eyre::{Result, WrapErr, eyre};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;
use std::fs;
use std::str::FromStr;

/// Embedded preset themes, registered by name. Each entry's YAML is pulled in
/// at compile time from `themes/<name>.yaml` at the repo root; they double as
/// worked examples for anyone writing their own theme file.
const BUILTINS: &[(&str, &str)] = &[
    (
        "catppuccin-mocha",
        include_str!("../themes/catppuccin-mocha.yaml"),
    ),
    ("gruvbox-dark", include_str!("../themes/gruvbox-dark.yaml")),
    ("dracula", include_str!("../themes/dracula.yaml")),
    ("vesper", include_str!("../themes/vesper.yaml")),
];

/// Names of every theme `theme::load` will accept, for error messages.
fn available_theme_names() -> Vec<&'static str> {
    std::iter::once("default")
        .chain(BUILTINS.iter().map(|(name, _)| *name))
        .collect()
}

/// Every theme name selectable in the in-app picker, de-duplicated:
/// `"default"`, then the embedded presets in their registered order, then any
/// additional `*.yaml` file stems under `store.themes_dir()` that aren't
/// already listed (sorted). A missing themes dir is simply skipped.
pub fn available(store: &Store) -> Vec<String> {
    let mut names: Vec<String> = std::iter::once("default".to_string())
        .chain(BUILTINS.iter().map(|(name, _)| name.to_string()))
        .collect();

    let mut extra: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(store.themes_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && !names.iter().any(|n| n == stem)
                && !extra.iter().any(|n| n == stem)
            {
                extra.push(stem.to_string());
            }
        }
    }
    extra.sort();
    names.extend(extra);
    names
}

/// The full set of semantic style slots the TUI draws from. `Theme::default`
/// is built purely from named ANSI colors so, out of the box, `wl` inherits
/// the terminal's own palette and looks identical to the pre-theming app.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Active tab, focused pane border, help group titles, input/modal borders.
    pub accent: Style,
    /// List highlight AND picker selected rows (unified).
    pub selection: Style,
    /// Inactive tabs, completed/archived rows, ghost completion, truncation
    /// ellipsis, note item counts.
    pub muted: Style,
    /// Footer hint line.
    pub hint: Style,
    /// Footer error message, confirm-delete border.
    pub error: Style,
    /// `@category` spans.
    pub category: Style,
    /// `#project` spans.
    pub project: Style,
    /// Due dates.
    pub due: Style,
    /// Overdue due dates.
    pub due_overdue: Style,
    /// INSERT badge in the edit modal.
    pub insert_mode: Style,
    /// NORMAL badge in the edit modal.
    pub normal_mode: Style,
    /// Inline markdown code.
    pub md_code: Style,
    /// Inline markdown links.
    pub md_link: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            selection: Style::default().add_modifier(Modifier::REVERSED),
            muted: Style::default().add_modifier(Modifier::DIM),
            hint: Style::default().fg(Color::DarkGray),
            error: Style::default().fg(Color::Red),
            category: Style::default().fg(Color::Green),
            project: Style::default().fg(Color::Magenta),
            due: Style::default().fg(Color::Yellow),
            due_overdue: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            insert_mode: Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            normal_mode: Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            md_code: Style::default().fg(Color::Yellow),
            md_link: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
        }
    }
}

impl Theme {
    /// Parse `file`'s present slots over [`Theme::default`]; missing slots
    /// (the common case for a sparse, hand-written theme) simply keep the
    /// default's value.
    pub fn resolve(file: ThemeFile) -> Result<Theme> {
        let mut theme = Theme::default();
        macro_rules! apply {
            ($field:ident) => {
                if let Some(spec) = &file.$field {
                    theme.$field = parse_style(spec, stringify!($field))?;
                }
            };
        }
        apply!(accent);
        apply!(selection);
        apply!(muted);
        apply!(hint);
        apply!(error);
        apply!(category);
        apply!(project);
        apply!(due);
        apply!(due_overdue);
        apply!(insert_mode);
        apply!(normal_mode);
        apply!(md_code);
        apply!(md_link);
        Ok(theme)
    }
}

/// On-disk representation of a theme file: every slot is an optional compact
/// style string (see [`parse_style`]). `deny_unknown_fields` so a typo'd slot
/// name fails loudly instead of silently doing nothing.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub selection: Option<String>,
    #[serde(default)]
    pub muted: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub due_overdue: Option<String>,
    #[serde(default)]
    pub insert_mode: Option<String>,
    #[serde(default)]
    pub normal_mode: Option<String>,
    #[serde(default)]
    pub md_code: Option<String>,
    #[serde(default)]
    pub md_link: Option<String>,
}

/// Parse one slot's style string: `<fg-color>? ("on" <bg-color>)? <modifier>*`.
///
/// Colors are anything `ratatui::style::Color::from_str` accepts (named,
/// `#rrggbb` hex, or indexed). Modifiers are `bold`, `dim`, `italic`,
/// `underlined`, `reversed`, `crossedout`. Tokens are whitespace-split; `on`
/// switches the color target from foreground to background; anything else
/// must parse as a modifier or a `Color`, or loading fails with a message
/// naming both the offending slot and token.
fn parse_style(spec: &str, slot: &str) -> Result<Style> {
    let mut style = Style::default();
    let mut target_bg = false;
    for token in spec.split_whitespace() {
        if token.eq_ignore_ascii_case("on") {
            target_bg = true;
            continue;
        }
        if let Some(modifier) = modifier_from_token(token) {
            style = style.add_modifier(modifier);
            continue;
        }
        match Color::from_str(token) {
            Ok(color) => {
                if target_bg {
                    style = style.bg(color);
                } else {
                    style = style.fg(color);
                }
            }
            Err(_) => {
                return Err(eyre!(
                    "theme slot `{slot}`: invalid token `{token}` in style `{spec}` \
                     (expected a color or a modifier: bold, dim, italic, underlined, \
                     reversed, crossedout)"
                ));
            }
        }
    }
    Ok(style)
}

fn modifier_from_token(token: &str) -> Option<Modifier> {
    match token.to_ascii_lowercase().as_str() {
        "bold" => Some(Modifier::BOLD),
        "dim" => Some(Modifier::DIM),
        "italic" => Some(Modifier::ITALIC),
        "underlined" => Some(Modifier::UNDERLINED),
        "reversed" => Some(Modifier::REVERSED),
        "crossedout" => Some(Modifier::CROSSED_OUT),
        _ => None,
    }
}

/// Resolve a theme by name: `"default"` (or a missing config value, which is
/// normalized to `"default"` by `Config`) never touches disk. Otherwise a
/// user file at `<store.themes_dir()>/<name>.yaml` wins over an embedded
/// preset of the same name; if neither exists, the load fails with a message
/// listing the available theme names.
pub fn load(store: &Store, name: &str) -> Result<Theme> {
    if name == "default" {
        return Ok(Theme::default());
    }

    let user_path = store.themes_dir().join(format!("{name}.yaml"));
    if user_path.exists() {
        let content = fs::read_to_string(&user_path)
            .wrap_err_with(|| format!("reading {}", user_path.display()))?;
        let file: ThemeFile = serde_norway::from_str(&content)
            .wrap_err_with(|| format!("parsing {}", user_path.display()))?;
        return Theme::resolve(file);
    }

    if let Some((_, yaml)) = BUILTINS.iter().find(|(builtin, _)| *builtin == name) {
        let file: ThemeFile = serde_norway::from_str(yaml)
            .wrap_err_with(|| format!("parsing embedded preset theme `{name}`"))?;
        return Theme::resolve(file);
    }

    Err(eyre!(
        "unknown theme `{name}`; available themes: {}",
        available_theme_names().join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ---- style-string grammar ----------------------------------------------

    #[test]
    fn parses_fg_only() {
        let style = parse_style("cyan", "accent").unwrap();
        assert_eq!(style.fg, Some(Color::Cyan));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn parses_fg_on_bg() {
        let style = parse_style("black on yellow", "normal_mode").unwrap();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::Yellow));
    }

    #[test]
    fn parses_modifiers() {
        let style = parse_style("black on yellow bold", "normal_mode").unwrap();
        assert!(style.add_modifier.contains(Modifier::BOLD));

        let style = parse_style("reversed", "selection").unwrap();
        assert!(style.add_modifier.contains(Modifier::REVERSED));

        let style = parse_style("underlined", "md_link").unwrap();
        assert!(style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn parses_hex_color() {
        let style = parse_style("#f38ba8", "error").unwrap();
        assert_eq!(style.fg, Some(Color::Rgb(0xf3, 0x8b, 0xa8)));
    }

    #[test]
    fn parses_indexed_color() {
        let style = parse_style("13", "accent").unwrap();
        assert_eq!(style.fg, Some(Color::Indexed(13)));
    }

    #[test]
    fn unknown_token_error_names_the_slot() {
        let err = parse_style("notacolor", "accent").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("accent"), "error names the slot: {msg}");
        assert!(msg.contains("notacolor"), "error names the token: {msg}");
    }

    // ---- sparse resolve -----------------------------------------------------

    #[test]
    fn sparse_resolve_falls_back_per_slot() {
        let file = ThemeFile {
            accent: Some("#fab387".to_string()),
            ..Default::default()
        };
        let theme = Theme::resolve(file).unwrap();
        let default = Theme::default();
        assert_ne!(theme.accent, default.accent);
        // every other slot untouched
        assert_eq!(theme.selection, default.selection);
        assert_eq!(theme.muted, default.muted);
        assert_eq!(theme.hint, default.hint);
        assert_eq!(theme.error, default.error);
        assert_eq!(theme.category, default.category);
        assert_eq!(theme.project, default.project);
        assert_eq!(theme.due, default.due);
        assert_eq!(theme.due_overdue, default.due_overdue);
        assert_eq!(theme.insert_mode, default.insert_mode);
        assert_eq!(theme.normal_mode, default.normal_mode);
        assert_eq!(theme.md_code, default.md_code);
        assert_eq!(theme.md_link, default.md_link);
    }

    // ---- theme::load ----------------------------------------------------------

    #[test]
    fn load_default_never_touches_disk() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let theme = load(&store, "default").unwrap();
        assert_eq!(theme, Theme::default());
        assert!(!store.themes_dir().exists());
    }

    #[test]
    fn load_unknown_theme_name_errors_listing_available() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let err = load(&store, "nope").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nope"));
        assert!(msg.contains("default"));
        assert!(msg.contains("catppuccin-mocha"));
        assert!(msg.contains("gruvbox-dark"));
        assert!(msg.contains("dracula"));
        assert!(msg.contains("vesper"));
    }

    #[test]
    fn load_embedded_preset_by_name() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let theme = load(&store, "dracula").unwrap();
        assert_ne!(theme, Theme::default());
    }

    #[test]
    fn user_file_shadows_builtin_of_same_name() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        fs::create_dir_all(store.themes_dir()).unwrap();
        fs::write(
            store.themes_dir().join("dracula.yaml"),
            "accent: \"#123456\"\n",
        )
        .unwrap();

        let theme = load(&store, "dracula").unwrap();
        assert_eq!(theme.accent.fg, Some(Color::Rgb(0x12, 0x34, 0x56)));
    }

    #[test]
    fn user_theme_file_loads_from_themes_dir() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        fs::create_dir_all(store.themes_dir()).unwrap();
        fs::write(
            store.themes_dir().join("mytheme.yaml"),
            "accent: \"#fab387\"\n",
        )
        .unwrap();

        let theme = load(&store, "mytheme").unwrap();
        assert_eq!(theme.accent.fg, Some(Color::Rgb(0xfa, 0xb3, 0x87)));
        assert_eq!(theme.selection, Theme::default().selection);
    }

    #[test]
    fn unknown_field_in_theme_file_is_rejected() {
        let file: std::result::Result<ThemeFile, _> =
            serde_norway::from_str("nonexistent_slot: cyan\n");
        assert!(file.is_err());
    }

    // ---- available -----------------------------------------------------------

    #[test]
    fn available_lists_default_and_builtins_when_no_themes_dir() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        assert!(!store.themes_dir().exists());
        assert_eq!(
            available(&store),
            vec![
                "default",
                "catppuccin-mocha",
                "gruvbox-dark",
                "dracula",
                "vesper"
            ]
        );
    }

    #[test]
    fn available_appends_user_theme_files_sorted() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        fs::create_dir_all(store.themes_dir()).unwrap();
        fs::write(store.themes_dir().join("mytheme.yaml"), "accent: cyan\n").unwrap();

        let names = available(&store);
        assert_eq!(
            names,
            vec![
                "default",
                "catppuccin-mocha",
                "gruvbox-dark",
                "dracula",
                "vesper",
                "mytheme"
            ]
        );
    }

    #[test]
    fn available_does_not_duplicate_a_builtin_shadowed_by_a_user_file() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        fs::create_dir_all(store.themes_dir()).unwrap();
        fs::write(store.themes_dir().join("dracula.yaml"), "accent: cyan\n").unwrap();

        let names = available(&store);
        assert_eq!(names.iter().filter(|n| *n == "dracula").count(), 1);
        assert_eq!(
            names,
            vec![
                "default",
                "catppuccin-mocha",
                "gruvbox-dark",
                "dracula",
                "vesper"
            ]
        );
    }

    // ---- embedded presets parse -----------------------------------------------

    #[test]
    fn all_embedded_presets_parse() {
        for (name, yaml) in BUILTINS {
            let file: ThemeFile = serde_norway::from_str(yaml)
                .unwrap_or_else(|e| panic!("preset `{name}` failed to parse: {e}"));
            let theme = Theme::resolve(file)
                .unwrap_or_else(|e| panic!("preset `{name}` failed to resolve: {e}"));
            // sanity: a real preset should differ from the bare default.
            assert_ne!(
                theme,
                Theme::default(),
                "preset `{name}` is identical to default"
            );
        }
    }
}
