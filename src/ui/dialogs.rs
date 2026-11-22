use crate::ui::icons::{AppIcon, symbolic_image};
use crate::ui::word_counter::{TargetUnit, WritingGoal};
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

pub fn show_confirm_unsaved_dialog(
    parent: &impl IsA<gtk4::Widget>,
    title_name: &str,
    on_choice: impl FnOnce(UnsavedChoice) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading("Save changes?")
        .body(format!(
            "“{title_name}” has unsaved changes. Changes will be lost if you close without saving."
        ))
        .default_response("save")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[
        ("cancel", "Cancel"),
        ("discard", "Discard"),
        ("save", "Save"),
    ]);
    dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

    let cb = std::cell::RefCell::new(Some(on_choice));
    dialog.choose(Some(parent), gtk4::gio::Cancellable::NONE, move |resp| {
        if let Some(f) = cb.borrow_mut().take() {
            let choice = match resp.as_str() {
                "save" => UnsavedChoice::Save,
                "discard" => UnsavedChoice::Discard,
                _ => UnsavedChoice::Cancel,
            };
            f(choice);
        }
    });
}

pub fn show_external_change_dialog(
    parent: &impl IsA<gtk4::Widget>,
    file_name: &str,
    on_reload: impl FnOnce(bool) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading("File changed on disk")
        .body(format!(
            "“{file_name}” was modified by another application. Do you want to reload it?"
        ))
        .default_response("reload")
        .close_response("keep")
        .build();
    dialog.add_responses(&[("keep", "Keep Current"), ("reload", "Reload")]);
    dialog.set_response_appearance("reload", adw::ResponseAppearance::Suggested);

    let cb = std::cell::RefCell::new(Some(on_reload));
    dialog.choose(Some(parent), gtk4::gio::Cancellable::NONE, move |resp| {
        if let Some(f) = cb.borrow_mut().take() {
            f(resp.as_str() == "reload");
        }
    });
}

pub fn show_info_dialog(parent: &impl IsA<gtk4::Widget>, heading: &str, body: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .default_response("ok")
        .close_response("ok")
        .build();
    dialog.add_responses(&[("ok", "OK")]);
    dialog.present(Some(parent));
}

pub fn show_comment_dialog(
    parent: &impl IsA<gtk4::Window>,
    initial_text: Option<&str>,
    is_edit: bool,
    on_done: impl Fn(Option<String>) + 'static,
) {
    let win = gtk4::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(if is_edit {
            "Edit Comment"
        } else {
            "Insert Comment"
        })
        .default_width(460)
        .default_height(280)
        .resizable(true)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    vbox.set_margin_top(14);
    vbox.set_margin_bottom(14);
    vbox.set_margin_start(14);
    vbox.set_margin_end(14);

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_has_frame(true);

    let tv = gtk4::TextView::new();
    tv.set_wrap_mode(gtk4::WrapMode::WordChar);
    tv.set_top_margin(8);
    tv.set_bottom_margin(8);
    tv.set_left_margin(10);
    tv.set_right_margin(10);
    if let Some(txt) = initial_text {
        tv.buffer().set_text(txt);
    }
    scroll.set_child(Some(&tv));
    vbox.append(&scroll);

    let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    btn_row.set_halign(gtk4::Align::End);

    let on_done = std::rc::Rc::new(on_done);

    if is_edit {
        let btn_del = gtk4::Button::with_label("Delete");
        btn_del.add_css_class("destructive-action");
        let w = win.clone();
        let cb = on_done.clone();
        btn_del.connect_clicked(move |_| {
            cb(None);
            w.close();
        });
        btn_row.append(&btn_del);
    }

    let btn_cancel = gtk4::Button::with_label("Cancel");
    {
        let w = win.clone();
        btn_cancel.connect_clicked(move |_| w.close());
    }
    btn_row.append(&btn_cancel);

    let btn_save = gtk4::Button::with_label("Save");
    btn_save.add_css_class("suggested-action");
    {
        let w = win.clone();
        let buf = tv.buffer();
        let cb = on_done.clone();
        btn_save.connect_clicked(move |_| {
            let text = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .to_string();
            cb(Some(text));
            w.close();
        });
    }
    btn_row.append(&btn_save);
    vbox.append(&btn_row);

    let key = gtk4::EventControllerKey::new();
    {
        let w = win.clone();
        let buf = tv.buffer();
        let cb = on_done;
        key.connect_key_pressed(move |_, keyval, _, mods| {
            if keyval == gdk::Key::Escape {
                w.close();
                return glib::Propagation::Stop;
            }
            if (keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter)
                && mods.contains(gdk::ModifierType::CONTROL_MASK)
            {
                let text = buf
                    .text(&buf.start_iter(), &buf.end_iter(), false)
                    .to_string();
                cb(Some(text));
                w.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    win.add_controller(key);

    win.set_child(Some(&vbox));
    win.present();
    tv.grab_focus();
}

pub fn show_image_url_dialog(parent: &impl IsA<gtk4::Window>, on_url: impl Fn(String) + 'static) {
    let win = gtk4::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Link Picture from URL")
        .default_width(420)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some("https://example.com/image.png"));
    vbox.append(&entry);

    let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    btn_row.set_halign(gtk4::Align::End);
    let btn_cancel = gtk4::Button::with_label("Cancel");
    let btn_ok = gtk4::Button::with_label("Insert");
    btn_ok.add_css_class("suggested-action");
    btn_row.append(&btn_cancel);
    btn_row.append(&btn_ok);
    vbox.append(&btn_row);

    let cb = std::rc::Rc::new(on_url);
    let submit = {
        let e = entry.clone();
        let w = win.clone();
        let cb = cb.clone();
        move || {
            let url = e.text().trim().to_string();
            if !url.is_empty() {
                cb(url);
            }
            w.close();
        }
    };

    {
        let s = submit.clone();
        btn_ok.connect_clicked(move |_| s());
    }
    {
        let s = submit;
        entry.connect_activate(move |_| s());
    }
    {
        let w = win.clone();
        btn_cancel.connect_clicked(move |_| w.close());
    }

    let key = gtk4::EventControllerKey::new();
    {
        let w = win.clone();
        key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                w.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    win.add_controller(key);

    win.set_child(Some(&vbox));
    win.present();
    entry.grab_focus();
}

pub fn show_writing_goal_dialog(
    parent: &impl IsA<gtk4::Window>,
    current: Option<WritingGoal>,
    on_goal: impl Fn(Option<WritingGoal>) + 'static,
) {
    let win = gtk4::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Writing Target")
        .default_width(340)
        .resizable(false)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let spin = gtk4::SpinButton::with_range(10.0, 500_000.0, 50.0);
    spin.set_value(current.map(|g| g.target as f64).unwrap_or(500.0));
    spin.set_hexpand(true);

    let unit_dd = gtk4::DropDown::from_strings(&["Words", "Characters"]);
    unit_dd.set_selected(match current.map(|g| g.unit) {
        Some(TargetUnit::Characters) => 1,
        _ => 0,
    });
    row.append(&spin);
    row.append(&unit_dd);
    vbox.append(&row);

    let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    btn_row.set_halign(gtk4::Align::End);

    let cb = std::rc::Rc::new(on_goal);
    if current.is_some() {
        let btn_clear = gtk4::Button::with_label("Clear Target");
        let w = win.clone();
        let cb_c = cb.clone();
        btn_clear.connect_clicked(move |_| {
            cb_c(None);
            w.close();
        });
        btn_row.append(&btn_clear);
    }

    let btn_cancel = gtk4::Button::with_label("Cancel");
    {
        let w = win.clone();
        btn_cancel.connect_clicked(move |_| w.close());
    }
    btn_row.append(&btn_cancel);

    let btn_set = gtk4::Button::with_label("Set Target");
    btn_set.add_css_class("suggested-action");
    {
        let w = win.clone();
        let sp = spin.clone();
        let dd = unit_dd.clone();
        let cb_c = cb;
        btn_set.connect_clicked(move |_| {
            let unit = if dd.selected() == 1 {
                TargetUnit::Characters
            } else {
                TargetUnit::Words
            };
            cb_c(Some(WritingGoal {
                target: sp.value_as_int().max(1) as usize,
                unit,
            }));
            w.close();
        });
    }
    btn_row.append(&btn_set);
    vbox.append(&btn_row);

    win.set_child(Some(&vbox));
    win.present();
    spin.grab_focus();
}

pub fn build_shortcut_cheatsheet() -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.add_css_class("sillpaper-shortcut-overlay");
    container.set_halign(gtk4::Align::Center);
    container.set_valign(gtk4::Align::End);
    container.set_margin_bottom(48);
    container.set_can_target(false);
    container.set_visible(false);

    let grid = gtk4::Grid::new();
    grid.set_column_spacing(34);
    grid.set_row_spacing(14);
    grid.set_column_homogeneous(false);

    enum ShortcutGlyph {
        Text(&'static str, &'static str),
        Icon(AppIcon),
    }

    let items: [(i32, i32, ShortcutGlyph, &str); 9] = [
        (0, 0, ShortcutGlyph::Text("H1", "sc-heading"), "CTRL + 1"),
        (0, 1, ShortcutGlyph::Text("H2", "sc-heading"), "CTRL + 2"),
        (0, 2, ShortcutGlyph::Text("H3", "sc-heading"), "CTRL + 3"),
        (1, 0, ShortcutGlyph::Text("B", "sc-bold"), "CTRL + B"),
        (1, 1, ShortcutGlyph::Text("I", "sc-italic"), "CTRL + I"),
        (1, 2, ShortcutGlyph::Icon(AppIcon::Link), "CTRL + K"),
        (2, 0, ShortcutGlyph::Text("U", "sc-underline"), "CTRL + U"),
        (2, 1, ShortcutGlyph::Text("S", "sc-strike"), "CTRL + D"),
        (2, 2, ShortcutGlyph::Icon(AppIcon::Search), "CTRL + F"),
    ];

    for (col, row, glyph, keys) in items {
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        cell.set_valign(gtk4::Align::Center);

        match glyph {
            ShortcutGlyph::Text(txt, cls) => {
                let g = gtk4::Label::new(Some(txt));
                g.add_css_class("sillpaper-shortcut-glyph");
                g.add_css_class(cls);
                g.set_size_request(24, -1);
                g.set_xalign(0.5);
                cell.append(&g);
            }
            ShortcutGlyph::Icon(icon) => {
                let img = symbolic_image(icon, 15);
                img.add_css_class("sillpaper-shortcut-icon");
                img.set_size_request(24, -1);
                cell.append(&img);
            }
        }

        let k = gtk4::Label::new(Some(keys));
        k.add_css_class("sillpaper-shortcut-keys");
        k.set_xalign(0.0);

        cell.append(&k);
        grid.attach(&cell, col, row, 1, 1);
    }

    container.append(&grid);
    container
}
