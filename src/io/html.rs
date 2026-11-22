use crate::config::{FontChoice, Preferences, TextWidth};
use crate::model::{
    Block, BlockKind, Document, DocumentFragment, Inline, InlineStyle, TextDirection, TextRun,
};

pub fn document_to_html(doc: &Document) -> String {
    document_to_html_with_prefs(doc, &Preferences::default())
}

pub fn document_to_html_with_prefs(doc: &Document, prefs: &Preferences) -> String {
    let title = doc
        .derived_title()
        .unwrap_or_else(|| "Untitled".to_string());
    let font_css = match &prefs.font {
        FontChoice::Serif => "Georgia, \"Liberation Serif\", \"DejaVu Serif\", serif".to_string(),
        FontChoice::Sans => {
            "\"Inter\", \"Cantarell\", \"Liberation Sans\", \"DejaVu Sans\", system-ui, sans-serif"
                .to_string()
        }
        FontChoice::Monospace => {
            "\"Adwaita Mono\", \"Liberation Mono\", \"DejaVu Sans Mono\", monospace".to_string()
        }
        FontChoice::System(name) => {
            format!("\"{}\", Georgia, serif", escape_attr(name))
        }
    };
    let font_px = (18.0 * (prefs.font_size_percent.clamp(60, 220) as f64 / 100.0)).round() as u32;
    let line_height = prefs.line_spacing.css_factor();
    let heading_lh = prefs.line_spacing.css_heading_factor();
    let max_width = match prefs.text_width {
        TextWidth::Narrow => "480px",
        TextWidth::Medium => "680px",
        TextWidth::Wide => "900px",
        TextWidth::ExtraWide => "1140px",
        TextWidth::Percent75 => "75vw",
        TextWidth::Full => "none",
    };
    let p_margin_bottom = if prefs.paragraph_margins {
        "1em"
    } else {
        "0.25em"
    };

    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    out.push_str("  <meta charset=\"utf-8\">\n");
    out.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str(&format!("  <title>{}</title>\n", escape_html(&title)));
    out.push_str(&format!(
        r#"  <style>
    :root {{ color-scheme: light dark; }}
    body {{
      margin: 0;
      padding: 48px 24px 96px;
      font-family: {font_css};
      font-size: {font_px}px;
      line-height: {line_height:.2};
      color: #2b2b2b;
      background: #fafafa;
    }}
    main {{ max-width: {max_width}; margin: 0 auto; }}
    h1, h2, h3, h4, h5, h6 {{ line-height: {heading_lh:.2}; margin: 1.35em 0 0.55em; font-weight: 700; }}
    h1 {{ font-size: 1.85em; }}
    h2 {{ font-size: 1.45em; }}
    h3 {{ font-size: 1.22em; }}
    h4 {{ font-size: 1.10em; }}
    p {{ margin: 0 0 {p_margin_bottom}; }}
    blockquote {{
      margin: 1em 0;
      padding: 0.2em 1.1em;
      border-inline-start: 3px solid rgba(128, 128, 128, 0.45);
      color: #555;
      font-style: italic;
    }}
    code {{
      font-family: "Adwaita Mono", "Liberation Mono", "DejaVu Sans Mono", monospace;
      font-size: 0.9em;
      background: rgba(128, 128, 128, 0.14);
      padding: 0.1em 0.35em;
      border-radius: 4px;
    }}
    pre {{
      background: rgba(128, 128, 128, 0.10);
      padding: 14px 16px;
      border-radius: 6px;
      overflow-x: auto;
      line-height: 1.45;
    }}
    pre code {{ background: none; padding: 0; }}
    hr {{ border: none; border-top: 1px solid rgba(128, 128, 128, 0.32); margin: 1.8em 0; }}
    a {{ color: #1a6fb5; text-decoration: underline; }}
    ul, ol {{ margin: 0.6em 0 1em; padding-inline-start: 1.6em; }}
    li {{ margin: 0.25em 0; }}
    img {{ max-width: 100%; height: auto; display: block; margin: 1em auto; border-radius: 4px; }}
    input[type="checkbox"] {{ vertical-align: middle; margin-inline-end: 0.35em; }}
    @media (prefers-color-scheme: dark) {{
      body {{ color: #e4e4e4; background: #1e1e1e; }}
      blockquote {{ color: #aaa; }}
      a {{ color: #5cc2ff; }}
    }}
    @media print {{
      body {{ background: #fff; color: #111; padding: 0; font-size: 12pt; }}
      main {{ max-width: none; }}
    }}
  </style>
"#
    ));
    out.push_str("</head>\n<body>\n<main>\n");
    out.push_str(&blocks_to_html_body(&doc.blocks));
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

pub fn fragment_to_html(frag: &DocumentFragment) -> String {
    blocks_to_html_body(&frag.blocks)
}

pub fn blocks_to_html_body(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut list_stack: Vec<(bool, u8)> = Vec::new();

    for block in blocks {
        match &block.kind {
            BlockKind::BulletListItem { depth } => {
                sync_list_stack(&mut out, &mut list_stack, false, *depth);
                out.push_str("<li");
                write_dir_attr(&mut out, block.direction);
                out.push('>');
                write_inlines_html(&mut out, &block.inlines);
                out.push_str("</li>\n");
            }
            BlockKind::OrderedListItem { depth, .. } => {
                sync_list_stack(&mut out, &mut list_stack, true, *depth);
                out.push_str("<li");
                write_dir_attr(&mut out, block.direction);
                out.push('>');
                write_inlines_html(&mut out, &block.inlines);
                out.push_str("</li>\n");
            }
            other => {
                close_all_lists(&mut out, &mut list_stack);
                match other {
                    BlockKind::Paragraph => {
                        out.push_str("<p");
                        write_dir_attr(&mut out, block.direction);
                        out.push('>');
                        if block.inlines.is_empty() {
                            out.push_str("<br/>");
                        } else {
                            write_inlines_html(&mut out, &block.inlines);
                        }
                        out.push_str("</p>\n");
                    }
                    BlockKind::Heading(level) => {
                        let lvl = (*level).clamp(1, 6);
                        out.push_str(&format!("<h{lvl}"));
                        write_dir_attr(&mut out, block.direction);
                        out.push('>');
                        write_inlines_html(&mut out, &block.inlines);
                        out.push_str(&format!("</h{lvl}>\n"));
                    }
                    BlockKind::Blockquote => {
                        out.push_str("<blockquote");
                        write_dir_attr(&mut out, block.direction);
                        out.push('>');
                        write_inlines_html(&mut out, &block.inlines);
                        out.push_str("</blockquote>\n");
                    }
                    BlockKind::CodeBlock { language } => {
                        out.push_str("<pre><code");
                        if let Some(lang) = language
                            && !lang.is_empty()
                        {
                            out.push_str(&format!(" class=\"language-{}\"", escape_attr(lang)));
                        }
                        out.push('>');
                        out.push_str(&escape_html(&block.plain_text()));
                        out.push_str("</code></pre>\n");
                    }
                    BlockKind::HorizontalRule => {
                        out.push_str("<hr/>\n");
                    }
                    BlockKind::BulletListItem { .. } | BlockKind::OrderedListItem { .. } => {
                        unreachable!()
                    }
                }
            }
        }
    }
    close_all_lists(&mut out, &mut list_stack);
    out
}

fn sync_list_stack(out: &mut String, stack: &mut Vec<(bool, u8)>, ordered: bool, depth: u8) {
    while let Some(&(_, top_depth)) = stack.last() {
        if top_depth > depth {
            let (was_ord, _) = stack.pop().unwrap();
            out.push_str(if was_ord { "</ol>\n" } else { "</ul>\n" });
        } else {
            break;
        }
    }
    if let Some(&(top_ord, top_depth)) = stack.last()
        && top_depth == depth
        && top_ord != ordered
    {
        stack.pop();
        out.push_str(if top_ord { "</ol>\n" } else { "</ul>\n" });
    }
    let start_depth = stack.last().map(|&(_, d)| d + 1).unwrap_or(0);
    if stack.last().map(|&(_, d)| d) != Some(depth) {
        for d in start_depth..=depth {
            out.push_str(if ordered { "<ol>\n" } else { "<ul>\n" });
            stack.push((ordered, d));
        }
    }
}

fn close_all_lists(out: &mut String, stack: &mut Vec<(bool, u8)>) {
    while let Some((was_ord, _)) = stack.pop() {
        out.push_str(if was_ord { "</ol>\n" } else { "</ul>\n" });
    }
}

fn write_dir_attr(out: &mut String, dir: TextDirection) {
    match dir {
        TextDirection::Rtl => out.push_str(" dir=\"rtl\" style=\"direction:rtl\""),
        TextDirection::Ltr => out.push_str(" dir=\"ltr\" style=\"direction:ltr\""),
        TextDirection::Auto => out.push_str(" dir=\"auto\""),
    }
}

fn write_inlines_html(out: &mut String, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text(run) => write_run_html(out, run),
            Inline::LineBreak => out.push_str("<br/>"),
            Inline::Checkbox { checked } => {
                if *checked {
                    out.push_str("<input type=\"checkbox\" checked=\"checked\"/>");
                } else {
                    out.push_str("<input type=\"checkbox\"/>");
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
                out.push_str("<img src=\"");
                out.push_str(&escape_attr(target));
                out.push_str("\" alt=\"");
                out.push_str(&escape_attr(alt));
                out.push_str("\"/>");
            }
        }
    }
}

fn write_run_html(out: &mut String, run: &TextRun) {
    let s = &run.style;
    if let Some(href) = &s.link {
        out.push_str("<a href=\"");
        out.push_str(&escape_attr(href));
        out.push_str("\">");
    }
    if s.bold {
        out.push_str("<strong>");
    }
    if s.italic {
        out.push_str("<em>");
    }
    if s.underline {
        out.push_str("<u>");
    }
    if s.strikethrough {
        out.push_str("<del>");
    }
    if s.code {
        out.push_str("<code>");
    }

    out.push_str(&escape_html(&run.text));

    if s.code {
        out.push_str("</code>");
    }
    if s.strikethrough {
        out.push_str("</del>");
    }
    if s.underline {
        out.push_str("</u>");
    }
    if s.italic {
        out.push_str("</em>");
    }
    if s.bold {
        out.push_str("</strong>");
    }
    if s.link.is_some() {
        out.push_str("</a>");
    }
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn html_to_document(html: &str) -> Document {
    let frag = html_to_fragment(html);
    Document::from_blocks(frag.blocks)
}

pub fn html_to_fragment(html: &str) -> DocumentFragment {
    let tokens = tokenize_html(html);
    let mut parser = HtmlDocBuilder::new();
    parser.consume_tokens(&tokens);
    parser.finish()
}

#[derive(Debug, Clone)]
enum HtmlToken {
    StartTag {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    EndTag(String),
    Text(String),
    Comment(String),
}

fn tokenize_html(input: &str) -> Vec<HtmlToken> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    while i < len {
        if bytes[i] == b'<' {
            if input[i..].starts_with("<!--") {
                if let Some(end_rel) = input[i + 4..].find("-->") {
                    let body = &input[i + 4..i + 4 + end_rel];
                    tokens.push(HtmlToken::Comment(body.to_string()));
                    i = i + 4 + end_rel + 3;
                    continue;
                } else {
                    tokens.push(HtmlToken::Comment(input[i + 4..].to_string()));
                    break;
                }
            }
            if input[i..].starts_with("<!") || input[i..].starts_with("<?") {
                if let Some(gt) = input[i..].find('>') {
                    i += gt + 1;
                    continue;
                } else {
                    break;
                }
            }
            if i + 1 < len && bytes[i + 1] == b'/' {
                if let Some(gt) = input[i + 2..].find('>') {
                    let name = input[i + 2..i + 2 + gt].trim().to_ascii_lowercase();
                    tokens.push(HtmlToken::EndTag(name));
                    i = i + 2 + gt + 1;
                    continue;
                } else {
                    break;
                }
            }
            if let Some((tag_token, next_i)) = parse_start_tag(input, i) {
                tokens.push(tag_token);
                i = next_i;
                continue;
            }
            tokens.push(HtmlToken::Text("<".to_string()));
            i += 1;
        } else {
            let start = i;
            while i < len && bytes[i] != b'<' {
                i += 1;
            }
            tokens.push(HtmlToken::Text(decode_html_entities(&input[start..i])));
        }
    }

    tokens
}

fn parse_start_tag(input: &str, start: usize) -> Option<(HtmlToken, usize)> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = start + 1;
    if i >= len || !bytes[i].is_ascii_alphabetic() {
        return None;
    }
    let name_start = i;
    while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b':') {
        i += 1;
    }
    let name = input[name_start..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut self_closing = false;

    while i < len {
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }
        if bytes[i] == b'>' {
            return Some((
                HtmlToken::StartTag {
                    name,
                    attrs,
                    self_closing,
                },
                i + 1,
            ));
        }
        if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'>' {
            self_closing = true;
            return Some((
                HtmlToken::StartTag {
                    name,
                    attrs,
                    self_closing,
                },
                i + 2,
            ));
        }
        let attr_start = i;
        while i < len
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        if i == attr_start {
            i += 1;
            continue;
        }
        let attr_name = input[attr_start..i].to_ascii_lowercase();
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut attr_val = String::new();
        if i < len && bytes[i] == b'=' {
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < len && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let val_start = i;
                while i < len && bytes[i] != quote {
                    i += 1;
                }
                attr_val = decode_html_entities(&input[val_start..i]);
                if i < len {
                    i += 1;
                }
            } else {
                let val_start = i;
                while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                attr_val = decode_html_entities(&input[val_start..i]);
            }
        }
        attrs.push((attr_name, attr_val));
    }

    None
}

pub fn decode_html_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'&'
            && let Some(semi) = s[i + 1..].find(';')
            && semi <= 12
        {
            let entity = &s[i + 1..i + 1 + semi];
            if let Some(decoded) = decode_single_entity(entity) {
                out.push_str(&decoded);
                i = i + 1 + semi + 1;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn decode_single_entity(entity: &str) -> Option<String> {
    if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        let code = u32::from_str_radix(hex, 16).ok()?;
        return char::from_u32(code).map(|c| c.to_string());
    }
    if let Some(dec) = entity.strip_prefix('#') {
        let code = dec.parse::<u32>().ok()?;
        return char::from_u32(code).map(|c| c.to_string());
    }
    let mapped = match entity {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{00A0}",
        "ndash" => "–",
        "mdash" => "—",
        "hellip" => "…",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "laquo" => "«",
        "raquo" => "»",
        "emsp" => "\u{2003}",
        "ensp" => "\u{2002}",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "larr" => "←",
        "rarr" => "→",
        "harr" => "↔",
        _ => return None,
    };
    Some(mapped.to_string())
}

struct HtmlDocBuilder {
    blocks: Vec<Block>,
    current: Option<Block>,
    style_stack: Vec<InlineStyle>,
    list_stack: Vec<bool>,
    in_pre: bool,
    in_head_or_skip: usize,
    blockquote_depth: usize,
}

impl HtmlDocBuilder {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            current: None,
            style_stack: vec![InlineStyle::default()],
            list_stack: Vec::new(),
            in_pre: false,
            in_head_or_skip: 0,
            blockquote_depth: 0,
        }
    }

    fn current_style(&self) -> InlineStyle {
        self.style_stack.last().cloned().unwrap_or_default()
    }

    fn flush_current(&mut self) {
        if let Some(mut b) = self.current.take() {
            if b.inlines.len() == 1 && matches!(b.inlines[0], Inline::LineBreak) {
                b.inlines.clear();
            }
            b.normalize();
            self.blocks.push(b);
        }
    }

    fn ensure_block(&mut self) -> &mut Block {
        if self.current.is_none() {
            let kind = if self.blockquote_depth > 0 {
                BlockKind::Blockquote
            } else if let Some(&ordered) = self.list_stack.last() {
                let depth = (self.list_stack.len().saturating_sub(1)).min(4) as u8;
                if ordered {
                    BlockKind::OrderedListItem { depth, number: 1 }
                } else {
                    BlockKind::BulletListItem { depth }
                }
            } else {
                BlockKind::Paragraph
            };
            self.current = Some(Block::new(kind));
        }
        self.current.as_mut().unwrap()
    }

    fn consume_tokens(&mut self, tokens: &[HtmlToken]) {
        for tok in tokens {
            match tok {
                HtmlToken::StartTag {
                    name,
                    attrs,
                    self_closing,
                } => {
                    if matches!(
                        name.as_str(),
                        "head" | "script" | "style" | "title" | "meta"
                    ) {
                        if !*self_closing && !matches!(name.as_str(), "meta") {
                            self.in_head_or_skip += 1;
                        }
                        continue;
                    }
                    if self.in_head_or_skip > 0 {
                        continue;
                    }

                    let dir = extract_direction(attrs);

                    match name.as_str() {
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            self.flush_current();
                            let lvl = name.as_bytes()[1] - b'0';
                            self.current =
                                Some(Block::new(BlockKind::Heading(lvl)).with_direction(dir));
                        }
                        "p" | "div" => {
                            if self.current.is_some()
                                && self.current.as_ref().is_some_and(|b| !b.is_empty())
                            {
                                self.flush_current();
                            }
                            if self.current.is_none() {
                                let kind = if self.blockquote_depth > 0 {
                                    BlockKind::Blockquote
                                } else {
                                    BlockKind::Paragraph
                                };
                                self.current = Some(Block::new(kind).with_direction(dir));
                            } else if dir != TextDirection::Auto
                                && let Some(b) = self.current.as_mut()
                            {
                                b.direction = dir;
                            }
                        }
                        "blockquote" => {
                            self.flush_current();
                            self.blockquote_depth += 1;
                            self.current =
                                Some(Block::new(BlockKind::Blockquote).with_direction(dir));
                        }
                        "ul" => {
                            self.flush_current();
                            self.list_stack.push(false);
                        }
                        "ol" => {
                            self.flush_current();
                            self.list_stack.push(true);
                        }
                        "li" => {
                            self.flush_current();
                            let ordered = self.list_stack.last().copied().unwrap_or(false);
                            let depth = self.list_stack.len().saturating_sub(1).min(4) as u8;
                            let kind = if ordered {
                                BlockKind::OrderedListItem { depth, number: 1 }
                            } else {
                                BlockKind::BulletListItem { depth }
                            };
                            self.current = Some(Block::new(kind).with_direction(dir));
                        }
                        "pre" => {
                            self.flush_current();
                            self.in_pre = true;
                            self.current =
                                Some(Block::new(BlockKind::CodeBlock { language: None }));
                        }
                        "hr" => {
                            self.flush_current();
                            self.blocks.push(Block::new(BlockKind::HorizontalRule));
                        }
                        "br" => {
                            let b = self.ensure_block();
                            b.inlines.push(Inline::LineBreak);
                        }
                        "input" => {
                            let is_checkbox = attrs
                                .iter()
                                .any(|(k, v)| k == "type" && v.eq_ignore_ascii_case("checkbox"));
                            if is_checkbox {
                                let checked = attrs.iter().any(|(k, _)| k == "checked");
                                let b = self.ensure_block();
                                b.inlines.push(Inline::Checkbox { checked });
                            }
                        }
                        "img" => {
                            let is_comment = attrs.iter().any(|(k, v)| {
                                k == "class" && v.split_whitespace().any(|c| c == "comments")
                            });
                            if is_comment {
                                let text = attrs
                                    .iter()
                                    .find(|(k, _)| k == "data-comment" || k == "title")
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or_default();
                                let b = self.ensure_block();
                                b.inlines.push(Inline::Comment { text });
                            } else if let Some((_, src)) = attrs.iter().find(|(k, _)| k == "src")
                                && is_safe_img_src(src)
                            {
                                let alt = attrs
                                    .iter()
                                    .find(|(k, _)| k == "alt")
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or_default();
                                let b = self.ensure_block();
                                b.inlines.push(Inline::Image {
                                    src: src.clone(),
                                    alt,
                                    original_src: None,
                                });
                            }
                        }
                        "b" | "strong" => {
                            if !*self_closing {
                                let mut s = self.current_style();
                                s.bold = true;
                                self.style_stack.push(s);
                            }
                        }
                        "i" | "em" => {
                            if !*self_closing {
                                let mut s = self.current_style();
                                s.italic = true;
                                self.style_stack.push(s);
                            }
                        }
                        "u" => {
                            if !*self_closing {
                                let mut s = self.current_style();
                                s.underline = true;
                                self.style_stack.push(s);
                            }
                        }
                        "s" | "del" | "strike" => {
                            if !*self_closing {
                                let mut s = self.current_style();
                                s.strikethrough = true;
                                self.style_stack.push(s);
                            }
                        }
                        "code" => {
                            if self.in_pre {
                                if let Some((_, cls)) = attrs.iter().find(|(k, _)| k == "class")
                                    && let Some(lang) = cls
                                        .split_whitespace()
                                        .find_map(|c| c.strip_prefix("language-"))
                                    && let Some(Block {
                                        kind: BlockKind::CodeBlock { language },
                                        ..
                                    }) = self.current.as_mut()
                                {
                                    *language = Some(lang.to_string());
                                }
                            } else if !*self_closing {
                                let mut s = self.current_style();
                                s.code = true;
                                self.style_stack.push(s);
                            }
                        }
                        "a" if !*self_closing => {
                            let mut s = self.current_style();
                            if let Some((_, href)) = attrs.iter().find(|(k, _)| k == "href")
                                && is_safe_href(href)
                            {
                                s.link = Some(href.clone());
                            }
                            self.style_stack.push(s);
                        }
                        _ => {}
                    }
                }
                HtmlToken::EndTag(name) => {
                    if matches!(name.as_str(), "head" | "script" | "style" | "title") {
                        self.in_head_or_skip = self.in_head_or_skip.saturating_sub(1);
                        continue;
                    }
                    if self.in_head_or_skip > 0 {
                        continue;
                    }

                    match name.as_str() {
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div" | "li" => {
                            self.flush_current();
                        }
                        "blockquote" => {
                            self.flush_current();
                            self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                        }
                        "ul" | "ol" => {
                            self.flush_current();
                            self.list_stack.pop();
                        }
                        "pre" => {
                            self.flush_current();
                            self.in_pre = false;
                        }
                        "b" | "strong" | "i" | "em" | "u" | "s" | "del" | "strike" | "a" => {
                            if self.style_stack.len() > 1 {
                                self.style_stack.pop();
                            }
                        }
                        "code" if !self.in_pre && self.style_stack.len() > 1 => {
                            self.style_stack.pop();
                        }
                        _ => {}
                    }
                }
                HtmlToken::Text(text) => {
                    if self.in_head_or_skip > 0 {
                        continue;
                    }
                    if self.in_pre {
                        let cleaned = text.replace("\r\n", "\n").replace('\r', "\n");
                        let b = self.ensure_block();
                        let parts: Vec<&str> = cleaned.split('\n').collect();
                        for (idx, part) in parts.iter().enumerate() {
                            if !part.is_empty() {
                                b.inlines.push(Inline::Text(TextRun::plain(*part)));
                            }
                            if idx + 1 < parts.len() {
                                b.inlines.push(Inline::LineBreak);
                            }
                        }
                    } else {
                        if self.current.is_none() && text.trim().is_empty() {
                            continue;
                        }
                        let collapsed = collapse_html_whitespace(text);
                        if collapsed.is_empty() {
                            continue;
                        }
                        let style = self.current_style();
                        let b = self.ensure_block();
                        if b.inlines.is_empty() && collapsed == " " && text.contains('\n') {
                            continue;
                        }
                        b.inlines.push(Inline::Text(TextRun::new(collapsed, style)));
                    }
                }
                HtmlToken::Comment(body) => {
                    if self.in_head_or_skip > 0 {
                        continue;
                    }
                    let trimmed = body.trim();
                    if is_ignorable_html_comment(trimmed) {
                        continue;
                    }
                    let b = self.ensure_block();
                    b.inlines.push(Inline::Comment { text: body.clone() });
                }
            }
        }
    }

    fn finish(mut self) -> DocumentFragment {
        self.flush_current();
        for b in &mut self.blocks {
            b.normalize();
        }
        DocumentFragment {
            blocks: self.blocks,
        }
    }
}

fn is_ignorable_html_comment(trimmed: &str) -> bool {
    trimmed.is_empty()
        || matches!(
            trimmed,
            "StartFragment" | "EndFragment" | "$" | "/$" | "$!" | "$?" | "[]" | "/"
        )
        || trimmed.starts_with("?lit$")
        || trimmed.starts_with("lit-part")
        || trimmed.starts_with("bindings=")
        || trimmed.starts_with("ng-")
        || trimmed.starts_with("[if ")
        || trimmed.starts_with("[endif]")
        || trimmed.starts_with("![endif]")
}

fn collapse_html_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = ch == ' ';
        }
    }
    out
}

fn extract_direction(attrs: &[(String, String)]) -> TextDirection {
    for (k, v) in attrs {
        if k == "dir" {
            if v.eq_ignore_ascii_case("rtl") {
                return TextDirection::Rtl;
            }
            if v.eq_ignore_ascii_case("ltr") {
                return TextDirection::Ltr;
            }
        }
        if k == "style" {
            let lower = v.to_ascii_lowercase().replace(' ', "");
            if lower.contains("direction:rtl") {
                return TextDirection::Rtl;
            }
            if lower.contains("direction:ltr") {
                return TextDirection::Ltr;
            }
        }
    }
    TextDirection::Auto
}

fn is_safe_href(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    !lower.starts_with("javascript:") && !lower.starts_with("vbscript:")
}

fn is_safe_img_src(src: &str) -> bool {
    let lower = src.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("data:image/")
        || lower.starts_with("file://")
        || !lower.contains(':')
}
