//! Long-running note documents: `notes/*.md`, YAML frontmatter + a simple
//! sections-and-items markdown body model.
//!
//! `NotesStore::create`/`Body::add_item`/`NotesStore::save` are wired into
//! the legacy importer (Unit 3). `list`/`load`/`Body::items`/`edit_item`/
//! `delete_item` remain unused in production until the Notes TUI view (Unit
//! 4) lands, so those specific items keep a narrow dead-code allow.

use chrono::{Local, NaiveDate};
use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: String,
    #[serde(default)]
    pub project: Option<String>,
    pub updated: NaiveDate,
}

/// A single `## Heading` section of a note body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Section {
    /// Empty for the implicit "preamble" section (content before the first
    /// heading, if any).
    pub heading: String,
    pub lines: Vec<Line>,
}

/// One line of a section's content: either a top-level `- ` list item, or a
/// free-form line (including blank lines, kept for round-trip fidelity).
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Item(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Body {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteDoc {
    pub slug: String,
    pub frontmatter: Frontmatter,
    pub body: Body,
}

impl Body {
    fn find_section_mut(&mut self, heading: &str) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.heading == heading)
    }

    /// Item lines (in order) under `heading`, or empty if the section
    /// doesn't exist.
    ///
    /// Unused in production until the Notes TUI view (Unit 4) reads items
    /// back out of a doc; kept now for the storage-layer spec and exercised
    /// by unit tests.
    #[allow(dead_code)]
    pub fn items(&self, heading: &str) -> Vec<&str> {
        self.sections
            .iter()
            .find(|s| s.heading == heading)
            .map(|s| {
                s.lines
                    .iter()
                    .filter_map(|l| match l {
                        Line::Item(text) => Some(text.as_str()),
                        Line::Text(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Append an item under `heading`, creating the section if it doesn't
    /// exist yet.
    pub fn add_item(&mut self, heading: &str, text: impl Into<String>) {
        if let Some(section) = self.find_section_mut(heading) {
            section.lines.push(Line::Item(text.into()));
        } else {
            self.sections.push(Section {
                heading: heading.to_string(),
                lines: vec![Line::Item(text.into())],
            });
        }
    }

    /// Replace the text of the `index`-th item (0-based, counting only
    /// items, not free-form lines) under `heading`.
    ///
    /// Unused in production until the Notes TUI view (Unit 4) wires up item
    /// editing.
    #[allow(dead_code)]
    pub fn edit_item(
        &mut self,
        heading: &str,
        index: usize,
        text: impl Into<String>,
    ) -> Result<()> {
        let pos = item_line_index(self, heading, index)?;
        let section = self.find_section_mut(heading).expect("checked above");
        section.lines[pos] = Line::Item(text.into());
        Ok(())
    }

    /// Remove the `index`-th item (0-based, counting only items) under
    /// `heading`.
    ///
    /// Unused in production until the Notes TUI view (Unit 4) wires up item
    /// deletion.
    #[allow(dead_code)]
    pub fn delete_item(&mut self, heading: &str, index: usize) -> Result<()> {
        let pos = item_line_index(self, heading, index)?;
        let section = self.find_section_mut(heading).expect("checked above");
        section.lines.remove(pos);
        Ok(())
    }
}

/// Helper for `edit_item`/`delete_item`, both unused in production for now.
#[allow(dead_code)]
fn item_line_index(body: &Body, heading: &str, index: usize) -> Result<usize> {
    let section = body
        .sections
        .iter()
        .find(|s| s.heading == heading)
        .ok_or_else(|| eyre!("no section {heading:?}"))?;
    section
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l, Line::Item(_)))
        .map(|(i, _)| i)
        .nth(index)
        .ok_or_else(|| eyre!("no item at index {index} in section {heading:?}"))
}

/// Parse a note body (everything after the YAML frontmatter) into sections.
pub fn parse_body(text: &str) -> Body {
    let mut sections: Vec<Section> = Vec::new();
    let mut current = Section::default();
    let mut started = false;

    for raw_line in text.lines() {
        if let Some(heading) = raw_line.strip_prefix("## ") {
            if started || !current.lines.is_empty() {
                sections.push(current);
            }
            current = Section {
                heading: heading.trim().to_string(),
                lines: Vec::new(),
            };
            started = true;
            continue;
        }

        if let Some(item) = raw_line.strip_prefix("- ") {
            current.lines.push(Line::Item(item.to_string()));
        } else {
            current.lines.push(Line::Text(raw_line.to_string()));
        }
    }

    if started || !current.lines.is_empty() {
        sections.push(current);
    }

    Body { sections }
}

/// Serialize a body back to markdown, matching `parse_body`'s conventions.
pub fn serialize_body(body: &Body) -> String {
    let mut out = String::new();
    for (i, section) in body.sections.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if !section.heading.is_empty() {
            out.push_str("## ");
            out.push_str(&section.heading);
            out.push('\n');
        }
        for line in &section.lines {
            match line {
                Line::Item(text) => {
                    out.push_str("- ");
                    out.push_str(text);
                }
                Line::Text(text) => out.push_str(text),
            }
            out.push('\n');
        }
    }
    out
}

/// Slugify a title/heading into a lowercase, dash-separated identifier.
/// Shared with the legacy importer, which uses the same rule to match
/// daily-note `###` subsection names against configured task categories.
pub(crate) fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_end_matches('-').to_string()
}

fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let rest = content.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    let after = &rest[end + "\n---".len()..];
    let after = after.strip_prefix('\n').unwrap_or(after);
    Some((frontmatter, after))
}

fn parse_doc(slug: &str, content: &str) -> Result<NoteDoc> {
    let (frontmatter_str, body_str) =
        split_frontmatter(content).ok_or_else(|| eyre!("missing YAML frontmatter"))?;
    let frontmatter: Frontmatter =
        serde_norway::from_str(frontmatter_str).wrap_err("parsing note frontmatter")?;
    let body = parse_body(body_str);
    Ok(NoteDoc {
        slug: slug.to_string(),
        frontmatter,
        body,
    })
}

fn serialize_doc(doc: &NoteDoc) -> Result<String> {
    let yaml = serde_norway::to_string(&doc.frontmatter).wrap_err("serializing frontmatter")?;
    let body = serialize_body(&doc.body);
    Ok(format!("---\n{yaml}---\n\n{body}"))
}

/// A directory of note documents (`~/.worklog/notes`).
pub struct NotesStore {
    dir: PathBuf,
}

impl NotesStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    fn path_for(&self, slug: &str) -> PathBuf {
        self.dir.join(format!("{slug}.md"))
    }

    /// List `(slug, title)` pairs for every note doc, sorted by slug.
    ///
    /// Unused in production until the Notes TUI view (Unit 4) lists docs.
    #[allow(dead_code)]
    pub fn list(&self) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        if !self.dir.exists() {
            return Ok(out);
        }
        for entry in
            fs::read_dir(&self.dir).wrap_err_with(|| format!("reading {}", self.dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let slug = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            match self.load(&slug) {
                Ok(doc) => out.push((slug, doc.frontmatter.title)),
                Err(err) => eprintln!("warning: skipping note {slug}: {err}"),
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn load(&self, slug: &str) -> Result<NoteDoc> {
        let path = self.path_for(slug);
        let content =
            fs::read_to_string(&path).wrap_err_with(|| format!("reading {}", path.display()))?;
        parse_doc(slug, &content)
    }

    /// Create a new, empty note doc from `title` (frontmatter `updated` set
    /// to today) and save it.
    pub fn create(&self, title: &str, project: Option<String>) -> Result<NoteDoc> {
        let slug = slugify(title);
        let mut doc = NoteDoc {
            slug,
            frontmatter: Frontmatter {
                title: title.to_string(),
                project,
                updated: Local::now().date_naive(),
            },
            body: Body::default(),
        };
        self.save(&mut doc)?;
        Ok(doc)
    }

    /// Write `doc` to disk (atomic temp-file + rename), bumping
    /// `frontmatter.updated` to today.
    pub fn save(&self, doc: &mut NoteDoc) -> Result<()> {
        doc.frontmatter.updated = Local::now().date_naive();

        fs::create_dir_all(&self.dir)
            .wrap_err_with(|| format!("creating {}", self.dir.display()))?;
        let path = self.path_for(&doc.slug);
        let content = serialize_doc(doc)?;

        let tmp_path = self
            .dir
            .join(format!(".{}.md.{}.tmp", doc.slug, std::process::id()));
        fs::write(&tmp_path, content)
            .wrap_err_with(|| format!("writing {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &path)
            .wrap_err_with(|| format!("renaming into {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Long-term goals"), "long-term-goals");
        assert_eq!(slugify("Auth Revamp!!"), "auth-revamp");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
    }

    #[test]
    fn parse_body_round_trips_design_doc_example() {
        let body_text = "## Areas to grow into\n\n\
            - Distributed systems depth\n\
            - Public speaking: volunteer for next brown-bag\n";
        let body = parse_body(body_text);
        assert_eq!(body.sections.len(), 1);
        assert_eq!(body.sections[0].heading, "Areas to grow into");
        assert_eq!(
            body.items("Areas to grow into"),
            vec![
                "Distributed systems depth",
                "Public speaking: volunteer for next brown-bag"
            ]
        );

        let serialized = serialize_body(&body);
        assert_eq!(serialized, body_text);
    }

    #[test]
    fn parse_body_handles_multiple_sections_and_preamble() {
        let text = "intro line\n\n## First\n- one\n- two\n\n## Second\nfree text\n- three\n";
        let body = parse_body(text);
        assert_eq!(body.sections.len(), 3);
        assert_eq!(body.sections[0].heading, "");
        assert_eq!(body.sections[1].heading, "First");
        assert_eq!(body.items("First"), vec!["one", "two"]);
        assert_eq!(body.sections[2].heading, "Second");
        assert_eq!(body.items("Second"), vec!["three"]);
    }

    #[test]
    fn add_edit_delete_item() {
        let mut body = Body::default();
        body.add_item("Goals", "first goal");
        body.add_item("Goals", "second goal");
        assert_eq!(body.items("Goals"), vec!["first goal", "second goal"]);

        body.edit_item("Goals", 1, "second goal, revised").unwrap();
        assert_eq!(
            body.items("Goals"),
            vec!["first goal", "second goal, revised"]
        );

        body.delete_item("Goals", 0).unwrap();
        assert_eq!(body.items("Goals"), vec!["second goal, revised"]);
    }

    #[test]
    fn edit_missing_item_errors() {
        let mut body = Body::default();
        body.add_item("Goals", "only one");
        assert!(body.edit_item("Goals", 5, "nope").is_err());
        assert!(body.edit_item("Missing", 0, "nope").is_err());
    }

    #[test]
    fn create_load_save_round_trip() {
        let dir = tempdir().unwrap();
        let store = NotesStore::new(dir.path());

        let mut doc = store.create("Long-term goals", None).unwrap();
        assert_eq!(doc.slug, "long-term-goals");
        assert_eq!(doc.frontmatter.title, "Long-term goals");

        doc.body.add_item("Areas to grow into", "read DDIA ch. 8-9");
        store.save(&mut doc).unwrap();

        let loaded = store.load("long-term-goals").unwrap();
        assert_eq!(loaded.frontmatter.title, "Long-term goals");
        assert_eq!(
            loaded.body.items("Areas to grow into"),
            vec!["read DDIA ch. 8-9"]
        );
    }

    #[test]
    fn list_returns_slug_and_title_pairs() {
        let dir = tempdir().unwrap();
        let store = NotesStore::new(dir.path());
        store.create("Zebra Notes", None).unwrap();
        store
            .create("Alpha Notes", Some("proj".to_string()))
            .unwrap();

        let listed = store.list().unwrap();
        assert_eq!(
            listed,
            vec![
                ("alpha-notes".to_string(), "Alpha Notes".to_string()),
                ("zebra-notes".to_string(), "Zebra Notes".to_string()),
            ]
        );
    }

    #[test]
    fn list_on_missing_dir_is_empty() {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("does-not-exist");
        let store = NotesStore { dir: notes_dir };
        assert_eq!(store.list().unwrap(), Vec::new());
    }
}
