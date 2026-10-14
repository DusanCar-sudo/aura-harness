//! File attachments: read user-picked text files into prompt context.

use crate::error::{Error, Result};
use std::path::Path;

/// 1 MiB per file guard — attachments are context, not data dumps.
pub const MAX_ATTACHMENT_BYTES: u64 = 1024 * 1024;

pub struct Attachment {
    pub path: String,
    pub content: String,
}

pub fn read_attachment(path: &Path) -> Result<Attachment> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_ATTACHMENT_BYTES {
        return Err(Error::Invalid(format!(
            "{} is {} bytes; max is {MAX_ATTACHMENT_BYTES}",
            path.display(),
            meta.len()
        )));
    }
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Invalid(format!("{}: not readable as UTF-8 text ({e})", path.display()))
    })?;
    Ok(Attachment {
        path: path.display().to_string(),
        content,
    })
}

/// Render attachments as fenced blocks appended to the user message.
pub fn render_into_prompt(attachments: &[Attachment]) -> String {
    let mut out = String::new();
    for a in attachments {
        out.push_str(&format!(
            "\n\n--- Attached file: {} ---\n{}\n--- End of {} ---",
            a.path, a.content, a.path
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_small_text_file() {
        let dir = std::env::temp_dir().join(format!("aura-harness-att-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");
        std::fs::write(&path, "hello attachment").unwrap();
        let att = read_attachment(&path).unwrap();
        assert_eq!(att.content, "hello attachment");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_oversize_file() {
        let dir = std::env::temp_dir().join(format!("aura-harness-att2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.txt");
        let big = "x".repeat((MAX_ATTACHMENT_BYTES + 1) as usize);
        std::fs::write(&path, big).unwrap();
        assert!(matches!(read_attachment(&path), Err(Error::Invalid(_))));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renders_fenced_blocks() {
        let atts = vec![Attachment {
            path: "a.txt".into(),
            content: "body".into(),
        }];
        let rendered = render_into_prompt(&atts);
        assert!(rendered.contains("--- Attached file: a.txt ---"));
        assert!(rendered.contains("body"));
        assert!(rendered.contains("--- End of a.txt ---"));
    }
}
