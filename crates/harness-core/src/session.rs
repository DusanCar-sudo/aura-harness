//! Conversation model + JSONL persistence.
//!
//! One JSON line per message in `<data>/aura-harness/sessions/<uuid>.jsonl`.
//! Line 1 is a header line recording the session title.

use crate::api::Message;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    pub id: String,
    pub title: String,
    pub created: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionLine {
    Header(SessionHeader),
    Message(Message),
}

#[derive(Debug, Clone)]
pub struct Session {
    pub header: SessionHeader,
    pub messages: Vec<Message>,
    path: PathBuf,
}

pub fn sessions_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("AURA_HARNESS_HOME") {
        return PathBuf::from(dir).join("sessions");
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local/share")
        });
    base.join("aura-harness/sessions")
}

impl Session {
    /// Create a brand-new in-memory session (not yet on disk).
    pub fn new(title: impl Into<String>) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        Self {
            header: SessionHeader {
                id: id.clone(),
                title: title.into(),
                created: chrono::Utc::now(),
            },
            messages: Vec::new(),
            path: sessions_dir().join(format!("{id}.jsonl")),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn title(&self) -> &str {
        &self.header.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) -> std::io::Result<()> {
        self.header.title = title.into();
        self.save()
    }

    /// Load a session from its JSONL file.
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut header = None;
        let mut messages = Vec::new();
        for line in std::io::BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<SessionLine>(&line)? {
                SessionLine::Header(h) => header = Some(h),
                SessionLine::Message(m) => messages.push(m),
            }
        }
        let header = header
            .ok_or_else(|| Error::Invalid(format!("no header in {}", path.display())))?;
        Ok(Self {
            header,
            messages,
            path: path.to_path_buf(),
        })
    }

    /// List all sessions on disk, newest first by created timestamp.
    pub fn list_all() -> Result<Vec<SessionHeader>> {
        let dir = sessions_dir();
        let mut out = Vec::new();
        if !dir.exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            // Fast path: header is the first line.
            let Ok(file) = std::fs::File::open(&path) else { continue };
            let Some(first) = std::io::BufReader::new(file).lines().next() else { continue };
            let Ok(first) = first else { continue };
            if let Ok(SessionLine::Header(h)) = serde_json::from_str::<SessionLine>(&first) {
                out.push(h);
            }
        }
        out.sort_by_key(|h| std::cmp::Reverse(h.created));
        Ok(out)
    }

    pub fn delete_on_disk(&self) -> std::io::Result<()> {
        std::fs::remove_file(&self.path)
    }

    /// Persist the whole session (atomic full rewrite; sessions stay small).
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        {
            let file = std::fs::File::create(&tmp)?;
            let mut w = std::io::BufWriter::new(file);
            writeln!(
                w,
                "{}",
                serde_json::to_string(&SessionLine::Header(self.header.clone()))
                    .map_err(std::io::Error::other)?
            )?;
            for m in &self.messages {
                writeln!(
                    w,
                    "{}",
                    serde_json::to_string(&SessionLine::Message(m.clone()))
                        .map_err(std::io::Error::other)?
                )?;
            }
            w.flush()?;
        }
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aura-harness-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_messages() {
        let _lock = crate::env_lock::guard();
        let dir = temp_home();
        std::env::set_var("AURA_HARNESS_HOME", &dir);
        let mut s = Session::new("test");
        s.messages.push(Message::user("hello"));
        s.messages.push(Message {
            role: "assistant".into(),
            content: "answer".into(),
            reasoning_content: "thought".into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            ts: Some(chrono::Utc::now()),
        });
        s.save().unwrap();
        let loaded = Session::load(&s.path).unwrap();
        assert_eq!(loaded.title(), "test");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[1].reasoning_content, "thought");
        std::env::remove_var("AURA_HARNESS_HOME");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_all_sorts_newest_first() {
        let _lock = crate::env_lock::guard();
        let dir = temp_home();
        std::env::set_var("AURA_HARNESS_HOME", &dir);
        let mut a = Session::new("old");
        a.header.created = chrono::Utc::now() - chrono::Duration::hours(2);
        a.save().unwrap();
        let mut b = Session::new("new");
        b.header.created = chrono::Utc::now();
        b.save().unwrap();
        let list = Session::list_all().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "new");
        assert_eq!(list[1].title, "old");
        std::env::remove_var("AURA_HARNESS_HOME");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
