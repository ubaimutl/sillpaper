use crate::model::{Block, BlockKind, Document, DocumentFragment, Inline, InlineStyle, TextRun};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone)]
pub struct MarkdownExportResult {
    pub markdown: String,
    pub converted_underline: bool,
}

pub fn markdown_to_document(md: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(md, options);

    let mut blocks: Vec<Block> = Vec::new();
    let mut current: Option<Block> = None;
    let mut style = InlineStyle::default();
    let mut blockquote_depth: usize = 0;
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut in_image = false;
    let mut image_src = String::new();
    let mut image_alt = String::new();

    let flush = |current: &mut Option<Block>, blocks: &mut Vec<Block>| {
        if let Some(mut b) = current.take() {
            b.normalize();
            blocks.push(b);
        }
    };

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if current.is_none() {
                        let kind = if blockquote_depth > 0 {
                            BlockKind::Blockquote
                        } else if let Some(ord) = list_stack.last() {
                            let depth = list_stack.len().saturating_sub(1).min(4) as u8;
                            if ord.is_some() {
                                BlockKind::OrderedListItem { depth, number: 1 }
                            } else {
                                BlockKind::BulletListItem { depth }
                            }
                        } else {
                            BlockKind::Paragraph
                        };
                        current = Some(Block::new(kind));
                    }
                }
                Tag::Heading { level, .. } => {
                    flush(&mut current, &mut blocks);
                    let lvl = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    current = Some(Block::new(BlockKind::Heading(lvl)));
                }
                Tag::BlockQuote(_) => {
                    flush(&mut current, &mut blocks);
                    blockquote_depth += 1;
                }
                Tag::List(start) => {
                    flush(&mut current, &mut blocks);
                    list_stack.push(start);
                }
                Tag::Item => {
                    flush(&mut current, &mut blocks);
                    let depth = list_stack.len().saturating_sub(1).min(4) as u8;
                    let kind = match list_stack.last().copied().flatten() {
                        Some(_) => BlockKind::OrderedListItem { depth, number: 1 },
                        None => BlockKind::BulletListItem { depth },
                    };
                    current = Some(Block::new(kind));
                }
                Tag::CodeBlock(kind) => {
                    flush(&mut current, &mut blocks);
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                        _ => None,
                    };
                    current = Some(Block::new(BlockKind::CodeBlock { language }));
                }
                Tag::Strong => style.bold = true,
                Tag::Emphasis => style.italic = true,
                Tag::Strikethrough => style.strikethrough = true,
                Tag::Link { dest_url, .. } => {
                    style.link = Some(dest_url.to_string());
                }
                Tag::Image { dest_url, .. } => {
                    in_image = true;
                    image_src = dest_url.to_string();
                    image_alt.clear();
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock => {
                    flush(&mut current, &mut blocks);
                }
                TagEnd::BlockQuote(_) => {
                    flush(&mut current, &mut blocks);
                    blockquote_depth = blockquote_depth.saturating_sub(1);
                }
                TagEnd::List(_) => {
                    flush(&mut current, &mut blocks);
                    list_stack.pop();
                }
                TagEnd::Strong => style.bold = false,
                TagEnd::Emphasis => style.italic = false,
                TagEnd::Strikethrough => style.strikethrough = false,
                TagEnd::Link => style.link = None,
                TagEnd::Image => {
                    in_image = false;
                    let b = current.get_or_insert_with(|| Block::new(BlockKind::Paragraph));
                    b.inlines.push(Inline::Image {
                        src: std::mem::take(&mut image_src),
                        alt: std::mem::take(&mut image_alt),
                        original_src: None,
                    });
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_image {
                    image_alt.push_str(&text);
                    continue;
                }
                let b = current.get_or_insert_with(|| {
                    if blockquote_depth > 0 {
                        Block::new(BlockKind::Blockquote)
                    } else {
                        Block::new(BlockKind::Paragraph)
                    }
                });
                if matches!(b.kind, BlockKind::CodeBlock { .. }) {
                    let trimmed = text.strip_suffix('\n').unwrap_or(&text);
                    let lines: Vec<&str> = trimmed.split('\n').collect();
                    for (i, line) in lines.iter().enumerate() {
                        if !line.is_empty() {
                            b.inlines.push(Inline::Text(TextRun::plain(*line)));
                        }
                        if i + 1 < lines.len() {
                            b.inlines.push(Inline::LineBreak);
                        }
                    }
                } else {
                    push_text_with_inline_checkboxes(b, &text, &style);
                }
            }
            Event::Code(code) => {
                if in_image {
                    image_alt.push_str(&code);
                    continue;
                }
                let mut code_style = style.clone();
                code_style.code = true;
                let b = current.get_or_insert_with(|| Block::new(BlockKind::Paragraph));
                b.inlines
                    .push(Inline::Text(TextRun::new(code.to_string(), code_style)));
            }
            Event::SoftBreak | Event::HardBreak => {
                let b = current.get_or_insert_with(|| Block::new(BlockKind::Paragraph));
                b.inlines.push(Inline::LineBreak);
            }
            Event::Rule => {
                flush(&mut current, &mut blocks);
                blocks.push(Block::new(BlockKind::HorizontalRule));
            }
            Event::TaskListMarker(checked) => {
                let b = current.get_or_insert_with(|| Block::new(BlockKind::Paragraph));
                b.inlines.push(Inline::Checkbox { checked });
                b.inlines.push(Inline::Text(TextRun::plain(" ")));
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                let trimmed = raw.trim();
                if let Some(comment) = trimmed
                    .strip_prefix("<!--")
                    .and_then(|s| s.strip_suffix("-->"))
                {
                    let b = current.get_or_insert_with(|| Block::new(BlockKind::Paragraph));
                    b.inlines.push(Inline::Comment {
                        text: comment.to_string(),
                    });
                } else if trimmed.eq_ignore_ascii_case("<u>") {
                    style.underline = true;
                } else if trimmed.eq_ignore_ascii_case("</u>") {
                    style.underline = false;
                } else if trimmed.eq_ignore_ascii_case("<br>")
                    || trimmed.eq_ignore_ascii_case("<br/>")
                    || trimmed.eq_ignore_ascii_case("<br />")
                {
                    let b = current.get_or_insert_with(|| Block::new(BlockKind::Paragraph));
                    b.inlines.push(Inline::LineBreak);
                }
            }
            _ => {}
        }
    }

    flush(&mut current, &mut blocks);
    Document::from_blocks(blocks)
}

fn push_text_with_inline_checkboxes(block: &mut Block, text: &str, style: &InlineStyle) {
    if block.inlines.is_empty() {
        if let Some(rest) = text.strip_prefix("[ ] ") {
            block.inlines.push(Inline::Checkbox { checked: false });
            block
                .inlines
                .push(Inline::Text(TextRun::new(" ", style.clone())));
            if !rest.is_empty() {
                block
                    .inlines
                    .push(Inline::Text(TextRun::new(rest, style.clone())));
            }
            return;
        }
        if let Some(rest) = text
            .strip_prefix("[x] ")
            .or_else(|| text.strip_prefix("[X] "))
        {
            block.inlines.push(Inline::Checkbox { checked: true });
            block
                .inlines
                .push(Inline::Text(TextRun::new(" ", style.clone())));
            if !rest.is_empty() {
                block
                    .inlines
                    .push(Inline::Text(TextRun::new(rest, style.clone())));
            }
            return;
        }
    }
    block
        .inlines
        .push(Inline::Text(TextRun::new(text, style.clone())));
}

pub fn document_to_markdown(doc: &Document) -> MarkdownExportResult {
    let mut res = blocks_to_markdown(&doc.blocks);
    if !res.markdown.is_empty() && !res.markdown.ends_with('\n') {
        res.markdown.push('\n');
    }
    res
}

pub fn fragment_to_markdown(frag: &DocumentFragment) -> MarkdownExportResult {
    blocks_to_markdown(&frag.blocks)
}

pub fn blocks_to_markdown(blocks: &[Block]) -> MarkdownExportResult {
    let mut out = String::new();

    for (idx, block) in blocks.iter().enumerate() {
        let prev_is_list = idx > 0 && blocks[idx - 1].kind.is_list();
        let curr_is_list = block.kind.is_list();
        let switching_top_list = idx > 0
            && blocks[idx - 1].kind.is_list()
            && block.kind.list_depth() == Some(0)
            && !blocks[idx - 1].kind.same_family(&block.kind);

        if idx > 0 {
            if prev_is_list && curr_is_list && !switching_top_list {
                out.push('\n');
            } else {
                out.push_str("\n\n");
            }
        }

        match &block.kind {
            BlockKind::Paragraph => {
                write_inlines_md(&mut out, &block.inlines);
            }
            BlockKind::Heading(lvl) => {
                for _ in 0..(*lvl).clamp(1, 6) {
                    out.push('#');
                }
                out.push(' ');
                write_inlines_md(&mut out, &block.inlines);
            }
            BlockKind::Blockquote => {
                let mut inner = String::new();
                write_inlines_md(&mut inner, &block.inlines);
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
                    out.push_str("    ");
                }
                out.push_str("- ");
                write_inlines_md(&mut out, &block.inlines);
            }
            BlockKind::OrderedListItem { depth, number } => {
                for _ in 0..*depth {
                    out.push_str("    ");
                }
                out.push_str(&format!("{number}. "));
                write_inlines_md(&mut out, &block.inlines);
            }
            BlockKind::CodeBlock { language } => {
                let body = block.plain_text();
                let fence = safe_code_fence(&body);
                out.push_str(&fence);
                if let Some(lang) = language {
                    out.push_str(lang);
                }
                out.push('\n');
                out.push_str(&body);
                out.push('\n');
                out.push_str(&fence);
            }
            BlockKind::HorizontalRule => {
                out.push_str("---");
            }
        }
    }

    MarkdownExportResult {
        markdown: out,
        converted_underline: false,
    }
}

fn safe_code_fence(body: &str) -> String {
    let mut max_run = 0usize;
    let mut cur = 0usize;
    for ch in body.chars() {
        if ch == '`' {
            cur += 1;
            max_run = max_run.max(cur);
        } else {
            cur = 0;
        }
    }
    "`".repeat((max_run + 1).max(3))
}

fn write_inline_code_md(out: &mut String, text: &str) {
    let mut max_run = 0usize;
    let mut cur = 0usize;
    for ch in text.chars() {
        if ch == '`' {
            cur += 1;
            max_run = max_run.max(cur);
        } else {
            cur = 0;
        }
    }
    let ticks = "`".repeat(max_run + 1);
    let needs_pad = text.starts_with('`')
        || text.ends_with('`')
        || (text.starts_with(' ') && text.ends_with(' ') && !text.trim().is_empty());
    out.push_str(&ticks);
    if needs_pad {
        out.push(' ');
    }
    out.push_str(text);
    if needs_pad {
        out.push(' ');
    }
    out.push_str(&ticks);
}

fn write_inlines_md(out: &mut String, inlines: &[Inline]) {
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(run) => write_run_md(out, run),
            Inline::LineBreak => out.push_str("  \n"),
            Inline::Checkbox { checked } => {
                out.push_str(if *checked { "[x]" } else { "[ ]" });
                let next_starts_with_space = matches!(
                    inlines.get(idx + 1),
                    Some(Inline::Text(r)) if r.text.starts_with(' ')
                );
                if !next_starts_with_space && idx + 1 < inlines.len() {
                    out.push(' ');
                }
            }
            Inline::Comment { text } => {
                let safe = text.replace("<!--", "").replace("-->", "");
                out.push_str("<!--");
                out.push_str(&safe);
                out.push_str("-->");
            }
            Inline::Image {
                src,
                alt,
                original_src,
            } => {
                let target = original_src.as_deref().unwrap_or(src.as_str());
                out.push_str("![");
                out.push_str(alt);
                out.push_str("](");
                write_md_url(out, target);
                out.push(')');
            }
        }
    }
}

fn write_run_md(out: &mut String, run: &TextRun) {
    let s = &run.style;
    if s.is_plain() {
        out.push_str(&run.text);
        return;
    }

    let leading_ws_len = run
        .text
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(run.text.len());
    if leading_ws_len == run.text.len() {
        out.push_str(&run.text);
        return;
    }
    let trailing_start = run
        .text
        .rfind(|c: char| !c.is_whitespace())
        .map(|i| i + run.text[i..].chars().next().unwrap().len_utf8())
        .unwrap_or(run.text.len());

    let leading = &run.text[..leading_ws_len];
    let core = &run.text[leading_ws_len..trailing_start];
    let trailing = &run.text[trailing_start..];

    out.push_str(leading);

    if s.link.is_some() {
        out.push('[');
    }
    if s.bold && s.italic {
        out.push_str("***");
    } else if s.bold {
        out.push_str("**");
    } else if s.italic {
        out.push('*');
    }
    if s.underline {
        out.push_str("<u>");
    }
    if s.strikethrough {
        out.push_str("~~");
    }
    if s.code {
        write_inline_code_md(out, core);
    } else {
        out.push_str(core);
    }
    if s.strikethrough {
        out.push_str("~~");
    }
    if s.underline {
        out.push_str("</u>");
    }
    if s.bold && s.italic {
        out.push_str("***");
    } else if s.bold {
        out.push_str("**");
    } else if s.italic {
        out.push('*');
    }
    if let Some(url) = &s.link {
        out.push_str("](");
        write_md_url(out, url);
        out.push(')');
    }

    out.push_str(trailing);
}

fn write_md_url(out: &mut String, url: &str) {
    if url.contains(' ') || url.contains('(') || url.contains(')') {
        out.push('<');
        out.push_str(url);
        out.push('>');
    } else {
        out.push_str(url);
    }
}
