use crate::config::GhostSuggestionMode;
use crate::extensions::{ExtensionDiagnostic, ExtensionManifest};
use crate::model::Selection;
use crate::ui::icons::{AppIcon, symbolic_button};
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type FixCallback = Rc<RefCell<Option<Box<dyn Fn(usize, ExtensionDiagnostic)>>>>;
type IgnoreCallback = Rc<RefCell<Option<Box<dyn Fn(usize, usize, usize)>>>>;
type PreviewCommitCallback = Rc<RefCell<Option<Box<dyn Fn(Selection, String, bool)>>>>;
type VoidCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;
type DismissHintCallback = Rc<RefCell<Option<Box<dyn Fn(String)>>>>;

#[derive(Clone)]
pub struct QuickFixCard {
    pub container: gtk4::Box,
    msg_label: gtk4::Label,
    fix_preview: gtk4::Label,
    btn_apply: gtk4::Button,
    current: Rc<RefCell<Option<(usize, ExtensionDiagnostic)>>>,
    on_apply: FixCallback,
    on_ignore: IgnoreCallback,
}

impl Default for QuickFixCard {
    fn default() -> Self {
        Self::new()
    }
}

impl QuickFixCard {
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        container.add_css_class("sillpaper-ext-card");
        container.set_halign(gtk4::Align::Start);
        container.set_valign(gtk4::Align::Start);
        container.set_visible(false);

        let msg_label = gtk4::Label::new(None);
        msg_label.add_css_class("ext-card-msg");
        msg_label.set_xalign(0.0);
        msg_label.set_wrap(true);
        msg_label.set_max_width_chars(38);

        let fix_preview = gtk4::Label::new(None);
        fix_preview.add_css_class("ext-card-fix");
        fix_preview.set_xalign(0.0);
        fix_preview.set_wrap(true);
        fix_preview.set_max_width_chars(38);

        let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        btn_row.set_halign(gtk4::Align::End);

        let btn_ignore = gtk4::Button::with_label("Ignore");
        btn_ignore.add_css_class("ext-btn-secondary");
        btn_ignore.set_can_focus(false);
        btn_ignore.set_focus_on_click(false);

        let btn_apply = gtk4::Button::with_label("Apply");
        btn_apply.add_css_class("ext-btn-primary");
        btn_apply.set_can_focus(false);
        btn_apply.set_focus_on_click(false);

        btn_row.append(&btn_ignore);
        btn_row.append(&btn_apply);

        container.append(&msg_label);
        container.append(&fix_preview);
        container.append(&btn_row);

        let current: Rc<RefCell<Option<(usize, ExtensionDiagnostic)>>> =
            Rc::new(RefCell::new(None));
        let on_apply: FixCallback = Rc::new(RefCell::new(None));
        let on_ignore: IgnoreCallback = Rc::new(RefCell::new(None));

        {
            let cur = current.clone();
            let cb = on_apply.clone();
            let box_c = container.clone();
            btn_apply.connect_clicked(move |_| {
                box_c.set_visible(false);
                if let Some((block, diag)) = cur.borrow_mut().take()
                    && let Some(f) = cb.borrow().as_ref()
                {
                    f(block, diag);
                }
            });
        }
        {
            let cur = current.clone();
            let cb = on_ignore.clone();
            let box_c = container.clone();
            btn_ignore.connect_clicked(move |_| {
                box_c.set_visible(false);
                if let Some((block, diag)) = cur.borrow_mut().take()
                    && let Some(f) = cb.borrow().as_ref()
                {
                    f(block, diag.start, diag.end);
                }
            });
        }

        Self {
            container,
            msg_label,
            fix_preview,
            btn_apply,
            current,
            on_apply,
            on_ignore,
        }
    }

    pub fn set_on_apply(&self, f: impl Fn(usize, ExtensionDiagnostic) + 'static) {
        *self.on_apply.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_ignore(&self, f: impl Fn(usize, usize, usize) + 'static) {
        *self.on_ignore.borrow_mut() = Some(Box::new(f));
    }

    pub fn is_visible(&self) -> bool {
        self.container.is_visible()
    }

    pub fn hide(&self) {
        self.container.set_visible(false);
        *self.current.borrow_mut() = None;
    }

    pub fn show_at(
        &self,
        block: usize,
        diag: ExtensionDiagnostic,
        bounds: (f32, f32, f32, f32),
        overlay: (f32, f32),
    ) {
        let (left, right, top, bottom) = bounds;
        let (overlay_w, overlay_h) = overlay;
        self.msg_label.set_text(&diag.message);
        if let Some(ref rep) = diag.replacement {
            self.fix_preview.set_text(&format!("→ {rep}"));
            self.fix_preview.set_visible(true);
            self.btn_apply.set_visible(true);
        } else {
            self.fix_preview.set_visible(false);
            self.btn_apply.set_visible(false);
        }
        *self.current.borrow_mut() = Some((block, diag));
        self.container.set_visible(true);

        let (_, nat_w, _, _) = self.container.measure(gtk4::Orientation::Horizontal, -1);
        let (_, nat_h, _, _) = self
            .container
            .measure(gtk4::Orientation::Vertical, nat_w.max(220));
        let card_w = (nat_w as f32).clamp(200.0, 340.0);
        let card_h = (nat_h as f32).clamp(72.0, 200.0);

        let center_x = (left + right) * 0.5;
        let x = (center_x - card_w * 0.5).clamp(12.0, (overlay_w - card_w - 12.0).max(12.0));
        let mut y = top - card_h - 10.0;
        if y < 50.0 {
            y = (bottom + 10.0).min((overlay_h - card_h - 12.0).max(50.0));
        }
        self.container.set_margin_start(x.round() as i32);
        self.container.set_margin_top(y.round() as i32);
    }
}

#[derive(Clone)]
pub struct ActionPreviewCard {
    pub container: gtk4::Box,
    title_label: gtk4::Label,
    result_label: gtk4::Label,
    btn_replace: gtk4::Button,
    btn_insert: gtk4::Button,
    pending_sel: Rc<RefCell<Option<Selection>>>,
    pending_text: Rc<RefCell<Option<String>>>,
    on_commit: PreviewCommitCallback,
    on_discard: VoidCallback,
}

impl Default for ActionPreviewCard {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionPreviewCard {
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        container.add_css_class("sillpaper-ext-preview");
        container.set_halign(gtk4::Align::Center);
        container.set_valign(gtk4::Align::End);
        container.set_margin_bottom(42);
        container.set_width_request(460);
        container.set_visible(false);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let title_label = gtk4::Label::new(Some("Suggestion"));
        title_label.add_css_class("ext-preview-title");
        title_label.set_hexpand(true);
        title_label.set_xalign(0.0);

        let btn_close = symbolic_button(AppIcon::Close);
        btn_close.add_css_class("sillpaper-icon-btn");
        btn_close.set_can_focus(false);
        btn_close.set_focus_on_click(false);
        header.append(&title_label);
        header.append(&btn_close);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_max_content_height(220);
        scroll.set_propagate_natural_height(true);

        let result_label = gtk4::Label::new(None);
        result_label.add_css_class("ext-preview-body");
        result_label.set_xalign(0.0);
        result_label.set_yalign(0.0);
        result_label.set_wrap(true);
        result_label.set_selectable(true);
        scroll.set_child(Some(&result_label));

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        actions.set_halign(gtk4::Align::End);

        let btn_discard = gtk4::Button::with_label("Discard");
        btn_discard.add_css_class("ext-btn-secondary");
        btn_discard.set_can_focus(false);
        btn_discard.set_focus_on_click(false);

        let btn_insert = gtk4::Button::with_label("Insert Below");
        btn_insert.add_css_class("ext-btn-secondary");
        btn_insert.set_can_focus(false);
        btn_insert.set_focus_on_click(false);

        let btn_replace = gtk4::Button::with_label("Replace");
        btn_replace.add_css_class("ext-btn-primary");
        btn_replace.set_can_focus(false);
        btn_replace.set_focus_on_click(false);

        actions.append(&btn_discard);
        actions.append(&btn_insert);
        actions.append(&btn_replace);

        container.append(&header);
        container.append(&scroll);
        container.append(&actions);

        let pending_sel: Rc<RefCell<Option<Selection>>> = Rc::new(RefCell::new(None));
        let pending_text: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let on_commit: PreviewCommitCallback = Rc::new(RefCell::new(None));
        let on_discard: VoidCallback = Rc::new(RefCell::new(None));

        let wire_commit = |btn: &gtk4::Button, insert_below: bool| {
            let ps = pending_sel.clone();
            let pt = pending_text.clone();
            let cb = on_commit.clone();
            let c = container.clone();
            btn.connect_clicked(move |_| {
                c.set_visible(false);
                if let (Some(sel), Some(text)) = (ps.borrow_mut().take(), pt.borrow_mut().take())
                    && let Some(f) = cb.borrow().as_ref()
                {
                    f(sel, text, insert_below);
                }
            });
        };
        wire_commit(&btn_replace, false);
        wire_commit(&btn_insert, true);

        let do_discard = {
            let ps = pending_sel.clone();
            let pt = pending_text.clone();
            let cb = on_discard.clone();
            let c = container.clone();
            move || {
                c.set_visible(false);
                *ps.borrow_mut() = None;
                *pt.borrow_mut() = None;
                if let Some(f) = cb.borrow().as_ref() {
                    f();
                }
            }
        };
        {
            let d = do_discard.clone();
            btn_discard.connect_clicked(move |_| d());
        }
        {
            let d = do_discard.clone();
            btn_close.connect_clicked(move |_| d());
        }
        {
            let key = gtk4::EventControllerKey::new();
            let d = do_discard;
            key.connect_key_pressed(move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
                    d();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            container.add_controller(key);
        }

        Self {
            container,
            title_label,
            result_label,
            btn_replace,
            btn_insert,
            pending_sel,
            pending_text,
            on_commit,
            on_discard,
        }
    }

    pub fn set_on_commit(&self, f: impl Fn(Selection, String, bool) + 'static) {
        *self.on_commit.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_discard(&self, f: impl Fn() + 'static) {
        *self.on_discard.borrow_mut() = Some(Box::new(f));
    }

    pub fn is_visible(&self) -> bool {
        self.container.is_visible()
    }

    pub fn show_loading(&self, label: &str, sel: Selection) {
        *self.pending_sel.borrow_mut() = Some(sel);
        *self.pending_text.borrow_mut() = None;
        self.title_label.set_text(&format!("✦ {label} — working…"));
        self.result_label
            .set_text("Waiting for extension response…");
        self.btn_replace.set_sensitive(false);
        self.btn_insert.set_sensitive(false);
        self.container.set_visible(true);
    }

    pub fn show_result(&self, label: &str, replacement: String) {
        self.title_label.set_text(&format!("✦ {label}"));
        self.result_label.set_text(&replacement);
        *self.pending_text.borrow_mut() = Some(replacement);
        self.btn_replace.set_sensitive(true);
        self.btn_insert.set_sensitive(true);
        self.container.set_visible(true);
    }

    pub fn show_error(&self, message: &str) {
        self.title_label.set_text("✦ Extension error");
        self.result_label.set_text(message);
        *self.pending_text.borrow_mut() = None;
        self.btn_replace.set_sensitive(false);
        self.btn_insert.set_sensitive(false);
        self.container.set_visible(true);
    }

    pub fn hide(&self) {
        self.container.set_visible(false);
        *self.pending_sel.borrow_mut() = None;
        *self.pending_text.borrow_mut() = None;
    }
}

#[derive(Clone)]
pub struct ExtensionHintBanner {
    pub container: gtk4::Box,
    title_label: gtk4::Label,
    tips_box: gtk4::Box,
    foot_label: gtk4::Label,
    current_ext_id: Rc<RefCell<Option<String>>>,
    on_dismiss: DismissHintCallback,
}

impl Default for ExtensionHintBanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionHintBanner {
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        container.add_css_class("sillpaper-ext-hint");
        container.set_halign(gtk4::Align::Start);
        container.set_valign(gtk4::Align::End);
        container.set_margin_start(16);
        container.set_margin_bottom(14);
        container.set_width_request(336);
        container.set_visible(false);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let title_label = gtk4::Label::new(Some("✦ Extension Ready"));
        title_label.add_css_class("ext-hint-title");
        title_label.set_xalign(0.0);
        title_label.set_hexpand(true);

        let btn_close = symbolic_button(AppIcon::Close);
        btn_close.add_css_class("sillpaper-icon-btn");
        btn_close.set_tooltip_text(Some("Dismiss permanently"));
        btn_close.set_can_focus(false);
        btn_close.set_focus_on_click(false);

        header.append(&title_label);
        header.append(&btn_close);

        let tips_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);

        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        footer.set_margin_top(2);
        let foot_label = gtk4::Label::new(Some("Configure anytime in Settings → Extensions."));
        foot_label.add_css_class("ext-hint-foot");
        foot_label.set_xalign(0.0);
        foot_label.set_wrap(true);
        foot_label.set_hexpand(true);

        let btn_got_it = gtk4::Button::with_label("Got it");
        btn_got_it.add_css_class("ext-btn-primary");
        btn_got_it.set_valign(gtk4::Align::Center);
        btn_got_it.set_can_focus(false);
        btn_got_it.set_focus_on_click(false);

        footer.append(&foot_label);
        footer.append(&btn_got_it);

        container.append(&header);
        container.append(&tips_box);
        container.append(&footer);

        let current_ext_id: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let on_dismiss: DismissHintCallback = Rc::new(RefCell::new(None));

        let dismiss = {
            let box_c = container.clone();
            let id_slot = current_ext_id.clone();
            let cb = on_dismiss.clone();
            move || {
                box_c.set_visible(false);
                if let Some(ext_id) = id_slot.borrow_mut().take()
                    && let Some(f) = cb.borrow().as_ref()
                {
                    f(ext_id);
                }
            }
        };

        {
            let d = dismiss.clone();
            btn_close.connect_clicked(move |_| d());
        }
        {
            let d = dismiss;
            btn_got_it.connect_clicked(move |_| d());
        }

        Self {
            container,
            title_label,
            tips_box,
            foot_label,
            current_ext_id,
            on_dismiss,
        }
    }

    pub fn set_on_dismiss(&self, f: impl Fn(String) + 'static) {
        *self.on_dismiss.borrow_mut() = Some(Box::new(f));
    }

    pub fn current_extension_id(&self) -> Option<String> {
        self.current_ext_id.borrow().clone()
    }

    pub fn is_visible(&self) -> bool {
        self.container.is_visible()
    }

    pub fn hide(&self) {
        self.container.set_visible(false);
        *self.current_ext_id.borrow_mut() = None;
    }

    pub fn show_for(
        &self,
        manifest: &ExtensionManifest,
        floating_toolbar_enabled: bool,
        ghost_mode: GhostSuggestionMode,
    ) {
        *self.current_ext_id.borrow_mut() = Some(manifest.id.clone());
        self.title_label
            .set_text(&format!("✦ {} — Quick Guide", manifest.name));

        while let Some(child) = self.tips_box.first_child() {
            self.tips_box.remove(&child);
        }

        let add_tip = |kbd: &str, desc: &str| {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            row.set_valign(gtk4::Align::Start);
            let k = gtk4::Label::new(Some(kbd));
            k.add_css_class("ext-hint-kbd");
            k.set_valign(gtk4::Align::Center);
            let d = gtk4::Label::new(Some(desc));
            d.add_css_class("ext-hint-desc");
            d.set_xalign(0.0);
            d.set_wrap(true);
            d.set_hexpand(true);
            d.set_max_width_chars(34);
            row.append(&k);
            row.append(&d);
            self.tips_box.append(&row);
        };

        if manifest.capabilities.completions {
            let desc = match ghost_mode {
                GhostSuggestionMode::OnShortcut => {
                    "Suggest inline text on demand (Tab accept · Ctrl+→ next word · Esc dismiss)"
                }
                GhostSuggestionMode::AutoWhileTyping => {
                    "Trigger inline suggestion anytime (also auto-suggests while typing; Tab accept · Esc dismiss)"
                }
                GhostSuggestionMode::Off => {
                    "Ghost suggestions are Off — switch to On Shortcut or Auto in Settings"
                }
            };
            add_tip("Ctrl+Space", desc);
        }

        if manifest.capabilities.diagnostics {
            add_tip(
                "Ctrl+.",
                "Check grammar & apply quick-fixes (or click a wavy underline)",
            );
        }

        if !manifest.capabilities.actions.is_empty() {
            let labels: Vec<&str> = manifest
                .capabilities
                .actions
                .iter()
                .map(|a| a.label.as_str())
                .take(4)
                .collect();
            let list = labels.join(", ");
            if floating_toolbar_enabled {
                add_tip("Select → ✦", &format!("Run {list} with live preview"));
            } else {
                add_tip(
                    "Select → ✦",
                    &format!("Enable popup toolbar in Settings to run {list}"),
                );
            }
        }

        self.foot_label
            .set_text("Close this card and it won't appear again. View tips anytime in Settings → Extensions.");
        self.container.set_visible(true);
    }
}
