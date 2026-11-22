pub mod backup;
pub mod docx;
pub mod html;
pub mod markdown;
pub mod print;

use crate::config::Preferences;
use crate::model::{Block, BlockKind, Document, Inline, TextRun};
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Html,
    Markdown,
    PlainText,
    Docx,
}

#[derive(Debug, Clone)]
pub struct SaveOutcome {
    pub warning: Option<String>,
}

impl FileFormat {
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "html" | "htm" => FileFormat::Html,
            "docx" => FileFormat::Docx,
            "txt" | "text" => FileFormat::PlainText,
            "md" | "markdown" | "mdown" | "mkdn" | "mkd" | "mdwn" | "mdtxt" | "mdtext" => {
                FileFormat::Markdown
            }
            _ => FileFormat::PlainText,
        }
    }

    pub fn supports_direct_save(self) -> bool {
        matches!(
            self,
            FileFormat::Html | FileFormat::Markdown | FileFormat::PlainText
        )
    }
}

pub fn document_to_plain_text(doc: &Document) -> String {
    let mut out = String::new();
    let mut prev_emitted_kind: Option<&BlockKind> = None;
    for block in &doc.blocks {
        let is_silent_image_only = matches!(block.kind, BlockKind::Paragraph)
            && !block.inlines.is_empty()
            && block
                .inlines
                .iter()
                .all(|i| matches!(i, Inline::Image { alt, .. } if alt.is_empty()));
        if is_silent_image_only {
            continue;
        }

        let prev_is_list = prev_emitted_kind.is_some_and(BlockKind::is_list);
        let curr_is_list = block.kind.is_list();
        let switching_top_list = prev_emitted_kind.is_some_and(|pk| {
            pk.is_list() && block.kind.list_depth() == Some(0) && !pk.same_family(&block.kind)
        });
        if prev_emitted_kind.is_some() {
            if prev_is_list && curr_is_list && !switching_top_list {
                out.push('\n');
            } else {
                out.push_str("\n\n");
            }
        }
        match &block.kind {
            BlockKind::HorizontalRule => out.push_str("---"),
            BlockKind::Blockquote => {
                let inner = inlines_to_clean_plain(&block.inlines);
                for (i, line) in inner.split('\n').enumerate() {
                    if i > 0 {
                        out.push('\n');
                    }
                    out.push_str("> ");
                    out.push_str(line);
                }
            }
            BlockKind::BulletListItem { depth } => {
                for _ in 0..*depth {
                    out.push_str("  ");
                }
                out.push_str("- ");
                out.push_str(&inlines_to_clean_plain(&block.inlines));
            }
            BlockKind::OrderedListItem { depth, number } => {
                for _ in 0..*depth {
                    out.push_str("  ");
                }
                out.push_str(&format!("{number}. "));
                out.push_str(&inlines_to_clean_plain(&block.inlines));
            }
            BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::CodeBlock { .. } => {
                out.push_str(&inlines_to_clean_plain(&block.inlines));
            }
        }
        prev_emitted_kind = Some(&block.kind);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn inlines_to_clean_plain(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(run) => out.push_str(&run.text),
            Inline::LineBreak => out.push('\n'),
            Inline::Checkbox { checked } => {
                out.push_str(if *checked { "[x]" } else { "[ ]" });
                let next_has_space = matches!(
                    inlines.get(idx + 1),
                    Some(Inline::Text(r)) if r.text.starts_with(' ')
                );
                if !next_has_space && idx + 1 < inlines.len() {
                    out.push(' ');
                }
            }
            Inline::Image { alt, .. } => {
                if !alt.is_empty() {
                    out.push_str(&format!("[Image: {alt}]"));
                }
            }
            Inline::Comment { .. } => {}
        }
    }
    out
}

pub fn plain_text_to_document(text: &str) -> Document {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.strip_suffix('\n').unwrap_or(&normalized);
    if trimmed.is_empty() {
        return Document::default();
    }
    let blocks: Vec<Block> = if trimmed.contains("\n\n") {
        trimmed
            .split("\n\n")
            .map(|para| {
                let mut b = Block::new(BlockKind::Paragraph);
                let lines: Vec<&str> = para.split('\n').collect();
                for (i, line) in lines.iter().enumerate() {
                    if !line.is_empty() {
                        b.inlines.push(Inline::Text(TextRun::plain(*line)));
                    }
                    if i + 1 < lines.len() {
                        b.inlines.push(Inline::LineBreak);
                    }
                }
                b
            })
            .collect()
    } else {
        trimmed
            .split('\n')
            .map(Block::paragraph_with_text)
            .collect()
    };
    Document::from_blocks(blocks)
}

pub fn load_document(path: &Path) -> Result<(Document, FileFormat), String> {
    let format = FileFormat::from_path(path);
    match format {
        FileFormat::Docx => {
            let bytes = fs::read(path).map_err(|e| format!("Could not read file: {e}"))?;
            let doc = docx::docx_bytes_to_document(&bytes)?;
            Ok((doc, format))
        }
        FileFormat::Html => {
            let text = fs::read_to_string(path).map_err(|e| format!("Could not read file: {e}"))?;
            Ok((html::html_to_document(&text), format))
        }
        FileFormat::Markdown => {
            let text = fs::read_to_string(path).map_err(|e| format!("Could not read file: {e}"))?;
            Ok((markdown::markdown_to_document(&text), format))
        }
        FileFormat::PlainText => {
            let text = fs::read_to_string(path).map_err(|e| format!("Could not read file: {e}"))?;
            Ok((plain_text_to_document(&text), format))
        }
    }
}

pub fn save_document(doc: &Document, path: &Path) -> Result<SaveOutcome, String> {
    save_document_with_prefs(doc, path, &Preferences::default())
}

pub fn save_document_with_prefs(
    doc: &Document,
    path: &Path,
    prefs: &Preferences,
) -> Result<SaveOutcome, String> {
    let format = FileFormat::from_path(path);
    match format {
        FileFormat::Html => {
            let content = html::document_to_html_with_prefs(doc, prefs);
            atomic_write(path, content.as_bytes())?;
            Ok(SaveOutcome { warning: None })
        }
        FileFormat::Markdown => {
            let res = markdown::document_to_markdown(doc);
            atomic_write(path, res.markdown.as_bytes())?;
            Ok(SaveOutcome { warning: None })
        }
        FileFormat::PlainText => {
            let content = document_to_plain_text(doc);
            atomic_write(path, content.as_bytes())?;
            let warning = if doc.has_rich_formatting() {
                Some(
                    "Saved as plain text (.txt). Rich text formatting is not preserved in plain text files."
                        .to_string(),
                )
            } else {
                None
            };
            Ok(SaveOutcome { warning })
        }
        FileFormat::Docx => {
            let bytes = docx::document_to_docx_bytes_with_options(doc, path.parent(), Some(prefs))?;
            atomic_write(path, &bytes)?;
            Ok(SaveOutcome { warning: None })
        }
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create directory: {e}"))?;
    }
    let tmp_name = format!(
        ".{}.sillpaper.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("doc")
    );
    let tmp_path = path.with_file_name(tmp_name);
    {
        let mut file =
            fs::File::create(&tmp_path).map_err(|e| format!("Failed creating temp file: {e}"))?;
        file.write_all(bytes)
            .map_err(|e| format!("Failed writing temp file: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Failed syncing temp file: {e}"))?;
    }
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("Failed replacing target file: {e}")
    })
}
