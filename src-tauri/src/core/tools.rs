//! Built-in tools. v0 ships one: local notes, confined to the app's own data
//! directory. Note names are slugified, which structurally prevents path
//! traversal — a note can never land outside `<data_dir>/notes/`.

use std::fs;
use std::path::{Path, PathBuf};

/// Lowercase, alphanumerics and single hyphens only, capped at 64 chars.
/// Shared by everything that turns a user-supplied name into a filename —
/// the slug can never contain path separators, so traversal is impossible.
pub(crate) fn slugify(title: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_hyphen = true; // suppress leading hyphen
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_hyphen = false;
        } else if !last_hyphen {
            out.push('-');
            last_hyphen = true;
        }
        if out.len() >= 64 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("note title must contain at least one letter or digit")]
    InvalidName,
    #[error("note not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct NotesTool {
    notes_dir: PathBuf,
}

impl NotesTool {
    /// This tool writes only inside `<data_dir>/notes` — no external side effects.
    pub const SIDE_EFFECTS: &'static str = "writes only within the app data directory";

    pub fn new(data_dir: &Path) -> Self {
        Self {
            notes_dir: data_dir.join("notes"),
        }
    }

    pub fn notes_dir(&self) -> &Path {
        &self.notes_dir
    }

    fn slug(title: &str) -> Option<String> {
        slugify(title)
    }

    fn path_for(&self, slug: &str) -> PathBuf {
        self.notes_dir.join(format!("{slug}.md"))
    }

    /// Saves (or overwrites) a note; returns its slug.
    pub fn save_note(&self, title: &str, content: &str) -> Result<String, ToolError> {
        let slug = Self::slug(title).ok_or(ToolError::InvalidName)?;
        fs::create_dir_all(&self.notes_dir)?;
        fs::write(self.path_for(&slug), content)?;
        Ok(slug)
    }

    pub fn list_notes(&self) -> Result<Vec<String>, ToolError> {
        if !self.notes_dir.exists() {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = fs::read_dir(&self.notes_dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    path.file_stem().map(|s| s.to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        Ok(names)
    }

    pub fn read_note(&self, name: &str) -> Result<String, ToolError> {
        let slug = Self::slug(name).ok_or(ToolError::InvalidName)?;
        let path = self.path_for(&slug);
        if !path.exists() {
            return Err(ToolError::NotFound(slug));
        }
        Ok(fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let tool = NotesTool::new(dir.path());
        let slug = tool.save_note("Grocery List", "eggs, milk").unwrap();
        assert_eq!(slug, "grocery-list");
        assert_eq!(tool.list_notes().unwrap(), vec!["grocery-list"]);
        assert_eq!(tool.read_note("Grocery List").unwrap(), "eggs, milk");
    }

    #[test]
    fn traversal_attempts_stay_inside_notes_dir() {
        let dir = tempfile::tempdir().unwrap();
        let tool = NotesTool::new(dir.path());
        let slug = tool.save_note("../../evil", "payload").unwrap();
        assert_eq!(slug, "evil");
        let written = tool.notes_dir().join("evil.md");
        assert!(written.exists());
        // Nothing escaped the notes directory.
        assert!(!dir.path().join("evil.md").exists());
        assert!(!dir.path().parent().unwrap().join("evil.md").exists());
    }

    #[test]
    fn garbage_titles_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let tool = NotesTool::new(dir.path());
        assert!(matches!(
            tool.save_note("///...///", "x"),
            Err(ToolError::InvalidName)
        ));
        assert!(matches!(
            tool.save_note("", "x"),
            Err(ToolError::InvalidName)
        ));
    }

    #[test]
    fn listing_empty_store_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let tool = NotesTool::new(dir.path());
        assert!(tool.list_notes().unwrap().is_empty());
        assert!(matches!(
            tool.read_note("nope"),
            Err(ToolError::NotFound(_))
        ));
    }
}
