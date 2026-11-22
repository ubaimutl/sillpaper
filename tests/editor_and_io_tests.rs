use sillpaper::config::{
    FontChoice, LineSpacing, Preferences, TextWidth, ThemeId, generate_app_css, resolve_palette,
};
use sillpaper::editor::EditorState;
use sillpaper::io::backup::{SessionOrder, StorageManager};
use sillpaper::io::docx::{
    document_to_docx_bytes, document_to_docx_bytes_with_options, docx_bytes_to_document,
};
use sillpaper::io::html::{
    document_to_html, document_to_html_with_prefs, fragment_to_html, html_to_document,
    html_to_fragment,
};
use sillpaper::io::markdown::{document_to_markdown, markdown_to_document};
use sillpaper::io::{
    document_to_plain_text, load_document, save_document, save_document_with_prefs,
};
use sillpaper::model::{
    Block, BlockKind, DocPos, Document, Inline, InlineAttr, InlineStyle, Selection, TextDirection,
    TextRun, compute_stats, extract_toc, find_matches, heading_slug,
};
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

fn temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sillpaper_test_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_text_insertion_deletion_and_navigation_edits() {
    let mut ed = EditorState::new(TextDirection::Auto);
    ed.insert_text("Hello", None);
    ed.insert_text(" ", None);
    ed.insert_text("world!", None);
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello world!");
    assert_eq!(ed.selection, Selection::collapsed(DocPos::new(0, 12)));

    ed.backspace();
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello world");

    ed.set_selection(Selection::collapsed(DocPos::new(0, 6)));
    ed.delete_forward();
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello orld");

    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks.len(), 2);
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello ");
    assert_eq!(ed.doc.blocks[1].plain_text(), "orld");
    assert_eq!(ed.selection, Selection::collapsed(DocPos::new(1, 0)));

    ed.backspace();
    assert_eq!(ed.doc.blocks.len(), 1);
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello orld");
    assert_eq!(ed.selection, Selection::collapsed(DocPos::new(0, 6)));

    ed.insert_soft_break();
    assert_eq!(ed.doc.blocks.len(), 1);
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello \norld");
}

#[test]
fn test_formatting_nested_formatting_and_collapsed_pending_style() {
    let mut ed = EditorState::new(TextDirection::Auto);
    ed.insert_text("The quick brown fox", None);

    ed.set_selection(Selection::range(DocPos::new(0, 4), DocPos::new(0, 15)));
    ed.toggle_inline_attr(InlineAttr::Bold);

    ed.set_selection(Selection::range(DocPos::new(0, 10), DocPos::new(0, 19)));
    ed.toggle_inline_attr(InlineAttr::Italic);

    ed.set_selection(Selection::range(DocPos::new(0, 10), DocPos::new(0, 15)));
    ed.toggle_inline_attr(InlineAttr::Underline);
    ed.toggle_inline_attr(InlineAttr::Strikethrough);
    ed.set_link(Some("example.com/fox".into()));

    let state = ed.toolbar_state();
    assert!(state.bold);
    assert!(state.italic);
    assert!(state.underline);
    assert!(state.strikethrough);
    assert_eq!(state.link.as_deref(), Some("https://example.com/fox"));

    ed.toggle_inline_attr(InlineAttr::Bold);
    let state2 = ed.toolbar_state();
    assert!(!state2.bold);
    assert!(state2.italic);
    assert!(state2.underline);
    assert!(state2.strikethrough);
    assert_eq!(state2.link.as_deref(), Some("https://example.com/fox"));

    ed.set_selection(Selection::collapsed(DocPos::new(0, 0)));
    ed.toggle_inline_attr(InlineAttr::Bold);
    ed.toggle_inline_attr(InlineAttr::Italic);
    ed.insert_text("WOW ", None);
    let first_run = match &ed.doc.blocks[0].inlines[0] {
        Inline::Text(r) => r,
        _ => panic!("Expected text run"),
    };
    assert_eq!(first_run.text, "WOW ");
    assert!(first_run.style.bold && first_run.style.italic);
}

#[test]
fn test_undo_redo_across_typing_formatting_blocks_and_objects() {
    let mut ed = EditorState::new(TextDirection::Auto);

    for ch in "Hello".chars() {
        ed.insert_text(&ch.to_string(), None);
    }
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello");
    assert!(ed.is_dirty());

    ed.insert_text(" ", None);
    for ch in "world".chars() {
        ed.insert_text(&ch.to_string(), None);
    }
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello world");

    ed.set_selection(Selection::range(DocPos::new(0, 6), DocPos::new(0, 11)));
    ed.toggle_inline_attr(InlineAttr::Bold);
    assert!(ed.toolbar_state().bold);

    ed.toggle_block_kind(BlockKind::Heading(1));
    assert_eq!(ed.doc.blocks[0].kind, BlockKind::Heading(1));

    ed.set_selection(Selection::collapsed(DocPos::new(0, 11)));
    ed.insert_inline_object(Inline::Comment {
        text: "Review this".into(),
    });
    ed.set_direction(TextDirection::Rtl);
    assert_eq!(ed.doc.blocks[0].direction, TextDirection::Rtl);

    ed.undo();
    assert_eq!(ed.doc.blocks[0].direction, TextDirection::Auto);

    ed.undo();
    assert_eq!(ed.doc.blocks[0].char_len(), 11);

    ed.undo();
    assert_eq!(ed.doc.blocks[0].kind, BlockKind::Paragraph);

    ed.undo();
    ed.set_selection(Selection::range(DocPos::new(0, 6), DocPos::new(0, 11)));
    assert!(!ed.toolbar_state().bold);

    ed.undo();
    assert_eq!(ed.doc.blocks[0].plain_text(), "Hello ");

    for _ in 0..5 {
        ed.redo();
    }
    assert_eq!(ed.doc.blocks[0].kind, BlockKind::Heading(1));
    assert_eq!(ed.doc.blocks[0].direction, TextDirection::Rtl);
    assert_eq!(ed.doc.blocks[0].char_len(), 12);
}

#[test]
fn test_multiline_selections_and_rich_copy_paste() {
    let mut ed = EditorState::new(TextDirection::Auto);
    ed.insert_text("First paragraph", None);
    ed.insert_paragraph();
    ed.insert_text("Second paragraph", None);
    ed.insert_paragraph();
    ed.insert_text("Third paragraph", None);

    ed.set_selection(Selection::range(DocPos::new(0, 6), DocPos::new(2, 5)));
    ed.toggle_inline_attr(InlineAttr::Bold);

    let stats = compute_stats(&ed.doc, Some(ed.selection), 270);
    assert!(stats.is_selection);
    assert_eq!(stats.words, 4);

    let frag = ed.copy_fragment(false).unwrap();
    let html = fragment_to_html(&frag);
    assert!(html.contains("<strong>"));
    let parsed = html_to_fragment(&html);
    assert_eq!(parsed.blocks.len(), 3);

    let mut ed2 = EditorState::new(TextDirection::Auto);
    ed2.replace_selection_with_fragment(parsed);
    assert_eq!(ed2.doc.blocks.len(), 3);
    assert_eq!(ed2.doc.blocks[0].plain_text(), "paragraph");
    assert_eq!(ed2.doc.blocks[1].plain_text(), "Second paragraph");
    assert_eq!(ed2.doc.blocks[2].plain_text(), "Third");
    ed2.set_selection(Selection::range(DocPos::new(1, 0), DocPos::new(1, 6)));
    assert!(ed2.toolbar_state().bold);

    ed.set_selection(Selection::range(DocPos::new(0, 5), DocPos::new(2, 5)));
    ed.delete_selection();
    assert_eq!(ed.doc.blocks.len(), 1);
    assert_eq!(ed.doc.blocks[0].plain_text(), "First paragraph");

    ed.set_selection(Selection::range(DocPos::new(0, 0), DocPos::new(0, 5)));
    ed.paste_plain_text("https://rust-lang.org");
    assert_eq!(ed.doc.blocks[0].plain_text(), "First paragraph");
    assert_eq!(
        ed.toolbar_state().link.as_deref(),
        Some("https://rust-lang.org")
    );
}

#[test]
fn test_lists_nesting_renumbering_and_blockquotes() {
    let mut ed = EditorState::new(TextDirection::Auto);
    ed.insert_text("Item one", None);
    ed.toggle_block_kind(BlockKind::OrderedListItem {
        depth: 0,
        number: 1,
    });
    ed.insert_paragraph();
    ed.insert_text("Item two", None);
    ed.insert_paragraph();
    ed.insert_text("Item three", None);

    assert_eq!(
        ed.doc.blocks[0].kind,
        BlockKind::OrderedListItem {
            depth: 0,
            number: 1
        }
    );
    assert_eq!(
        ed.doc.blocks[1].kind,
        BlockKind::OrderedListItem {
            depth: 0,
            number: 2
        }
    );
    assert_eq!(
        ed.doc.blocks[2].kind,
        BlockKind::OrderedListItem {
            depth: 0,
            number: 3
        }
    );

    ed.set_selection(Selection::collapsed(DocPos::new(1, 0)));
    ed.adjust_list_indent(true);
    assert_eq!(
        ed.doc.blocks[1].kind,
        BlockKind::OrderedListItem {
            depth: 1,
            number: 1
        }
    );
    assert_eq!(
        ed.doc.blocks[2].kind,
        BlockKind::OrderedListItem {
            depth: 0,
            number: 2
        }
    );

    ed.set_selection(Selection::collapsed(DocPos::new(2, 10)));
    ed.insert_paragraph();
    assert!(ed.doc.blocks[3].kind.is_list());
    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks[3].kind, BlockKind::Paragraph);

    ed.toggle_block_kind(BlockKind::Blockquote);
    assert_eq!(ed.doc.blocks[3].kind, BlockKind::Blockquote);
    ed.backspace();
    assert_eq!(ed.doc.blocks[3].kind, BlockKind::Paragraph);
}

#[test]
fn test_smart_typing_markdown_punctuation_and_wrapping() {
    let prefs = Preferences::default();
    let mut ed = EditorState::new(TextDirection::Auto);

    for ch in "# ".chars() {
        ed.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed.doc.blocks[0].kind, BlockKind::Heading(1));
    assert_eq!(ed.doc.blocks[0].plain_text(), "");

    ed.insert_text("Title", Some(&prefs));
    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks[1].kind, BlockKind::Paragraph);

    for ch in "Say \"hi\" -- a -> b... **strong** and *soft* and ~~gone~~".chars() {
        ed.insert_text(&ch.to_string(), Some(&prefs));
    }
    let text = ed.doc.blocks[1].plain_text();
    assert!(text.contains("“hi”"), "got: {text}");
    assert!(text.contains('–'), "got: {text}");
    assert!(text.contains('→'), "got: {text}");
    assert!(text.contains('…'), "got: {text}");
    assert!(text.contains("strong"));
    assert!(!text.contains("**"));

    assert!(ed.doc.blocks[1].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.text == "strong" && r.style.bold
    )));
    assert!(ed.doc.blocks[1].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.text == "soft" && r.style.italic
    )));
    assert!(ed.doc.blocks[1].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.text == "gone" && r.style.strikethrough
    )));

    ed.insert_paragraph();
    for ch in "----".chars() {
        ed.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed.doc.blocks[2].plain_text(), "---");
    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks[2].kind, BlockKind::HorizontalRule);
    assert_eq!(ed.doc.blocks[3].kind, BlockKind::Paragraph);

    for ch in "[x]".chars() {
        ed.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert!(matches!(
        ed.doc.blocks[3].inlines.first(),
        Some(Inline::Checkbox { checked: true })
    ));

    ed.insert_paragraph();
    ed.insert_text("note", None);
    ed.set_selection(Selection::range(DocPos::new(4, 0), DocPos::new(4, 4)));
    ed.insert_text("(", Some(&prefs));
    assert_eq!(ed.doc.blocks[4].plain_text(), "(note)");

    let mut ed2 = EditorState::new(TextDirection::Auto);
    for ch in "##Heading".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[0].kind, BlockKind::Heading(2));
    assert_eq!(ed2.doc.blocks[0].plain_text(), "Heading");
    assert_eq!(ed2.selection.focus, DocPos::new(0, 7));

    ed2.insert_paragraph();
    for ch in "#Title".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[1].kind, BlockKind::Heading(1));
    assert_eq!(ed2.doc.blocks[1].plain_text(), "Title");

    ed2.insert_paragraph();
    for ch in "###Sub".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[2].kind, BlockKind::Heading(3));
    assert_eq!(ed2.doc.blocks[2].plain_text(), "Sub");

    ed2.insert_paragraph();
    for ch in "##عنوان".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[3].kind, BlockKind::Heading(2));
    assert_eq!(ed2.doc.blocks[3].plain_text(), "عنوان");

    ed2.insert_paragraph();
    for ch in "####Literal".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[4].kind, BlockKind::Paragraph);
    assert_eq!(ed2.doc.blocks[4].plain_text(), "####Literal");

    ed2.insert_paragraph();
    for ch in "#h".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[5].kind, BlockKind::Heading(1));
    assert_eq!(ed2.doc.blocks[5].plain_text(), "h");
    ed2.undo();
    assert_eq!(ed2.doc.blocks[5].kind, BlockKind::Paragraph);
    assert_eq!(ed2.doc.blocks[5].plain_text(), "#h");
    for ch in "ashtag".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[5].kind, BlockKind::Paragraph);
    assert_eq!(ed2.doc.blocks[5].plain_text(), "#hashtag");

    ed2.insert_paragraph();
    for ch in ">Quote".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[6].kind, BlockKind::Blockquote);
    assert_eq!(ed2.doc.blocks[6].plain_text(), "Quote");

    ed2.insert_paragraph();
    for ch in ">>".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[7].kind, BlockKind::Paragraph);
    assert_eq!(ed2.doc.blocks[7].plain_text(), "»");

    ed2.insert_paragraph();
    for ch in ">->".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[8].kind, BlockKind::Paragraph);
    assert_eq!(ed2.doc.blocks[8].plain_text(), "↣");

    ed2.insert_paragraph();
    for ch in ">= 5".chars() {
        ed2.insert_text(&ch.to_string(), Some(&prefs));
    }
    assert_eq!(ed2.doc.blocks[9].kind, BlockKind::Paragraph);
    assert_eq!(ed2.doc.blocks[9].plain_text(), ">= 5");
}

#[test]
fn test_unicode_and_rtl_roundtrip() {
    let mut ed = EditorState::new(TextDirection::Auto);
    let arabic = "مرحبا بالعالم — اختبار النص العربي";
    let hebrew = "שלום עולם";
    let cjk_emoji = "日本語テスト 🎉 café e\u{0301}";

    ed.insert_text(arabic, None);
    ed.set_direction(TextDirection::Rtl);
    ed.insert_paragraph();
    ed.insert_text(hebrew, None);
    ed.set_direction(TextDirection::Rtl);
    ed.insert_paragraph();
    ed.insert_text(cjk_emoji, None);
    ed.set_direction(TextDirection::Ltr);

    ed.set_selection(Selection::range(DocPos::new(0, 6), DocPos::new(0, 13)));
    ed.toggle_inline_attr(InlineAttr::Bold);
    assert!(ed.toolbar_state().bold);

    let hits = find_matches(&ed.doc, "بالعالم");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].start, DocPos::new(0, 6));
    assert_eq!(hits[0].end, DocPos::new(0, 13));

    let html = document_to_html(&ed.doc);
    assert!(html.contains("direction:rtl"));
    assert!(html.contains("direction:ltr"));
    let restored_html = html_to_document(&html);
    assert_eq!(restored_html.blocks[0].direction, TextDirection::Rtl);
    assert_eq!(restored_html.blocks[0].plain_text(), arabic);
    assert_eq!(restored_html.blocks[1].plain_text(), hebrew);
    assert_eq!(restored_html.blocks[2].plain_text(), cjk_emoji);

    let docx_bytes = document_to_docx_bytes(&ed.doc).unwrap();
    let restored_docx = docx_bytes_to_document(&docx_bytes).unwrap();
    assert_eq!(restored_docx.blocks[0].direction, TextDirection::Rtl);
    assert_eq!(restored_docx.blocks[0].plain_text(), arabic);
    assert_eq!(restored_docx.blocks[2].plain_text(), cjk_emoji);
}

#[test]
fn test_save_and_reopen_all_formats_and_pdf() {
    let dir = temp_test_dir("formats");

    let doc = Document::from_blocks(vec![
        Block::new(BlockKind::Heading(1))
            .with_inlines(vec![Inline::Text(TextRun::plain("Chapter One"))]),
        Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Text(TextRun::plain("Intro with ")),
            Inline::Text(TextRun::new(
                "bold",
                InlineStyle {
                    bold: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(", ")),
            Inline::Text(TextRun::new(
                "underlined",
                InlineStyle {
                    underline: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(", and ")),
            Inline::Text(TextRun::new(
                "a link",
                InlineStyle {
                    link: Some("https://example.org".into()),
                    ..Default::default()
                },
            )),
            Inline::Comment {
                text: "Author note".into(),
            },
        ]),
        Block::new(BlockKind::Blockquote)
            .with_inlines(vec![Inline::Text(TextRun::plain("Quiet writing."))]),
        Block::new(BlockKind::BulletListItem { depth: 0 }).with_inlines(vec![
            Inline::Checkbox { checked: true },
            Inline::Text(TextRun::plain(" Done task")),
        ]),
        Block::new(BlockKind::HorizontalRule),
        Block::new(BlockKind::CodeBlock {
            language: Some("rust".into()),
        })
        .with_inlines(vec![Inline::Text(TextRun::plain("fn main() {}"))]),
    ]);

    let html_path = dir.join("story.html");
    let out_html = save_document(&doc, &html_path).unwrap();
    assert!(out_html.warning.is_none());
    let (reopened_html, _) = load_document(&html_path).unwrap();
    assert_eq!(reopened_html.blocks.len(), doc.blocks.len());
    assert_eq!(reopened_html.blocks[0].kind, BlockKind::Heading(1));
    assert!(reopened_html.has_underline());
    assert!(reopened_html.blocks[1].inlines.iter().any(|i| matches!(
        i,
        Inline::Comment { text } if text == "Author note"
    )));

    let md_path = dir.join("story.md");
    let out_md = save_document(&doc, &md_path).unwrap();
    assert!(out_md.warning.is_none());
    let md_raw = std::fs::read_to_string(&md_path).unwrap();
    assert!(md_raw.contains("<!--Author note-->"));
    assert!(md_raw.contains("<u>underlined</u>"));
    let (reopened_md, _) = load_document(&md_path).unwrap();
    assert_eq!(reopened_md.blocks[0].kind, BlockKind::Heading(1));
    assert_eq!(reopened_md.blocks[0].plain_text(), "Chapter One");
    assert!(reopened_md.has_underline());

    let txt_path = dir.join("story.txt");
    let out_txt = save_document(&doc, &txt_path).unwrap();
    assert!(out_txt.warning.is_some());
    let (reopened_txt, fmt_txt) = load_document(&txt_path).unwrap();
    assert_eq!(fmt_txt, sillpaper::io::FileFormat::PlainText);
    assert_eq!(reopened_txt.blocks[0].plain_text(), "Chapter One");
    assert!(reopened_txt.blocks[1].plain_text().contains("underlined"));

    let plain_doc = Document::from_blocks(vec![
        Block::paragraph_with_text("Line one plain."),
        Block::paragraph_with_text("Line two plain."),
    ]);
    let plain_txt_path = dir.join("plain.txt");
    let out_plain = save_document(&plain_doc, &plain_txt_path).unwrap();
    assert!(out_plain.warning.is_none());
    let (reopened_plain, _) = load_document(&plain_txt_path).unwrap();
    assert_eq!(reopened_plain.blocks.len(), 2);
    assert_eq!(reopened_plain.blocks[0].plain_text(), "Line one plain.");
    assert_eq!(reopened_plain.blocks[1].plain_text(), "Line two plain.");

    let docx_path = dir.join("story.docx");
    save_document(&doc, &docx_path).unwrap();
    let (reopened_docx, _) = load_document(&docx_path).unwrap();
    assert_eq!(reopened_docx.blocks[0].kind, BlockKind::Heading(1));
    assert!(reopened_docx.has_underline());
    assert!(reopened_docx.blocks[1].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.style.link.as_deref() == Some("https://example.org")
    )));

    let pdf_path = dir.join("story.pdf");
    sillpaper::io::print::export_document_to_pdf(&doc, &Preferences::default(), &pdf_path).unwrap();
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"), "Expected valid PDF header");
    assert!(pdf_bytes.len() > 1000);

    let toc = extract_toc(&doc);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].slug, heading_slug("Chapter One"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_autosave_and_crash_recovery_including_corrupt_json_fallback() {
    let dir = temp_test_dir("recovery");
    let storage = StorageManager::new(dir.clone());

    let doc1 = Document::from_blocks(vec![
        Block::new(BlockKind::Heading(1))
            .with_inlines(vec![Inline::Text(TextRun::plain("Draft 1"))]),
        Block::paragraph_with_text("Unsaved prose before crash"),
    ]);
    let doc2 = Document::from_blocks(vec![Block::paragraph_with_text("Second tab content")]);

    let b1 = StorageManager::build_backup("tab_a", &doc1, None, false, None, 14);
    let b2 = StorageManager::build_backup(
        "tab_b",
        &doc2,
        Some(dir.join("file2.html")),
        true,
        Some(12345),
        5,
    );

    storage.save_tab_backup(&b1, 1).unwrap();
    storage.save_tab_backup(&b2, 1).unwrap();
    storage
        .save_session_order(&SessionOrder {
            active_tab_id: Some("tab_b".into()),
            tab_ids: vec!["tab_b".into(), "tab_a".into()],
        })
        .unwrap();

    let (restored, active_id) = storage.restore_all_backups();
    assert_eq!(active_id.as_deref(), Some("tab_b"));
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].0.id, "tab_b");
    assert_eq!(restored[1].0.id, "tab_a");
    assert!(!restored[1].0.is_saved);
    assert_eq!(restored[1].0.cursor_flat_offset, 14);
    assert_eq!(
        restored[1].1.blocks[1].plain_text(),
        "Unsaved prose before crash"
    );

    std::fs::write(storage.backup_json_path("tab_a"), "{corrupted json").unwrap();
    let (restored2, _) = storage.restore_all_backups();
    let recovered_a = restored2.iter().find(|(b, _)| b.id == "tab_a").unwrap();
    assert_eq!(
        recovered_a.1.blocks[1].plain_text(),
        "Unsaved prose before crash"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_all_themes_and_css_generation() {
    for theme in ThemeId::ALL {
        let prefs = Preferences {
            theme,
            ..Default::default()
        };
        let pal = resolve_palette(&prefs);
        assert!(!pal.background.is_empty());
        assert!(!pal.text.is_empty());
        let css = generate_app_css(&prefs);
        assert!(css.contains("textview.sillpaper-editor"));
        assert!(css.contains(&pal.background));
    }
}

#[test]
fn test_large_document_performance_50k_words() {
    let para_text = "The quiet room held only the soft rhythm of thoughtful sentences unfolding across a clean page without clutter or distraction. "
        .repeat(5);
    let mut blocks = Vec::with_capacity(550);
    for i in 0..500 {
        if i % 25 == 0 {
            blocks.push(
                Block::new(BlockKind::Heading(2)).with_inlines(vec![Inline::Text(TextRun::plain(
                    format!("Section {}", i / 25 + 1),
                ))]),
            );
        }
        blocks.push(Block::paragraph_with_text(para_text.clone()));
    }
    let doc = Document::from_blocks(blocks);

    let t_stats = Instant::now();
    let stats = compute_stats(&doc, None, 270);
    let stats_ms = t_stats.elapsed().as_millis();
    assert!(stats.words >= 50_000, "words = {}", stats.words);
    assert!(stats_ms < 250, "stats took {stats_ms}ms");

    let mut ed = EditorState::from_document(doc, TextDirection::Auto);

    ed.set_selection(Selection::collapsed(DocPos::new(250, 10)));
    let t_edit = Instant::now();
    for i in 0..500 {
        let ch = if i % 6 == 5 { " " } else { "a" };
        ed.insert_text(ch, None);
    }
    let edit_us_per_char = t_edit.elapsed().as_micros() / 500;
    assert!(
        edit_us_per_char < 500,
        "Expected <500us/keystroke on 50k words, got {edit_us_per_char}us"
    );

    let t_fmt = Instant::now();
    ed.set_selection(Selection::range(DocPos::new(250, 0), DocPos::new(250, 40)));
    ed.toggle_inline_attr(InlineAttr::Bold);
    ed.undo();
    ed.redo();
    let fmt_ms = t_fmt.elapsed().as_millis();
    assert!(fmt_ms < 50, "format + undo/redo took {fmt_ms}ms");

    let t_search = Instant::now();
    let matches = find_matches(&ed.doc, "distraction");
    let search_ms = t_search.elapsed().as_millis();
    assert_eq!(matches.len(), 2500);
    assert!(search_ms < 300, "search took {search_ms}ms");

    let t_html = Instant::now();
    let html = document_to_html(&ed.doc);
    let html_ms = t_html.elapsed().as_millis();
    assert!(html.len() > 250_000);
    assert!(html_ms < 300, "HTML serialize took {html_ms}ms");

    let t_md = Instant::now();
    let md = document_to_markdown(&ed.doc);
    let md_ms = t_md.elapsed().as_millis();
    assert!(md.markdown.len() > 250_000);
    assert!(md_ms < 300, "Markdown serialize took {md_ms}ms");
    let _ = markdown_to_document("# Quick smoke");
}

fn write_test_png(path: &std::path::Path) {
    let pixbuf =
        gtk4::gdk_pixbuf::Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, true, 8, 320, 180)
            .unwrap();
    pixbuf.fill(0x1F73B8FF);
    for y in 24..156 {
        for x in 24..296 {
            pixbuf.put_pixel(x, y, 245, 199, 64, 255);
        }
    }
    pixbuf.savev(path, "png", &[]).unwrap();
}

#[test]
fn test_markdown_edge_cases_and_nested_list_roundtrip() {
    let doc = Document::from_blocks(vec![
        Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Text(TextRun::plain("Before")),
            Inline::Text(TextRun::new(
                " bold with spaces ",
                InlineStyle {
                    bold: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::new(
                "italic_word",
                InlineStyle {
                    italic: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(" and ")),
            Inline::Text(TextRun::new(
                "both",
                InlineStyle {
                    bold: true,
                    underline: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(" code: ")),
            Inline::Text(TextRun::new(
                "`tick`",
                InlineStyle {
                    code: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(" link: ")),
            Inline::Text(TextRun::new(
                "docs",
                InlineStyle {
                    link: Some("https://example.org/my (doc)".into()),
                    ..Default::default()
                },
            )),
        ]),
        Block::new(BlockKind::OrderedListItem {
            depth: 0,
            number: 1,
        })
        .with_inlines(vec![Inline::Text(TextRun::plain("Top ordered"))]),
        Block::new(BlockKind::OrderedListItem {
            depth: 1,
            number: 1,
        })
        .with_inlines(vec![Inline::Text(TextRun::plain("Nested ordered"))]),
        Block::new(BlockKind::BulletListItem { depth: 2 })
            .with_inlines(vec![Inline::Text(TextRun::plain("Deep bullet"))]),
        Block::new(BlockKind::CodeBlock {
            language: Some("markdown".into()),
        })
        .with_inlines(vec![Inline::Text(TextRun::plain(
            "```rust\nfn hi() {}\n```",
        ))]),
    ]);

    let res = document_to_markdown(&doc);
    assert!(!res.converted_underline);
    assert!(
        res.markdown.contains(" **bold with spaces** "),
        "got:\n{}",
        res.markdown
    );
    assert!(
        res.markdown.contains("**<u>both</u>**"),
        "got:\n{}",
        res.markdown
    );
    assert!(
        res.markdown.contains("`` `tick` ``"),
        "got:\n{}",
        res.markdown
    );
    assert!(
        res.markdown.contains("````markdown\n"),
        "got:\n{}",
        res.markdown
    );
    assert!(
        res.markdown
            .contains("[docs](<https://example.org/my (doc)>)"),
        "got:\n{}",
        res.markdown
    );

    let rt = markdown_to_document(&res.markdown);
    assert!(rt.has_underline());
    assert!(rt.blocks[0].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.text == "bold with spaces" && r.style.bold
    )));
    assert!(rt.blocks[0].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.text == "`tick`" && r.style.code
    )));
    assert_eq!(
        rt.blocks[1].kind,
        BlockKind::OrderedListItem {
            depth: 0,
            number: 1
        }
    );
    assert_eq!(
        rt.blocks[2].kind,
        BlockKind::OrderedListItem {
            depth: 1,
            number: 1
        }
    );
    assert_eq!(rt.blocks[3].kind, BlockKind::BulletListItem { depth: 2 });
    assert_eq!(rt.blocks[4].plain_text(), "```rust\nfn hi() {}\n```");
}

#[test]
fn test_docx_comprehensive_ooxml_images_comments_rtl_and_prefs() {
    let dir = temp_test_dir("docx_full");
    let img_path = dir.join("sample.png");
    write_test_png(&img_path);

    let doc = Document::from_blocks(vec![
        Block::new(BlockKind::Heading(1))
            .with_inlines(vec![Inline::Text(TextRun::plain("Export Verification"))]),
        Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Text(TextRun::plain("Control char test\u{000B} & <xml> ")),
            Inline::Text(TextRun::new(
                "inline_code()",
                InlineStyle {
                    code: true,
                    ..Default::default()
                },
            )),
            Inline::Comment {
                text: "Check typography & spacing".into(),
            },
        ]),
        Block::new(BlockKind::Blockquote).with_inlines(vec![Inline::Text(TextRun::plain(
            "مرحبا بالعالم في اقتباس عربي",
        ))]),
        Block::new(BlockKind::BulletListItem { depth: 0 }).with_inlines(vec![
            Inline::Checkbox { checked: true },
            Inline::Text(TextRun::plain(" Checked item")),
        ]),
        Block::new(BlockKind::BulletListItem { depth: 1 }).with_inlines(vec![
            Inline::Checkbox { checked: false },
            Inline::Text(TextRun::plain("Unchecked item")),
        ]),
        Block::new(BlockKind::Paragraph).with_inlines(vec![Inline::Image {
            src: "sample.png".into(),
            alt: "Blue & gold banner".into(),
            original_src: None,
        }]),
    ]);

    let prefs = Preferences {
        font: FontChoice::Sans,
        line_spacing: LineSpacing::Spacious,
        paragraph_margins: false,
        ..Default::default()
    };

    let bytes = document_to_docx_bytes_with_options(&doc, Some(&dir), Some(&prefs)).unwrap();

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut found_media = false;
    let mut found_comments = false;
    let mut styles_xml = String::new();
    let mut doc_xml = String::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        if name == "word/media/image1.png" {
            found_media = true;
        }
        if name == "word/comments.xml" {
            found_comments = true;
        }
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let mut xml = String::new();
            entry.read_to_string(&mut xml).unwrap();
            assert!(!xml.contains('\u{000B}'), "Illegal XML char in {name}");
            let mut reader = quick_xml::Reader::from_str(&xml);
            let mut buf = Vec::new();
            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(quick_xml::events::Event::Eof) => break,
                    Ok(_) => {}
                    Err(e) => panic!("Malformed XML in {name}: {e}"),
                }
                buf.clear();
            }
            if name == "word/styles.xml" {
                styles_xml = xml;
            } else if name == "word/document.xml" {
                doc_xml = xml;
            }
        }
    }
    assert!(found_media, "Expected embedded PNG inside DOCX");
    assert!(found_comments, "Expected word/comments.xml inside DOCX");
    assert!(styles_xml.contains("Liberation Sans"));
    assert!(styles_xml.contains("w:line=\"348\""));
    assert!(doc_xml.contains("<w:bidi/>"));
    assert!(doc_xml.contains("w:right w:val=\"single\""));
    assert!(doc_xml.contains("wp:cNvGraphicFramePr"));

    let rt = docx_bytes_to_document(&bytes).unwrap();
    assert_eq!(rt.blocks[0].kind, BlockKind::Heading(1));
    assert!(
        rt.blocks[1]
            .plain_text()
            .contains("Control char test & <xml> "),
        "Expected XML entities to round-trip, got: {:?}",
        rt.blocks[1].plain_text()
    );
    assert!(rt.blocks[1].inlines.iter().any(|i| matches!(
        i,
        Inline::Comment { text } if text == "Check typography & spacing"
    )));
    assert!(
        rt.blocks[3]
            .inlines
            .iter()
            .any(|i| matches!(i, Inline::Checkbox { checked: true }))
    );
    assert!(
        rt.blocks[4]
            .inlines
            .iter()
            .any(|i| matches!(i, Inline::Checkbox { checked: false }))
    );
    assert_eq!(rt.blocks[2].direction, TextDirection::Rtl);
    assert!(
        rt.blocks[5].inlines.iter().any(|i| matches!(
            i,
            Inline::Image { src, alt, .. }
                if src.starts_with("data:image/png;base64,") && alt == "Blue & gold banner"
        )),
        "Expected embedded PNG image to round-trip from DOCX as data URI, got: {:?}",
        rt.blocks[5].inlines
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_html5_prefs_and_plain_text_formatting() {
    let dir = temp_test_dir("html_txt");
    let doc = Document::from_blocks(vec![
        Block::new(BlockKind::Heading(1)).with_inlines(vec![Inline::Text(TextRun::plain("Title"))]),
        Block::new(BlockKind::Blockquote)
            .with_inlines(vec![Inline::Text(TextRun::plain("A calm quote"))]),
        Block::new(BlockKind::BulletListItem { depth: 0 }).with_inlines(vec![
            Inline::Checkbox { checked: true },
            Inline::Text(TextRun::plain(" Done")),
        ]),
        Block::new(BlockKind::BulletListItem { depth: 1 })
            .with_inlines(vec![Inline::Text(TextRun::plain("Nested"))]),
        Block::new(BlockKind::OrderedListItem {
            depth: 0,
            number: 1,
        })
        .with_inlines(vec![Inline::Text(TextRun::plain("First"))]),
        Block::new(BlockKind::HorizontalRule),
    ]);

    let prefs = Preferences {
        font: FontChoice::Monospace,
        text_width: TextWidth::Narrow,
        line_spacing: LineSpacing::Relaxed,
        ..Default::default()
    };
    let html = document_to_html_with_prefs(&doc, &prefs);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<meta charset=\"utf-8\">"));
    assert!(html.contains("max-width: 480px"));
    assert!(html.contains("line-height: 1.60"));
    assert!(LineSpacing::Compact.factor() >= 1.0);
    assert!(LineSpacing::Compact.factor() < LineSpacing::Normal.factor());
    assert!(html.contains("border-inline-start:"));
    assert!(html.contains("dir=\"auto\""));

    let html_path = dir.join("styled.html");
    save_document_with_prefs(&doc, &html_path, &prefs).unwrap();
    let (rt, _) = load_document(&html_path).unwrap();
    assert_eq!(rt.blocks.len(), doc.blocks.len());
    assert_eq!(rt.blocks[0].direction, TextDirection::Auto);
    assert_eq!(rt.blocks[3].kind, BlockKind::BulletListItem { depth: 1 });

    let txt = document_to_plain_text(&doc);
    assert!(txt.contains("> A calm quote"), "got:\n{txt}");
    assert!(
        txt.contains("- [x] Done\n  - Nested\n\n1. First"),
        "got:\n{txt}"
    );
    assert!(txt.contains("\n\n---\n"), "got:\n{txt}");
    assert!(
        !txt.contains("[x]  Done"),
        "unexpected double space:\n{txt}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_pdf_multipage_rich_export_with_image_links_rtl_and_checkboxes() {
    let keep_dir = std::env::var("SILLPAPER_DUMP_DIR").ok().map(PathBuf::from);
    let dir = match &keep_dir {
        Some(d) => {
            std::fs::create_dir_all(d).unwrap();
            d.clone()
        }
        None => temp_test_dir("pdf_rich"),
    };
    let img_path = dir.join("chart.png");
    write_test_png(&img_path);

    let long_para = "A calm writing environment keeps the mind focused on rhythm, clarity, and structure without visual noise or sluggish redraws. "
        .repeat(32);
    let long_code = (1..=48)
        .map(|i| format!("let line_{i:02} = compute_layout_metrics({i});"))
        .collect::<Vec<_>>()
        .join("\n");

    let doc = Document::from_blocks(vec![
        Block::new(BlockKind::Heading(1))
            .with_inlines(vec![Inline::Text(TextRun::plain("Sillpaper Export Showcase"))]),
        Block::new(BlockKind::Heading(2))
            .with_inlines(vec![Inline::Text(TextRun::plain("Typography, Formatting & Links"))]),
        Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Text(TextRun::plain("Mixed ")),
            Inline::Text(TextRun::new(
                "bold",
                InlineStyle {
                    bold: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(", ")),
            Inline::Text(TextRun::new(
                "italic",
                InlineStyle {
                    italic: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(", ")),
            Inline::Text(TextRun::new(
                "underlined",
                InlineStyle {
                    underline: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(", ")),
            Inline::Text(TextRun::new(
                "struck",
                InlineStyle {
                    strikethrough: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(", ")),
            Inline::Text(TextRun::new(
                "inline_code()",
                InlineStyle {
                    code: true,
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(", and ")),
            Inline::Text(TextRun::new(
                "clickable link",
                InlineStyle {
                    link: Some("https://www.rust-lang.org".into()),
                    ..Default::default()
                },
            )),
            Inline::Text(TextRun::plain(".")),
            Inline::Comment {
                text: "Hidden in PDF, preserved in HTML/MD/DOCX".into(),
            },
        ]),
        Block::new(BlockKind::Blockquote)
            .with_inlines(vec![Inline::Text(TextRun::plain("Simplicity is the ultimate sophistication — clarity in typography lets words breathe without distraction."))]),
        Block::new(BlockKind::Blockquote)
            .with_inlines(vec![Inline::Text(TextRun::plain("الكتابة الهادئة تصنع أفكاراً واضحة وتمنح الكلمات مساحة للتنفس."))]),
        Block::new(BlockKind::BulletListItem { depth: 0 }).with_inlines(vec![
            Inline::Checkbox { checked: true },
            Inline::Text(TextRun::plain(" Completed task with crisp vector checkbox")),
        ]),
        Block::new(BlockKind::BulletListItem { depth: 1 }).with_inlines(vec![
            Inline::Checkbox { checked: false },
            Inline::Text(TextRun::plain(" Pending nested sub-task")),
        ]),
        Block::new(BlockKind::OrderedListItem {
            depth: 0,
            number: 1,
        })
        .with_inlines(vec![Inline::Text(TextRun::plain("First numbered item"))]),
        Block::new(BlockKind::OrderedListItem {
            depth: 0,
            number: 2,
        })
        .with_inlines(vec![Inline::Text(TextRun::plain("Second numbered item"))]),
        Block::new(BlockKind::BulletListItem { depth: 0 }).with_inlines(vec![
            Inline::Checkbox { checked: true },
            Inline::Text(TextRun::plain(" مهمة عربية مكتملة داخل قائمة")),
        ]),
        Block::new(BlockKind::HorizontalRule),
        Block::new(BlockKind::Paragraph).with_inlines(vec![Inline::Image {
            src: "chart.png".into(),
            alt: "Sample banner".into(),
            original_src: None,
        }]),
        Block::new(BlockKind::Heading(2))
            .with_inlines(vec![Inline::Text(TextRun::plain("Multi-Page Pagination Check"))]),
        Block::paragraph_with_text(long_para),
        Block::new(BlockKind::Heading(3))
            .with_inlines(vec![Inline::Text(TextRun::plain("Multi-Page Code Block"))]),
        Block::new(BlockKind::CodeBlock {
            language: Some("rust".into()),
        })
        .with_inlines(vec![Inline::Text(TextRun::plain(long_code))]),
    ]);

    let prefs = Preferences::default();
    let pdf_path = dir.join("showcase.pdf");
    sillpaper::io::print::export_document_to_pdf_with_base(&doc, &prefs, &pdf_path, Some(&dir))
        .unwrap();

    if keep_dir.is_some() {
        save_document_with_prefs(&doc, &dir.join("showcase.html"), &prefs).unwrap();
        save_document_with_prefs(&doc, &dir.join("showcase.md"), &prefs).unwrap();
        save_document_with_prefs(&doc, &dir.join("showcase.txt"), &prefs).unwrap();
        save_document_with_prefs(&doc, &dir.join("showcase.docx"), &prefs).unwrap();
    }

    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));
    let pdf_lossy = String::from_utf8_lossy(&pdf_bytes);
    assert!(
        pdf_lossy.contains("/Subtype /Image"),
        "Expected embedded image XObject in PDF"
    );
    assert!(
        pdf_lossy.contains("/Type /ObjStm") || pdf_lossy.contains("https://www.rust-lang.org"),
        "Expected PDF 1.7 structure / link annotation stream"
    );
    assert!(pdf_bytes.len() > 8000);

    if let Ok(out) = std::process::Command::new("pdftotext")
        .arg(&pdf_path)
        .arg("-")
        .output()
        && out.status.success()
    {
        let extracted = String::from_utf8_lossy(&out.stdout);
        assert!(extracted.contains("Sillpaper Export Showcase"));
        assert!(extracted.contains("clickable link"));
        assert!(extracted.contains("let line_01 = compute_layout_metrics(1);"));
        assert!(extracted.contains("let line_48 = compute_layout_metrics(48);"));
        assert!(extracted.contains("1 / ") || extracted.contains("1/"));
    }

    if keep_dir.is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn test_docx_complex_ooxml_alternate_content_styles_numbering_images_and_tables() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let dir = temp_test_dir("docx_complex_ooxml");
    let png_path = dir.join("avatar.png");
    write_test_png(&png_path);
    let png_bytes = std::fs::read(&png_path).unwrap();

    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
</Types>"#;

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdImg1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/avatar.png"/>
  <Relationship Id="rIdLink1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="mailto:ada@example.org" TargetMode="External"/>
</Relationships>"#;

    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="berschrift1">
    <w:name w:val="Überschrift 1"/>
  </w:style>
  <w:style w:type="paragraph" w:customStyle="1" w:styleId="Aufzhlung">
    <w:name w:val="Aufzählung"/>
    <w:pPr><w:numPr><w:numId w:val="2"/></w:numPr></w:pPr>
    <w:rPr><w:b/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:customStyle="1" w:styleId="Flietext">
    <w:name w:val="Fließtext"/>
    <w:basedOn w:val="Aufzhlung"/>
    <w:pPr><w:numPr><w:numId w:val="0"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:type="character" w:styleId="StrongChar">
    <w:name w:val="Strong"/>
    <w:rPr><w:b/></w:rPr>
  </w:style>
</w:styles>"#;

    let numbering = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:pStyle w:val="Aufzhlung"/></w:lvl>
    <w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/></w:lvl>
    <w:lvl w:ilvl="2"><w:numFmt w:val="none"/></w:lvl>
  </w:abstractNum>
  <w:num w:numId="2"><w:abstractNumId w:val="0"/></w:num>
</w:numbering>"#;

    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
            xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
            xmlns:v="urn:schemas-microsoft-com:vml"
            xmlns:o="urn:schemas-microsoft-com:office:office"
            xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">
  <w:body>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:anchor>
            <wp:docPr id="1" name="Profile Photo"/>
            <a:graphic><a:graphicData><pic:pic><pic:blipFill>
              <a:blip r:embed="rIdImg1"/>
            </pic:blipFill></pic:pic></a:graphicData></a:graphic>
          </wp:anchor>
        </w:drawing>
      </w:r>
      <w:r>
        <mc:AlternateContent>
          <mc:Choice Requires="wps">
            <w:drawing><wp:anchor><a:graphic><a:graphicData><wps:wsp><wps:txbx>
              <w:txbxContent>
                <w:p><w:pPr><w:pStyle w:val="berschrift1"/></w:pPr><w:r><w:t>Ada Lovelace</w:t></w:r></w:p>
                <w:p><w:r><w:rPr><w:i/></w:rPr><w:t>Curriculum Vitae</w:t></w:r></w:p>
              </w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing>
          </mc:Choice>
          <mc:Fallback>
            <w:pict><v:rect o:gfxdata="HUGEBASE64BLOB"><v:textbox>
              <w:txbxContent>
                <w:p><w:pPr><w:pStyle w:val="berschrift1"/></w:pPr><w:r><w:t>Ada Lovelace</w:t></w:r></w:p>
                <w:p><w:r><w:rPr><w:i/></w:rPr><w:t>Curriculum Vitae</w:t></w:r></w:p>
              </w:txbxContent>
            </v:textbox></v:rect></w:pict>
          </mc:Fallback>
        </mc:AlternateContent>
      </w:r>
    </w:p>
    <w:p><w:pPr><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:t xml:space="preserve">2020 - now</w:t><w:tab/><w:t>Lead Engineer</w:t></w:r></w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="Aufzhlung"/><w:tabs><w:tab w:val="clear" w:pos="1985"/></w:tabs></w:pPr>
      <w:r><w:rPr><w:b w:val="0"/></w:rPr><w:t>Built analytical engine</w:t></w:r>
      <w:del><w:r><w:delText>DELETED TEXT</w:delText></w:r></w:del>
    </w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="Aufzhlung"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="0"/></w:numPr></w:pPr>
    </w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="Flietext"/></w:pPr>
      <w:r><w:rPr><w:rStyle w:val="StrongChar"/></w:rPr><w:t>Summary</w:t></w:r>
      <w:r><w:t xml:space="preserve"> with </w:t></w:r>
      <w:r><w:fldChar w:fldCharType="begin"/></w:r>
      <w:r><w:instrText> HYPERLINK "https://example.org/ada" </w:instrText></w:r>
      <w:r><w:fldChar w:fldCharType="separate"/></w:r>
      <w:r><w:t>portfolio</w:t></w:r>
      <w:r><w:fldChar w:fldCharType="end"/></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="2"/></w:numPr></w:pPr>
      <w:r><w:t>Ordered level 1 item</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:numPr><w:ilvl w:val="2"/><w:numId w:val="2"/></w:numPr></w:pPr>
      <w:r><w:t>Unnumbered level 2 paragraph</w:t></w:r>
    </w:p>
    <w:p>
      <w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val="1"/></w14:checkbox></w:sdtPr><w:sdtContent/></w:sdt>
      <w:r><w:t xml:space="preserve"> Shipped</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:pict><v:shape><v:imagedata r:id="rIdImg1" o:title="Legacy VML Badge"/></v:shape></w:pict></w:r>
    </w:p>
    <w:tbl>
      <w:tr>
        <w:tc><w:p><w:r><w:t>Language</w:t></w:r></w:p></w:tc>
        <w:tc><w:p><w:r><w:t>Rust</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cur);
        let opts = SimpleFileOptions::default();
        for (name, data) in [
            ("[Content_Types].xml", content_types.as_bytes()),
            ("word/_rels/document.xml.rels", rels.as_bytes()),
            ("word/styles.xml", styles.as_bytes()),
            ("word/numbering.xml", numbering.as_bytes()),
            ("word/document.xml", document_xml.as_bytes()),
            ("word/media/avatar.png", png_bytes.as_slice()),
        ] {
            zw.start_file(name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }

    let doc = docx_bytes_to_document(&cur.into_inner()).unwrap();
    let full_text = doc.plain_text(false);

    assert_eq!(full_text.matches("Ada Lovelace").count(), 1);
    assert_eq!(full_text.matches("Curriculum Vitae").count(), 1);
    assert!(!full_text.contains("DELETED TEXT"));
    assert!(!full_text.contains("HYPERLINK"));

    assert!(matches!(
        &doc.blocks[0].inlines[..],
        [Inline::Image { src, alt, .. }]
            if src.starts_with("data:image/png;base64,") && alt == "Profile Photo"
    ));
    assert_eq!(doc.blocks[1].kind, BlockKind::Heading(1));
    assert_eq!(doc.blocks[1].plain_text(), "Ada Lovelace");
    assert_eq!(doc.blocks[2].plain_text(), "Curriculum Vitae");
    assert!(matches!(
        &doc.blocks[2].inlines[0],
        Inline::Text(r) if r.style.italic
    ));

    assert_eq!(doc.blocks[3].plain_text(), "2020 - now\tLead Engineer");
    assert!(matches!(
        &doc.blocks[3].inlines[0],
        Inline::Text(r) if !r.style.bold
    ));

    assert_eq!(doc.blocks[4].kind, BlockKind::BulletListItem { depth: 0 });
    assert_eq!(doc.blocks[4].plain_text(), "Built analytical engine");
    assert!(!doc.blocks[4].plain_text().starts_with('\t'));

    assert_eq!(doc.blocks[5].kind, BlockKind::Paragraph);
    assert_eq!(doc.blocks[5].plain_text(), "Summary with portfolio");
    assert!(doc.blocks[5].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.text == "Summary" && r.style.bold
    )));
    assert!(doc.blocks[5].inlines.iter().any(|i| matches!(
        i,
        Inline::Text(r) if r.text == "portfolio" && r.style.link.as_deref() == Some("https://example.org/ada")
    )));

    assert_eq!(
        doc.blocks[6].kind,
        BlockKind::OrderedListItem {
            depth: 1,
            number: 1
        }
    );
    assert_eq!(doc.blocks[7].kind, BlockKind::Paragraph);
    assert_eq!(doc.blocks[7].plain_text(), "Unnumbered level 2 paragraph");

    assert!(matches!(
        &doc.blocks[8].inlines[0],
        Inline::Checkbox { checked: true }
    ));

    assert!(matches!(
        &doc.blocks[9].inlines[..],
        [Inline::Image { src, alt, .. }]
            if src.starts_with("data:image/png;base64,") && alt == "Legacy VML Badge"
    ));

    assert_eq!(doc.blocks[10].plain_text(), "Language\tRust");

    let rt_bytes = document_to_docx_bytes(&doc).unwrap();
    let rt_doc = docx_bytes_to_document(&rt_bytes).unwrap();
    let rt_imgs = rt_doc
        .blocks
        .iter()
        .flat_map(|b| &b.inlines)
        .filter(
            |i| matches!(i, Inline::Image { src, .. } if src.starts_with("data:image/png;base64,")),
        )
        .count();
    assert_eq!(rt_imgs, 2);
    assert!(
        rt_doc
            .plain_text(false)
            .contains("2020 - now\tLead Engineer"),
        "Expected tab inside run to round-trip through DOCX export"
    );

    let pdf_path = dir.join("complex_data_uri.pdf");
    sillpaper::io::print::export_document_to_pdf(&doc, &Preferences::default(), &pdf_path).unwrap();
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    assert!(String::from_utf8_lossy(&pdf_bytes).contains("/Subtype /Image"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_code_block_fenced_typing_indent_paste_exit_and_highlighting() {
    use sillpaper::ui::code_highlight::{TokenKind, highlight_code};

    let prefs = Preferences::default();
    let mut ed = EditorState::new(TextDirection::Auto);

    ed.insert_text("```rust", Some(&prefs));
    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks.len(), 1);
    assert_eq!(
        ed.doc.blocks[0].kind,
        BlockKind::CodeBlock {
            language: Some("rust".into())
        }
    );

    ed.insert_text("fn main() {", Some(&prefs));
    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks.len(), 1);

    assert!(ed.indent_code_block(true).is_some());
    ed.insert_text("let msg = \"ok\";", Some(&prefs));
    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks.len(), 1);
    assert_eq!(
        ed.doc.blocks[0].plain_text(),
        "fn main() {\n    let msg = \"ok\";\n    "
    );

    assert!(ed.indent_code_block(false).is_some());
    ed.insert_text("}", Some(&prefs));
    assert_eq!(
        ed.doc.blocks[0].plain_text(),
        "fn main() {\n    let msg = \"ok\";\n}"
    );

    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks.len(), 1);
    ed.insert_paragraph();
    assert_eq!(ed.doc.blocks.len(), 2);
    assert_eq!(ed.doc.blocks[1].kind, BlockKind::Paragraph);
    assert_eq!(
        ed.doc.blocks[0].plain_text(),
        "fn main() {\n    let msg = \"ok\";\n}"
    );

    ed.set_selection(Selection::collapsed(DocPos::new(
        0,
        ed.doc.blocks[0].char_len(),
    )));
    ed.insert_paragraph();
    ed.paste_plain_text("let a = 1;\nlet b = 2;");
    assert_eq!(ed.doc.blocks.len(), 2);
    assert!(
        ed.doc.blocks[0]
            .plain_text()
            .contains("let a = 1;\nlet b = 2;")
    );

    let len0 = ed.doc.blocks[0].char_len();
    ed.set_selection(Selection::range(
        DocPos::new(0, len0 - 2),
        DocPos::new(0, len0 - 1),
    ));
    ed.insert_text("\"", Some(&prefs));
    assert!(ed.doc.blocks[0].plain_text().contains("\"2\""));
    assert!(!ed.doc.blocks[0].plain_text().contains('“'));

    ed.set_selection(Selection::collapsed(DocPos::new(0, 3)));
    assert!(ed.exit_code_block_below().is_some());
    assert_eq!(ed.doc.blocks.len(), 3);
    assert_eq!(ed.doc.blocks[1].kind, BlockKind::Paragraph);
    assert_eq!(ed.selection.focus, DocPos::new(1, 0));

    let rs_code = "fn demo() -> String { let n = 42; println!(\"hi\"); // note\n}";
    let rs_spans = highlight_code(rs_code, Some("rust"));
    assert!(
        rs_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Keyword && &rs_code[s..e] == "fn")
    );
    assert!(
        rs_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Type && &rs_code[s..e] == "String")
    );
    assert!(
        rs_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Number && &rs_code[s..e] == "42")
    );
    assert!(
        rs_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Type && &rs_code[s..e] == "println!")
    );
    assert!(
        rs_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::String && &rs_code[s..e] == "\"hi\"")
    );
    assert!(
        rs_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Comment && &rs_code[s..e] == "// note")
    );

    let py_code = "@cache\ndef fib(n): # fast\n    return n";
    let py_spans = highlight_code(py_code, Some("python"));
    assert!(
        py_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Type && &py_code[s..e] == "@cache")
    );
    assert!(
        py_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Keyword && &py_code[s..e] == "def")
    );
    assert!(
        py_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Comment && &py_code[s..e] == "# fast")
    );

    let sql_code = "SELECT id FROM users -- active";
    let sql_spans = highlight_code(sql_code, Some("sql"));
    assert!(
        sql_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Keyword && &sql_code[s..e] == "SELECT")
    );
    assert!(
        sql_spans
            .iter()
            .any(|&(s, e, k)| k == TokenKind::Comment && &sql_code[s..e] == "-- active")
    );
}
