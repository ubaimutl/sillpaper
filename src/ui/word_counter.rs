use crate::config::{Preferences, resolve_palette};
use crate::model::stats::{DocumentStats, format_reading_time};
use gtk4::glib;
use gtk4::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetUnit {
    Words,
    Characters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WritingGoal {
    pub target: usize,
    pub unit: TargetUnit,
}

#[derive(Clone)]
pub struct WordCounterWidget {
    pub pill: gtk4::Button,
    sub_label: gtk4::Label,
    words_label: gtk4::Label,
    chars_label: gtk4::Label,
    time_label: gtk4::Label,
    pub toast: gtk4::Label,
}

impl Default for WordCounterWidget {
    fn default() -> Self {
        Self::new()
    }
}

pub fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let len = s.len();
    if len <= 3 {
        return s;
    }
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

impl WordCounterWidget {
    pub fn new() -> Self {
        let pill = gtk4::Button::new();
        pill.add_css_class("sillpaper-counter-pill");
        pill.set_halign(gtk4::Align::End);
        pill.set_valign(gtk4::Align::End);
        pill.set_margin_end(14);
        pill.set_margin_bottom(0);
        pill.set_can_focus(false);
        pill.set_focus_on_click(false);
        pill.set_tooltip_text(Some("Click to set a writing target"));

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        vbox.set_valign(gtk4::Align::Center);
        let sub_label = gtk4::Label::new(Some("selected text"));
        sub_label.add_css_class("counter-sub");
        sub_label.set_xalign(0.0);
        sub_label.set_visible(false);

        let metrics_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        metrics_row.set_valign(gtk4::Align::Center);

        let mk_metric = || {
            let l = gtk4::Label::new(None);
            l.add_css_class("counter-main");
            l.set_use_markup(true);
            l.set_xalign(0.0);
            l
        };
        let words_label = mk_metric();
        let chars_label = mk_metric();
        let time_label = mk_metric();

        metrics_row.append(&words_label);
        metrics_row.append(&chars_label);
        metrics_row.append(&time_label);

        vbox.append(&sub_label);
        vbox.append(&metrics_row);
        pill.set_child(Some(&vbox));

        let toast = gtk4::Label::new(Some("Target achieved!"));
        toast.add_css_class("sillpaper-toast");
        toast.set_halign(gtk4::Align::Center);
        toast.set_valign(gtk4::Align::End);
        toast.set_margin_bottom(56);
        toast.set_visible(false);

        Self {
            pill,
            sub_label,
            words_label,
            chars_label,
            time_label,
            toast,
        }
    }

    pub fn update(
        &self,
        stats: DocumentStats,
        prefs: &Preferences,
        goal: Option<WritingGoal>,
    ) -> bool {
        let pal = resolve_palette(prefs);
        let num_color = pal.text;

        let chars = if prefs.count_spaces_in_chars {
            stats.chars_with_spaces
        } else {
            stats.chars_no_spaces
        };

        let mut goal_reached = false;
        if let Some(g) = goal
            && !stats.is_selection
        {
            goal_reached = match g.unit {
                TargetUnit::Words => stats.words >= g.target,
                TargetUnit::Characters => chars >= g.target,
            };
        }

        if !prefs.word_counter_always_visible {
            if self.pill.is_visible() {
                self.pill.set_visible(false);
            }
            return goal_reached;
        }

        self.sub_label.set_visible(stats.is_selection);

        if let Some(g) = goal
            && !stats.is_selection
        {
            match g.unit {
                TargetUnit::Words => {
                    self.words_label.set_markup(&format!(
                        "<span foreground=\"{num_color}\" weight=\"semibold\">{}</span> of {} words",
                        format_with_commas(stats.words),
                        format_with_commas(g.target)
                    ));
                }
                TargetUnit::Characters => {
                    self.words_label.set_markup(&format!(
                        "<span foreground=\"{num_color}\" weight=\"semibold\">{}</span> of {} characters",
                        format_with_commas(chars),
                        format_with_commas(g.target)
                    ));
                }
            }
        } else {
            let word_lbl = if stats.words == 1 { "word" } else { "words" };
            self.words_label.set_markup(&format!(
                "<span foreground=\"{num_color}\" weight=\"semibold\">{}</span> {word_lbl}",
                format_with_commas(stats.words)
            ));
        }

        let show_chars = prefs.show_character_count
            && !matches!(goal, Some(WritingGoal { unit: TargetUnit::Characters, .. }) if !stats.is_selection);
        self.chars_label.set_visible(show_chars);
        if show_chars {
            self.chars_label.set_markup(&format!(
                "<span foreground=\"{num_color}\" weight=\"semibold\">{}</span> characters",
                format_with_commas(chars)
            ));
        }

        self.time_label.set_visible(prefs.show_reading_time);
        if prefs.show_reading_time {
            let rt = format_reading_time(stats.reading_seconds);
            self.time_label.set_markup(&format!(
                "<span foreground=\"{num_color}\" weight=\"semibold\">{rt}</span> reading time"
            ));
        }

        if goal_reached {
            self.pill.add_css_class("goal-reached");
        } else {
            self.pill.remove_css_class("goal-reached");
        }

        self.pill.set_visible(prefs.word_counter_always_visible);
        goal_reached
    }

    pub fn show_toast(&self, msg: &str) {
        self.toast.set_text(msg);
        self.toast.set_visible(true);
        let toast_c = self.toast.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(2600), move || {
            toast_c.set_visible(false);
        });
    }
}
