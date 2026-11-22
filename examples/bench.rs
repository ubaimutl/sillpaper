use sillpaper::config::Preferences;
use sillpaper::editor::EditorState;
use sillpaper::io::docx::{document_to_docx_bytes, docx_bytes_to_document};
use sillpaper::io::html::{document_to_html, html_to_document};
use sillpaper::io::markdown::{document_to_markdown, markdown_to_document};
use sillpaper::model::{
    Block, BlockKind, DocPos, Document, Inline, InlineAttr, InlineStyle, Selection, TextDirection,
    TextRun, compute_stats, find_matches,
};
use std::time::Instant;

fn build_large_doc(target_words: usize) -> Document {
    let sentence = "A calm writing canvas lets thoughts flow naturally without heavy browser processes consuming gigabytes of memory. ";
    let mut blocks = Vec::new();
    let mut words = 0usize;
    let mut section = 1usize;

    while words < target_words {
        if blocks.len() % 20 == 0 {
            blocks.push(
                Block::new(BlockKind::Heading(2)).with_inlines(vec![Inline::Text(TextRun::plain(
                    format!("Section {section}"),
                ))]),
            );
            section += 1;
            words += 2;
        }
        let para = sentence.repeat(6);
        blocks.push(Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Text(TextRun::plain(&para[..100])),
            Inline::Text(TextRun::new(
                "emphasized prose",
                InlineStyle {
                    italic: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(&para[100..])),
        ]));
        words += 98;
    }

    Document::from_blocks(blocks)
}

fn main() {
    println!("=== Sillpaper Engine Benchmark (50,000-word rich document) ===");

    let t = Instant::now();
    let doc = build_large_doc(50_000);
    let stats = compute_stats(&doc, None, 270);
    println!(
        "Build + stats ({} words, {} blocks): {:.2} ms",
        stats.words,
        doc.blocks.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );

    let t = Instant::now();
    let _ = compute_stats(&doc, None, 270);
    println!(
        "Word/char/reading-time recount: {:.2} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );

    let t = Instant::now();
    let hits = find_matches(&doc, "memory");
    println!(
        "Case-insensitive Unicode search ({} matches): {:.2} ms",
        hits.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );

    let prefs = Preferences::default();
    let mut ed = EditorState::from_document(doc.clone(), TextDirection::Auto);
    ed.set_selection(Selection::collapsed(DocPos::new(doc.blocks.len() / 2, 10)));
    let sample = "Testing fast incremental typing -- with smart quotes \"like this\" -> done. ";
    let chars: Vec<String> = sample
        .chars()
        .cycle()
        .take(1000)
        .map(|c| c.to_string())
        .collect();

    let t = Instant::now();
    for ch in &chars {
        ed.insert_text(ch, Some(&prefs));
    }
    let total_us = t.elapsed().as_secs_f64() * 1_000_000.0;
    println!(
        "1,000 smart-typed keystrokes (mid 50k-word doc): {:.2} µs/keystroke ({:.2} ms total)",
        total_us / 1000.0,
        total_us / 1000.0
    );

    ed.set_selection(Selection::range(
        DocPos::new(doc.blocks.len() / 2, 0),
        DocPos::new(doc.blocks.len() / 2, 40),
    ));
    let t = Instant::now();
    for _ in 0..100 {
        ed.toggle_inline_attr(InlineAttr::Bold);
        ed.undo();
        ed.redo();
    }
    println!(
        "100x format+undo+redo cycles (mid 50k-word doc): {:.2} µs/cycle",
        t.elapsed().as_secs_f64() * 1_000_000.0 / 100.0
    );

    let t = Instant::now();
    let html = document_to_html(&doc);
    let html_ser_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let _ = html_to_document(&html);
    let html_parse_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "HTML round-trip ({} KB): serialize {:.2} ms, parse {:.2} ms",
        html.len() / 1024,
        html_ser_ms,
        html_parse_ms
    );

    let t = Instant::now();
    let md = document_to_markdown(&doc);
    let md_ser_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let _ = markdown_to_document(&md.markdown);
    let md_parse_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "Markdown round-trip ({} KB): serialize {:.2} ms, parse {:.2} ms",
        md.markdown.len() / 1024,
        md_ser_ms,
        md_parse_ms
    );

    let t = Instant::now();
    let docx = document_to_docx_bytes(&doc).unwrap();
    let docx_ser_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let _ = docx_bytes_to_document(&docx).unwrap();
    let docx_parse_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "DOCX OOXML round-trip ({} KB zip): export {:.2} ms, import {:.2} ms",
        docx.len() / 1024,
        docx_ser_ms,
        docx_parse_ms
    );
}
