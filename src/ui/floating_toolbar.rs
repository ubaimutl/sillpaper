use crate::editor::ToolbarState;
use crate::extensions::ExtensionActionDef;
use crate::ui::icons::{AppIcon, symbolic_button, symbolic_image};
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum ToolbarAction {
    Paragraph,
    Heading(u8),
    Bold,
    Italic,
    Underline,
    Strikethrough,
    OpenLink,
    SetLink(Option<String>),
    Blockquote,
    BulletList,
    OrderedList,
    OpenExtensionPrompt {
        ext_id: String,
        action_id: String,
        label: String,
    },
    RunExtensionAction {
        ext_id: String,
        action_id: String,
        label: String,
        prompt: Option<String>,
    },
}

type ActionCallback = Rc<RefCell<Option<Box<dyn Fn(ToolbarAction)>>>>;
type VoidCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

#[derive(Clone)]
pub struct FloatingToolbar {
    pub container: gtk4::Box,
    caption_pill: gtk4::Box,
    bar: gtk4::Box,
    stack: gtk4::Stack,
    style_trigger_btn: gtk4::Button,
    style_trigger_label: gtk4::Label,
    btn_body: gtk4::Button,
    btn_h1: gtk4::Button,
    btn_h2: gtk4::Button,
    btn_h3: gtk4::Button,
    btn_quote: gtk4::Button,
    btn_ul: gtk4::Button,
    btn_ol: gtk4::Button,
    btn_bold: gtk4::Button,
    btn_italic: gtk4::Button,
    btn_underline: gtk4::Button,
    btn_strike: gtk4::Button,
    btn_link: gtk4::Button,
    btn_ext: gtk4::Button,
    ext_actions_box: gtk4::Box,
    ext_prompt_entry: gtk4::Entry,
    pending_prompt_target: Rc<RefCell<Option<(String, String, String)>>>,
    caption_label: gtk4::Label,
    link_entry: gtk4::Entry,
    on_action: ActionCallback,
    on_return_focus: VoidCallback,
}

impl Default for FloatingToolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatingToolbar {
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 7);
        container.set_halign(gtk4::Align::Start);
        container.set_valign(gtk4::Align::Start);
        container.set_visible(false);

        let caption_pill = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        caption_pill.add_css_class("sillpaper-toolbar-caption");
        caption_pill.set_halign(gtk4::Align::Center);
        caption_pill.set_can_target(false);
        caption_pill.set_can_focus(false);
        let caption_label = gtk4::Label::new(Some(" "));
        caption_pill.append(&caption_label);

        let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        bar.add_css_class("sillpaper-toolbar-bar");
        bar.set_halign(gtk4::Align::Center);

        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        stack.set_transition_duration(120);
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(true);

        let attach_hover = |widget: &gtk4::Button, caption: &'static str| {
            let motion = gtk4::EventControllerMotion::new();
            let pill = caption_pill.clone();
            let lbl = caption_label.clone();
            motion.connect_enter(move |_, _, _| {
                lbl.set_text(caption);
                pill.add_css_class("caption-visible");
            });
            let pill_l = caption_pill.clone();
            motion.connect_leave(move |_| {
                pill_l.remove_css_class("caption-visible");
            });
            widget.add_controller(motion);
        };

        let mk_circle_label = |label: &str, css_class: &str, caption: &'static str| {
            let b = gtk4::Button::with_label(label);
            b.add_css_class("ftb-circle");
            b.add_css_class(css_class);
            b.set_focus_on_click(false);
            b.set_can_focus(false);
            attach_hover(&b, caption);
            b
        };

        let mk_circle_icon = |icon: AppIcon, css_class: &str, caption: &'static str| {
            let b = symbolic_button(icon);
            b.add_css_class("ftb-circle");
            if !css_class.is_empty() {
                b.add_css_class(css_class);
            }
            b.set_focus_on_click(false);
            b.set_can_focus(false);
            attach_hover(&b, caption);
            b
        };

        let buttons_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        buttons_box.set_valign(gtk4::Align::Center);

        let style_trigger_btn = gtk4::Button::new();
        style_trigger_btn.add_css_class("ftb-style-trigger");
        style_trigger_btn.add_css_class("ftb-style-selector");
        style_trigger_btn.set_focus_on_click(false);
        style_trigger_btn.set_can_focus(false);
        let trig_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        trig_row.set_valign(gtk4::Align::Center);
        let style_trigger_label = gtk4::Label::new(Some("Text"));
        let chevron = symbolic_image(AppIcon::ChevronDown, 12);
        chevron.add_css_class("ftb-chevron");
        trig_row.append(&style_trigger_label);
        trig_row.append(&chevron);
        style_trigger_btn.set_child(Some(&trig_row));
        attach_hover(&style_trigger_btn, "Block style");

        let btn_bold = mk_circle_label("B", "ftb-bold", "Bold  ·  Ctrl+B");
        let btn_italic = mk_circle_label("I", "ftb-italic", "Italic  ·  Ctrl+I");
        let btn_underline = mk_circle_label("U", "ftb-underline", "Underline  ·  Ctrl+U");
        let btn_strike = mk_circle_label("S", "ftb-strike", "Strikethrough  ·  Ctrl+D");
        let btn_link = mk_circle_icon(AppIcon::Link, "", "Link  ·  Ctrl+K");
        let btn_ext = mk_circle_label("✦", "ftb-extension", "Extension actions");
        btn_ext.set_visible(false);

        let inline_tools = gtk4::Box::new(gtk4::Orientation::Horizontal, 1);
        inline_tools.add_css_class("ftb-group");
        for button in [
            &btn_bold,
            &btn_italic,
            &btn_underline,
            &btn_strike,
            &btn_link,
            &btn_ext,
        ] {
            inline_tools.append(button);
        }
        buttons_box.append(&style_trigger_btn);
        buttons_box.append(&inline_tools);

        let styles_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 3);
        styles_box.add_css_class("ftb-group");
        styles_box.set_valign(gtk4::Align::Center);

        let btn_styles_back = mk_circle_icon(AppIcon::ChevronLeft, "", "Back");
        let btn_body = mk_circle_label("¶", "ftb-heading", "Body text");
        let btn_h1 = mk_circle_label("H1", "ftb-heading", "Heading 1  ·  Ctrl+1");
        let btn_h2 = mk_circle_label("H2", "ftb-heading", "Heading 2  ·  Ctrl+2");
        let btn_h3 = mk_circle_label("H3", "ftb-heading", "Heading 3  ·  Ctrl+3");
        let styles_sep = gtk4::Separator::new(gtk4::Orientation::Vertical);
        let btn_quote = mk_circle_label("“", "ftb-quote", "Blockquote  ·  Ctrl+Shift+B");
        let btn_ul = mk_circle_icon(AppIcon::ListBullet, "", "Bulleted list");
        let btn_ol = mk_circle_icon(AppIcon::ListOrdered, "", "Numbered list");

        for w in [
            btn_styles_back.upcast_ref::<gtk4::Widget>(),
            btn_body.upcast_ref(),
            btn_h1.upcast_ref(),
            btn_h2.upcast_ref(),
            btn_h3.upcast_ref(),
            styles_sep.upcast_ref(),
            btn_quote.upcast_ref(),
            btn_ul.upcast_ref(),
            btn_ol.upcast_ref(),
        ] {
            styles_box.append(w);
        }

        let link_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        link_box.add_css_class("ftb-group");
        link_box.set_valign(gtk4::Align::Center);
        let link_entry = gtk4::Entry::new();
        link_entry.set_placeholder_text(Some("Paste or type a link…"));
        link_entry.set_width_chars(24);
        let btn_apply_link = mk_circle_icon(AppIcon::Check, "ftb-primary-cap", "Apply link");
        let btn_cancel_link = mk_circle_icon(AppIcon::Close, "", "Cancel  ·  Esc");
        link_box.append(&link_entry);
        link_box.append(&btn_apply_link);
        link_box.append(&btn_cancel_link);

        let ext_actions_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        ext_actions_box.add_css_class("ftb-group");
        ext_actions_box.set_valign(gtk4::Align::Center);

        let ext_prompt_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        ext_prompt_box.add_css_class("ftb-group");
        ext_prompt_box.set_valign(gtk4::Align::Center);
        let ext_prompt_entry = gtk4::Entry::new();
        ext_prompt_entry.set_placeholder_text(Some("Instruction for selection…"));
        ext_prompt_entry.set_width_chars(24);
        let btn_run_prompt = mk_circle_icon(AppIcon::Check, "ftb-primary-cap", "Run instruction");
        let btn_cancel_prompt = mk_circle_icon(AppIcon::Close, "", "Cancel  ·  Esc");
        ext_prompt_box.append(&ext_prompt_entry);
        ext_prompt_box.append(&btn_run_prompt);
        ext_prompt_box.append(&btn_cancel_prompt);

        stack.add_named(&buttons_box, Some("buttons"));
        stack.add_named(&styles_box, Some("styles"));
        stack.add_named(&link_box, Some("link"));
        stack.add_named(&ext_actions_box, Some("ext_actions"));
        stack.add_named(&ext_prompt_box, Some("ext_prompt"));
        bar.append(&stack);

        container.append(&caption_pill);
        container.append(&bar);

        let on_action: ActionCallback = Rc::new(RefCell::new(None));
        let on_return_focus: VoidCallback = Rc::new(RefCell::new(None));

        let wire = |btn: &gtk4::Button, action: ToolbarAction| {
            let cb = on_action.clone();
            btn.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() {
                    f(action.clone());
                }
            });
        };

        let wire_style = |btn: &gtk4::Button, action: ToolbarAction| {
            let cb = on_action.clone();
            let st = stack.clone();
            let pill = caption_pill.clone();
            btn.connect_clicked(move |_| {
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("buttons");
                if let Some(f) = cb.borrow().as_ref() {
                    f(action.clone());
                }
            });
        };

        wire_style(&btn_body, ToolbarAction::Paragraph);
        wire_style(&btn_h1, ToolbarAction::Heading(1));
        wire_style(&btn_h2, ToolbarAction::Heading(2));
        wire_style(&btn_h3, ToolbarAction::Heading(3));
        wire_style(&btn_quote, ToolbarAction::Blockquote);
        wire_style(&btn_ul, ToolbarAction::BulletList);
        wire_style(&btn_ol, ToolbarAction::OrderedList);

        wire(&btn_bold, ToolbarAction::Bold);
        wire(&btn_italic, ToolbarAction::Italic);
        wire(&btn_underline, ToolbarAction::Underline);
        wire(&btn_strike, ToolbarAction::Strikethrough);

        {
            let st = stack.clone();
            let pill = caption_pill.clone();
            style_trigger_btn.connect_clicked(move |_| {
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("styles");
            });
        }
        {
            let st = stack.clone();
            let pill = caption_pill.clone();
            btn_styles_back.connect_clicked(move |_| {
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("buttons");
            });
        }

        {
            let st = stack.clone();
            let pill = caption_pill.clone();
            btn_ext.connect_clicked(move |_| {
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("ext_actions");
            });
        }

        let pending_prompt_target: Rc<RefCell<Option<(String, String, String)>>> =
            Rc::new(RefCell::new(None));

        let tb = Self {
            container,
            caption_pill: caption_pill.clone(),
            bar,
            stack: stack.clone(),
            style_trigger_btn,
            style_trigger_label,
            btn_body,
            btn_h1,
            btn_h2,
            btn_h3,
            btn_quote,
            btn_ul,
            btn_ol,
            btn_bold,
            btn_italic,
            btn_underline,
            btn_strike,
            btn_link: btn_link.clone(),
            btn_ext,
            ext_actions_box,
            ext_prompt_entry: ext_prompt_entry.clone(),
            pending_prompt_target: pending_prompt_target.clone(),
            caption_label,
            link_entry: link_entry.clone(),
            on_action: on_action.clone(),
            on_return_focus: on_return_focus.clone(),
        };

        let run_prompt = {
            let entry = ext_prompt_entry.clone();
            let cb = on_action.clone();
            let st = stack.clone();
            let pill = caption_pill.clone();
            let target = pending_prompt_target;
            move || {
                let text = entry.text().trim().to_string();
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("buttons");
                if text.is_empty() {
                    return;
                }
                if let Some((ext_id, action_id, label)) = target.borrow_mut().take()
                    && let Some(f) = cb.borrow().as_ref()
                {
                    f(ToolbarAction::RunExtensionAction {
                        ext_id,
                        action_id,
                        label,
                        prompt: Some(text),
                    });
                }
            }
        };
        {
            let f = run_prompt.clone();
            ext_prompt_entry.connect_activate(move |_| f());
        }
        {
            let f = run_prompt;
            btn_run_prompt.connect_clicked(move |_| f());
        }
        let cancel_prompt = {
            let st = stack.clone();
            let focus_cb = on_return_focus.clone();
            let pill = caption_pill.clone();
            move || {
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("buttons");
                if let Some(f) = focus_cb.borrow().as_ref() {
                    f();
                }
            }
        };
        {
            let f = cancel_prompt.clone();
            btn_cancel_prompt.connect_clicked(move |_| f());
        }
        {
            let key = gtk4::EventControllerKey::new();
            let f = cancel_prompt;
            key.connect_key_pressed(move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
                    f();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            ext_prompt_entry.add_controller(key);
        }

        {
            let cb = on_action.clone();
            btn_link.connect_clicked(move |_| {
                if let Some(f) = cb.borrow().as_ref() {
                    f(ToolbarAction::OpenLink);
                }
            });
        }

        let apply_link = {
            let entry = link_entry.clone();
            let cb = on_action;
            let st = stack.clone();
            let pill = caption_pill.clone();
            move || {
                let text = entry.text().trim().to_string();
                let url = if text.is_empty() { None } else { Some(text) };
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("buttons");
                if let Some(f) = cb.borrow().as_ref() {
                    f(ToolbarAction::SetLink(url));
                }
            }
        };
        {
            let f = apply_link.clone();
            link_entry.connect_activate(move |_| f());
        }
        {
            let f = apply_link;
            btn_apply_link.connect_clicked(move |_| f());
        }

        let cancel_link = {
            let st = stack;
            let focus_cb = on_return_focus;
            let pill = caption_pill;
            move || {
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("buttons");
                if let Some(f) = focus_cb.borrow().as_ref() {
                    f();
                }
            }
        };
        {
            let f = cancel_link.clone();
            btn_cancel_link.connect_clicked(move |_| f());
        }
        {
            let key = gtk4::EventControllerKey::new();
            let f = cancel_link;
            key.connect_key_pressed(move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
                    f();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            link_entry.add_controller(key);
        }

        tb
    }

    pub fn set_on_action(&self, f: impl Fn(ToolbarAction) + 'static) {
        *self.on_action.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_on_return_focus(&self, f: impl Fn() + 'static) {
        *self.on_return_focus.borrow_mut() = Some(Box::new(f));
    }

    pub fn update_state(&self, state: &ToolbarState) {
        let is_plain_body = state.heading.is_none()
            && !state.blockquote
            && !state.bullet_list
            && !state.ordered_list;

        let trigger_text = match state.heading {
            Some(1) => "H1",
            Some(2) => "H2",
            Some(3) => "H3",
            Some(_) => "Heading",
            None if state.blockquote => "Quote",
            None if state.bullet_list => "Bullets",
            None if state.ordered_list => "List",
            None => "Text",
        };
        self.style_trigger_label.set_text(trigger_text);
        set_active(&self.style_trigger_btn, !is_plain_body);

        set_active(&self.btn_body, is_plain_body);
        set_active(&self.btn_h1, state.heading == Some(1));
        set_active(&self.btn_h2, state.heading == Some(2));
        set_active(&self.btn_h3, state.heading == Some(3));
        set_active(&self.btn_bold, state.bold);
        set_active(&self.btn_italic, state.italic);
        set_active(&self.btn_underline, state.underline);
        set_active(&self.btn_strike, state.strikethrough);
        set_active(&self.btn_link, state.link.is_some());
        set_active(&self.btn_quote, state.blockquote);
        set_active(&self.btn_ul, state.bullet_list);
        set_active(&self.btn_ol, state.ordered_list);

        let any_block = state.can_heading || state.can_blockquote || state.can_list;
        self.style_trigger_btn.set_sensitive(any_block);
        self.btn_body.set_sensitive(any_block);
        self.btn_h1.set_sensitive(state.can_heading);
        self.btn_h2.set_sensitive(state.can_heading);
        self.btn_h3.set_sensitive(state.can_heading);
        self.btn_bold.set_sensitive(state.can_inline);
        self.btn_italic.set_sensitive(state.can_inline);
        self.btn_underline.set_sensitive(state.can_inline);
        self.btn_strike.set_sensitive(state.can_inline);
        self.btn_link.set_sensitive(state.can_inline);
        self.btn_quote.set_sensitive(state.can_blockquote);
        self.btn_ul.set_sensitive(state.can_list);
        self.btn_ol.set_sensitive(state.can_list);

        if self.stack.visible_child_name().as_deref() != Some("link") {
            if let Some(url) = &state.link {
                self.link_entry.set_text(url);
            } else {
                self.link_entry.set_text("");
            }
        }
    }

    pub fn show_clamped(
        &self,
        sel_left: f32,
        sel_right: f32,
        sel_top: f32,
        sel_bottom: f32,
        overlay_w: f32,
        overlay_h: f32,
    ) {
        self.container.set_visible(true);
        let (_, bar_w, _, _) = self.bar.measure(gtk4::Orientation::Horizontal, -1);
        let (_, bar_h, _, _) = self.bar.measure(gtk4::Orientation::Vertical, -1);
        let (_, cap_h, _, _) = self.caption_pill.measure(gtk4::Orientation::Vertical, -1);
        let tw = bar_w as f32;
        let bh = (bar_h as f32).max(46.0);
        let ch = (cap_h as f32).max(26.0) + 7.0;
        let total_h = bh + ch;

        let pad = 12.0_f32;
        let top_safe = 44.0_f32;
        let sel_mid_x = (sel_left + sel_right) * 0.5;
        let max_x = (overlay_w - tw - pad).max(pad);
        let x = (sel_mid_x - tw * 0.5).clamp(pad, max_x);

        let place_above = sel_top - bh - 12.0 >= top_safe;
        let y = if place_above {
            self.container
                .reorder_child_after(&self.bar, Some(&self.caption_pill));
            (sel_top - total_h - 10.0).clamp(8.0, (overlay_h - total_h - pad).max(8.0))
        } else {
            self.container
                .reorder_child_after(&self.caption_pill, Some(&self.bar));
            (sel_bottom + 10.0).clamp(top_safe, (overlay_h - total_h - pad).max(top_safe))
        };

        self.container.set_margin_start(x.round() as i32);
        self.container.set_margin_top(y.round() as i32);
    }

    pub fn open_link_input(&self, prefill: Option<&str>) {
        if let Some(url) = prefill {
            self.link_entry.set_text(url);
        } else {
            self.link_entry.set_text("");
        }
        self.caption_pill.remove_css_class("caption-visible");
        self.stack.set_visible_child_name("link");
        self.container.set_visible(true);
        self.link_entry.grab_focus();
        let entry_c = self.link_entry.clone();
        let tb_c = self.clone();
        glib::idle_add_local_once(move || {
            if tb_c.is_link_mode() {
                entry_c.grab_focus();
                if !entry_c.text().is_empty() {
                    entry_c.select_region(0, -1);
                }
            }
        });
    }

    pub fn cancel_link_input(&self) {
        self.caption_pill.remove_css_class("caption-visible");
        self.stack.set_visible_child_name("buttons");
        if let Some(f) = self.on_return_focus.borrow().as_ref() {
            f();
        }
    }

    pub fn set_extension_actions(&self, actions: &[(String, ExtensionActionDef)]) {
        while let Some(child) = self.ext_actions_box.first_child() {
            self.ext_actions_box.remove(&child);
        }
        if actions.is_empty() {
            self.btn_ext.set_visible(false);
            if matches!(
                self.stack.visible_child_name().as_deref(),
                Some("ext_actions") | Some("ext_prompt")
            ) {
                self.stack.set_visible_child_name("buttons");
            }
            return;
        }

        let btn_back = symbolic_button(AppIcon::ChevronLeft);
        btn_back.add_css_class("ftb-circle");
        btn_back.set_focus_on_click(false);
        btn_back.set_can_focus(false);
        {
            let st = self.stack.clone();
            let pill = self.caption_pill.clone();
            btn_back.connect_clicked(move |_| {
                pill.remove_css_class("caption-visible");
                st.set_visible_child_name("buttons");
            });
        }
        self.ext_actions_box.append(&btn_back);

        for (ext_id, act) in actions {
            let b = gtk4::Button::with_label(&act.label);
            b.add_css_class("ftb-style-trigger");
            b.set_focus_on_click(false);
            b.set_can_focus(false);

            if !act.tooltip.is_empty() {
                let motion = gtk4::EventControllerMotion::new();
                let pill = self.caption_pill.clone();
                let lbl = self.caption_label.clone();
                let tip = act.tooltip.clone();
                motion.connect_enter(move |_, _, _| {
                    lbl.set_text(&tip);
                    pill.add_css_class("caption-visible");
                });
                let pill_l = self.caption_pill.clone();
                motion.connect_leave(move |_| {
                    pill_l.remove_css_class("caption-visible");
                });
                b.add_controller(motion);
            }

            let cb = self.on_action.clone();
            let st = self.stack.clone();
            let pill = self.caption_pill.clone();
            let eid = ext_id.clone();
            let aid = act.id.clone();
            let label = act.label.clone();
            let prompt_input = act.prompt_input;
            b.connect_clicked(move |_| {
                pill.remove_css_class("caption-visible");
                if prompt_input {
                    if let Some(f) = cb.borrow().as_ref() {
                        f(ToolbarAction::OpenExtensionPrompt {
                            ext_id: eid.clone(),
                            action_id: aid.clone(),
                            label: label.clone(),
                        });
                    }
                } else {
                    st.set_visible_child_name("buttons");
                    if let Some(f) = cb.borrow().as_ref() {
                        f(ToolbarAction::RunExtensionAction {
                            ext_id: eid.clone(),
                            action_id: aid.clone(),
                            label: label.clone(),
                            prompt: None,
                        });
                    }
                }
            });
            self.ext_actions_box.append(&b);
        }
        self.btn_ext.set_visible(true);
    }

    pub fn open_extension_prompt(&self, ext_id: String, action_id: String, label: String) {
        *self.pending_prompt_target.borrow_mut() = Some((ext_id, action_id, label));
        self.ext_prompt_entry.set_text("");
        self.caption_pill.remove_css_class("caption-visible");
        self.stack.set_visible_child_name("ext_prompt");
        self.container.set_visible(true);
        self.ext_prompt_entry.grab_focus();
        let entry_c = self.ext_prompt_entry.clone();
        let tb_c = self.clone();
        glib::idle_add_local_once(move || {
            if tb_c.is_input_mode() {
                entry_c.grab_focus();
            }
        });
    }

    pub fn link_entry(&self) -> &gtk4::Entry {
        &self.link_entry
    }

    pub fn is_link_mode(&self) -> bool {
        self.container.is_visible() && self.stack.visible_child_name().as_deref() == Some("link")
    }

    pub fn is_input_mode(&self) -> bool {
        self.container.is_visible()
            && matches!(
                self.stack.visible_child_name().as_deref(),
                Some("link") | Some("ext_prompt")
            )
    }

    pub fn hide(&self) {
        self.caption_pill.remove_css_class("caption-visible");
        self.stack.set_visible_child_name("buttons");
        if self.container.is_visible() {
            self.container.set_visible(false);
        }
    }
}

fn set_active(btn: &gtk4::Button, active: bool) {
    if active {
        btn.add_css_class("active-format");
    } else {
        btn.remove_css_class("active-format");
    }
}
