use crate::config::{
    AccentSwatch, DefaultSaveFormat, FontChoice, GhostSuggestionMode, LineSpacing, Preferences,
    RendererBackend, TextWidth, ThemeId,
};
use crate::extensions::{ExtensionManager, SettingFieldKind};
use crate::model::TextDirection;
use gtk4::gio;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

pub struct PreferencesPanel {
    pub root: gtk4::Box,
    pub(super) header: gtk4::Box,
    scroll: gtk4::ScrolledWindow,
    page_section: gtk4::Expander,
    font_size_label: gtk4::Label,
    focus_check: gtk4::CheckButton,
    typewriter_check: gtk4::CheckButton,
    counter_check: gtk4::CheckButton,
    syncing: Rc<Cell<bool>>,
}

impl PreferencesPanel {
    pub fn new(
        initial: &Preferences,
        system_fonts: &[String],
        active_renderer: RendererBackend,
        ext_manager: Rc<RefCell<ExtensionManager>>,
        on_change: impl Fn(Preferences) + 'static,
        on_restart: impl Fn() + 'static,
    ) -> Self {
        let prefs = Rc::new(RefCell::new(initial.clone()));
        let on_change = Rc::new(on_change);
        let syncing = Rc::new(Cell::new(false));

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("sillpaper-settings-panel");
        root.set_hexpand(true);
        root.set_vexpand(true);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        header.add_css_class("sillpaper-settings-header");

        let title = gtk4::Label::new(Some("Settings"));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("sillpaper-settings-title");
        header.append(&title);

        let sections = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.add_css_class("sillpaper-settings-scroll");
        scroll.set_vexpand(true);
        scroll.set_overlay_scrolling(true);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_child(Some(&sections));

        let emit = {
            let p = prefs.clone();
            let cb = on_change.clone();
            let s = syncing.clone();
            move || {
                if !s.get() {
                    cb(p.borrow().clone());
                }
            }
        };

        let (page_section, content) = settings_section(&sections, "Page");
        page_section.set_expanded(true);

        content.append(&sub_label("Theme"));
        let theme_group = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        theme_group.add_css_class("sillpaper-seg-group");
        theme_group.add_css_class("sillpaper-theme-selector");
        theme_group.set_homogeneous(true);

        let custom_bg_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        custom_bg_row.set_margin_top(4);
        custom_bg_row.set_visible(initial.theme == ThemeId::Custom);
        let custom_bg_entry = gtk4::Entry::new();
        custom_bg_entry.set_text(&initial.custom_theme_bg);
        custom_bg_entry.set_placeholder_text(Some("#FAFDFF"));
        custom_bg_entry.set_width_chars(8);
        custom_bg_entry.set_hexpand(true);
        let custom_bg_apply = gtk4::Button::with_label("Apply");
        custom_bg_apply.add_css_class("sillpaper-target-btn");
        custom_bg_row.append(&custom_bg_entry);
        custom_bg_row.append(&custom_bg_apply);

        let theme_items: [(ThemeId, &str, &str); 7] = [
            (ThemeId::Light, "☀", "Light"),
            (ThemeId::Dark, "☾", "Dark"),
            (ThemeId::Black, "●", "True Black"),
            (ThemeId::DarkBlue, "◐", "Dark Blue"),
            (ThemeId::Dune, "▤", "Dune / Sepia"),
            (ThemeId::OldSchool, "▦", "Classic Blue"),
            (ThemeId::Custom, "✎", "Custom Background"),
        ];
        let theme_buttons: Rc<RefCell<Vec<(ThemeId, gtk4::Button)>>> =
            Rc::new(RefCell::new(Vec::new()));
        for (tid, glyph, tip) in theme_items {
            let btn = gtk4::Button::with_label(glyph);
            btn.set_tooltip_text(Some(tip));
            btn.set_can_focus(false);
            btn.set_focus_on_click(false);
            if initial.theme == tid {
                btn.add_css_class("seg-active");
            }
            theme_buttons.borrow_mut().push((tid, btn.clone()));
            theme_group.append(&btn);
        }
        for (tid, btn) in theme_buttons.borrow().iter() {
            let tid = *tid;
            let btns = theme_buttons.clone();
            let p = prefs.clone();
            let emit = emit.clone();
            let row = custom_bg_row.clone();
            btn.connect_clicked(move |_| {
                p.borrow_mut().theme = tid;
                row.set_visible(tid == ThemeId::Custom);
                for (id, b) in btns.borrow().iter() {
                    if *id == tid {
                        b.add_css_class("seg-active");
                    } else {
                        b.remove_css_class("seg-active");
                    }
                }
                emit();
            });
        }
        {
            let p = prefs.clone();
            let emit = emit.clone();
            let entry = custom_bg_entry.clone();
            custom_bg_apply.connect_clicked(move |_| {
                p.borrow_mut().custom_theme_bg = entry.text().trim().to_string();
                p.borrow_mut().theme = ThemeId::Custom;
                emit();
            });
        }
        content.append(&theme_group);
        content.append(&custom_bg_row);

        content.append(&sub_label("Text width"));
        let width_group = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        width_group.add_css_class("sillpaper-seg-group");
        width_group.set_homogeneous(true);
        let width_items: [(TextWidth, &str, &str); 6] = [
            (TextWidth::Narrow, "·≡·", "Narrow (440px)"),
            (TextWidth::Medium, "≡", "Medium (680px)"),
            (TextWidth::Wide, "☰", "Wide (900px)"),
            (TextWidth::ExtraWide, "XL", "Extra Wide (1140px)"),
            (TextWidth::Percent75, "75%", "75% of Window"),
            (TextWidth::Full, "↔", "Full Width"),
        ];
        let width_buttons: Rc<RefCell<Vec<(TextWidth, gtk4::Button)>>> =
            Rc::new(RefCell::new(Vec::new()));
        for (tw, glyph, tip) in width_items {
            let btn = gtk4::Button::with_label(glyph);
            btn.set_tooltip_text(Some(tip));
            btn.set_can_focus(false);
            btn.set_focus_on_click(false);
            if initial.text_width == tw {
                btn.add_css_class("seg-active");
            }
            width_buttons.borrow_mut().push((tw, btn.clone()));
            width_group.append(&btn);
        }
        for (tw, btn) in width_buttons.borrow().iter() {
            let tw = *tw;
            let btns = width_buttons.clone();
            let p = prefs.clone();
            let emit = emit.clone();
            btn.connect_clicked(move |_| {
                p.borrow_mut().text_width = tw;
                for (id, b) in btns.borrow().iter() {
                    if *id == tw {
                        b.add_css_class("seg-active");
                    } else {
                        b.remove_css_class("seg-active");
                    }
                }
                emit();
            });
        }
        content.append(&width_group);

        let mut font_choices: Vec<(String, FontChoice)> = vec![
            ("Serif".into(), FontChoice::Serif),
            ("Sans Serif".into(), FontChoice::Sans),
            ("Monospace".into(), FontChoice::Monospace),
        ];
        for fam in system_fonts {
            font_choices.push((fam.clone(), FontChoice::System(fam.clone())));
        }
        let font_labels: Vec<&str> = font_choices.iter().map(|(l, _)| l.as_str()).collect();
        let font_dd = gtk4::DropDown::from_strings(&font_labels);
        font_dd.set_enable_search(true);
        font_dd.set_search_match_mode(gtk4::StringFilterMatchMode::Substring);
        let font_expr = gtk4::PropertyExpression::new(
            gtk4::StringObject::static_type(),
            gtk4::Expression::NONE,
            "string",
        );
        font_dd.set_expression(Some(&font_expr));
        let initial_font_idx = font_choices
            .iter()
            .position(|(_, fc)| fc == &initial.font)
            .unwrap_or(0) as u32;
        font_dd.set_selected(initial_font_idx);
        {
            let p = prefs.clone();
            let emit = emit.clone();
            let choices = Rc::new(font_choices);
            font_dd.connect_selected_notify(move |dd| {
                if let Some((_, fc)) = choices.get(dd.selected() as usize) {
                    p.borrow_mut().font = fc.clone();
                    emit();
                }
            });
        }
        append_control(&content, "Font", &font_dd);

        let size_group = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        size_group.add_css_class("sillpaper-seg-group");
        size_group.set_homogeneous(true);
        let btn_dec = gtk4::Button::with_label("A⁻");
        btn_dec.set_tooltip_text(Some("Decrease font size (Ctrl+-)"));
        btn_dec.set_can_focus(false);
        btn_dec.set_focus_on_click(false);
        let font_size_label = gtk4::Label::new(Some(&format!("↺ {}%", initial.font_size_percent)));
        let btn_reset = gtk4::Button::new();
        btn_reset.set_child(Some(&font_size_label));
        btn_reset.set_tooltip_text(Some("Reset font size to 100% (Ctrl+0)"));
        btn_reset.set_can_focus(false);
        btn_reset.set_focus_on_click(false);
        let btn_inc = gtk4::Button::with_label("A⁺");
        btn_inc.set_tooltip_text(Some("Increase font size (Ctrl++)"));
        btn_inc.set_can_focus(false);
        btn_inc.set_focus_on_click(false);
        size_group.append(&btn_dec);
        size_group.append(&btn_reset);
        size_group.append(&btn_inc);

        {
            let p = prefs.clone();
            let emit = emit.clone();
            let lbl = font_size_label.clone();
            btn_dec.connect_clicked(move |_| {
                p.borrow_mut().bump_font_size(-5);
                lbl.set_text(&format!("↺ {}%", p.borrow().font_size_percent));
                emit();
            });
        }
        {
            let p = prefs.clone();
            let emit = emit.clone();
            let lbl = font_size_label.clone();
            btn_reset.connect_clicked(move |_| {
                p.borrow_mut().font_size_percent = 100;
                lbl.set_text("↺ 100%");
                emit();
            });
        }
        {
            let p = prefs.clone();
            let emit = emit.clone();
            let lbl = font_size_label.clone();
            btn_inc.connect_clicked(move |_| {
                p.borrow_mut().bump_font_size(5);
                lbl.set_text(&format!("↺ {}%", p.borrow().font_size_percent));
                emit();
            });
        }
        append_control(&content, "Font size", &size_group);
        content.append(&hint_note(
            "Tip: Ctrl + Scroll wheel adjusts font size anytime.",
        ));

        let font_colors =
            build_swatch_row(initial.text_color, prefs.clone(), emit.clone(), |p, sw| {
                p.text_color = sw;
            });
        append_control(&content, "Font color", &font_colors);

        let caret_colors =
            build_swatch_row(initial.caret_color, prefs.clone(), emit.clone(), |p, sw| {
                p.caret_color = sw;
            });
        append_control(&content, "Caret color", &caret_colors);

        let p_margin = add_check_row(
            &content,
            "Margin between paragraphs",
            initial.paragraph_margins,
            prefs.clone(),
            emit.clone(),
            |p, v| p.paragraph_margins = v,
        );
        p_margin.set_margin_top(6);

        content.append(&sub_label("Line spacing"));
        let ls_group = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        ls_group.add_css_class("sillpaper-seg-group");
        ls_group.set_homogeneous(true);
        let ls_buttons: Rc<RefCell<Vec<(LineSpacing, gtk4::Button)>>> =
            Rc::new(RefCell::new(Vec::new()));
        for ls in LineSpacing::ALL {
            let btn = gtk4::Button::with_label(ls.label());
            btn.set_can_focus(false);
            btn.set_focus_on_click(false);
            if initial.line_spacing == ls {
                btn.add_css_class("seg-active");
            }
            ls_buttons.borrow_mut().push((ls, btn.clone()));
            ls_group.append(&btn);
        }
        for (ls, btn) in ls_buttons.borrow().iter() {
            let ls = *ls;
            let btns = ls_buttons.clone();
            let p = prefs.clone();
            let emit = emit.clone();
            btn.connect_clicked(move |_| {
                p.borrow_mut().line_spacing = ls;
                for (id, b) in btns.borrow().iter() {
                    if *id == ls {
                        b.add_css_class("seg-active");
                    } else {
                        b.remove_css_class("seg-active");
                    }
                }
                emit();
            });
        }
        content.append(&ls_group);

        let (_, content) = settings_section(&sections, "Focus");

        let focus_check = add_check_row_with_hint(
            &content,
            "Focus Mode",
            Some("CTRL + M"),
            initial.focus_mode,
            prefs.clone(),
            emit.clone(),
            |p, v| p.focus_mode = v,
        );

        let opacity_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        opacity_row.set_margin_start(26);
        opacity_row.set_margin_bottom(4);
        let dim_ico = gtk4::Label::new(Some("◔"));
        dim_ico.set_opacity(0.45);
        let opacity_scale =
            gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.05, 0.90, 0.05);
        opacity_scale.set_value(initial.focus_mode_opacity);
        opacity_scale.set_hexpand(true);
        opacity_scale.set_draw_value(false);
        let bright_ico = gtk4::Label::new(Some("●"));
        bright_ico.set_opacity(0.85);
        {
            let p = prefs.clone();
            let emit = emit.clone();
            opacity_scale.connect_value_changed(move |sc| {
                p.borrow_mut().focus_mode_opacity = sc.value();
                emit();
            });
        }
        opacity_row.append(&dim_ico);
        opacity_row.append(&opacity_scale);
        opacity_row.append(&bright_ico);
        content.append(&opacity_row);
        content.append(&hint_note(
            "Dims surrounding text so you can stay focused on the active paragraph.",
        ));

        let typewriter_check = add_check_row_with_hint(
            &content,
            "Typewriter Mode",
            Some("CTRL + SHIFT + T"),
            initial.typewriter_mode,
            prefs.clone(),
            emit.clone(),
            |p, v| p.typewriter_mode = v,
        );
        content.append(&hint_note(
            "Keeps the active line vertically centered while you write.",
        ));
        add_check_row(
            &content,
            "Hide tabs & buttons while typing",
            initial.hide_chrome_while_typing,
            prefs.clone(),
            emit.clone(),
            |p, v| p.hide_chrome_while_typing = v,
        );
        add_check_row(
            &content,
            "Hide tabs & buttons unless hovered",
            initial.hide_chrome_unless_hovered,
            prefs.clone(),
            emit.clone(),
            |p, v| p.hide_chrome_unless_hovered = v,
        );
        add_check_row(
            &content,
            "Hide caret while typing",
            initial.hide_caret_while_typing,
            prefs.clone(),
            emit.clone(),
            |p, v| p.hide_caret_while_typing = v,
        );
        add_check_row(
            &content,
            "Show Table of Contents icon",
            initial.show_toc_button,
            prefs.clone(),
            emit.clone(),
            |p, v| p.show_toc_button = v,
        );
        content.append(&hint_note(
            "Tip: Hold Ctrl briefly anytime to view the shortcut cheatsheet.",
        ));

        let (_, content) = settings_section(&sections, "Formatting");

        add_check_row(
            &content,
            "Popup formatting toolbar on selection",
            initial.floating_toolbar,
            prefs.clone(),
            emit.clone(),
            |p, v| p.floating_toolbar = v,
        );
        content.append(&hint_note(
            "Shows formatting controls and extension (✦) actions when selecting text.",
        ));
        add_check_row(
            &content,
            "Smart quotes & guillemets",
            initial.smart_quotes,
            prefs.clone(),
            emit.clone(),
            |p, v| p.smart_quotes = v,
        );
        add_check_row(
            &content,
            "En / Em dashes (-- / ---)",
            initial.smart_dashes,
            prefs.clone(),
            emit.clone(),
            |p, v| p.smart_dashes = v,
        );
        add_check_row(
            &content,
            "Smart ellipses (...)",
            initial.smart_ellipses,
            prefs.clone(),
            emit.clone(),
            |p, v| p.smart_ellipses = v,
        );
        add_check_row(
            &content,
            "Smart arrows (-> / <-)",
            initial.smart_arrows,
            prefs.clone(),
            emit.clone(),
            |p, v| p.smart_arrows = v,
        );
        add_check_row(
            &content,
            "Start bulleted list with * or -",
            initial.markdown_asterisk_lists,
            prefs.clone(),
            emit.clone(),
            |p, v| p.markdown_asterisk_lists = v,
        );
        add_check_row(
            &content,
            "Start ordered list with 1.",
            initial.markdown_ordered_lists,
            prefs.clone(),
            emit.clone(),
            |p, v| p.markdown_ordered_lists = v,
        );
        add_check_row(
            &content,
            "Bold & italic with asterisks (* / **)",
            initial.markdown_inline_asterisks,
            prefs.clone(),
            emit.clone(),
            |p, v| p.markdown_inline_asterisks = v,
        );
        add_check_row(
            &content,
            "Insert zero-width non-joiner (Shift+Space)",
            initial.zwnj_on_shift_space,
            prefs.clone(),
            emit.clone(),
            |p, v| p.zwnj_on_shift_space = v,
        );

        let dir_group = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        dir_group.add_css_class("sillpaper-seg-group");
        dir_group.set_homogeneous(true);
        let dir_items = [
            (TextDirection::Auto, "Auto"),
            (TextDirection::Ltr, "LTR"),
            (TextDirection::Rtl, "RTL"),
        ];
        let dir_buttons: Rc<RefCell<Vec<(TextDirection, gtk4::Button)>>> =
            Rc::new(RefCell::new(Vec::new()));
        for (dir, lbl) in dir_items {
            let btn = gtk4::Button::with_label(lbl);
            btn.set_can_focus(false);
            btn.set_focus_on_click(false);
            if initial.default_direction == dir {
                btn.add_css_class("seg-active");
            }
            dir_buttons.borrow_mut().push((dir, btn.clone()));
            dir_group.append(&btn);
        }
        for (dir, btn) in dir_buttons.borrow().iter() {
            let dir = *dir;
            let btns = dir_buttons.clone();
            let p = prefs.clone();
            let emit = emit.clone();
            btn.connect_clicked(move |_| {
                p.borrow_mut().default_direction = dir;
                for (id, b) in btns.borrow().iter() {
                    if *id == dir {
                        b.add_css_class("seg-active");
                    } else {
                        b.remove_css_class("seg-active");
                    }
                }
                emit();
            });
        }
        append_control(&content, "Text direction", &dir_group);

        let (_, content) = settings_section(&sections, "Counting & saving");

        let counter_check = add_check_row_with_hint(
            &content,
            "Always show word counter",
            Some("CTRL + SHIFT + C"),
            initial.word_counter_always_visible,
            prefs.clone(),
            emit.clone(),
            |p, v| p.word_counter_always_visible = v,
        );
        add_check_row(
            &content,
            "Show character count",
            initial.show_character_count,
            prefs.clone(),
            emit.clone(),
            |p, v| p.show_character_count = v,
        );
        add_check_row(
            &content,
            "Include spaces in character count",
            initial.count_spaces_in_chars,
            prefs.clone(),
            emit.clone(),
            |p, v| p.count_spaces_in_chars = v,
        );
        add_check_row(
            &content,
            "Show reading time",
            initial.show_reading_time,
            prefs.clone(),
            emit.clone(),
            |p, v| p.show_reading_time = v,
        );

        let wpm_spin = gtk4::SpinButton::with_range(50.0, 2000.0, 10.0);
        wpm_spin.set_width_chars(5);
        wpm_spin.set_value(initial.words_per_minute as f64);
        {
            let p = prefs.clone();
            let emit = emit.clone();
            wpm_spin.connect_value_changed(move |sp| {
                p.borrow_mut().words_per_minute = sp.value_as_int().clamp(1, 25_000) as u32;
                emit();
            });
        }
        append_control(&content, "Reading speed (WPM)", &wpm_spin);

        let fmt_dd = gtk4::DropDown::from_strings(&[
            DefaultSaveFormat::Html.label(),
            DefaultSaveFormat::Markdown.label(),
            DefaultSaveFormat::PlainText.label(),
        ]);
        fmt_dd.set_selected(match initial.default_save_format {
            DefaultSaveFormat::Html => 0,
            DefaultSaveFormat::Markdown => 1,
            DefaultSaveFormat::PlainText => 2,
        });
        {
            let p = prefs.clone();
            let emit = emit.clone();
            fmt_dd.connect_selected_notify(move |dd| {
                p.borrow_mut().default_save_format = match dd.selected() {
                    1 => DefaultSaveFormat::Markdown,
                    2 => DefaultSaveFormat::PlainText,
                    _ => DefaultSaveFormat::Html,
                };
                emit();
            });
        }
        append_control(&content, "Default save format", &fmt_dd);

        add_check_row(
            &content,
            "Prefix new filenames with date (YYYY-MM-DD)",
            initial.include_date_in_filenames,
            prefs.clone(),
            emit.clone(),
            |p, v| p.include_date_in_filenames = v,
        );

        let (_, content) = settings_section(&sections, "Performance");
        content.append(&sub_label("Graphics renderer (GSK)"));

        let renderer_group = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        renderer_group.add_css_class("sillpaper-seg-group");
        renderer_group.set_homogeneous(true);

        let restart_btn = gtk4::Button::with_label("Restart Sillpaper to apply renderer");
        restart_btn.add_css_class("sillpaper-target-btn");
        restart_btn.set_can_focus(false);
        restart_btn.set_focus_on_click(false);
        restart_btn.set_margin_top(4);
        restart_btn.set_visible(initial.renderer != active_renderer);
        {
            restart_btn.connect_clicked(move |_| on_restart());
        }

        let ren_buttons: Rc<RefCell<Vec<(RendererBackend, gtk4::Button)>>> =
            Rc::new(RefCell::new(Vec::new()));
        for rb in RendererBackend::ALL {
            let btn = gtk4::Button::with_label(rb.label());
            btn.set_can_focus(false);
            btn.set_focus_on_click(false);
            if initial.renderer == rb {
                btn.add_css_class("seg-active");
            }
            ren_buttons.borrow_mut().push((rb, btn.clone()));
            renderer_group.append(&btn);
        }
        for (rb, btn) in ren_buttons.borrow().iter() {
            let rb = *rb;
            let btns = ren_buttons.clone();
            let p = prefs.clone();
            let emit = emit.clone();
            let rbtn = restart_btn.clone();
            btn.connect_clicked(move |_| {
                p.borrow_mut().renderer = rb;
                for (id, b) in btns.borrow().iter() {
                    if *id == rb {
                        b.add_css_class("seg-active");
                    } else {
                        b.remove_css_class("seg-active");
                    }
                }
                rbtn.set_visible(rb != active_renderer);
                emit();
            });
        }

        content.append(&renderer_group);
        content.append(&restart_btn);

        let (_, content) = settings_section(&sections, "Extensions");

        let ext_list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        populate_extensions_section(
            &ext_list_box,
            prefs.clone(),
            ext_manager.clone(),
            emit.clone(),
        );
        content.append(&ext_list_box);

        let ext_footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        ext_footer.set_margin_top(4);
        let btn_open_ext_dir = gtk4::Button::with_label("Open Folder");
        btn_open_ext_dir.add_css_class("sillpaper-target-btn");
        btn_open_ext_dir.set_can_focus(false);
        btn_open_ext_dir.set_focus_on_click(false);
        {
            let mgr = ext_manager.clone();
            btn_open_ext_dir.connect_clicked(move |_| {
                let dir = mgr.borrow().root_dir().to_path_buf();
                let _ = std::fs::create_dir_all(&dir);
                let uri = format!("file://{}", dir.display());
                let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
            });
        }
        let btn_rescan_ext = gtk4::Button::with_label("Rescan");
        btn_rescan_ext.add_css_class("sillpaper-target-btn");
        btn_rescan_ext.set_can_focus(false);
        btn_rescan_ext.set_focus_on_click(false);
        {
            let mgr = ext_manager;
            let list_b = ext_list_box;
            let p = prefs;
            let em = emit;
            btn_rescan_ext.connect_clicked(move |_| {
                mgr.borrow_mut().rescan();
                populate_extensions_section(&list_b, p.clone(), mgr.clone(), em.clone());
                em();
            });
        }
        ext_footer.append(&btn_open_ext_dir);
        ext_footer.append(&btn_rescan_ext);
        content.append(&ext_footer);

        root.append(&header);
        root.append(&scroll);

        Self {
            root,
            header,
            scroll,
            page_section,
            font_size_label,
            focus_check,
            typewriter_check,
            counter_check,
            syncing,
        }
    }

    pub(super) fn reset_to_page(&self) {
        self.page_section.set_expanded(true);
        self.scroll.vadjustment().set_value(0.0);
    }

    pub fn sync_external(&self, prefs: &Preferences) {
        self.syncing.set(true);
        self.font_size_label
            .set_text(&format!("↺ {}%", prefs.font_size_percent));
        self.focus_check.set_active(prefs.focus_mode);
        self.typewriter_check.set_active(prefs.typewriter_mode);
        self.counter_check
            .set_active(prefs.word_counter_always_visible);
        self.syncing.set(false);
    }
}

fn settings_section(sections: &gtk4::Box, title: &str) -> (gtk4::Expander, gtk4::Box) {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.add_css_class("sillpaper-settings-section");

    let section = gtk4::Expander::new(Some(title));
    section.add_css_class("sillpaper-settings-category");
    section.set_child(Some(&content));
    sections.append(&section);

    section.connect_expanded_notify(|expanded| {
        if !expanded.is_expanded() {
            return;
        }
        let Some(parent) = expanded.parent() else {
            return;
        };
        let mut sibling = parent.first_child();
        while let Some(widget) = sibling {
            sibling = widget.next_sibling();
            if let Ok(section) = widget.downcast::<gtk4::Expander>()
                && section != *expanded
            {
                section.set_expanded(false);
            }
        }
        let expanded = expanded.downgrade();
        gtk4::glib::idle_add_local_once(move || {
            if let Some(section) = expanded
                .upgrade()
                .filter(|s| s.is_expanded() && s.is_mapped())
                && let Some(viewport) = section
                    .ancestor(gtk4::Viewport::static_type())
                    .and_downcast::<gtk4::Viewport>()
                && let Some(label) = section.label_widget()
            {
                viewport.scroll_to(&label, None);
            }
        });
    });

    (section, content)
}

fn append_control(parent: &gtk4::Box, text: &str, control: &impl IsA<gtk4::Widget>) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_hexpand(true);
    row.append(&label);
    row.append(control);
    parent.append(&row);
}

fn sub_label(text: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.set_xalign(0.0);
    l.add_css_class("sillpaper-sub-label");
    l
}

fn hint_note(text: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.add_css_class("sillpaper-hint-note");
    l
}

fn add_check_row(
    parent: &gtk4::Box,
    title: &str,
    active: bool,
    prefs: Rc<RefCell<Preferences>>,
    emit: impl Fn() + 'static,
    setter: impl Fn(&mut Preferences, bool) + 'static,
) -> gtk4::CheckButton {
    add_check_row_with_hint(parent, title, None, active, prefs, emit, setter)
}

fn add_check_row_with_hint(
    parent: &gtk4::Box,
    title: &str,
    shortcut_hint: Option<&str>,
    active: bool,
    prefs: Rc<RefCell<Preferences>>,
    emit: impl Fn() + 'static,
    setter: impl Fn(&mut Preferences, bool) + 'static,
) -> gtk4::CheckButton {
    let cb = gtk4::CheckButton::new();
    cb.set_active(active);
    cb.set_can_focus(false);
    cb.set_hexpand(true);

    let lbl = gtk4::Label::new(Some(title));
    lbl.set_xalign(0.0);
    lbl.set_wrap(true);
    lbl.set_hexpand(true);
    cb.set_child(Some(&lbl));

    cb.connect_toggled(move |c| {
        setter(&mut prefs.borrow_mut(), c.is_active());
        emit();
    });

    if let Some(hint) = shortcut_hint {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.set_valign(gtk4::Align::Center);
        let hint_lbl = gtk4::Label::new(Some(hint));
        hint_lbl.add_css_class("sillpaper-setting-shortcut");
        hint_lbl.set_valign(gtk4::Align::Center);
        row.append(&cb);
        row.append(&hint_lbl);
        parent.append(&row);
    } else {
        parent.append(&cb);
    }
    cb
}

fn build_swatch_row(
    initial: AccentSwatch,
    prefs: Rc<RefCell<Preferences>>,
    emit: impl Fn() + Clone + 'static,
    setter: impl Fn(&mut Preferences, AccentSwatch) + Copy + 'static,
) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let swatches: [(AccentSwatch, &str, &str); 5] = [
        (AccentSwatch::Default, "swatch-default", "Default"),
        (AccentSwatch::Blue, "swatch-blue", "Blue"),
        (AccentSwatch::Pink, "swatch-pink", "Pink"),
        (AccentSwatch::Green, "swatch-green", "Green"),
        (AccentSwatch::Orange, "swatch-orange", "Orange"),
    ];
    let buttons: Rc<RefCell<Vec<(AccentSwatch, gtk4::Button)>>> = Rc::new(RefCell::new(Vec::new()));
    for (sw, cls, tip) in swatches {
        let btn = gtk4::Button::with_label(if initial == sw { "✓" } else { "" });
        btn.add_css_class("sillpaper-swatch");
        btn.add_css_class(cls);
        btn.set_tooltip_text(Some(tip));
        btn.set_can_focus(false);
        btn.set_focus_on_click(false);
        if initial == sw {
            btn.add_css_class("swatch-active");
        }
        buttons.borrow_mut().push((sw, btn.clone()));
        row.append(&btn);
    }
    for (sw, btn) in buttons.borrow().iter() {
        let sw = *sw;
        let btns = buttons.clone();
        let p = prefs.clone();
        let emit = emit.clone();
        btn.connect_clicked(move |_| {
            setter(&mut p.borrow_mut(), sw);
            for (id, b) in btns.borrow().iter() {
                if *id == sw {
                    b.set_label("✓");
                    b.add_css_class("swatch-active");
                } else {
                    b.set_label("");
                    b.remove_css_class("swatch-active");
                }
            }
            emit();
        });
    }
    row
}

enum ExtSettingWidget {
    Text(gtk4::Entry),
    Secret(gtk4::PasswordEntry),
    Select(gtk4::DropDown, Vec<String>),
}

fn populate_extensions_section(
    container: &gtk4::Box,
    prefs: Rc<RefCell<Preferences>>,
    ext_manager: Rc<RefCell<ExtensionManager>>,
    emit: impl Fn() + Clone + 'static,
) {
    while let Some(ch) = container.first_child() {
        container.remove(&ch);
    }

    let discovered = ext_manager.borrow().discovered().to_vec();
    if discovered.is_empty() {
        let dir_str = ext_manager.borrow().root_dir().display().to_string();
        let empty = gtk4::Label::new(Some(&format!(
            "No external extensions installed. Core Sillpaper stays 100% offline with zero overhead.\nInstall optional extensions into:\n{dir_str}"
        )));
        empty.add_css_class("sillpaper-hint-note");
        empty.set_xalign(0.0);
        empty.set_wrap(true);
        container.append(&empty);
        return;
    }

    container.append(&sub_label("Ghost suggestions"));
    let ghost_group = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    ghost_group.add_css_class("sillpaper-seg-group");
    ghost_group.set_homogeneous(true);
    let initial_ghost = prefs.borrow().ghost_suggestion_mode;
    let ghost_buttons: Rc<RefCell<Vec<(GhostSuggestionMode, gtk4::Button)>>> =
        Rc::new(RefCell::new(Vec::new()));
    for mode in GhostSuggestionMode::ALL {
        let btn = gtk4::Button::with_label(mode.label());
        btn.set_can_focus(false);
        btn.set_focus_on_click(false);
        let tip = match mode {
            GhostSuggestionMode::OnShortcut => {
                "Only suggest when you press Ctrl+Space (never interrupts typing)"
            }
            GhostSuggestionMode::AutoWhileTyping => {
                "Automatically suggest inline continuations after pausing while typing"
            }
            GhostSuggestionMode::Off => "Disable inline ghost suggestions",
        };
        btn.set_tooltip_text(Some(tip));
        if initial_ghost == mode {
            btn.add_css_class("seg-active");
        }
        ghost_buttons.borrow_mut().push((mode, btn.clone()));
        ghost_group.append(&btn);
    }
    for (mode, btn) in ghost_buttons.borrow().iter() {
        let mode = *mode;
        let btns = ghost_buttons.clone();
        let p = prefs.clone();
        let em = emit.clone();
        btn.connect_clicked(move |_| {
            p.borrow_mut().ghost_suggestion_mode = mode;
            for (id, b) in btns.borrow().iter() {
                if *id == mode {
                    b.add_css_class("seg-active");
                } else {
                    b.remove_css_class("seg-active");
                }
            }
            em();
        });
    }
    container.append(&ghost_group);
    container.append(&hint_note(
        "On Shortcut (Ctrl+Space) keeps writing distraction-free. Tab accepts all · Ctrl+→ next word · Esc dismisses.",
    ));

    let initial_auto_diag = prefs.borrow().auto_inline_diagnostics;
    add_check_row_with_hint(
        container,
        "Auto inline grammar & style hints",
        Some("CTRL + ."),
        initial_auto_diag,
        prefs.clone(),
        emit.clone(),
        |p, v| p.auto_inline_diagnostics = v,
    );
    container.append(&hint_note(
        "Press Ctrl+. or click any wavy underline for quick-fixes. Select text → ✦ on the popup toolbar for rewrites (enable popup toolbar above).",
    ));

    for ext in discovered {
        let ext_id = ext.manifest.id.clone();
        let is_enabled = prefs.borrow().enabled_extensions.contains(&ext_id);

        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        card.add_css_class("sillpaper-ext-setting-card");

        let title = if ext.manifest.version.is_empty() {
            ext.manifest.name.clone()
        } else {
            format!("{} ({})", ext.manifest.name, ext.manifest.version)
        };
        let cb = gtk4::CheckButton::new();
        cb.set_active(is_enabled);
        cb.set_can_focus(false);
        let lbl = gtk4::Label::new(Some(&title));
        lbl.set_xalign(0.0);
        lbl.set_wrap(true);
        cb.set_child(Some(&lbl));
        card.append(&cb);

        if !ext.manifest.description.is_empty() {
            let desc = gtk4::Label::new(Some(&ext.manifest.description));
            desc.add_css_class("sillpaper-hint-note");
            desc.set_xalign(0.0);
            desc.set_wrap(true);
            card.append(&desc);
        }

        let fields_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        fields_box.set_margin_top(4);
        fields_box.set_visible(is_enabled && !ext.manifest.settings.is_empty());

        let current_cfg = ext_manager.borrow().load_config(&ext_id);
        struct ExtFieldRow {
            key: String,
            row: gtk4::Box,
            visible_when: HashMap<String, Vec<String>>,
            widget: ExtSettingWidget,
        }
        let mut field_rows: Vec<ExtFieldRow> = Vec::new();

        for s in &ext.manifest.settings {
            let row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            row.append(&sub_label(&s.label));
            let val = current_cfg
                .get(&s.key)
                .cloned()
                .unwrap_or_else(|| s.default.clone());
            let widget = match s.kind {
                SettingFieldKind::Select => {
                    let opts: Vec<&str> = s.options.iter().map(String::as_str).collect();
                    let dd = gtk4::DropDown::from_strings(&opts);
                    let idx = s.options.iter().position(|o| o == &val).unwrap_or(0) as u32;
                    dd.set_selected(idx);
                    row.append(&dd);
                    ExtSettingWidget::Select(dd, s.options.clone())
                }
                SettingFieldKind::Secret => {
                    let pe = gtk4::PasswordEntry::new();
                    pe.set_show_peek_icon(true);
                    pe.set_text(&val);
                    if !s.placeholder.is_empty() {
                        pe.set_placeholder_text(Some(&s.placeholder));
                    }
                    row.append(&pe);
                    ExtSettingWidget::Secret(pe)
                }
                SettingFieldKind::Text => {
                    let ent = gtk4::Entry::new();
                    ent.set_text(&val);
                    if !s.placeholder.is_empty() {
                        ent.set_placeholder_text(Some(&s.placeholder));
                    }
                    row.append(&ent);
                    ExtSettingWidget::Text(ent)
                }
            };
            fields_box.append(&row);
            field_rows.push(ExtFieldRow {
                key: s.key.clone(),
                row,
                visible_when: s.visible_when.clone(),
                widget,
            });
        }

        if !ext.manifest.settings.is_empty() {
            fields_box.append(&hint_note(
                "Leave Model blank to use the recommended default for the chosen provider.",
            ));
            let save_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            save_row.set_margin_top(6);
            let btn_save = gtk4::Button::with_label("Save Settings");
            btn_save.add_css_class("sillpaper-target-btn");
            btn_save.set_can_focus(false);
            btn_save.set_focus_on_click(false);
            let status_lbl = gtk4::Label::new(None);
            status_lbl.add_css_class("sillpaper-hint-note");
            save_row.append(&btn_save);
            save_row.append(&status_lbl);
            fields_box.append(&save_row);

            let rows_rc = Rc::new(field_rows);
            let read_values = {
                let rows = rows_rc.clone();
                move || -> HashMap<String, String> {
                    let mut map = HashMap::new();
                    for f in rows.iter() {
                        let v = match &f.widget {
                            ExtSettingWidget::Text(e) => e.text().trim().to_string(),
                            ExtSettingWidget::Secret(pe) => pe.text().trim().to_string(),
                            ExtSettingWidget::Select(dd, opts) => opts
                                .get(dd.selected() as usize)
                                .cloned()
                                .unwrap_or_default(),
                        };
                        map.insert(f.key.clone(), v);
                    }
                    map
                }
            };

            let sync_vis = {
                let rows = rows_rc.clone();
                let rv = read_values.clone();
                move || {
                    let vals = rv();
                    for f in rows.iter() {
                        if f.visible_when.is_empty() {
                            f.row.set_visible(true);
                            continue;
                        }
                        let show = f.visible_when.iter().all(|(dep_key, allowed)| {
                            vals.get(dep_key)
                                .is_some_and(|cur| allowed.iter().any(|a| a == cur))
                        });
                        f.row.set_visible(show);
                    }
                }
            };
            sync_vis();

            for f in rows_rc.iter() {
                if let ExtSettingWidget::Select(ref dd, _) = f.widget {
                    let sv = sync_vis.clone();
                    let st = status_lbl.clone();
                    dd.connect_selected_notify(move |_| {
                        sv();
                        st.set_text("");
                    });
                }
            }

            let mgr_c = ext_manager.clone();
            let eid_c = ext_id.clone();
            let st_c = status_lbl;
            btn_save.connect_clicked(move |_| {
                let map = read_values();
                match mgr_c.borrow_mut().save_config(&eid_c, &map) {
                    Ok(()) => st_c.set_text("Saved ✓"),
                    Err(e) => st_c.set_text(&e),
                }
            });
        }

        card.append(&fields_box);

        {
            let p = prefs.clone();
            let em = emit.clone();
            let eid = ext_id.clone();
            let fb = fields_box;
            let has_settings = !ext.manifest.settings.is_empty();
            cb.connect_toggled(move |c| {
                let active = c.is_active();
                {
                    let mut pb = p.borrow_mut();
                    if active {
                        if !pb.enabled_extensions.contains(&eid) {
                            pb.enabled_extensions.push(eid.clone());
                        }
                    } else {
                        pb.enabled_extensions.retain(|x| x != &eid);
                    }
                }
                fb.set_visible(active && has_settings);
                em();
            });
        }

        container.append(&card);
    }
}
