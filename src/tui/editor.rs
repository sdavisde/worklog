//! `$EDITOR` escape hatch: resolve the editor command and spawn it on a note
//! file, inheriting stdio (the lazygit suspend/resume pattern).
//!
//! The terminal suspend (`ratatui::restore()`) and resume
//! (`ratatui::init()` + `terminal.clear()`) live in the event loop
//! ([`crate::tui`]) because they need the owned terminal handle. The process
//! spawn is factored out here so it can be unit tested without a TTY.

use crate::config::Config;
use std::path::Path;
use std::process::{Command, ExitStatus};

/// Resolve the editor command using the documented precedence:
/// `$EDITOR` environment variable → config `editor_command` → `vi`.
pub fn resolve_editor(config: &Config) -> String {
    resolve_editor_from(std::env::var("EDITOR").ok(), config)
}

/// Pure core of [`resolve_editor`], taking the `$EDITOR` value explicitly so
/// the precedence chain can be unit tested without mutating process
/// environment (which is `unsafe` under Rust 2024 and forbidden here).
fn resolve_editor_from(env: Option<String>, config: &Config) -> String {
    if let Some(editor) = env {
        if !editor.trim().is_empty() {
            return editor;
        }
    }
    if !config.editor_command.trim().is_empty() {
        return config.editor_command.clone();
    }
    "vi".to_string()
}

/// Spawn `editor <path>` inheriting the parent's stdio and block until it
/// exits. The caller is responsible for restoring/re-initializing the
/// terminal around this call.
pub fn run_editor(editor: &str, path: &Path) -> std::io::Result<ExitStatus> {
    Command::new(editor).arg(path).status()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(editor_command: &str) -> Config {
        Config {
            categories: vec![],
            editor_command: editor_command.to_string(),
        }
    }

    #[test]
    fn resolve_prefers_env_over_config() {
        assert_eq!(
            resolve_editor_from(Some("envtor".to_string()), &cfg("configtor")),
            "envtor"
        );
    }

    #[test]
    fn resolve_falls_back_to_config_when_env_unset_or_blank() {
        assert_eq!(resolve_editor_from(None, &cfg("configtor")), "configtor");
        assert_eq!(
            resolve_editor_from(Some("   ".to_string()), &cfg("configtor")),
            "configtor"
        );
    }

    #[test]
    fn resolve_falls_back_to_vi_when_nothing_set() {
        assert_eq!(resolve_editor_from(None, &cfg("")), "vi");
    }
}
