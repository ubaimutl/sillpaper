use crate::config::{FontChoice, Preferences};
use crate::io::html::escape_attr;
use crate::model::{Block, BlockKind, Document, Inline, InlineStyle, TextDirection, TextRun};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

struct EmbeddedImage {
    rid: String,
    media_path: String,
    ext: &'static str,
    bytes: Vec<u8>,
    cx_emu: u64,
    cy_emu: u64,
}

pub fn document_to_docx_bytes(doc: &Document) -> Result<Vec<u8>, String> {
    document_to_docx_bytes_with_options(doc, None, None)
}

pub fn document_to_docx_bytes_with_base(
    doc: &Document,
    doc_dir: Option<&Path>,
) -> Result<Vec<u8>, String> {
    document_to_docx_bytes_with_options(doc, doc_dir, None)
}

pub fn document_to_docx_bytes_with_options(
    doc: &Document,
    doc_dir: Option<&Path>,
    prefs: Option<&Preferences>,
) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut cursor);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut link_rels: Vec<(String, String)> = Vec::new();
    let mut comments: Vec<(usize, String)> = Vec::new();
    let mut images: Vec<EmbeddedImage> = Vec::new();
    let mut rel_counter = 3usize;
    let mut doc_pr_counter = 1usize;

    let mut body_xml = String::new();
    for block in &doc.blocks {
        write_block_ooxml(
            &mut body_xml,
            block,
            doc_dir,
            &mut link_rels,
            &mut comments,
            &mut images,
            &mut rel_counter,
            &mut doc_pr_counter,
        );
    }

    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
  <w:body>
{body_xml}    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
    </w:sectPr>
  </w:body>
</w:document>"#
    );

    let styles_xml = build_styles_xml(prefs);

    let mut rels_entries = String::from(
        r#"  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
"#,
    );
    if !comments.is_empty() {
        rels_entries.push_str(
            "  <Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"comments.xml\"/>\n",
        );
    }
    for (rid, target) in &link_rels {
        rels_entries.push_str(&format!(
            "  <Relationship Id=\"{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>\n",
            escape_attr(&sanitize_xml(target))
        ));
    }
    for img in &images {
        rels_entries.push_str(&format!(
            "  <Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>\n",
            img.rid,
            img.media_path.trim_start_matches("word/")
        ));
    }

    let doc_rels_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
{rels_entries}</Relationships>"#
    );

    let mut extra_ct = String::new();
    if !comments.is_empty() {
        extra_ct.push_str(
            "  <Override PartName=\"/word/comments.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml\"/>\n",
        );
    }
    if images.iter().any(|i| i.ext == "png") {
        extra_ct.push_str("  <Default Extension=\"png\" ContentType=\"image/png\"/>\n");
    }
    if images.iter().any(|i| i.ext == "jpg") {
        extra_ct.push_str("  <Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>\n");
    }
    if images.iter().any(|i| i.ext == "gif") {
        extra_ct.push_str("  <Default Extension=\"gif\" ContentType=\"image/gif\"/>\n");
    }
    if images.iter().any(|i| i.ext == "webp") {
        extra_ct.push_str("  <Default Extension=\"webp\" ContentType=\"image/webp\"/>\n");
    }
    if images.iter().any(|i| i.ext == "bmp") {
        extra_ct.push_str("  <Default Extension=\"bmp\" ContentType=\"image/bmp\"/>\n");
    }

    let content_types_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
{extra_ct}  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>"#
    );

    let entries: &[(&str, &str)] = &[
        ("[Content_Types].xml", &content_types_xml),
        ("_rels/.rels", ROOT_RELS_XML),
        ("word/styles.xml", &styles_xml),
        ("word/numbering.xml", NUMBERING_XML),
        ("word/_rels/document.xml.rels", &doc_rels_xml),
        ("word/document.xml", &document_xml),
    ];

    for (path, content) in entries {
        zip.start_file(*path, opts)
            .map_err(|e| format!("DOCX zip start {path}: {e}"))?;
        zip.write_all(content.as_bytes())
            .map_err(|e| format!("DOCX zip write {path}: {e}"))?;
    }

    if !comments.is_empty() {
        let mut c_xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
"#,
        );
        for (cid, text) in &comments {
            c_xml.push_str(&format!(
                "  <w:comment w:id=\"{cid}\" w:author=\"Sillpaper\">\n    <w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>\n  </w:comment>\n",
                escape_xml_text(&sanitize_xml(text))
            ));
        }
        c_xml.push_str("</w:comments>");
        zip.start_file("word/comments.xml", opts)
            .map_err(|e| format!("DOCX zip start comments.xml: {e}"))?;
        zip.write_all(c_xml.as_bytes())
            .map_err(|e| format!("DOCX zip write comments.xml: {e}"))?;
    }

    for img in &images {
        zip.start_file(&img.media_path, opts)
            .map_err(|e| format!("DOCX zip start {}: {e}", img.media_path))?;
        zip.write_all(&img.bytes)
            .map_err(|e| format!("DOCX zip write {}: {e}", img.media_path))?;
    }

    zip.finish().map_err(|e| format!("DOCX zip finish: {e}"))?;
    Ok(cursor.into_inner())
}

#[allow(clippy::too_many_arguments)]
fn write_block_ooxml(
    out: &mut String,
    block: &Block,
    doc_dir: Option<&Path>,
    link_rels: &mut Vec<(String, String)>,
    comments: &mut Vec<(usize, String)>,
    images: &mut Vec<EmbeddedImage>,
    rel_counter: &mut usize,
    doc_pr_counter: &mut usize,
) {
    let is_rtl = match block.direction {
        TextDirection::Rtl => true,
        TextDirection::Ltr => false,
        TextDirection::Auto => has_rtl_base_dir(&block.plain_text()),
    };

    out.push_str("    <w:p>\n      <w:pPr>");
    match &block.kind {
        BlockKind::Heading(lvl) => {
            let l = (*lvl).clamp(1, 6);
            out.push_str(&format!("<w:pStyle w:val=\"Heading{l}\"/>"));
        }
        BlockKind::Blockquote => {
            out.push_str("<w:pStyle w:val=\"Quote\"/>");
            if is_rtl {
                out.push_str(
                    "<w:pBdr><w:left w:val=\"none\"/><w:right w:val=\"single\" w:sz=\"18\" w:space=\"12\" w:color=\"BDBDBD\"/></w:pBdr>",
                );
            }
        }
        BlockKind::BulletListItem { depth } => {
            out.push_str(&format!(
                "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"1\"/></w:numPr>",
                (*depth).min(4)
            ));
        }
        BlockKind::OrderedListItem { depth, .. } => {
            out.push_str(&format!(
                "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"2\"/></w:numPr>",
                (*depth).min(4)
            ));
        }
        BlockKind::CodeBlock { .. } => {
            out.push_str("<w:pStyle w:val=\"CodeBlock\"/>");
        }
        BlockKind::HorizontalRule => {
            out.push_str(
                "<w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"CCCCCC\"/></w:pBdr>",
            );
        }
        BlockKind::Paragraph => {}
    }

    if is_rtl {
        out.push_str("<w:bidi/>");
    }
    out.push_str("</w:pPr>\n");

    let is_code_block = matches!(block.kind, BlockKind::CodeBlock { .. });

    for (idx, inline) in block.inlines.iter().enumerate() {
        match inline {
            Inline::Text(run) => {
                if let Some(url) = &run.style.link {
                    *rel_counter += 1;
                    let rid = format!("rId{}", *rel_counter);
                    link_rels.push((rid.clone(), url.clone()));
                    out.push_str(&format!("      <w:hyperlink r:id=\"{rid}\">"));
                    write_run_ooxml(out, &run.text, &run.style, is_code_block, is_rtl, true);
                    out.push_str("</w:hyperlink>\n");
                } else {
                    out.push_str("      ");
                    write_run_ooxml(out, &run.text, &run.style, is_code_block, is_rtl, false);
                    out.push('\n');
                }
            }
            Inline::LineBreak => {
                out.push_str("      <w:r><w:br/></w:r>\n");
            }
            Inline::Checkbox { checked } => {
                let next_has_space = matches!(
                    block.inlines.get(idx + 1),
                    Some(Inline::Text(r)) if r.text.starts_with(' ')
                );
                let sym = match (*checked, next_has_space) {
                    (true, true) => "☑",
                    (true, false) => "☑ ",
                    (false, true) => "☐",
                    (false, false) => "☐ ",
                };
                out.push_str(&format!(
                    "      <w:r><w:t xml:space=\"preserve\">{sym}</w:t></w:r>\n"
                ));
            }
            Inline::Comment { text } => {
                let cid = comments.len();
                comments.push((cid, text.clone()));
                out.push_str(&format!(
                    "      <w:commentRangeStart w:id=\"{cid}\"/><w:commentRangeEnd w:id=\"{cid}\"/><w:r><w:commentReference w:id=\"{cid}\"/></w:r>\n"
                ));
            }
            Inline::Image { src, alt, .. } => {
                if let Some(emb) =
                    load_local_image_for_docx(src, doc_dir, rel_counter, images.len() + 1)
                {
                    let pr_id = *doc_pr_counter;
                    *doc_pr_counter += 1;
                    let name = if alt.is_empty() {
                        format!("Picture {pr_id}")
                    } else {
                        sanitize_xml(alt)
                    };
                    out.push_str(&format!(
                        r#"      <w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{pr_id}" name="{name}"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{pr_id}" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
"#,
                        cx = emb.cx_emu,
                        cy = emb.cy_emu,
                        pr_id = pr_id,
                        name = escape_attr(&name),
                        rid = emb.rid,
                    ));
                    images.push(emb);
                }
            }
        }
    }

    out.push_str("    </w:p>\n");
}

fn load_local_image_for_docx(
    src: &str,
    doc_dir: Option<&Path>,
    rel_counter: &mut usize,
    img_idx: usize,
) -> Option<EmbeddedImage> {
    let bytes = if let Some((_, data)) = decode_data_uri_image(src) {
        data
    } else {
        let path = resolve_local_image_path(src, doc_dir)?;
        std::fs::read(&path).ok()?
    };
    let (ext, w_px, h_px) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]).max(1);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]).max(1);
        ("png", w, h)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        let (w, h) = jpeg_dimensions(&bytes).unwrap_or((640, 480));
        ("jpg", w.max(1), h.max(1))
    } else if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        ("gif", w.max(1), h.max(1))
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let (w, h) = webp_dimensions(&bytes).unwrap_or((640, 480));
        ("webp", w.max(1), h.max(1))
    } else if bytes.starts_with(b"BM") && bytes.len() >= 26 {
        let w = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]).unsigned_abs();
        let h = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]).unsigned_abs();
        ("bmp", w.max(1), h.max(1))
    } else {
        return None;
    };

    let emu_per_px = 9525.0_f64;
    let mut cx = w_px as f64 * emu_per_px;
    let mut cy = h_px as f64 * emu_per_px;
    let max_w = 5_580_000.0_f64;
    let max_h = 7_200_000.0_f64;
    if cx > max_w {
        let s = max_w / cx;
        cx *= s;
        cy *= s;
    }
    if cy > max_h {
        let s = max_h / cy;
        cx *= s;
        cy *= s;
    }

    *rel_counter += 1;
    let rid = format!("rId{}", *rel_counter);
    let media_path = format!("word/media/image{img_idx}.{ext}");
    Some(EmbeddedImage {
        rid,
        media_path,
        ext,
        bytes,
        cx_emu: cx.round() as u64,
        cy_emu: cy.round() as u64,
    })
}

pub fn resolve_local_image_path(src: &str, doc_dir: Option<&Path>) -> Option<PathBuf> {
    let p = if let Some(rest) = src.strip_prefix("file://") {
        PathBuf::from(rest)
    } else if !src.contains(':') {
        doc_dir
            .map(|d| d.join(src))
            .unwrap_or_else(|| PathBuf::from(src))
    } else {
        return None;
    };
    if p.is_file() { Some(p) } else { None }
}

fn jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2usize;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;
        if marker == 0xD8 || marker == 0xD9 {
            continue;
        }
        if i + 2 > data.len() {
            break;
        }
        let seg_len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
        if matches!(marker, 0xC0..=0xC2) && i + 7 < data.len() {
            let h = u16::from_be_bytes([data[i + 3], data[i + 4]]) as u32;
            let w = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            return Some((w, h));
        }
        if seg_len < 2 {
            break;
        }
        i += seg_len;
    }
    None
}

fn write_run_ooxml(
    out: &mut String,
    text: &str,
    style: &InlineStyle,
    is_code_block: bool,
    is_rtl: bool,
    is_link: bool,
) {
    out.push_str("<w:r><w:rPr>");
    if style.code || is_code_block {
        out.push_str("<w:rFonts w:ascii=\"Liberation Mono\" w:hAnsi=\"Liberation Mono\" w:cs=\"Liberation Mono\"/>");
    }
    if style.bold {
        out.push_str("<w:b/><w:bCs/>");
    }
    if style.italic {
        out.push_str("<w:i/><w:iCs/>");
    }
    if style.strikethrough {
        out.push_str("<w:strike/>");
    }
    if is_link {
        out.push_str("<w:color w:val=\"1A6FB5\"/>");
    }
    if style.underline || is_link {
        out.push_str("<w:u w:val=\"single\"/>");
    }
    if style.code && !is_code_block {
        out.push_str("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"EFEFEF\"/>");
    }
    if is_rtl {
        out.push_str("<w:rtl/>");
    }
    out.push_str("</w:rPr>");
    if text.contains('\t') {
        for (idx, seg) in text.split('\t').enumerate() {
            if idx > 0 {
                out.push_str("<w:tab/>");
            }
            if !seg.is_empty() {
                out.push_str("<w:t xml:space=\"preserve\">");
                out.push_str(&escape_xml_text(&sanitize_xml(seg)));
                out.push_str("</w:t>");
            }
        }
    } else {
        out.push_str("<w:t xml:space=\"preserve\">");
        out.push_str(&escape_xml_text(&sanitize_xml(text)));
        out.push_str("</w:t>");
    }
    out.push_str("</w:r>");
}

fn has_rtl_base_dir(s: &str) -> bool {
    for ch in s.chars() {
        let cp = ch as u32;
        if (0x0590..=0x08FF).contains(&cp)
            || (0xFB1D..=0xFDFF).contains(&cp)
            || (0xFE70..=0xFEFF).contains(&cp)
        {
            return true;
        }
        if ch.is_alphabetic() {
            return false;
        }
    }
    false
}

fn sanitize_xml(s: &str) -> String {
    s.chars()
        .filter(|&c| matches!(c, '\t' | '\n' | '\r' | '\u{0020}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'))
        .collect()
}

fn escape_xml_text(s: &str) -> String {
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

fn webp_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 30 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }
    match &data[12..16] {
        b"VP8X" if data.len() >= 30 => {
            let w = 1 + u32::from_le_bytes([data[24], data[25], data[26], 0]);
            let h = 1 + u32::from_le_bytes([data[27], data[28], data[29], 0]);
            Some((w, h))
        }
        b"VP8L" if data.len() >= 25 && data[20] == 0x2F => {
            let bits = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
            let w = (bits & 0x3FFF) + 1;
            let h = ((bits >> 14) & 0x3FFF) + 1;
            Some((w, h))
        }
        b"VP8 " if data.len() >= 30 && data[23..26] == [0x9D, 0x01, 0x2A] => {
            let w = (u16::from_le_bytes([data[26], data[27]]) & 0x3FFF) as u32;
            let h = (u16::from_le_bytes([data[28], data[29]]) & 0x3FFF) as u32;
            Some((w, h))
        }
        _ => None,
    }
}

pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn decode_data_uri_image(src: &str) -> Option<(String, Vec<u8>)> {
    let rest = src.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mut parts = meta.split(';');
    let mime = parts.next()?.trim();
    if !mime.to_ascii_lowercase().starts_with("image/") {
        return None;
    }
    let is_b64 = parts.any(|p| p.trim().eq_ignore_ascii_case("base64"));
    if !is_b64 {
        return None;
    }
    let bytes = base64_decode(payload)?;
    Some((mime.to_ascii_lowercase(), bytes))
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = [0u8; 4];
    let mut count = 0usize;
    let mut pad = 0usize;

    for b in input.bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => {
                pad += 1;
                0
            }
            _ => return None,
        };
        buf[count] = val;
        count += 1;
        if count == 4 {
            let n = ((buf[0] as u32) << 18)
                | ((buf[1] as u32) << 12)
                | ((buf[2] as u32) << 6)
                | (buf[3] as u32);
            out.push(((n >> 16) & 0xFF) as u8);
            if pad < 2 {
                out.push(((n >> 8) & 0xFF) as u8);
            }
            if pad < 1 {
                out.push((n & 0xFF) as u8);
            }
            count = 0;
            pad = 0;
        }
    }
    if count >= 2 {
        let n = ((buf[0] as u32) << 18)
            | ((buf[1] as u32) << 12)
            | if count > 2 { (buf[2] as u32) << 6 } else { 0 };
        out.push(((n >> 16) & 0xFF) as u8);
        if count == 3 {
            out.push(((n >> 8) & 0xFF) as u8);
        }
    }
    Some(out)
}

pub fn docx_bytes_to_document(bytes: &[u8]) -> Result<Document, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid DOCX zip: {e}"))?;

    let content_types = read_zip_xml_tree(&mut archive, "[Content_Types].xml")
        .map(|root| ContentTypes::from_xml(&root))
        .unwrap_or_default();

    let rels = read_zip_xml_tree(&mut archive, "word/_rels/document.xml.rels")
        .map(|root| Relationships::from_xml(&root))
        .unwrap_or_default();

    let styles = read_zip_xml_tree(&mut archive, "word/styles.xml")
        .map(|root| DocxStyles::from_xml(&root))
        .unwrap_or_default();

    let numbering = read_zip_xml_tree(&mut archive, "word/numbering.xml")
        .map(|root| DocxNumbering::from_xml(&root))
        .unwrap_or_default();

    let comments = read_zip_xml_tree(&mut archive, "word/comments.xml")
        .map(|root| parse_comments_tree(&root))
        .unwrap_or_default();

    let doc_xml = read_zip_text(&mut archive, "word/document.xml")
        .ok_or_else(|| "DOCX missing word/document.xml".to_string())?;
    let Some(doc_root) = parse_xml_tree(&doc_xml)? else {
        return Ok(Document::default());
    };
    let doc_root = collapse_alternate_content_root(doc_root);

    let mut reader = DocxBodyReader {
        rels: &rels,
        content_types: &content_types,
        styles: &styles,
        numbering: &numbering,
        comments: &comments,
        archive: &mut archive,
        image_cache: HashMap::new(),
        complex_field_stack: Vec::new(),
        current_instr_text: String::new(),
        deleted_paragraph_inlines: Vec::new(),
    };

    let body = doc_root.first_child("body").unwrap_or(&doc_root);
    let blocks = reader.read_block_children(&body.children);
    Ok(Document::from_blocks(blocks))
}

fn read_zip_text(archive: &mut ZipArchive<Cursor<&[u8]>>, path: &str) -> Option<String> {
    let mut file = archive.by_name(path).ok()?;
    let mut out = String::new();
    file.read_to_string(&mut out).ok()?;
    if out.starts_with('\u{FEFF}') {
        out.remove(0);
    }
    Some(out)
}

fn read_zip_xml_tree(archive: &mut ZipArchive<Cursor<&[u8]>>, path: &str) -> Option<XmlElement> {
    let xml = read_zip_text(archive, path)?;
    parse_xml_tree(&xml).ok().flatten()
}

#[derive(Debug, Clone)]
enum XmlNode {
    Element(XmlElement),
    Text(String),
}

#[derive(Debug, Clone)]
struct XmlElement {
    name: String,
    attrs: Vec<(Option<String>, String, String)>,
    children: Vec<XmlNode>,
}

impl XmlElement {
    fn attr(&self, target_local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(_, l, _)| l == target_local)
            .map(|(_, _, v)| v.as_str())
    }

    fn attr_prefixed(&self, prefix: &str, target_local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(p, l, _)| l == target_local && p.as_deref() == Some(prefix))
            .map(|(_, _, v)| v.as_str())
            .or_else(|| self.attr(target_local))
    }

    fn first_child(&self, local: &str) -> Option<&XmlElement> {
        self.children.iter().find_map(|n| match n {
            XmlNode::Element(el) if el.name == local => Some(el),
            _ => None,
        })
    }

    fn child_elements(&self) -> impl Iterator<Item = &XmlElement> {
        self.children.iter().filter_map(|n| match n {
            XmlNode::Element(el) => Some(el),
            _ => None,
        })
    }

    fn collect_descendants<'a>(&'a self, local: &str, out: &mut Vec<&'a XmlElement>) {
        for child in self.child_elements() {
            if child.name == local {
                out.push(child);
            }
            child.collect_descendants(local, out);
        }
    }

    fn text_content(&self) -> String {
        let mut out = String::new();
        self.append_text(&mut out);
        out
    }

    fn append_text(&self, out: &mut String) {
        for child in &self.children {
            match child {
                XmlNode::Text(t) => out.push_str(t),
                XmlNode::Element(el) => el.append_text(out),
            }
        }
    }
}

fn parse_xml_tree(xml: &str) -> Result<Option<XmlElement>, String> {
    let trimmed = xml.trim_start_matches('\u{FEFF}');
    let mut reader = Reader::from_str(trimmed);
    let mut buf = Vec::new();
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut root: Option<XmlElement> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                stack.push(build_xml_element(&e));
            }
            Ok(Event::Empty(e)) => {
                let el = build_xml_element(&e);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(XmlNode::Element(el));
                } else {
                    root = Some(el);
                }
            }
            Ok(Event::Text(e)) => {
                if let Some(parent) = stack.last_mut() {
                    let txt: &str = e.as_ref();
                    if !txt.is_empty() {
                        push_text_node(&mut parent.children, txt);
                    }
                }
            }
            Ok(Event::CData(e)) => {
                if let Some(parent) = stack.last_mut() {
                    let txt: &str = e.as_ref();
                    if !txt.is_empty() {
                        push_text_node(&mut parent.children, txt);
                    }
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if let Some(parent) = stack.last_mut() {
                    let resolved = resolve_general_ref(&e);
                    if !resolved.is_empty() {
                        push_text_node(&mut parent.children, &resolved);
                    }
                }
            }
            Ok(Event::End(_)) => {
                if let Some(el) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlNode::Element(el));
                    } else {
                        root = Some(el);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("DOCX XML parse error: {e}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(root)
}

fn build_xml_element(e: &quick_xml::events::BytesStart<'_>) -> XmlElement {
    let qname = e.name();
    let (_, local) = split_qname(qname.as_ref());
    let mut attrs = Vec::new();
    for attr in e.attributes().flatten() {
        let (prefix, alocal) = split_qname(attr.key.as_ref());
        if alocal == "gfxdata" {
            continue;
        }
        if let Ok(val) = attr.normalized_value(XmlVersion::Implicit1_0) {
            attrs.push((
                prefix.map(str::to_string),
                alocal.to_string(),
                val.into_owned(),
            ));
        }
    }
    XmlElement {
        name: local.to_string(),
        attrs,
        children: Vec::new(),
    }
}

fn push_text_node(children: &mut Vec<XmlNode>, text: &str) {
    if let Some(XmlNode::Text(existing)) = children.last_mut() {
        existing.push_str(text);
    } else {
        children.push(XmlNode::Text(text.to_string()));
    }
}

fn split_qname(full: &str) -> (Option<&str>, &str) {
    match full.rsplit_once(':') {
        Some((p, l)) => (Some(p), l),
        None => (None, full),
    }
}

fn collapse_alternate_content_root(mut el: XmlElement) -> XmlElement {
    el.children = collapse_nodes(el.children);
    el
}

fn collapse_nodes(nodes: Vec<XmlNode>) -> Vec<XmlNode> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            XmlNode::Text(t) => out.push(XmlNode::Text(t)),
            XmlNode::Element(mut el) => {
                if el.name == "AlternateContent" {
                    let chosen = select_alternate_content_branch(el.children);
                    out.extend(collapse_nodes(chosen));
                } else {
                    el.children = collapse_nodes(el.children);
                    out.push(XmlNode::Element(el));
                }
            }
        }
    }
    out
}

fn select_alternate_content_branch(children: Vec<XmlNode>) -> Vec<XmlNode> {
    let mut fallback: Option<XmlElement> = None;
    let mut first_choice: Option<XmlElement> = None;
    for node in children {
        if let XmlNode::Element(el) = node {
            if el.name == "Fallback" && fallback.is_none() {
                fallback = Some(el);
            } else if el.name == "Choice" && first_choice.is_none() {
                first_choice = Some(el);
            }
        }
    }
    match (fallback, first_choice) {
        (Some(fb), Some(ch)) => {
            if element_has_substantive_content(&fb) || !element_has_substantive_content(&ch) {
                fb.children
            } else {
                ch.children
            }
        }
        (Some(fb), None) => fb.children,
        (None, Some(ch)) => ch.children,
        (None, None) => Vec::new(),
    }
}

fn element_has_substantive_content(el: &XmlElement) -> bool {
    if matches!(
        el.name.as_str(),
        "t" | "blip" | "imagedata" | "txbxContent" | "p" | "tbl"
    ) {
        return true;
    }
    el.child_elements().any(element_has_substantive_content)
}

#[derive(Default)]
struct ContentTypes {
    defaults: HashMap<String, String>,
    overrides: HashMap<String, String>,
}

impl ContentTypes {
    fn from_xml(root: &XmlElement) -> Self {
        let mut ct = Self::default();
        for el in root.child_elements() {
            if el.name == "Default" {
                if let (Some(ext), Some(mime)) = (el.attr("Extension"), el.attr("ContentType")) {
                    ct.defaults
                        .insert(ext.to_ascii_lowercase(), mime.to_ascii_lowercase());
                }
            } else if el.name == "Override"
                && let (Some(part), Some(mime)) = (el.attr("PartName"), el.attr("ContentType"))
            {
                let norm = part.trim_start_matches('/').to_ascii_lowercase();
                ct.overrides.insert(norm, mime.to_ascii_lowercase());
            }
        }
        ct
    }

    fn content_type_for(&self, zip_path: &str, bytes: &[u8]) -> Option<String> {
        let norm = zip_path.trim_start_matches('/').to_ascii_lowercase();
        if let Some(m) = self.overrides.get(&norm)
            && is_supported_image_mime(m)
        {
            return Some(m.clone());
        }
        if let Some((_, ext)) = norm.rsplit_once('.')
            && let Some(m) = self.defaults.get(ext)
            && is_supported_image_mime(m)
        {
            return Some(m.clone());
        }
        sniff_image_mime(bytes).map(str::to_string)
    }
}

fn is_supported_image_mime(mime: &str) -> bool {
    matches!(
        mime,
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/bmp"
            | "image/svg+xml"
            | "image/tiff"
    )
}

fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else if bytes
        .iter()
        .take(256)
        .copied()
        .collect::<Vec<_>>()
        .windows(4)
        .any(|w| w.eq_ignore_ascii_case(b"<svg"))
    {
        Some("image/svg+xml")
    } else {
        None
    }
}

#[derive(Default)]
struct Relationships {
    by_id: HashMap<String, RelEntry>,
}

struct RelEntry {
    target: String,
    external: bool,
}

impl Relationships {
    fn from_xml(root: &XmlElement) -> Self {
        let mut rels = Self::default();
        for el in root.child_elements() {
            if el.name == "Relationship"
                && let (Some(id), Some(target)) = (el.attr("Id"), el.attr("Target"))
            {
                let external = el
                    .attr("TargetMode")
                    .is_some_and(|m| m.eq_ignore_ascii_case("External"));
                rels.by_id.insert(
                    id.to_string(),
                    RelEntry {
                        target: target.to_string(),
                        external,
                    },
                );
            }
        }
        rels
    }

    fn get(&self, id: &str) -> Option<&RelEntry> {
        self.by_id.get(id)
    }
}

fn resolve_word_zip_path(target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut segments: Vec<&str> = vec!["word"];
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    segments.join("/")
}

fn percent_decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            out.push((h << 4) | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[derive(Default)]
struct DocxStyles {
    paragraph: HashMap<String, ParagraphStyleDef>,
    character: HashMap<String, CharacterStyleDef>,
    numbering: HashMap<String, String>,
}

struct ParagraphStyleDef {
    name: Option<String>,
    based_on: Option<String>,
    num_id: Option<String>,
    ilvl: Option<u8>,
    bidi: Option<bool>,
}

struct CharacterStyleDef {
    name: Option<String>,
    based_on: Option<String>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strikethrough: Option<bool>,
    code: Option<bool>,
}

impl DocxStyles {
    fn from_xml(root: &XmlElement) -> Self {
        let mut out = Self::default();
        for el in root.child_elements() {
            if el.name != "style" {
                continue;
            }
            let Some(style_id) = el.attr("styleId") else {
                continue;
            };
            let style_type = el.attr("type").unwrap_or("");
            let name = el
                .first_child("name")
                .and_then(|n| n.attr("val"))
                .map(str::to_string);
            let based_on = el
                .first_child("basedOn")
                .and_then(|n| n.attr("val"))
                .map(str::to_string);

            match style_type {
                "paragraph" => {
                    let ppr = el.first_child("pPr");
                    let num_pr = ppr.and_then(|p| p.first_child("numPr"));
                    let num_id = num_pr
                        .and_then(|n| n.first_child("numId"))
                        .and_then(|n| n.attr("val"))
                        .map(str::to_string);
                    let ilvl = num_pr
                        .and_then(|n| n.first_child("ilvl"))
                        .and_then(|n| n.attr("val"))
                        .and_then(|v| v.parse::<u8>().ok());
                    let bidi = ppr.and_then(|p| p.first_child("bidi")).map(read_ooxml_bool);
                    out.paragraph.insert(
                        style_id.to_string(),
                        ParagraphStyleDef {
                            name,
                            based_on,
                            num_id,
                            ilvl,
                            bidi,
                        },
                    );
                }
                "character" => {
                    let rpr = el.first_child("rPr");
                    let bold = rpr.and_then(|r| r.first_child("b")).map(read_ooxml_bool);
                    let italic = rpr.and_then(|r| r.first_child("i")).map(read_ooxml_bool);
                    let underline = rpr
                        .and_then(|r| r.first_child("u"))
                        .map(read_ooxml_underline);
                    let strikethrough = rpr
                        .and_then(|r| r.first_child("strike").or_else(|| r.first_child("dstrike")))
                        .map(read_ooxml_bool);
                    let code = rpr.and_then(|r| r.first_child("rFonts")).and_then(|f| {
                        let font = f.attr("ascii").or_else(|| f.attr("hAnsi"))?;
                        is_monospace_font(font).then_some(true)
                    });
                    out.character.insert(
                        style_id.to_string(),
                        CharacterStyleDef {
                            name,
                            based_on,
                            bold,
                            italic,
                            underline,
                            strikethrough,
                            code,
                        },
                    );
                }
                "numbering" => {
                    if let Some(num_id) = el
                        .first_child("pPr")
                        .and_then(|p| p.first_child("numPr"))
                        .and_then(|n| n.first_child("numId"))
                        .and_then(|n| n.attr("val"))
                    {
                        out.numbering
                            .insert(style_id.to_string(), num_id.to_string());
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn resolve_block_kind(&self, style_id: &str) -> Option<BlockKind> {
        let mut cur = Some(style_id);
        for _ in 0..16 {
            let sid = cur?;
            if let Some(kind) = classify_paragraph_style_label(sid) {
                return Some(kind);
            }
            let def = self.paragraph.get(sid)?;
            if let Some(name) = def.name.as_deref()
                && let Some(kind) = classify_paragraph_style_label(name)
            {
                return Some(kind);
            }
            cur = def.based_on.as_deref();
        }
        None
    }

    fn resolve_bidi(&self, style_id: &str) -> Option<bool> {
        let mut cur = Some(style_id);
        for _ in 0..16 {
            let def = self.paragraph.get(cur?)?;
            if let Some(b) = def.bidi {
                return Some(b);
            }
            cur = def.based_on.as_deref();
        }
        None
    }

    fn resolve_character_style(&self, style_id: &str) -> InlineStyle {
        let mut chain: Vec<(&str, Option<&CharacterStyleDef>)> = Vec::new();
        let mut cur = Some(style_id);
        for _ in 0..16 {
            let Some(sid) = cur else {
                break;
            };
            let def = self.character.get(sid);
            chain.push((sid, def));
            cur = def.and_then(|d| d.based_on.as_deref());
        }
        let mut style = InlineStyle::default();
        for (sid, def) in chain.into_iter().rev() {
            let lower_id = sid.to_ascii_lowercase();
            if lower_id == "strong" {
                style.bold = true;
            } else if lower_id == "emphasis" {
                style.italic = true;
            } else if lower_id.contains("code") || lower_id == "verbatimchar" {
                style.code = true;
            }
            if let Some(d) = def {
                if let Some(name) = d.name.as_deref() {
                    let lname = name.to_ascii_lowercase();
                    if lname == "strong" {
                        style.bold = true;
                    } else if lname == "emphasis" {
                        style.italic = true;
                    } else if lname == "code" || lname == "verbatim char" || lname == "inline code"
                    {
                        style.code = true;
                    }
                }
                if let Some(b) = d.bold {
                    style.bold = b;
                }
                if let Some(i) = d.italic {
                    style.italic = i;
                }
                if let Some(u) = d.underline {
                    style.underline = u;
                }
                if let Some(s) = d.strikethrough {
                    style.strikethrough = s;
                }
                if let Some(c) = d.code {
                    style.code = c;
                }
            }
        }
        style
    }
}

fn classify_paragraph_style_label(raw: &str) -> Option<BlockKind> {
    let lower = raw.trim().to_ascii_lowercase();
    for prefix in [
        "heading ",
        "heading",
        "überschrift ",
        "überschrift",
        "berschrift ",
        "berschrift",
        "titre ",
        "titre",
        "título ",
        "título",
        "titulo ",
        "titulo",
        "encabezado ",
        "encabezado",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix)
            && let Ok(num) = rest.trim().parse::<u8>()
            && (1..=6).contains(&num)
        {
            return Some(BlockKind::Heading(num));
        }
    }
    if matches!(lower.as_str(), "title" | "titel") {
        return Some(BlockKind::Heading(1));
    }
    if matches!(lower.as_str(), "subtitle" | "untertitel") {
        return Some(BlockKind::Heading(2));
    }
    if lower.contains("quote") || lower == "zitat" || lower == "citation" {
        return Some(BlockKind::Blockquote);
    }
    if matches!(
        lower.as_str(),
        "codeblock"
            | "code block"
            | "code"
            | "source text"
            | "preformatted text"
            | "html preformatted"
    ) {
        return Some(BlockKind::CodeBlock { language: None });
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Ordered,
    None,
}

#[derive(Debug, Clone, Copy)]
struct NumLevel {
    kind: ListKind,
    depth: u8,
}

#[derive(Default)]
struct AbstractNumDef {
    levels: HashMap<u8, NumLevel>,
    num_style_link: Option<String>,
}

#[derive(Default)]
struct NumDef {
    abstract_num_id: String,
    lvl_overrides: HashMap<u8, NumLevel>,
}

#[derive(Default)]
struct DocxNumbering {
    abstract_nums: HashMap<String, AbstractNumDef>,
    nums: HashMap<String, NumDef>,
    levels_by_pstyle: HashMap<String, NumLevel>,
}

impl DocxNumbering {
    fn from_xml(root: &XmlElement) -> Self {
        let mut out = Self::default();
        for el in root.child_elements() {
            if el.name == "abstractNum" {
                let Some(aid) = el.attr("abstractNumId") else {
                    continue;
                };
                let mut def = AbstractNumDef {
                    num_style_link: el
                        .first_child("numStyleLink")
                        .and_then(|n| n.attr("val"))
                        .map(str::to_string),
                    ..Default::default()
                };
                for child in el.child_elements() {
                    if child.name == "lvl"
                        && let Some((ilvl, level, p_style)) = parse_numbering_lvl(child)
                    {
                        def.levels.insert(ilvl, level);
                        if let Some(ps) = p_style {
                            out.levels_by_pstyle.insert(ps, level);
                        }
                    }
                }
                out.abstract_nums.insert(aid.to_string(), def);
            } else if el.name == "num" {
                let Some(nid) = el.attr("numId") else {
                    continue;
                };
                let Some(aid) = el.first_child("abstractNumId").and_then(|n| n.attr("val")) else {
                    continue;
                };
                let mut def = NumDef {
                    abstract_num_id: aid.to_string(),
                    lvl_overrides: HashMap::new(),
                };
                for child in el.child_elements() {
                    if child.name == "lvlOverride"
                        && let Some(lvl_el) = child.first_child("lvl")
                        && let Some((ilvl, level, _)) = parse_numbering_lvl(lvl_el)
                    {
                        def.lvl_overrides.insert(ilvl, level);
                    }
                }
                out.nums.insert(nid.to_string(), def);
            }
        }
        out
    }

    fn find_level(
        &self,
        num_id: &str,
        ilvl: u8,
        styles: &DocxStyles,
        guard: u8,
    ) -> Option<NumLevel> {
        if guard == 0 || num_id == "0" {
            return None;
        }
        let num = self.nums.get(num_id)?;
        if let Some(&ov) = num.lvl_overrides.get(&ilvl) {
            return Some(ov);
        }
        let abs = self.abstract_nums.get(&num.abstract_num_id)?;
        if let Some(link) = abs.num_style_link.as_deref()
            && let Some(target_num_id) = styles.numbering.get(link)
        {
            return self.find_level(target_num_id, ilvl, styles, guard - 1);
        }
        abs.levels.get(&ilvl).copied()
    }

    fn resolve_for_paragraph(
        &self,
        direct_num_id: Option<&str>,
        direct_ilvl: Option<u8>,
        p_style_id: Option<&str>,
        styles: &DocxStyles,
    ) -> Option<NumLevel> {
        if let Some(nid) = direct_num_id {
            if nid == "0" {
                return None;
            }
            let level = direct_ilvl.unwrap_or(0);
            return self.find_level(nid, level, styles, 8).or(Some(NumLevel {
                kind: ListKind::Bullet,
                depth: level,
            }));
        }

        let mut cur = p_style_id;
        for _ in 0..16 {
            let sid = cur?;
            if let Some(def) = styles.paragraph.get(sid)
                && def.num_id.as_deref() == Some("0")
            {
                return None;
            }
            if let Some(&lvl) = self.levels_by_pstyle.get(sid) {
                return Some(lvl);
            }
            if let Some(def) = styles.paragraph.get(sid) {
                if let Some(nid) = def.num_id.as_deref() {
                    let level = direct_ilvl.or(def.ilvl).unwrap_or(0);
                    return self.find_level(nid, level, styles, 8);
                }
                cur = def.based_on.as_deref();
            } else {
                break;
            }
        }
        None
    }
}

fn parse_numbering_lvl(el: &XmlElement) -> Option<(u8, NumLevel, Option<String>)> {
    let ilvl = el.attr("ilvl").and_then(|v| v.parse::<u8>().ok())?;
    let num_fmt = el
        .first_child("numFmt")
        .and_then(|n| n.attr("val"))
        .unwrap_or("decimal");
    let kind = match num_fmt {
        "bullet" => ListKind::Bullet,
        "none" => ListKind::None,
        _ => ListKind::Ordered,
    };
    let p_style = el
        .first_child("pStyle")
        .and_then(|n| n.attr("val"))
        .map(str::to_string);
    Some((ilvl, NumLevel { kind, depth: ilvl }, p_style))
}

fn parse_comments_tree(root: &XmlElement) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for el in root.child_elements() {
        if el.name == "comment"
            && let Some(id) = el.attr("id")
        {
            let mut ts = Vec::new();
            el.collect_descendants("t", &mut ts);
            let mut text = String::new();
            for t in ts {
                text.push_str(&t.text_content());
            }
            out.insert(id.to_string(), text);
        }
    }
    out
}

enum ComplexField {
    Begin { checked: Option<bool> },
    Hyperlink { href: String },
    Checkbox { checked: bool },
    Unknown,
}

struct DocxBodyReader<'a> {
    rels: &'a Relationships,
    content_types: &'a ContentTypes,
    styles: &'a DocxStyles,
    numbering: &'a DocxNumbering,
    comments: &'a HashMap<String, String>,
    archive: &'a mut ZipArchive<Cursor<&'a [u8]>>,
    image_cache: HashMap<String, Option<String>>,
    complex_field_stack: Vec<ComplexField>,
    current_instr_text: String,
    deleted_paragraph_inlines: Vec<Inline>,
}

impl<'a> DocxBodyReader<'a> {
    fn read_block_children(&mut self, nodes: &[XmlNode]) -> Vec<Block> {
        let mut blocks = Vec::new();
        for node in nodes {
            if let XmlNode::Element(el) = node {
                self.read_block_element(el, &mut blocks);
            }
        }
        blocks
    }

    fn read_block_element(&mut self, el: &XmlElement, out: &mut Vec<Block>) {
        match el.name.as_str() {
            "p" => {
                let mut produced = self.read_paragraph(el);
                out.append(&mut produced);
            }
            "tbl" => {
                let mut produced = self.read_table(el);
                out.append(&mut produced);
            }
            "sdt" => {
                if let Some(content) = el.first_child("sdtContent") {
                    let mut produced = self.read_block_children(&content.children);
                    out.append(&mut produced);
                }
            }
            "ins" | "smartTag" | "customXml" | "txbxContent" => {
                let mut produced = self.read_block_children(&el.children);
                out.append(&mut produced);
            }
            _ => {}
        }
    }

    fn read_paragraph(&mut self, el: &XmlElement) -> Vec<Block> {
        let ppr = el.first_child("pPr");

        let is_deleted_para = ppr
            .and_then(|p| p.first_child("rPr"))
            .and_then(|r| r.first_child("del"))
            .is_some();

        let mut inlines = Vec::new();
        let mut extra_blocks = Vec::new();
        self.read_inline_nodes(&el.children, None, &mut inlines, &mut extra_blocks);

        if is_deleted_para {
            self.deleted_paragraph_inlines.append(&mut inlines);
            return extra_blocks;
        }

        if !self.deleted_paragraph_inlines.is_empty() {
            let mut merged = std::mem::take(&mut self.deleted_paragraph_inlines);
            merged.append(&mut inlines);
            inlines = merged;
        }

        let p_style = ppr
            .and_then(|p| p.first_child("pStyle"))
            .and_then(|s| s.attr("val"));
        let num_pr = ppr.and_then(|p| p.first_child("numPr"));
        let direct_num_id = num_pr
            .and_then(|n| n.first_child("numId"))
            .and_then(|n| n.attr("val"));
        let direct_ilvl = num_pr
            .and_then(|n| n.first_child("ilvl"))
            .and_then(|n| n.attr("val"))
            .and_then(|v| v.parse::<u8>().ok());

        let has_bottom_border = ppr
            .and_then(|p| p.first_child("pBdr"))
            .and_then(|b| b.first_child("bottom"))
            .is_some_and(|b| {
                b.attr("val")
                    .is_none_or(|v| !matches!(v, "none" | "nil" | "0" | "false"))
            });

        let bidi = ppr
            .and_then(|p| p.first_child("bidi"))
            .map(read_ooxml_bool)
            .or_else(|| p_style.and_then(|s| self.styles.resolve_bidi(s)));

        let mut kind = if let Some(lvl) =
            self.numbering
                .resolve_for_paragraph(direct_num_id, direct_ilvl, p_style, self.styles)
        {
            match lvl.kind {
                ListKind::Bullet => BlockKind::BulletListItem {
                    depth: lvl.depth.min(4),
                },
                ListKind::Ordered => BlockKind::OrderedListItem {
                    depth: lvl.depth.min(4),
                    number: 1,
                },
                ListKind::None => BlockKind::Paragraph,
            }
        } else {
            BlockKind::Paragraph
        };

        if kind == BlockKind::Paragraph {
            if let Some(styled_kind) = p_style.and_then(|s| self.styles.resolve_block_kind(s)) {
                kind = styled_kind;
            } else if has_bottom_border && inlines.is_empty() {
                kind = BlockKind::HorizontalRule;
            }
        }

        if matches!(kind, BlockKind::CodeBlock { .. }) {
            for inline in &mut inlines {
                if let Inline::Text(run) = inline {
                    run.style.code = false;
                }
            }
        }

        if kind.is_list() {
            while matches!(inlines.last(), Some(Inline::LineBreak)) {
                inlines.pop();
            }
        }

        let mut block = Block::new(kind).with_inlines(inlines);
        if bidi == Some(true) {
            block.direction = TextDirection::Rtl;
        }
        block.normalize();

        let keep_block = match block.kind {
            BlockKind::HorizontalRule => true,
            BlockKind::BulletListItem { .. } | BlockKind::OrderedListItem { .. } => {
                block_has_visible_content(&block)
            }
            _ => !block.inlines.is_empty(),
        };

        let mut result = Vec::with_capacity(1 + extra_blocks.len());
        if keep_block {
            result.push(block);
        }
        result.append(&mut extra_blocks);
        result
    }

    fn read_table(&mut self, tbl: &XmlElement) -> Vec<Block> {
        let mut out = Vec::new();
        for tr in tbl.child_elements() {
            if tr.name != "tr" {
                continue;
            }
            let mut row_cells: Vec<Vec<Block>> = Vec::new();
            for tc in tr.child_elements() {
                if tc.name != "tc" {
                    continue;
                }
                let is_vmerge_cont = tc
                    .first_child("tcPr")
                    .and_then(|p| p.first_child("vMerge"))
                    .is_some_and(|vm| vm.attr("val") != Some("restart"));
                let cell_blocks = self.read_block_children(&tc.children);
                if is_vmerge_cont && cell_blocks.is_empty() {
                    continue;
                }
                row_cells.push(cell_blocks);
            }

            let non_empty_cells: Vec<&Vec<Block>> =
                row_cells.iter().filter(|c| !c.is_empty()).collect();
            let can_compact_row = row_cells.len() >= 2
                && !non_empty_cells.is_empty()
                && non_empty_cells.iter().all(|blocks| {
                    blocks.len() == 1
                        && blocks[0].kind == BlockKind::Paragraph
                        && !blocks[0]
                            .inlines
                            .iter()
                            .any(|i| matches!(i, Inline::Image { .. } | Inline::LineBreak))
                });

            if can_compact_row {
                let mut combined = Vec::new();
                let mut dir = TextDirection::Auto;
                for (idx, cell) in non_empty_cells.iter().enumerate() {
                    if idx > 0 {
                        combined.push(Inline::Text(TextRun::plain("\t")));
                    }
                    if idx == 0 {
                        dir = cell[0].direction;
                    }
                    combined.extend(cell[0].inlines.iter().cloned());
                }
                let mut b = Block::new(BlockKind::Paragraph)
                    .with_direction(dir)
                    .with_inlines(combined);
                b.normalize();
                out.push(b);
            } else {
                for mut cell in row_cells {
                    out.append(&mut cell);
                }
            }
        }
        out
    }

    fn read_inline_nodes(
        &mut self,
        nodes: &[XmlNode],
        active_link: Option<&str>,
        inlines: &mut Vec<Inline>,
        extra_blocks: &mut Vec<Block>,
    ) {
        for node in nodes {
            if let XmlNode::Element(el) = node {
                self.read_inline_element(el, active_link, inlines, extra_blocks);
            }
        }
    }

    fn read_inline_element(
        &mut self,
        el: &XmlElement,
        active_link: Option<&str>,
        inlines: &mut Vec<Inline>,
        extra_blocks: &mut Vec<Block>,
    ) {
        match el.name.as_str() {
            "pPr"
            | "rPr"
            | "del"
            | "proofErr"
            | "bookmarkStart"
            | "bookmarkEnd"
            | "commentRangeStart"
            | "commentRangeEnd"
            | "lastRenderedPageBreak" => {}
            "r" => self.read_run(el, active_link, inlines, extra_blocks),
            "hyperlink" => {
                let href = self.resolve_hyperlink_target(el);
                let link_ref = href.as_deref().or(active_link);
                self.read_inline_nodes(&el.children, link_ref, inlines, extra_blocks);
            }
            "sdt" => {
                if let Some(cb) = el
                    .first_child("sdtPr")
                    .and_then(|p| p.first_child("checkbox"))
                {
                    let checked = cb.first_child("checked").is_some_and(read_ooxml_bool);
                    inlines.push(Inline::Checkbox { checked });
                } else if let Some(content) = el.first_child("sdtContent") {
                    self.read_inline_nodes(&content.children, active_link, inlines, extra_blocks);
                }
            }
            "ins" | "smartTag" | "customXml" | "fldSimple" => {
                let fld_link = if el.name == "fldSimple" {
                    el.attr("instr").and_then(parse_hyperlink_instr)
                } else {
                    None
                };
                let link_ref = fld_link.as_deref().or(active_link);
                self.read_inline_nodes(&el.children, link_ref, inlines, extra_blocks);
            }
            "drawing" => self.read_drawing(el, inlines, extra_blocks),
            "pict" => self.read_pict(el, inlines, extra_blocks),
            "object" => self.read_inline_nodes(&el.children, active_link, inlines, extra_blocks),
            "commentReference" => {
                if let Some(cid) = el.attr("id")
                    && let Some(text) = self.comments.get(cid)
                {
                    inlines.push(Inline::Comment { text: text.clone() });
                }
            }
            _ => {}
        }
    }

    fn read_run(
        &mut self,
        el: &XmlElement,
        active_link: Option<&str>,
        inlines: &mut Vec<Inline>,
        extra_blocks: &mut Vec<Block>,
    ) {
        let mut style = self.read_run_style(el.first_child("rPr"));
        let field_link = self.current_complex_field_hyperlink();
        let resolved_link = active_link.map(str::to_string).or(field_link);
        if let Some(url) = resolved_link {
            style.link = Some(url);
            style.underline = false;
        }

        for child in el.child_elements() {
            match child.name.as_str() {
                "rPr" | "delText" | "lastRenderedPageBreak" => {}
                "t" => {
                    let txt = child.text_content();
                    if !txt.is_empty() {
                        push_docx_text_inline(&txt, &style, inlines);
                    }
                }
                "tab" => {
                    inlines.push(Inline::Text(TextRun::new("\t", style.clone())));
                }
                "br" => {
                    let btype = child.attr("type");
                    if btype.is_none() || btype == Some("textWrapping") {
                        inlines.push(Inline::LineBreak);
                    }
                }
                "cr" => {
                    inlines.push(Inline::LineBreak);
                }
                "noBreakHyphen" => {
                    inlines.push(Inline::Text(TextRun::new("\u{2011}", style.clone())));
                }
                "softHyphen" => {
                    inlines.push(Inline::Text(TextRun::new("\u{00AD}", style.clone())));
                }
                "sym" => {
                    if let Some(ch) = decode_ooxml_sym(child.attr("font"), child.attr("char")) {
                        inlines.push(Inline::Text(TextRun::new(ch.to_string(), style.clone())));
                    }
                }
                "fldChar" => {
                    self.handle_fld_char(child, inlines);
                }
                "instrText" => {
                    self.current_instr_text.push_str(&child.text_content());
                }
                "commentReference" => {
                    if let Some(cid) = child.attr("id")
                        && let Some(text) = self.comments.get(cid)
                    {
                        inlines.push(Inline::Comment { text: text.clone() });
                    }
                }
                "drawing" => self.read_drawing(child, inlines, extra_blocks),
                "pict" => self.read_pict(child, inlines, extra_blocks),
                "object" => {
                    self.read_inline_nodes(&child.children, active_link, inlines, extra_blocks)
                }
                _ => {}
            }
        }
    }

    fn read_run_style(&self, rpr: Option<&XmlElement>) -> InlineStyle {
        let Some(rpr) = rpr else {
            return InlineStyle::default();
        };
        let mut style = rpr
            .first_child("rStyle")
            .and_then(|s| s.attr("val"))
            .map(|sid| self.styles.resolve_character_style(sid))
            .unwrap_or_default();

        if let Some(b) = rpr.first_child("b") {
            style.bold = read_ooxml_bool(b);
        }
        if let Some(i) = rpr.first_child("i") {
            style.italic = read_ooxml_bool(i);
        }
        if let Some(u) = rpr.first_child("u") {
            style.underline = read_ooxml_underline(u);
        }
        if let Some(s) = rpr
            .first_child("strike")
            .or_else(|| rpr.first_child("dstrike"))
        {
            style.strikethrough = read_ooxml_bool(s);
        }
        if let Some(fonts) = rpr.first_child("rFonts")
            && let Some(f) = fonts.attr("ascii").or_else(|| fonts.attr("hAnsi"))
            && is_monospace_font(f)
        {
            style.code = true;
        }
        style
    }

    fn resolve_hyperlink_target(&self, el: &XmlElement) -> Option<String> {
        let rid = el.attr_prefixed("r", "id");
        let anchor = el.attr("anchor");
        if let Some(rid) = rid
            && let Some(rel) = self.rels.get(rid)
        {
            let mut href = rel.target.clone();
            if let Some(anc) = anchor
                && !anc.is_empty()
            {
                if let Some((base, _)) = href.split_once('#') {
                    href = format!("{base}#{anc}");
                } else {
                    href = format!("{href}#{anc}");
                }
            }
            return Some(href);
        }
        anchor.filter(|a| !a.is_empty()).map(|a| format!("#{a}"))
    }

    fn handle_fld_char(&mut self, el: &XmlElement, inlines: &mut Vec<Inline>) {
        match el.attr("fldCharType") {
            Some("begin") => {
                let checked = el
                    .first_child("ffData")
                    .and_then(|f| f.first_child("checkBox"))
                    .map(|cb| {
                        cb.first_child("checked")
                            .or_else(|| cb.first_child("default"))
                            .is_some_and(read_ooxml_bool)
                    });
                self.complex_field_stack
                    .push(ComplexField::Begin { checked });
                self.current_instr_text.clear();
            }
            Some("separate") => {
                if let Some(top) = self.complex_field_stack.pop() {
                    let parsed = self.parse_current_field(top);
                    self.complex_field_stack.push(parsed);
                }
            }
            Some("end") => {
                if let Some(top) = self.complex_field_stack.pop() {
                    let resolved = match top {
                        ComplexField::Begin { .. } => self.parse_current_field(top),
                        other => other,
                    };
                    if let ComplexField::Checkbox { checked } = resolved {
                        inlines.push(Inline::Checkbox { checked });
                    }
                }
                self.current_instr_text.clear();
            }
            _ => {}
        }
    }

    fn parse_current_field(&mut self, top: ComplexField) -> ComplexField {
        let instr = std::mem::take(&mut self.current_instr_text);
        if let Some(href) = parse_hyperlink_instr(&instr) {
            return ComplexField::Hyperlink { href };
        }
        if instr.to_ascii_uppercase().contains("FORMCHECKBOX") {
            let checked = match top {
                ComplexField::Begin { checked } => checked.unwrap_or(false),
                _ => false,
            };
            return ComplexField::Checkbox { checked };
        }
        ComplexField::Unknown
    }

    fn current_complex_field_hyperlink(&self) -> Option<String> {
        self.complex_field_stack.iter().rev().find_map(|f| match f {
            ComplexField::Hyperlink { href } => Some(href.clone()),
            _ => None,
        })
    }

    fn read_drawing(
        &mut self,
        drawing: &XmlElement,
        inlines: &mut Vec<Inline>,
        extra_blocks: &mut Vec<Block>,
    ) {
        let mut containers: Vec<&XmlElement> = drawing
            .child_elements()
            .filter(|c| matches!(c.name.as_str(), "inline" | "anchor"))
            .collect();
        if containers.is_empty() {
            containers.push(drawing);
        }

        for container in containers {
            let alt = container
                .first_child("docPr")
                .and_then(|dp| {
                    dp.attr("descr")
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| dp.attr("title").filter(|s| !s.trim().is_empty()))
                        .or_else(|| dp.attr("name").filter(|s| !s.trim().is_empty()))
                })
                .unwrap_or("")
                .to_string();

            let mut blips = Vec::new();
            container.collect_descendants("blip", &mut blips);
            for blip in blips {
                if let Some(rid) = blip.attr_prefixed("r", "embed") {
                    if let Some(src) = self.load_embedded_image_data_uri(rid) {
                        inlines.push(Inline::Image {
                            src,
                            alt: alt.clone(),
                            original_src: None,
                        });
                    }
                } else if let Some(rid) = blip.attr_prefixed("r", "link")
                    && let Some(rel) = self.rels.get(rid)
                {
                    let src = if rel.external {
                        rel.target.clone()
                    } else {
                        self.load_embedded_image_data_uri(rid)
                            .unwrap_or_else(|| rel.target.clone())
                    };
                    inlines.push(Inline::Image {
                        src,
                        alt: alt.clone(),
                        original_src: None,
                    });
                }
            }

            let mut txbx_list = Vec::new();
            container.collect_descendants("txbxContent", &mut txbx_list);
            for txbx in txbx_list {
                let mut blocks = self.read_block_children(&txbx.children);
                extra_blocks.append(&mut blocks);
            }
        }
    }

    fn read_pict(
        &mut self,
        pict: &XmlElement,
        inlines: &mut Vec<Inline>,
        extra_blocks: &mut Vec<Block>,
    ) {
        let mut imagedatas = Vec::new();
        pict.collect_descendants("imagedata", &mut imagedatas);
        for img in imagedatas {
            if let Some(rid) = img
                .attr_prefixed("r", "id")
                .or_else(|| img.attr_prefixed("o", "relid"))
                && let Some(src) = self.load_embedded_image_data_uri(rid)
            {
                let alt = img
                    .attr("title")
                    .or_else(|| img.attr("alt"))
                    .unwrap_or("")
                    .to_string();
                inlines.push(Inline::Image {
                    src,
                    alt,
                    original_src: None,
                });
            }
        }

        let mut txbx_list = Vec::new();
        pict.collect_descendants("txbxContent", &mut txbx_list);
        for txbx in txbx_list {
            let mut blocks = self.read_block_children(&txbx.children);
            extra_blocks.append(&mut blocks);
        }
    }

    fn load_embedded_image_data_uri(&mut self, rid: &str) -> Option<String> {
        let rel = self.rels.get(rid)?;
        if rel.external {
            return Some(rel.target.clone());
        }
        let zip_path = resolve_word_zip_path(&rel.target);
        if let Some(cached) = self.image_cache.get(&zip_path) {
            return cached.clone();
        }

        let bytes = read_zip_bytes(self.archive, &zip_path).or_else(|| {
            let decoded = percent_decode_path(&zip_path);
            if decoded != zip_path {
                read_zip_bytes(self.archive, &decoded)
            } else {
                None
            }
        });

        let data_uri = bytes.and_then(|raw| {
            let mime = self.content_types.content_type_for(&zip_path, &raw)?;
            let normalized_mime = if mime == "image/jpg" {
                "image/jpeg"
            } else {
                mime.as_str()
            };
            Some(format!(
                "data:{normalized_mime};base64,{}",
                base64_encode(&raw)
            ))
        });

        self.image_cache.insert(zip_path, data_uri.clone());
        data_uri
    }
}

fn read_zip_bytes(archive: &mut ZipArchive<Cursor<&[u8]>>, path: &str) -> Option<Vec<u8>> {
    if let Ok(mut file) = archive.by_name(path) {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).ok()?;
        return Some(bytes);
    }
    let target_lower = path.to_ascii_lowercase();
    for idx in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(idx)
            && file
                .name()
                .is_ok_and(|n| n.to_ascii_lowercase() == target_lower)
        {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).ok()?;
            return Some(bytes);
        }
    }
    None
}

fn push_docx_text_inline(text: &str, style: &InlineStyle, inlines: &mut Vec<Inline>) {
    if text == "☑" || text == "☑ " {
        inlines.push(Inline::Checkbox { checked: true });
        if text.ends_with(' ') {
            inlines.push(Inline::Text(TextRun::plain(" ")));
        }
    } else if text == "☐" || text == "☐ " {
        inlines.push(Inline::Checkbox { checked: false });
        if text.ends_with(' ') {
            inlines.push(Inline::Text(TextRun::plain(" ")));
        }
    } else {
        inlines.push(Inline::Text(TextRun::new(text, style.clone())));
    }
}

fn block_has_visible_content(block: &Block) -> bool {
    block.inlines.iter().any(|inline| match inline {
        Inline::Text(run) => run.text.chars().any(|c| !c.is_whitespace()),
        Inline::Image { .. } | Inline::Checkbox { .. } | Inline::Comment { .. } => true,
        Inline::LineBreak => false,
    })
}

fn read_ooxml_bool(el: &XmlElement) -> bool {
    el.attr("val")
        .is_none_or(|v| !matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "off"))
}

fn read_ooxml_underline(el: &XmlElement) -> bool {
    el.attr("val").is_some_and(|v| {
        !matches!(
            v.to_ascii_lowercase().as_str(),
            "0" | "false" | "none" | "off"
        )
    })
}

fn is_monospace_font(font: &str) -> bool {
    let l = font.to_ascii_lowercase();
    l.contains("mono")
        || l.contains("courier")
        || l.contains("consolas")
        || l.contains("menlo")
        || l.contains("fira code")
}

fn parse_hyperlink_instr(instr: &str) -> Option<String> {
    let upper = instr.to_ascii_uppercase();
    let idx = upper.find("HYPERLINK")?;
    let rest = instr[idx + "HYPERLINK".len()..].trim();
    if let Some(after_l) = rest
        .strip_prefix("\\l")
        .or_else(|| rest.strip_prefix("\\L"))
    {
        let anchor = extract_quoted_or_token(after_l.trim())?;
        return Some(format!("#{anchor}"));
    }
    extract_quoted_or_token(rest)
}

fn extract_quoted_or_token(s: &str) -> Option<String> {
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        let val = &rest[..end];
        if !val.is_empty() {
            return Some(val.to_string());
        }
    } else {
        let tok = s.split_whitespace().next()?;
        if !tok.is_empty() {
            return Some(tok.to_string());
        }
    }
    None
}

fn decode_ooxml_sym(font: Option<&str>, char_hex: Option<&str>) -> Option<char> {
    let hex = char_hex?.trim();
    let code = u32::from_str_radix(hex, 16).ok()?;
    let byte = (code & 0xFF) as u8;
    let f = font.unwrap_or("").to_ascii_lowercase();
    if f.contains("wingdings") {
        return match byte {
            0x6C | 0x9F => Some('•'),
            0x6E | 0xA7 => Some('▪'),
            0x6F | 0xA8 => Some('☐'),
            0x70 => Some('▫'),
            0xFC => Some('✓'),
            0xFD | 0xFE => Some('☑'),
            0xE0 | 0xE8 => Some('→'),
            0xDF | 0xE7 => Some('←'),
            _ => None,
        };
    }
    if f.contains("symbol") {
        return match byte {
            0xB7 => Some('•'),
            0xAE => Some('→'),
            0xAC => Some('←'),
            0xB1 => Some('±'),
            0xB4 => Some('×'),
            0xB8 => Some('÷'),
            _ => None,
        };
    }
    char::from_u32(code)
}

fn resolve_general_ref(e: &quick_xml::events::BytesRef<'_>) -> String {
    if let Ok(Some(ch)) = e.resolve_char_ref() {
        return ch.to_string();
    }
    let name: &str = e.as_ref();
    crate::io::html::decode_html_entities(&format!("&{name};"))
}

const ROOT_RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

fn build_styles_xml(prefs: Option<&Preferences>) -> String {
    let default_prefs = Preferences::default();
    let p = prefs.unwrap_or(&default_prefs);
    let font_name = match &p.font {
        FontChoice::Serif => "Georgia".to_string(),
        FontChoice::Sans => "Liberation Sans".to_string(),
        FontChoice::Monospace => "Liberation Mono".to_string(),
        FontChoice::System(s) if !s.trim().is_empty() => escape_attr(&sanitize_xml(s.trim())),
        FontChoice::System(_) => "Georgia".to_string(),
    };
    let pct = (p.font_size_percent as f64 / 100.0).clamp(0.75, 1.5);
    let base_half_pt = (22.4 * pct).round() as u32;
    let h1_sz = (base_half_pt as f64 * 1.85).round() as u32;
    let h2_sz = (base_half_pt as f64 * 1.45).round() as u32;
    let h3_sz = (base_half_pt as f64 * 1.22).round() as u32;
    let h4_sz = (base_half_pt as f64 * 1.10).round() as u32;
    let h5_sz = base_half_pt;
    let h6_sz = (base_half_pt as f64 * 0.95).round() as u32;
    let code_sz = (base_half_pt as f64 * 0.88).round() as u32;
    let line_240 = (p.line_spacing.factor() as f64 * 240.0).round() as u32;
    let p_after = if p.paragraph_margins { 180 } else { 40 };

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
    <w:name w:val="Normal"/>
    <w:pPr><w:spacing w:after="{p_after}" w:line="{line_240}" w:lineRule="auto"/></w:pPr>
    <w:rPr>
      <w:rFonts w:ascii="{font_name}" w:hAnsi="{font_name}" w:cs="{font_name}"/>
      <w:color w:val="262626"/>
      <w:sz w:val="{base_half_pt}"/><w:szCs w:val="{base_half_pt}"/>
    </w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/>
    <w:pPr><w:keepNext/><w:spacing w:before="360" w:after="140"/></w:pPr>
    <w:rPr><w:b/><w:bCs/><w:sz w:val="{h1_sz}"/><w:szCs w:val="{h1_sz}"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/>
    <w:pPr><w:keepNext/><w:spacing w:before="280" w:after="110"/></w:pPr>
    <w:rPr><w:b/><w:bCs/><w:sz w:val="{h2_sz}"/><w:szCs w:val="{h2_sz}"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading3">
    <w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/>
    <w:pPr><w:keepNext/><w:spacing w:before="220" w:after="90"/></w:pPr>
    <w:rPr><w:b/><w:bCs/><w:sz w:val="{h3_sz}"/><w:szCs w:val="{h3_sz}"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading4">
    <w:name w:val="heading 4"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/>
    <w:pPr><w:keepNext/><w:spacing w:before="180" w:after="70"/></w:pPr>
    <w:rPr><w:b/><w:bCs/><w:sz w:val="{h4_sz}"/><w:szCs w:val="{h4_sz}"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading5">
    <w:name w:val="heading 5"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/>
    <w:pPr><w:keepNext/><w:spacing w:before="160" w:after="60"/></w:pPr>
    <w:rPr><w:b/><w:bCs/><w:sz w:val="{h5_sz}"/><w:szCs w:val="{h5_sz}"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading6">
    <w:name w:val="heading 6"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/>
    <w:pPr><w:keepNext/><w:spacing w:before="140" w:after="60"/></w:pPr>
    <w:rPr><w:b/><w:bCs/><w:color w:val="555555"/><w:sz w:val="{h6_sz}"/><w:szCs w:val="{h6_sz}"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Quote">
    <w:name w:val="Quote"/><w:basedOn w:val="Normal"/>
    <w:pPr>
      <w:pBdr><w:left w:val="single" w:sz="18" w:space="12" w:color="BDBDBD"/></w:pBdr>
      <w:spacing w:before="140" w:after="160"/>
      <w:ind w:left="540" w:right="360"/>
    </w:pPr>
    <w:rPr><w:i/><w:iCs/><w:color w:val="555555"/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="CodeBlock">
    <w:name w:val="Code Block"/><w:basedOn w:val="Normal"/>
    <w:pPr>
      <w:shd w:val="clear" w:color="auto" w:fill="F3F4F6"/>
      <w:spacing w:before="120" w:after="160" w:line="280" w:lineRule="auto"/>
      <w:ind w:left="220" w:right="220"/>
    </w:pPr>
    <w:rPr>
      <w:rFonts w:ascii="Liberation Mono" w:hAnsi="Liberation Mono" w:cs="Liberation Mono"/>
      <w:sz w:val="{code_sz}"/><w:szCs w:val="{code_sz}"/>
    </w:rPr>
  </w:style>
</w:styles>"#
    )
}

const NUMBERING_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="◦"/><w:pPr><w:ind w:left="1440" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="2"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="▪"/><w:pPr><w:ind w:left="2160" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="3"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/><w:pPr><w:ind w:left="2880" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="4"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="◦"/><w:pPr><w:ind w:left="3600" w:hanging="360"/></w:pPr></w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%2."/><w:pPr><w:ind w:left="1440" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="2"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%3."/><w:pPr><w:ind w:left="2160" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="3"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%4."/><w:pPr><w:ind w:left="2880" w:hanging="360"/></w:pPr></w:lvl>
    <w:lvl w:ilvl="4"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%5."/><w:pPr><w:ind w:left="3600" w:hanging="360"/></w:pPr></w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;
