use gtk4::prelude::*;
use sillpaper::config::{Preferences, ThemeId, generate_app_css};
use sillpaper::io::backup::StorageManager;
use sillpaper::model::{
    Block, BlockKind, DocPos, Document, Inline, InlineAttr, Selection, TextDirection, TextRun,
};
use sillpaper::ui::editor_tab::{EditorTab, EditorTabCallbacks};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

fn make_tab(doc: Document, prefs: Preferences) -> EditorTab {
    let dir = std::env::temp_dir().join(format!("sillpaper_gtk_test_{}", std::process::id()));
    let storage = Rc::new(StorageManager::new(dir));
    let callbacks = EditorTabCallbacks {
        on_state_changed: Rc::new(|| {}),
        on_selection_changed: Rc::new(|_| {}),
        on_hide_toolbar: Rc::new(|| {}),
        on_typing_activity: Rc::new(|| {}),
        on_external_file_changed: Rc::new(|_| {}),
        on_open_link_input: Rc::new(|| {}),
        on_extension_trigger: Rc::new(|_| {}),
        on_toast: Rc::new(|_| {}),
        get_window: Rc::new(|| None),
    };
    EditorTab::new(
        "test_tab".into(),
        doc,
        None,
        false,
        None,
        0,
        Rc::new(RefCell::new(prefs)),
        storage,
        callbacks,
    )
}

#[test]
fn test_gtk_editor_tab_sync_formatting_focus_search_and_large_doc() {
    if libadwaita::init().is_err() {
        eprintln!("Skipping GTK/Adwaita integration test: no display available");
        return;
    }

    let prefs = Preferences {
        focus_mode: true,
        ..Default::default()
    };

    let tab = make_tab(Document::new_empty(TextDirection::Auto), prefs.clone());
    let buffer = tab.view.buffer();

    let patch = tab.state.borrow_mut().insert_text("Hello world", None);
    tab.apply_patch(patch);
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
        "Hello world"
    );

    tab.state
        .borrow_mut()
        .set_selection(Selection::range(DocPos::new(0, 6), DocPos::new(0, 11)));
    tab.toggle_inline(InlineAttr::Bold);
    tab.toggle_inline(InlineAttr::Italic);
    tab.set_link(Some("https://example.org".into()));

    let bold_tag = buffer.tag_table().lookup("bold").unwrap();
    let link_tag = buffer.tag_table().lookup("link").unwrap();
    let check_iter = buffer.iter_at_line_offset(0, 7).unwrap();
    assert!(check_iter.has_tag(&bold_tag));
    assert!(check_iter.has_tag(&link_tag));

    tab.state
        .borrow_mut()
        .set_selection(Selection::collapsed(DocPos::new(0, 5)));
    let p = tab.state.borrow_mut().insert_paragraph();
    tab.apply_patch(p);
    assert_eq!(buffer.line_count(), 2);

    tab.toggle_block(BlockKind::Heading(1));
    let h1_tag = buffer.tag_table().lookup("h1").unwrap();
    let h1_iter = buffer.iter_at_line_offset(1, 0).unwrap();
    assert!(h1_iter.has_tag(&h1_tag));

    let focus_tag = buffer.tag_table().lookup("focus_dim").unwrap();
    assert!(
        buffer
            .iter_at_line_offset(0, 0)
            .unwrap()
            .has_tag(&focus_tag)
    );
    assert!(
        !buffer
            .iter_at_line_offset(1, 0)
            .unwrap()
            .has_tag(&focus_tag)
    );

    let (cur, total) = tab.run_search("world");
    assert_eq!((cur, total), (0, 1));
    let search_cur_tag = buffer.tag_table().lookup("search_current").unwrap();
    assert!(
        buffer
            .iter_at_line_offset(1, 2)
            .unwrap()
            .has_tag(&search_cur_tag)
    );
    tab.clear_search();
    assert!(
        !buffer
            .iter_at_line_offset(1, 2)
            .unwrap()
            .has_tag(&search_cur_tag)
    );

    tab.undo();
    tab.undo();
    assert_eq!(buffer.line_count(), 1);
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
        "Hello world"
    );

    let p_rtl = tab.state.borrow_mut().insert_paragraph();
    tab.apply_patch(p_rtl);
    let p_ar = tab.state.borrow_mut().insert_text("مرحبا بالعالم", None);
    tab.apply_patch(p_ar);
    tab.set_direction(TextDirection::Rtl);
    let rtl_tag = buffer.tag_table().lookup("dir_rtl").unwrap();
    assert!(buffer.iter_at_line_offset(1, 0).unwrap().has_tag(&rtl_tag));

    let para = "Smooth native Linux writing without Electron bloat or sluggish frames. ".repeat(10);
    let mut blocks = Vec::with_capacity(260);
    for i in 0..250 {
        if i % 25 == 0 {
            blocks.push(
                Block::new(BlockKind::Heading(2)).with_inlines(vec![Inline::Text(TextRun::plain(
                    format!("Chapter {}", i / 25 + 1),
                ))]),
            );
        }
        blocks.push(Block::paragraph_with_text(para.clone()));
    }
    let t0 = Instant::now();
    let big_tab = make_tab(Document::from_blocks(blocks), prefs);
    let render_ms = t0.elapsed().as_millis();
    assert_eq!(big_tab.view.buffer().line_count(), 260);
    println!("GTK4 25k-word full buffer load & tag apply: {render_ms} ms");

    big_tab
        .state
        .borrow_mut()
        .set_selection(Selection::collapsed(DocPos::new(130, 5)));
    let t1 = Instant::now();
    for _ in 0..50 {
        let p = big_tab.state.borrow_mut().insert_text("x", None);
        big_tab.apply_patch(p);
    }
    let avg_us = t1.elapsed().as_micros() / 50;
    println!("GTK4 25k-word incremental edit+tag sync: {avg_us} µs/keystroke");
    assert!(avg_us < 5_000, "GTK incremental edit too slow: {avg_us} µs");

    let css_errors = Rc::new(RefCell::new(Vec::<String>::new()));
    let provider = gtk4::CssProvider::new();
    {
        let errs = css_errors.clone();
        provider.connect_parsing_error(move |_, section, err| {
            errs.borrow_mut().push(format!(
                "CSS error at {:?}: {}",
                section.to_str(),
                err.message()
            ));
        });
    }
    for theme in ThemeId::ALL {
        let p = Preferences {
            theme,
            ..Default::default()
        };
        let css = generate_app_css(&p);
        provider.load_from_string(&css);
        assert!(
            css_errors.borrow().is_empty(),
            "Theme {theme:?} produced GTK CSS errors: {:?}",
            css_errors.borrow()
        );
    }

    let smart_tab = make_tab(
        Document::new_empty(TextDirection::Auto),
        Preferences::default(),
    );
    let p_prefs = Preferences::default();
    for ch in "***both***".chars() {
        let patch = smart_tab
            .state
            .borrow_mut()
            .insert_text(&ch.to_string(), Some(&p_prefs));
        smart_tab.apply_patch(patch);
    }
    let sb = smart_tab.view.buffer();
    assert_eq!(sb.text(&sb.start_iter(), &sb.end_iter(), false), "both");
    let it0 = sb.iter_at_line_offset(0, 0).unwrap();
    assert!(it0.has_tag(&sb.tag_table().lookup("bold").unwrap()));
    assert!(it0.has_tag(&sb.tag_table().lookup("italic").unwrap()));

    let context = gtk4::glib::MainContext::default();
    let type_text = |tab: &EditorTab, input: &str| {
        let buffer = tab.view.buffer();
        for ch in input.chars() {
            buffer.insert_at_cursor(&ch.to_string());
            while context.iteration(false) {}
        }
    };
    for (input, kind, text) in [
        ("#Heading", BlockKind::Heading(1), "Heading"),
        ("# Heading", BlockKind::Heading(1), "Heading"),
        ("##Heading", BlockKind::Heading(2), "Heading"),
        ("### Heading", BlockKind::Heading(3), "Heading"),
        ("* First", BlockKind::BulletListItem { depth: 0 }, "First"),
        ("- Second", BlockKind::BulletListItem { depth: 0 }, "Second"),
        (
            "1. Item",
            BlockKind::OrderedListItem {
                depth: 0,
                number: 1,
            },
            "Item",
        ),
        ("> Quote", BlockKind::Blockquote, "Quote"),
    ] {
        let tab = make_tab(
            Document::new_empty(TextDirection::Auto),
            Preferences::default(),
        );
        type_text(&tab, input);
        let state = tab.state.borrow();
        assert_eq!(state.doc.blocks[0].kind, kind, "{input:?}");
        assert_eq!(state.doc.blocks[0].plain_text(), text, "{input:?}");
        let buffer = tab.view.buffer();
        assert_eq!(
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
            text
        );
    }

    let heading_tab = make_tab(
        Document::new_empty(TextDirection::Auto),
        Preferences::default(),
    );
    type_text(&heading_tab, "#Heading");
    let heading_buffer = heading_tab.view.buffer();
    heading_buffer.select_range(&heading_buffer.end_iter(), &heading_buffer.start_iter());
    let buffer_changes = Rc::new(std::cell::Cell::new(0));
    heading_buffer.connect_changed({
        let buffer_changes = buffer_changes.clone();
        move |_| buffer_changes.set(buffer_changes.get() + 1)
    });
    heading_tab.toggle_block(BlockKind::Heading(2));
    assert_eq!(
        heading_tab.state.borrow().doc.blocks[0].kind,
        BlockKind::Heading(2)
    );
    assert_eq!(
        buffer_changes.get(),
        0,
        "Heading changes should preserve the buffer text"
    );
    assert!(
        heading_buffer
            .start_iter()
            .has_tag(&heading_buffer.tag_table().lookup("h2").unwrap())
    );
    let (selection_start, selection_end) = heading_buffer.selection_bounds().unwrap();
    assert_eq!(
        heading_buffer.text(&selection_start, &selection_end, false),
        "Heading"
    );
    let undo = heading_tab.state.borrow_mut().undo();
    heading_tab.apply_patch(undo);
    assert_eq!(
        heading_tab.state.borrow().doc.blocks[0].kind,
        BlockKind::Heading(1)
    );

    let narrow_prefs = Preferences {
        text_width: sillpaper::config::TextWidth::Narrow,
        ..Default::default()
    };
    let paste_tab = make_tab(Document::new_empty(TextDirection::Auto), narrow_prefs);
    let frag = sillpaper::io::html::html_to_fragment(
        "<p>Intro</p><ul><li><strong>Cause</strong>: centered bullet</li><li>Fix: clean</li></ul>",
    );
    let patch = paste_tab
        .state
        .borrow_mut()
        .replace_selection_with_fragment(frag);
    paste_tab.apply_patch(patch);
    let (min_w, _, _, _) = paste_tab
        .scrolled
        .measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        min_w < 250,
        "ScrolledWindow min_width ({min_w}px) must stay small so window resizing works freely!"
    );
    let b0_tag = paste_tab
        .view
        .buffer()
        .tag_table()
        .lookup("bullet_0")
        .unwrap();
    assert!(b0_tag.is_accumulative_margin());
    assert_eq!(paste_tab.view.left_margin() + b0_tag.left_margin(), 44);

    let css_provider = gtk4::CssProvider::new();
    let css_err = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let css_err_c = css_err.clone();
    css_provider.connect_parsing_error(move |_, section, err| {
        css_err_c
            .borrow_mut()
            .push(format!("{:?}: {}", section.to_str(), err));
    });
    css_provider.load_from_string(&sillpaper::config::generate_app_css(&Preferences::default()));
    assert!(
        css_err.borrow().is_empty(),
        "Sillpaper CSS parse errors: {:?}",
        css_err.borrow()
    );

    let settings_dir =
        std::env::temp_dir().join(format!("sillpaper_settings_{}", std::process::id()));
    let settings = sillpaper::ui::preferences_ui::PreferencesPanel::new(
        &Preferences {
            words_per_minute: 2000,
            ..Preferences::default()
        },
        &[],
        sillpaper::config::RendererBackend::Auto,
        Rc::new(RefCell::new(sillpaper::extensions::ExtensionManager::new(
            settings_dir,
        ))),
        |_| {},
        || {},
    );
    let settings_scroll = settings
        .root
        .last_child()
        .and_downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    let settings_viewport = settings_scroll
        .child()
        .and_downcast::<gtk4::Viewport>()
        .unwrap();
    let sections = settings_viewport.child().unwrap().observe_children();
    let section = |index| {
        sections
            .item(index)
            .and_downcast::<gtk4::Expander>()
            .unwrap()
    };
    let expanded_count = || {
        (0..sections.n_items())
            .filter(|&index| section(index).is_expanded())
            .count()
    };
    assert_eq!(sections.n_items(), 6);
    assert_eq!(section(0).label().as_deref(), Some("Page"));
    assert!(section(0).is_expanded());
    assert_eq!(expanded_count(), 1);
    let counting = section(3);
    assert_eq!(counting.label().as_deref(), Some("Counting & saving"));
    counting.set_expanded(true);
    assert!(!section(0).is_expanded());
    assert_eq!(expanded_count(), 1);

    let settings_window = gtk4::Window::builder()
        .default_width(350)
        .default_height(400)
        .child(&settings.root)
        .build();
    settings_window.add_css_class("sillpaper-window");
    let settings_display = settings.root.display();
    gtk4::style_context_add_provider_for_display(
        &settings_display,
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    settings_window.present();
    let settings_context = gtk4::glib::MainContext::default();
    let settings_start = Instant::now();
    while settings_start.elapsed() < std::time::Duration::from_millis(120) {
        while settings_context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let counting_content = counting.child().unwrap();
    let mut row = counting_content.first_child();
    let reading_speed = loop {
        let widget = row.expect("Reading speed control should be in Counting & saving");
        if let Some(spin) = widget.last_child().and_downcast::<gtk4::SpinButton>() {
            break spin;
        }
        row = widget.next_sibling();
    };
    let editable = reading_speed
        .delegate()
        .and_downcast::<gtk4::Text>()
        .unwrap();
    assert_eq!(reading_speed.text(), "2000");
    assert!(
        editable.width()
            >= reading_speed
                .create_pango_layout(Some("2000"))
                .pixel_size()
                .0,
        "The full reading speed value should fit inside the input"
    );
    settings_window.close();
    gtk4::style_context_remove_provider_for_display(&settings_display, &css_provider);

    let tw_blocks: Vec<Block> = (0..60)
        .map(|i| Block::paragraph_with_text(format!("Typewriter paragraph line number {i}.")))
        .collect();
    let tw_prefs = Preferences {
        typewriter_mode: true,
        ..Default::default()
    };
    let tw_tab = make_tab(Document::from_blocks(tw_blocks), tw_prefs);
    let win = gtk4::Window::builder()
        .default_width(800)
        .default_height(600)
        .child(&tw_tab.root)
        .build();
    win.present();
    let main_ctx = gtk4::glib::MainContext::default();
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_millis(120) {
        while main_ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    tw_tab.recompute_canvas_margins();
    assert!(
        tw_tab.view.top_margin() > 120,
        "Typewriter mode must expand top_margin (got {})",
        tw_tab.view.top_margin()
    );
    assert!(
        tw_tab.view.bottom_margin() > 160,
        "Typewriter mode must expand bottom_margin (got {})",
        tw_tab.view.bottom_margin()
    );
    let buf = tw_tab.view.buffer();
    let it30 = buf.iter_at_line(30).unwrap();
    buf.place_cursor(&it30);
    tw_tab.scroll_caret_comfortably();
    let vadj = tw_tab.scrolled.vadjustment();
    let expected_center = vadj.page_size() * 0.45;
    let glide_deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
    while std::time::Instant::now() < glide_deadline {
        while main_ctx.iteration(false) {}
        let loc = tw_tab.view.iter_location(&it30);
        let (_, wy) =
            tw_tab
                .view
                .buffer_to_window_coords(gtk4::TextWindowType::Widget, loc.x(), loc.y());
        if vadj.value() > 100.0 && ((wy as f64) - expected_center).abs() < 35.0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        vadj.value() > 100.0,
        "Expected Typewriter Mode to scroll down to center line 30 (vadj={})",
        vadj.value()
    );
    let loc30 = tw_tab.view.iter_location(&it30);
    let (_, widget_y) =
        tw_tab
            .view
            .buffer_to_window_coords(gtk4::TextWindowType::Widget, loc30.x(), loc30.y());
    assert!(
        ((widget_y as f64) - expected_center).abs() < 40.0,
        "Expected caret near vertical center ({expected_center:.0}px), got widget_y={widget_y}px"
    );

    let centered_scroll = vadj.value();
    buf.place_cursor(&buf.start_iter());
    tw_tab.scroll_caret_comfortably();
    let manual_pos = (centered_scroll - 15.0).max(0.0);
    vadj.set_value(manual_pos);
    let cancel_wait = std::time::Instant::now();
    while cancel_wait.elapsed() < std::time::Duration::from_millis(420) {
        while main_ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        (vadj.value() - manual_pos).abs() < 2.0,
        "Manual scroll must cancel active Typewriter glide (expected ~{manual_pos:.1}, got {:.1})",
        vadj.value()
    );
    win.close();

    let link_label = "Sillpaper";
    let link_url = "https://github.com/ubaimutl/sillpaper";
    let link_start = 6;
    let link_end = link_start + link_label.chars().count() as i32;
    let link_doc = Document::from_blocks(vec![Block::paragraph_with_text(format!(
        "Visit {link_label} now."
    ))]);
    let toolbar = sillpaper::ui::floating_toolbar::FloatingToolbar::new();
    let hide_count = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let hide_c = hide_count.clone();
    let tb_hide = toolbar.clone();
    let link_tab = sillpaper::ui::editor_tab::EditorTab::new(
        "link_test".into(),
        link_doc,
        None,
        false,
        None,
        0,
        std::rc::Rc::new(std::cell::RefCell::new(Preferences::default())),
        std::rc::Rc::new(StorageManager::new(
            std::env::temp_dir().join(format!("sillpaper_gtk_link_{}", std::process::id())),
        )),
        EditorTabCallbacks {
            on_state_changed: std::rc::Rc::new(|| {}),
            on_selection_changed: std::rc::Rc::new(|_| {}),
            on_hide_toolbar: std::rc::Rc::new(move || {
                hide_c.set(hide_c.get() + 1);
                tb_hide.hide();
            }),
            on_typing_activity: std::rc::Rc::new(|| {}),
            on_external_file_changed: std::rc::Rc::new(|_| {}),
            on_open_link_input: std::rc::Rc::new(|| {}),
            on_extension_trigger: std::rc::Rc::new(|_| {}),
            on_toast: std::rc::Rc::new(|_| {}),
            get_window: std::rc::Rc::new(|| None),
        },
    );
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&link_tab.root));
    overlay.add_overlay(&toolbar.container);
    let link_win = gtk4::Window::builder()
        .default_width(640)
        .default_height(400)
        .child(&overlay)
        .build();
    link_win.present();
    while main_ctx.iteration(false) {}

    let lbuf = link_tab.view.buffer();
    let s_it = lbuf.iter_at_offset(link_start);
    let e_it = lbuf.iter_at_offset(link_end);
    lbuf.select_range(&s_it, &e_it);
    link_tab.state.borrow_mut().set_selection(Selection::range(
        DocPos::new(0, link_start as usize),
        DocPos::new(0, link_end as usize),
    ));

    assert!(link_tab.begin_pending_link().is_some());
    toolbar.open_link_input(None);
    let collapsed = lbuf.iter_at_offset(link_end);
    lbuf.place_cursor(&collapsed);
    while main_ctx.iteration(false) {}

    assert!(
        toolbar.is_link_mode(),
        "Link toolbar vanished when GtkEntry took focus!"
    );
    assert!(
        toolbar.link_entry().grab_focus(),
        "link_entry could not acquire keyboard focus (check ancestor can_focus flags)!"
    );
    assert!(link_tab.has_pending_link());
    assert!(
        !link_tab.view.selection_rects().is_empty(),
        "Override selection rects vanished when GtkTextBuffer collapsed selection!"
    );

    link_tab.commit_pending_link(Some(link_url.into()));
    assert!(!link_tab.has_pending_link());
    assert!(
        link_tab.state.borrow().doc.blocks[0]
            .inlines
            .iter()
            .any(|i| matches!(
                i,
                sillpaper::model::Inline::Text(r)
                    if r.text == link_label
                        && r.style.link.as_deref() == Some(link_url)
            ))
    );
    link_win.close();

    let sel_doc = Document::from_blocks(vec![
        Block::new(BlockKind::Heading(1)).with_inlines(vec![Inline::Text(TextRun::plain(
            "Heading Title That Wraps Cleanly Across Two Lines",
        ))]),
        Block::paragraph_with_text(
            "Am frühen Morgen öffnet der Wochenmarkt auf dem alten Marktplatz. \
             Zwischen den Ständen stehen kleine Gruppen im Gespräch, während die Händler \
             frisches Obst, Brot und Blumen aus den umliegenden Dörfern anbieten.",
        ),
        Block::paragraph_with_text("WordPress ist mir aus der Praxis vertraut."),
        Block::paragraph_with_text("مرحبا hello عالم test mixed BiDi line"),
    ]);
    let sel_prefs = Preferences {
        paragraph_margins: true,
        ..Default::default()
    };
    let sel_tab = make_tab(sel_doc, sel_prefs);
    let sel_win = gtk4::Window::builder()
        .default_width(540)
        .default_height(600)
        .child(&sel_tab.root)
        .build();
    sel_win.present();
    let t_sel = std::time::Instant::now();
    while t_sel.elapsed() < std::time::Duration::from_millis(120) {
        while main_ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let sbuf = sel_tab.view.buffer();
    let p1_start = sbuf.iter_at_line(1).unwrap();
    let mut p1_end = p1_start;
    p1_end.forward_to_line_end();
    sbuf.select_range(&p1_start, &p1_end);
    while main_ctx.iteration(false) {}
    let p1_rects = sel_tab.view.selection_rects();
    assert!(
        p1_rects.len() >= 3,
        "Expected paragraph 1 to wrap across >= 3 lines, got {} rects",
        p1_rects.len()
    );
    let first_h = p1_rects[0].height();
    let last_h = p1_rects.last().unwrap().height();
    assert!(
        (last_h - first_h).abs() <= 1.5,
        "Last wrapped line selection height ({last_h}px) inflated vs first line ({first_h}px)!"
    );
    let (p1_yr_y, p1_yr_h) = sel_tab.view.line_yrange(&p1_start);
    let last_bot = p1_rects.last().unwrap().y() + last_h;
    assert!(
        last_bot <= (p1_yr_y + p1_yr_h - 12) as f32 + 0.5,
        "Last line selection bottom ({last_bot}) bled into pixels_below_lines (content_bottom={})",
        p1_yr_y + p1_yr_h - 12
    );

    let h1_start = sbuf.iter_at_line(0).unwrap();
    let mut h1_end = h1_start;
    h1_end.forward_to_line_end();
    sbuf.select_range(&h1_start, &h1_end);
    let h1_rects = sel_tab.view.selection_rects();
    assert!(!h1_rects.is_empty());
    let (h1_yr_y, h1_yr_h) = sel_tab.view.line_yrange(&h1_start);
    assert!(
        h1_rects[0].y() >= (h1_yr_y + 18) as f32 - 0.5,
        "H1 selection top ({}) bled into pixels_above_lines ({})",
        h1_rects[0].y(),
        h1_yr_y + 18
    );
    let h1_last = h1_rects.last().unwrap();
    assert!(
        h1_last.y() + h1_last.height() <= (h1_yr_y + h1_yr_h - 8) as f32 + 0.5,
        "H1 selection bottom ({}) bled into pixels_below_lines ({})",
        h1_last.y() + h1_last.height(),
        h1_yr_y + h1_yr_h - 8
    );

    let bidi_s = sbuf.iter_at_line_offset(3, 2).unwrap();
    let bidi_e = sbuf.iter_at_line_offset(3, 9).unwrap();
    sbuf.select_range(&bidi_s, &bidi_e);
    let bidi_rects = sel_tab.view.selection_rects();
    assert_eq!(
        bidi_rects.len(),
        2,
        "Expected 2 disjoint BiDi selection spans, got {:?}",
        bidi_rects
    );

    sel_win.close();

    let tv = libadwaita::TabView::new();
    let tb = sillpaper::ui::tab_bar::CompactTabBar::new(&tv);
    for i in 0..9 {
        let p = tv.append(&gtk4::Label::new(Some(&format!("Doc {i}"))));
        p.set_title(&format!("Long Document Title {i}"));
        p.set_needs_attention(i % 3 == 0);
    }
    let (tb_min_w, _, _, _) = tb.root.measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        tb_min_w < 160,
        "CompactTabBar min_width ({tb_min_w}px) must stay small so narrow windows squeeze tabs!"
    );

    let ext_tab = make_tab(
        Document::from_blocks(vec![Block::paragraph_with_text("This is teh draft")]),
        Preferences::default(),
    );
    ext_tab
        .state
        .borrow_mut()
        .set_selection(Selection::collapsed(DocPos::new(0, 17)));
    let ebuf = ext_tab.view.buffer();
    ebuf.place_cursor(&ebuf.end_iter());
    ext_tab.set_ghost_suggestion(0, 17, " for today.".into());
    assert!(ext_tab.has_ghost_suggestion());
    assert!(ext_tab.accept_ghost_suggestion(true));
    assert_eq!(
        ebuf.text(&ebuf.start_iter(), &ebuf.end_iter(), false),
        "This is teh draft for"
    );
    assert!(ext_tab.has_ghost_suggestion());
    assert!(ext_tab.accept_ghost_suggestion(false));
    assert_eq!(
        ebuf.text(&ebuf.start_iter(), &ebuf.end_iter(), false),
        "This is teh draft for today."
    );
    assert!(!ext_tab.has_ghost_suggestion());

    let diag = sillpaper::extensions::ExtensionDiagnostic {
        start: 8,
        end: 11,
        severity: "warning".into(),
        message: "Spelling error".into(),
        replacement: Some("the".into()),
    };
    ext_tab.set_block_diagnostics(0, vec![diag.clone()]);
    assert!(ext_tab.diagnostic_at(0, 9).is_some());
    let diag_tag = ebuf.tag_table().lookup("diagnostic").unwrap();
    assert!(ebuf.iter_at_line_offset(0, 9).unwrap().has_tag(&diag_tag));
    ext_tab.apply_diagnostic_fix(0, &diag);
    assert_eq!(
        ebuf.text(&ebuf.start_iter(), &ebuf.end_iter(), false),
        "This is the draft for today."
    );
    ext_tab.undo();
    assert_eq!(
        ebuf.text(&ebuf.start_iter(), &ebuf.end_iter(), false),
        "This is teh draft for today."
    );

    let ext_root = std::env::temp_dir().join(format!("sillpaper_ext_test_{}", std::process::id()));
    let mock_dir = ext_root.join("mock-ext");
    let _ = std::fs::remove_dir_all(&ext_root);
    std::fs::create_dir_all(&mock_dir).unwrap();
    std::fs::write(
        mock_dir.join("manifest.json"),
        r#"{
            "id": "mock-ext",
            "name": "Mock Assistant",
            "version": "0.1.0",
            "command": "python3",
            "args": ["mock.py"],
            "capabilities": {
                "completions": true,
                "diagnostics": true,
                "actions": [{"id": "fix", "label": "Fix"}]
            }
        }"#,
    )
    .unwrap();
    std::fs::write(
        mock_dir.join("mock.py"),
        r#"import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    m = msg.get("method")
    i = msg.get("id")
    if m == "config/update":
        print(json.dumps({"id": i, "result": {"ok": True}}), flush=True)
    elif m == "completion/request":
        print(json.dumps({"id": i, "result": {"suggestion": " smoothly."}}), flush=True)
    elif m == "diagnostics/request":
        print(json.dumps({"id": i, "result": {"diagnostics": [{"start": 0, "end": 3, "message": "Typo", "replacement": "The"}]}}), flush=True)
    elif m == "action/run":
        print(json.dumps({"id": i, "result": {"replacement": "Fixed text."}}), flush=True)
"#,
    )
    .unwrap();

    let mut mgr = sillpaper::extensions::ExtensionManager::new(ext_root.clone());
    assert_eq!(mgr.discovered().len(), 1);
    let wake_hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    {
        let hits = wake_hits.clone();
        mgr.set_wake_callback(std::sync::Arc::new(move || {
            hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));
    }
    mgr.sync_enabled(&["mock-ext".into()]);
    assert!(mgr.has_completions());
    assert!(mgr.has_diagnostics());
    assert_eq!(mgr.available_actions().len(), 1);
    assert_eq!(mgr.running_manifests().len(), 1);

    mgr.request_completion(0, 5, "Hello", "", "Hello", true);
    mgr.request_diagnostics(0, "Teh test", false);
    mgr.run_action("mock-ext", "fix", "Teh test", None);

    let mut got_comp = false;
    let mut got_diag = false;
    let mut got_act = false;
    let t_ext = Instant::now();
    while t_ext.elapsed() < std::time::Duration::from_secs(3) && !(got_comp && got_diag && got_act)
    {
        for ev in mgr.poll_events() {
            match ev {
                sillpaper::extensions::ExtensionEvent::Completion { suggestion, .. } => {
                    assert_eq!(suggestion, " smoothly.");
                    got_comp = true;
                }
                sillpaper::extensions::ExtensionEvent::Diagnostics { items, .. } => {
                    assert_eq!(items.len(), 1);
                    assert_eq!(items[0].replacement.as_deref(), Some("The"));
                    got_diag = true;
                }
                sillpaper::extensions::ExtensionEvent::ActionResult { replacement, .. } => {
                    assert_eq!(replacement, "Fixed text.");
                    got_act = true;
                }
                sillpaper::extensions::ExtensionEvent::Error { message, .. } => {
                    panic!("Unexpected mock extension error: {message}");
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(
        got_comp && got_diag && got_act,
        "Missing mock extension events: comp={got_comp} diag={got_diag} act={got_act}"
    );
    assert!(
        wake_hits.load(std::sync::atomic::Ordering::Relaxed) >= 3,
        "Event-driven wake callback should fire for each incoming extension response"
    );

    let def_prefs = Preferences::default();
    assert_eq!(
        def_prefs.ghost_suggestion_mode,
        sillpaper::config::GhostSuggestionMode::OnShortcut
    );
    assert!(def_prefs.dismissed_extension_hints.is_empty());

    let legacy_json =
        r#"{"auto_inline_suggestions": true, "enabled_extensions": ["sillpaper-ai"]}"#;
    let parsed_legacy: Preferences = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(
        parsed_legacy.ghost_suggestion_mode,
        sillpaper::config::GhostSuggestionMode::OnShortcut
    );
    assert_eq!(parsed_legacy.enabled_extensions, vec!["sillpaper-ai"]);

    ext_tab
        .state
        .borrow_mut()
        .set_selection(Selection::collapsed(DocPos::new(0, 28)));
    ext_tab.set_ghost_loading(0, 28);
    assert!(!ext_tab.has_ghost_suggestion());
    assert!(ext_tab.view.ghost_text().is_some());
    assert!(!ext_tab.accept_ghost_suggestion(false));
    ext_tab.clear_ghost_suggestion();
    assert!(ext_tab.view.ghost_text().is_none());

    let narrow = Preferences {
        text_width: sillpaper::config::TextWidth::Narrow,
        ..def_prefs.clone()
    };
    let shared = sillpaper::ui::editor_tab::create_shared_tag_table(&narrow);
    let dir = std::env::temp_dir().join(format!("sillpaper_shared_tbl_{}", std::process::id()));
    let storage = Rc::new(StorageManager::new(dir));
    let mk_cb = || EditorTabCallbacks {
        on_state_changed: Rc::new(|| {}),
        on_selection_changed: Rc::new(|_| {}),
        on_hide_toolbar: Rc::new(|| {}),
        on_typing_activity: Rc::new(|| {}),
        on_external_file_changed: Rc::new(|_| {}),
        on_open_link_input: Rc::new(|| {}),
        on_extension_trigger: Rc::new(|_| {}),
        on_toast: Rc::new(|_| {}),
        get_window: Rc::new(|| None),
    };
    let t_a = EditorTab::new_with_tag_table(
        "t_a".into(),
        Document::from_blocks(vec![Block::paragraph_with_text("Alpha")]),
        None,
        false,
        None,
        0,
        Rc::new(RefCell::new(narrow.clone())),
        storage.clone(),
        mk_cb(),
        Some(&shared),
    );
    let t_b = EditorTab::new_with_tag_table(
        "t_b".into(),
        Document::from_blocks(vec![Block::paragraph_with_text("Beta")]),
        None,
        false,
        None,
        0,
        Rc::new(RefCell::new(narrow.clone())),
        storage,
        mk_cb(),
        Some(&shared),
    );
    assert_eq!(t_a.view.buffer().tag_table(), t_b.view.buffer().tag_table());

    let wide_window = gtk4::Window::builder()
        .default_width(1200)
        .default_height(400)
        .child(&t_a.root)
        .build();
    let small_window = gtk4::Window::builder()
        .default_width(680)
        .default_height(400)
        .child(&t_b.root)
        .build();
    wide_window.present();
    small_window.present();
    let wait_start = Instant::now();
    while wait_start.elapsed() < std::time::Duration::from_millis(120) {
        while main_ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(t_a.scrolled.width() > t_b.scrolled.width());
    t_a.recompute_canvas_margins();
    t_b.recompute_canvas_margins();
    for tab in [&t_a, &t_b] {
        let start = tab.view.buffer().start_iter();
        let inset = tab.view.iter_location(&start).x();
        let column_width = tab.scrolled.width() - 2 * inset;
        assert!(
            (440..=441).contains(&column_width),
            "Narrow text should stay centered at 440px (got {column_width}px)"
        );
    }
    wide_window.close();
    small_window.close();

    let banner = sillpaper::ui::extension_overlays::ExtensionHintBanner::new();
    let dismissed_ids = Rc::new(RefCell::new(Vec::<String>::new()));
    {
        let out = dismissed_ids.clone();
        banner.set_on_dismiss(move |id| out.borrow_mut().push(id));
    }
    let manifest = mgr.running_manifests()[0].clone();
    banner.show_for(
        &manifest,
        false,
        sillpaper::config::GhostSuggestionMode::OnShortcut,
    );
    assert!(banner.is_visible());
    assert_eq!(banner.current_extension_id().as_deref(), Some("mock-ext"));
    banner.hide();
    assert!(!banner.is_visible());

    let anchor_doc = Document::from_blocks(vec![
        Block::paragraph_with_text("Intro line before anchors"),
        Block::new(BlockKind::HorizontalRule),
        Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Checkbox { checked: false },
            Inline::Text(TextRun::plain(" First task item")),
        ]),
        Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Text(TextRun::plain("Note ")),
            Inline::Comment {
                text: "Margin note".into(),
            },
            Inline::Text(TextRun::plain(" and trailing prose")),
        ]),
    ]);
    let paste_tab = make_tab(anchor_doc, Preferences::default());
    let paste_win = gtk4::Window::builder()
        .default_width(720)
        .default_height(480)
        .child(&paste_tab.root)
        .build();
    paste_win.present();
    let ctx = gtk4::glib::MainContext::default();
    for _ in 0..30 {
        if !ctx.iteration(false) {
            break;
        }
    }
    let last_b = paste_tab.state.borrow().doc.blocks.len() - 1;
    let last_off = paste_tab.state.borrow().doc.blocks[last_b].char_len();
    paste_tab.state.borrow_mut().set_selection(Selection::range(
        DocPos::new(0, 0),
        DocPos::new(last_b, last_off),
    ));
    let pbuf = paste_tab.view.buffer();
    pbuf.select_range(&pbuf.start_iter(), &pbuf.end_iter());

    let pasted_prose = "Finding Your Voice\n\n\
Writing isn’t about waiting for inspiration to strike. Sometimes, it is simply about showing up on the page and letting your thoughts wander.\n\n\
It is the quiet act of sitting down, face-to-face with a blank canvas, and allowing your imagination to wander freely. Establishing a simple daily routine can help inspire a creative process.\n\n\
Every word you write is a valuable part of the journey. The page is always open—feel free to explore new ideas.";
    let patch = paste_tab.state.borrow_mut().paste_plain_text(pasted_prose);
    paste_tab.apply_patch(patch);
    assert!(
        pbuf.text(&pbuf.start_iter(), &pbuf.end_iter(), false)
            .contains("Finding Your Voice")
    );
    assert!(
        pbuf.text(&pbuf.start_iter(), &pbuf.end_iter(), false)
            .contains("explore new ideas.")
    );
    paste_tab.undo();
    assert_eq!(pbuf.line_count(), 4);
    paste_tab.redo();
    paste_tab.full_render_buffer();
    paste_win.close();

    let browser_html = "<!--StartFragment--><p>Finding<!-- --> Your<!----> Voice<!--$--><!--/$--></p><!--EndFragment-->";
    let frag = sillpaper::io::html::html_to_fragment(browser_html);
    assert_eq!(frag.blocks.len(), 1);
    assert_eq!(frag.blocks[0].plain_text(), "Finding Your Voice");
    assert!(
        frag.blocks[0]
            .inlines
            .iter()
            .all(|i| !matches!(i, Inline::Comment { .. })),
        "Browser framework HTML comments should be ignored on paste"
    );

    let pixbuf =
        gtk4::gdk_pixbuf::Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, 64, 32).unwrap();
    let png_bytes = pixbuf.save_to_bufferv("png", &[]).unwrap();
    let img_b64 = sillpaper::io::docx::base64_encode(&png_bytes);
    let img_doc = Document::from_blocks(vec![
        Block::new(BlockKind::Paragraph).with_inlines(vec![Inline::Image {
            src: format!("data:image/png;base64,{img_b64}"),
            alt: "Embedded Avatar".into(),
            original_src: None,
        }]),
        Block::paragraph_with_text("Caption below embedded image"),
    ]);
    let img_tab = make_tab(img_doc, Preferences::default());
    let img_win = gtk4::Window::builder()
        .default_width(640)
        .default_height(420)
        .child(&img_tab.root)
        .build();
    img_win.present();
    for _ in 0..20 {
        if !ctx.iteration(false) {
            break;
        }
    }
    let img_buf = img_tab.view.buffer();
    let img_anchor = img_buf
        .start_iter()
        .child_anchor()
        .expect("Expected child anchor for embedded data-URI image");
    let anchor_widgets = img_anchor.widgets();
    assert_eq!(anchor_widgets.len(), 1);
    let pic = anchor_widgets[0]
        .downcast_ref::<gtk4::Picture>()
        .expect("Expected data-URI image to render as gtk4::Picture, not fallback Label");
    assert!(
        pic.paintable().is_some(),
        "Picture must have decoded texture"
    );
    assert_eq!(pic.size_request(), (64, 32));
    assert_eq!(pic.tooltip_text().as_deref(), Some("Embedded Avatar"));
    img_tab
        .state
        .borrow_mut()
        .set_selection(Selection::range(DocPos::new(0, 0), DocPos::new(1, 7)));
    let img_patch = img_tab.state.borrow_mut().paste_plain_text("Replaced");
    img_tab.apply_patch(img_patch);
    assert!(
        img_buf
            .text(&img_buf.start_iter(), &img_buf.end_iter(), false)
            .starts_with("Replaced")
    );
    img_win.close();

    let code_doc = Document::from_blocks(vec![
        Block::new(BlockKind::CodeBlock {
            language: Some("rust".into()),
        })
        .with_inlines(vec![Inline::Text(TextRun::plain(
            "fn calm() -> usize { let x = 7; \"ok\" } // hi",
        ))]),
    ]);
    let code_tab = make_tab(code_doc, Preferences::default());
    let cbuf = code_tab.view.buffer();
    let table = cbuf.tag_table();
    let cb_tag = table.lookup("codeblock").expect("codeblock tag missing");
    let kw_tag = table.lookup("code_kw").expect("code_kw tag missing");
    assert!(cbuf.iter_at_line_offset(0, 0).unwrap().has_tag(&cb_tag));
    assert!(
        !cbuf.iter_at_line_offset(0, 0).unwrap().has_tag(&kw_tag),
        "Code block syntax highlighting should stay disabled for calm uniform monospace"
    );

    let sb_doc = Document::from_blocks(vec![Block::paragraph_with_text("Hello")]);
    let sb_tab = make_tab(sb_doc, Preferences::default());
    let sb_win = gtk4::Window::builder()
        .default_width(600)
        .default_height(300)
        .child(&sb_tab.root)
        .build();
    sb_win.present();
    for _ in 0..20 {
        if !ctx.iteration(false) {
            break;
        }
    }
    let sbbuf = sb_tab.view.buffer();
    let line0_y = sb_tab.view.iter_location(&sbbuf.start_iter()).y();
    sb_tab
        .state
        .borrow_mut()
        .set_selection(Selection::collapsed(DocPos::new(0, 5)));
    let sb_patch = sb_tab.state.borrow_mut().insert_soft_break();
    sb_tab.apply_patch(sb_patch);
    for _ in 0..10 {
        if !ctx.iteration(false) {
            break;
        }
    }
    let caret_after_sb = sbbuf.iter_at_mark(&sbbuf.get_insert());
    let after_sb_loc = sb_tab.view.iter_location(&caret_after_sb);
    assert!(
        after_sb_loc.y() > line0_y,
        "Shift+Enter caret must visually move to the next line immediately (line0_y={line0_y}, caret_y={})",
        after_sb_loc.y()
    );
    let type_patch = sb_tab.state.borrow_mut().insert_text("W", None);
    sb_tab.apply_patch(type_patch);
    let buf_text = sbbuf.text(&sbbuf.start_iter(), &sbbuf.end_iter(), false);
    assert_eq!(buf_text.as_str(), "Hello\u{2028}W");
    sb_win.close();

    let paste_img_tab = make_tab(Document::default(), Preferences::default());
    paste_img_tab.insert_image_bytes(&png_bytes, "Clip Shot");
    assert!(matches!(
        paste_img_tab.state.borrow().doc.blocks[0].inlines.first(),
        Some(Inline::Image { src, .. }) if src.starts_with("data:image/png;base64,")
    ));
    let unsaved_anchor = paste_img_tab
        .view
        .buffer()
        .start_iter()
        .child_anchor()
        .expect("Expected anchor after insert_image_bytes");
    assert!(unsaved_anchor.widgets()[0].is::<gtk4::Picture>());

    let md_dir =
        std::env::temp_dir().join(format!("sillpaper_gtk_md_assets_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&md_dir);
    std::fs::create_dir_all(&md_dir).unwrap();
    let md_file = md_dir.join("notes.md");
    std::fs::write(&md_file, "").unwrap();
    let md_tab = make_tab(Document::default(), Preferences::default());
    *md_tab.file_path.borrow_mut() = Some(md_file.clone());
    md_tab.insert_image_bytes(&png_bytes, "Diagram");
    let rel_src = match md_tab.state.borrow().doc.blocks[0].inlines.first() {
        Some(Inline::Image { src, .. }) => src.clone(),
        other => panic!("Expected Inline::Image, got {other:?}"),
    };
    assert!(rel_src.starts_with("assets/image-") && rel_src.ends_with(".png"));
    assert!(md_dir.join(&rel_src).is_file());
    let md_anchor = md_tab
        .view
        .buffer()
        .start_iter()
        .child_anchor()
        .expect("Expected anchor for saved markdown asset image");
    assert!(md_anchor.widgets()[0].is::<gtk4::Picture>());
    let _ = std::fs::remove_dir_all(&md_dir);

    let mut ul_run = TextRun::plain("simple daily routine");
    ul_run.style.underline = true;
    let mut bold_run = TextRun::plain("Writing isn\u{2019}t");
    bold_run.style.bold = true;
    let clip_doc = Document::from_blocks(vec![
        Block::new(BlockKind::Heading(2))
            .with_inlines(vec![Inline::Text(TextRun::plain("Finding Your Voice"))]),
        Block::new(BlockKind::Paragraph),
        Block::new(BlockKind::Paragraph).with_inlines(vec![
            Inline::Text(bold_run),
            Inline::Text(TextRun::plain(" about waiting\u{2014}establish a ")),
            Inline::Text(ul_run),
            Inline::Text(TextRun::plain(".")),
        ]),
    ]);
    let clip_tab = make_tab(clip_doc, Preferences::default());
    let clip_win = gtk4::Window::builder()
        .default_width(500)
        .default_height(300)
        .child(&clip_tab.root)
        .build();
    clip_win.present();
    for _ in 0..30 {
        if !ctx.iteration(false) {
            break;
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(30));
    for _ in 0..30 {
        if !ctx.iteration(false) {
            break;
        }
    }
    let last_block_len = clip_tab.state.borrow().doc.blocks[2].char_len();
    clip_tab.state.borrow_mut().set_selection(Selection::range(
        DocPos::new(0, 0),
        DocPos::new(2, last_block_len),
    ));
    clip_tab.copy_to_clipboard();

    let raw_formats = clip_tab.view.clipboard().formats();
    assert!(
        raw_formats.contains_type(String::static_type()),
        "Clipboard must include typed String provider for native GTK4 plain-text reads"
    );
    let advertised = raw_formats.union_serialize_mime_types();
    for mime in ["text/html", "text/plain", "text/plain;charset=utf-8"] {
        assert!(
            advertised.contain_mime_type(mime),
            "Clipboard must advertise `{mime}`; got {:?}",
            advertised.mime_types()
        );
    }

    let rich_dest = make_tab(Document::default(), Preferences::default());
    rich_dest.paste_from_clipboard(false);
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while Instant::now() < deadline {
        if rich_dest.state.borrow().doc.blocks.len() == 3 {
            break;
        }
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    {
        let st = rich_dest.state.borrow();
        assert_eq!(
            st.doc.blocks.len(),
            3,
            "Async rich self-paste did not complete in time (blocks={:?})",
            st.doc.blocks
        );
        assert_eq!(st.doc.blocks[0].kind, BlockKind::Heading(2));
        assert_eq!(st.doc.blocks[0].plain_text(), "Finding Your Voice");
        assert_eq!(
            st.doc.blocks[2].plain_text(),
            "Writing isn\u{2019}t about waiting\u{2014}establish a simple daily routine."
        );
        assert!(
            st.doc.blocks[2].inlines.iter().any(
                |i| matches!(i, Inline::Text(r) if r.style.bold && r.text.contains("Writing"))
            ),
            "Bold formatting must survive rich clipboard round-trip"
        );
        assert!(
            st.doc.blocks[2].inlines.iter().any(
                |i| matches!(i, Inline::Text(r) if r.style.underline && r.text.contains("daily"))
            ),
            "Underline formatting must survive rich clipboard round-trip"
        );
    }

    if !clip_tab.view.clipboard().is_local() {
        clip_tab.copy_to_clipboard();
    }
    let plain_dest = make_tab(Document::default(), Preferences::default());
    plain_dest.paste_from_clipboard(true);
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while Instant::now() < deadline {
        if !plain_dest.state.borrow().doc.blocks[0]
            .plain_text()
            .is_empty()
        {
            break;
        }
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let pasted_plain = plain_dest.state.borrow().doc.plain_text(false);
    assert!(
        pasted_plain.contains("Finding Your Voice")
            && pasted_plain.contains("simple daily routine"),
        "Plain-text paste from union provider failed: {pasted_plain:?}"
    );

    if !clip_tab.view.clipboard().is_local() {
        clip_tab.copy_to_clipboard();
    }
    let bare_plain_result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    {
        let out_c = bare_plain_result.clone();
        clip_tab.view.clipboard().read_async(
            &["text/plain"],
            gtk4::glib::Priority::DEFAULT,
            gtk4::gio::Cancellable::NONE,
            move |res| {
                if let Ok((stream, _)) = res {
                    fn pump(
                        stream: gtk4::gio::InputStream,
                        mut acc: Vec<u8>,
                        out: Rc<RefCell<Option<String>>>,
                    ) {
                        let s_c = stream.clone();
                        stream.read_bytes_async(
                            16 * 1024,
                            gtk4::glib::Priority::DEFAULT,
                            gtk4::gio::Cancellable::NONE,
                            move |r| match r {
                                Ok(b) if b.is_empty() => {
                                    *out.borrow_mut() = String::from_utf8(acc).ok();
                                }
                                Ok(b) => {
                                    acc.extend_from_slice(&b);
                                    pump(s_c, acc, out);
                                }
                                Err(_) => {}
                            },
                        );
                    }
                    pump(stream, Vec::new(), out_c);
                }
            },
        );
    }
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while Instant::now() < deadline {
        if bare_plain_result.borrow().is_some() {
            break;
        }
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let bare_plain = bare_plain_result
        .borrow()
        .clone()
        .expect("Reading bare `text/plain` from clipboard failed");
    assert!(
        bare_plain.contains("Writing isn\u{2019}t about waiting\u{2014}establish"),
        "Bare `text/plain` must preserve raw UTF-8 without ASCII hex escapes: {bare_plain:?}"
    );

    let big_line = "Streaming async clipboard HTML safely across multiple pipe chunks. ".repeat(20);
    let big_blocks: Vec<Block> = (0..80)
        .map(|i| Block::paragraph_with_text(format!("P{i:02}: {big_line}")))
        .collect();
    let big_clip_tab = make_tab(Document::from_blocks(big_blocks), Preferences::default());
    let big_end_len = big_clip_tab.state.borrow().doc.blocks[79].char_len();
    big_clip_tab
        .state
        .borrow_mut()
        .set_selection(Selection::range(
            DocPos::new(0, 0),
            DocPos::new(79, big_end_len),
        ));
    big_clip_tab.copy_to_clipboard();
    let big_dest = make_tab(Document::default(), Preferences::default());
    big_dest.paste_from_clipboard(false);
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while Instant::now() < deadline {
        if big_dest.state.borrow().doc.blocks.len() == 80 {
            break;
        }
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        big_dest.state.borrow().doc.blocks.len(),
        80,
        "Multi-chunk >80 KiB HTML clipboard self-paste did not complete cleanly"
    );
    assert!(
        big_dest.state.borrow().doc.blocks[79]
            .plain_text()
            .starts_with("P79: "),
        "Final block of >80 KiB clipboard HTML paste was truncated"
    );
    clip_win.close();

    let live_md_tab = make_tab(
        Document::new_empty(TextDirection::Auto),
        Preferences::default(),
    );
    let live_buf = live_md_tab.view.buffer();
    let drain_idle = || {
        for _ in 0..20 {
            if !ctx.iteration(false) {
                break;
            }
        }
    };

    for ch in "##Heading".chars() {
        live_buf.insert_at_cursor(&ch.to_string());
        drain_idle();
    }
    assert_eq!(
        live_md_tab.state.borrow().doc.blocks[0].kind,
        BlockKind::Heading(2)
    );
    assert_eq!(
        live_buf.text(&live_buf.start_iter(), &live_buf.end_iter(), false),
        "Heading"
    );
    let h2_tag = live_buf.tag_table().lookup("h2").unwrap();
    assert!(live_buf.iter_at_line_offset(0, 0).unwrap().has_tag(&h2_tag));
    let ins_it = live_buf.iter_at_mark(&live_buf.get_insert());
    assert_eq!((ins_it.line(), ins_it.line_offset()), (0, 7));

    let prepend_tab = make_tab(
        Document::from_blocks(vec![Block::paragraph_with_text("World")]),
        Preferences::default(),
    );
    let pbuf = prepend_tab.view.buffer();
    pbuf.place_cursor(&pbuf.start_iter());
    drain_idle();
    for ch in "## ".chars() {
        pbuf.insert_at_cursor(&ch.to_string());
        drain_idle();
    }
    assert_eq!(
        prepend_tab.state.borrow().doc.blocks[0].kind,
        BlockKind::Heading(2)
    );
    assert_eq!(
        pbuf.text(&pbuf.start_iter(), &pbuf.end_iter(), false),
        "World"
    );
    let p_ins = pbuf.iter_at_mark(&pbuf.get_insert());
    assert_eq!(
        (p_ins.line(), p_ins.line_offset()),
        (0, 0),
        "Caret must stay at col 0 after prepending `## ` to existing text"
    );

    let inline_tab = make_tab(
        Document::new_empty(TextDirection::Auto),
        Preferences::default(),
    );
    let ibuf = inline_tab.view.buffer();
    for ch in "**bold** plain".chars() {
        ibuf.insert_at_cursor(&ch.to_string());
        drain_idle();
    }
    assert_eq!(
        ibuf.text(&ibuf.start_iter(), &ibuf.end_iter(), false),
        "bold plain"
    );
    let ibold_tag = ibuf.tag_table().lookup("bold").unwrap();
    assert!(ibuf.iter_at_line_offset(0, 1).unwrap().has_tag(&ibold_tag));
    assert!(
        !ibuf.iter_at_line_offset(0, 6).unwrap().has_tag(&ibold_tag),
        "Text typed after closing `**` must reset to plain (pending_style must not be wiped by mark-set)"
    );

    let wide_pixels = vec![64u8; 1400 * 700 * 4];
    let wide_tex = gtk4::gdk::MemoryTexture::new(
        1400,
        700,
        gtk4::gdk::MemoryFormat::R8g8b8a8,
        &gtk4::glib::Bytes::from_owned(wide_pixels),
        1400 * 4,
    );
    let wide_png = wide_tex.save_to_png_bytes();
    let wide_b64 = sillpaper::io::docx::base64_encode(wide_png.as_ref());
    let wide_uri = format!("data:image/png;base64,{wide_b64}");

    let breakout_doc = Document::from_blocks(vec![
        Block::paragraph_with_text("Centered narrow prose above the showcase image."),
        Block::new(BlockKind::Paragraph).with_inlines(vec![Inline::Image {
            src: wide_uri,
            alt: "Wide Showcase".into(),
            original_src: None,
        }]),
        Block::paragraph_with_text("Centered narrow prose below the showcase image."),
    ]);
    let breakout_prefs = Preferences {
        text_width: sillpaper::config::TextWidth::Narrow,
        focus_mode: true,
        focus_mode_opacity: 0.25,
        ..Default::default()
    };
    let breakout_tab = make_tab(breakout_doc, breakout_prefs);
    let breakout_win = gtk4::Window::builder()
        .default_width(960)
        .default_height(720)
        .child(&breakout_tab.root)
        .build();
    breakout_win.present();
    let t_bo = Instant::now();
    while t_bo.elapsed() < std::time::Duration::from_millis(120) {
        while ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    breakout_tab.recompute_canvas_margins();
    drain_idle();

    let (bo_min_w, _, _, _) = breakout_tab
        .scrolled
        .measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        bo_min_w < 100,
        "Wide breakout image must NOT block window shrinking (min_w={bo_min_w})"
    );

    let bo_buf = breakout_tab.view.buffer();
    let text_loc = breakout_tab.view.iter_location(&bo_buf.start_iter());
    let img_iter = bo_buf.iter_at_line(1).unwrap();
    let img_anchor = img_iter
        .child_anchor()
        .expect("Expected child anchor for wide breakout image");
    let img_widget = img_anchor.widgets()[0].clone();
    let img_pic = img_widget
        .downcast_ref::<gtk4::Picture>()
        .expect("Expected gtk4::Picture for wide breakout image");
    let (pic_w, pic_h) = img_pic.size_request();
    let sw_w = breakout_tab.scrolled.width();
    let sw_h = breakout_tab.scrolled.height();
    assert!(
        pic_w > 650 && pic_w < sw_w,
        "Wide standalone image should break out past 440px narrow column (pic={pic_w}x{pic_h}, sw={sw_w}x{sw_h})"
    );
    assert!(
        pic_h <= ((sw_h as f64) * 0.66).ceil() as i32 + 2,
        "Image height ({pic_h}) must respect 66% viewport height cap (sw_h={sw_h})"
    );
    if let Some(bounds) = img_pic.compute_bounds(&breakout_tab.scrolled) {
        let img_center_x = bounds.x() + bounds.width() * 0.5;
        let vp_center_x = sw_w as f32 * 0.5;
        assert!(
            (img_center_x - vp_center_x).abs() < 24.0,
            "Standalone image must be centered in viewport (img_center={img_center_x:.1}, vp_center={vp_center_x:.1})"
        );
        assert!(
            bounds.x() < (text_loc.x() as f32) - 80.0,
            "Wide image left edge ({:.1}) should break out left of narrow text column (text_x={})",
            bounds.x(),
            text_loc.x()
        );
    }

    assert!(
        img_pic.opacity() < 0.5,
        "Focus Mode should dim inactive image (opacity={})",
        img_pic.opacity()
    );
    breakout_tab
        .state
        .borrow_mut()
        .set_selection(Selection::collapsed(DocPos::new(1, 0)));
    breakout_tab.refresh_focus_mode(false);
    assert!(
        (img_pic.opacity() - 1.0).abs() < 0.01,
        "Focus Mode should undim active image line (opacity={})",
        img_pic.opacity()
    );

    let cap_patch = breakout_tab
        .state
        .borrow_mut()
        .insert_text("Caption ", None);
    breakout_tab.apply_patch(cap_patch);
    drain_idle();
    let (inline_w, _) = img_pic.size_request();
    assert!(
        inline_w <= 440,
        "Image with inline text on the same line should clamp to text column width <= 440 (got {inline_w})"
    );
    breakout_win.close();

    use sillpaper::ui::icons::{AppIcon, symbolic_button, symbolic_image};
    let all_icons = [
        AppIcon::SidebarLeft,
        AppIcon::SidebarRight,
        AppIcon::Link,
        AppIcon::ChevronDown,
        AppIcon::ChevronLeft,
        AppIcon::ChevronRight,
        AppIcon::ChevronUp,
        AppIcon::ListBullet,
        AppIcon::ListOrdered,
        AppIcon::Check,
        AppIcon::Close,
        AppIcon::Plus,
        AppIcon::Search,
        AppIcon::FileNew,
        AppIcon::WindowNew,
        AppIcon::FolderOpen,
        AppIcon::Save,
        AppIcon::SaveAs,
        AppIcon::ExportDocx,
        AppIcon::ExportPdf,
        AppIcon::Fullscreen,
        AppIcon::Print,
        AppIcon::Settings,
    ];
    let fg = gtk4::gdk::RGBA::new(0.92, 0.92, 0.92, 1.0);
    for icon in all_icons {
        let img = symbolic_image(icon, 16);
        let paintable = img
            .paintable()
            .expect("symbolic_image must attach a SymbolicIconPaintable");
        assert_eq!(paintable.intrinsic_width(), 16);
        assert_eq!(paintable.intrinsic_height(), 16);
        let sym = paintable
            .downcast::<gtk4::SymbolicPaintable>()
            .expect("Icon paintable must implement gtk4::SymbolicPaintable for theme recoloring");
        let snap = gtk4::Snapshot::new();
        sym.snapshot_symbolic(&snap, 16.0, 16.0, &[fg]);
        assert!(
            snap.to_node().is_some(),
            "AppIcon::{icon:?} produced an empty render node"
        );
        let btn = symbolic_button(icon);
        assert!(btn.has_css_class("image-button"));
    }

    let pd_tab = make_tab(
        Document::from_blocks(vec![Block::paragraph_with_text(
            "Opening the sidebar should slide this full-width canvas right without squeezing text.",
        )]),
        Preferences::default(),
    );
    let pd_sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let drawer = sillpaper::ui::push_drawer::PushDrawer::new();
    drawer.set_sidebar_width(340.0);
    drawer.set_sidebar(Some(&pd_sidebar));
    drawer.set_content(Some(&pd_tab.root));
    let pd_win = gtk4::Window::builder()
        .default_width(900)
        .default_height(600)
        .child(&drawer)
        .build();
    pd_win.present();
    let t_pd = Instant::now();
    while t_pd.elapsed() < std::time::Duration::from_millis(120) {
        while ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let width_closed = pd_tab.scrolled.width();
    assert!(
        width_closed >= 800,
        "Expected full canvas width, got {width_closed}"
    );
    assert!(!drawer.scrim_visible());

    drawer.set_show_sidebar(true);
    let open_dl = Instant::now() + std::time::Duration::from_millis(700);
    while Instant::now() < open_dl {
        while ctx.iteration(false) {}
        if drawer.sidebar_progress() >= 0.99 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(drawer.shows_sidebar());
    assert!(
        drawer.scrim_visible(),
        "Dismiss scrim must cover pushed canvas while sidebar is open"
    );
    assert_eq!(
        pd_tab.scrolled.width(),
        width_closed,
        "Opening sidebar must push canvas aside without squeezing its width"
    );
    if let Some(bounds) = pd_tab.root.compute_bounds(&drawer) {
        assert!(
            bounds.x() >= 330.0,
            "Canvas should be translated right by ~340px (got x={:.1})",
            bounds.x()
        );
    }

    drawer.set_show_sidebar(false);
    let close_dl = Instant::now() + std::time::Duration::from_millis(700);
    while Instant::now() < close_dl {
        while ctx.iteration(false) {}
        if drawer.sidebar_progress() <= 0.01 && !drawer.scrim_visible() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!drawer.shows_sidebar());
    assert!(
        !drawer.scrim_visible(),
        "Dismiss scrim must hide once sidebar finishes closing"
    );
    if let Some(bounds) = pd_tab.root.compute_bounds(&drawer) {
        assert!(
            bounds.x().abs() < 2.0,
            "Canvas should slide cleanly back to x=0 (got x={:.1})",
            bounds.x()
        );
    }
    pd_win.close();

    mgr.stop_all();
    let _ = std::fs::remove_dir_all(&ext_root);
}
