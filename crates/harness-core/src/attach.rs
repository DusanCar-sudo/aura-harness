//! File attachments: read user-picked text files and images into prompt
//! context.
//!
//! Text rides in the prompt as a fenced block. Images are carried separately
//! as base64 content parts and only reach providers/models that accept them
//! (see [`crate::models::supports_images`]).

use crate::api::ImagePart;
use crate::error::{Error, Result};
use base64::Engine;
use std::path::Path;

/// 1 MiB per text file guard — attachments are context, not data dumps.
pub const MAX_ATTACHMENT_BYTES: u64 = 1024 * 1024;
/// Images travel base64-encoded (~33% larger on the wire), so keep the raw
/// file well under the 20 MB provider request limits.
pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentKind {
    /// UTF-8 text, already decoded.
    Text(String),
    /// Image bytes as standard base64 plus the detected MIME type.
    Image { mime: &'static str, data: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub path: String,
    pub kind: AttachmentKind,
    /// Raw file size, for the composer label.
    pub size: u64,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        matches!(self.kind, AttachmentKind::Image { .. })
    }

    /// File name only, for compact UI labels.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

pub fn read_attachment(path: &Path) -> Result<Attachment> {
    let meta = std::fs::metadata(path)?;
    let bytes = std::fs::read(path)?;
    let display = path.display().to_string();

    if let Some(mime) = sniff_image(&bytes) {
        if meta.len() > MAX_IMAGE_BYTES {
            return Err(Error::Invalid(format!(
                "{display} is {} MB; images max out at {} MB",
                meta.len() / (1024 * 1024),
                MAX_IMAGE_BYTES / (1024 * 1024)
            )));
        }
        let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(Attachment {
            path: display,
            kind: AttachmentKind::Image { mime, data },
            size: meta.len(),
        });
    }

    if meta.len() > MAX_ATTACHMENT_BYTES {
        return Err(Error::Invalid(format!(
            "{display} is {} bytes; max is {MAX_ATTACHMENT_BYTES}",
            meta.len()
        )));
    }
    let content = String::from_utf8(bytes).map_err(|_| {
        Error::Invalid(format!(
            "{display}: not text or a supported image (PNG, JPEG, GIF, WebP)"
        ))
    })?;
    Ok(Attachment {
        path: display,
        kind: AttachmentKind::Text(content),
        size: meta.len(),
    })
}

/// Detect an image by magic bytes rather than extension: a `.png` that is
/// really text should still attach as text, and a mislabelled photo should
/// still reach a vision model.
fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Render text attachments as fenced blocks appended to the user message.
/// Images are sent as content parts, not as prompt text.
pub fn render_into_prompt(attachments: &[Attachment]) -> String {
    let mut out = String::new();
    for a in attachments {
        let AttachmentKind::Text(content) = &a.kind else {
            continue;
        };
        out.push_str(&format!(
            "\n\n--- Attached file: {} ---\n{}\n--- End of {} ---",
            a.path, content, a.path
        ));
    }
    out
}

/// Image attachments as provider-ready base64 parts.
pub fn image_parts(attachments: &[Attachment]) -> Vec<ImagePart> {
    attachments
        .iter()
        .filter_map(|a| match &a.kind {
            AttachmentKind::Image { mime, data } => Some(ImagePart {
                mime: (*mime).to_string(),
                data: data.clone(),
            }),
            AttachmentKind::Text(_) => None,
        })
        .collect()
}

/// A title for a conversation whose first turn carried only images.
pub fn title_for(attachments: &[Attachment]) -> String {
    match attachments {
        [] => "New chat".to_string(),
        [one] => format!("Attached {}", one.name()),
        _ => format!("Attached {} files", attachments.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, bytes: &[u8]) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("aura-harness-att-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn reads_small_text_file() {
        let (dir, path) = temp_file("note.txt", b"hello attachment");
        let att = read_attachment(&path).unwrap();
        assert!(matches!(&att.kind, AttachmentKind::Text(t) if t == "hello attachment"));
        assert!(!att.is_image());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_oversize_text_file() {
        let big = "x".repeat((MAX_ATTACHMENT_BYTES + 1) as usize);
        let (dir, path) = temp_file("big.txt", big.as_bytes());
        assert!(matches!(read_attachment(&path), Err(Error::Invalid(_))));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_binary_non_image() {
        let (dir, path) = temp_file("blob.bin", &[0, 159, 146, 150, 0]);
        assert!(matches!(read_attachment(&path), Err(Error::Invalid(_))));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_png_as_image_part() {
        // 1x1 transparent PNG.
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
            .unwrap();
        let (dir, path) = temp_file("dot.png", &png);
        let att = read_attachment(&path).unwrap();
        assert!(att.is_image());
        let parts = image_parts(std::slice::from_ref(&att));
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].mime, "image/png");
        assert!(!parts[0].data.is_empty());
        // Images never leak into the text prompt.
        assert!(render_into_prompt(std::slice::from_ref(&att)).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renders_fenced_blocks() {
        let atts = vec![Attachment {
            path: "a.txt".into(),
            kind: AttachmentKind::Text("body".into()),
            size: 4,
        }];
        let rendered = render_into_prompt(&atts);
        assert!(rendered.contains("--- Attached file: a.txt ---"));
        assert!(rendered.contains("body"));
        assert!(rendered.contains("--- End of a.txt ---"));
    }

    #[test]
    fn image_only_title_uses_file_name() {
        let atts = vec![Attachment {
            path: "/tmp/cat.png".into(),
            kind: AttachmentKind::Image {
                mime: "image/png",
                data: "abc".into(),
            },
            size: 3,
        }];
        assert_eq!(title_for(&atts), "Attached cat.png");
    }
}
