use crate::model::TextDirection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeId {
    #[default]
    Light,
    Dark,
    Black,
    DarkBlue,
    Dune,
    OldSchool,
    Custom,
}

impl ThemeId {
    pub const ALL: [ThemeId; 7] = [
        ThemeId::Light,
        ThemeId::Dark,
        ThemeId::Black,
        ThemeId::DarkBlue,
        ThemeId::Dune,
        ThemeId::OldSchool,
        ThemeId::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Light => "Light",
            ThemeId::Dark => "Dark",
            ThemeId::Black => "Black",
            ThemeId::DarkBlue => "Dark Blue",
            ThemeId::Dune => "Dune",
            ThemeId::OldSchool => "Classic Blue",
            ThemeId::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextWidth {
    Narrow,
    #[default]
    Medium,
    Wide,
    ExtraWide,
    Percent75,
    Full,
}

impl TextWidth {
    pub const ALL: [TextWidth; 6] = [
        TextWidth::Narrow,
        TextWidth::Medium,
        TextWidth::Wide,
        TextWidth::ExtraWide,
        TextWidth::Percent75,
        TextWidth::Full,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TextWidth::Narrow => "Narrow",
            TextWidth::Medium => "Medium",
            TextWidth::Wide => "Wide",
            TextWidth::ExtraWide => "Extra Wide",
            TextWidth::Percent75 => "75%",
            TextWidth::Full => "Full",
        }
    }

    pub fn resolve_px(self, viewport_width: i32) -> i32 {
        match self {
            TextWidth::Narrow => 440,
            TextWidth::Medium => 680,
            TextWidth::Wide => 900,
            TextWidth::ExtraWide => 1140,
            TextWidth::Percent75 => ((viewport_width as f64) * 0.75) as i32,
            TextWidth::Full => (viewport_width - 56).max(200),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FontChoice {
    #[default]
    Serif,
    Sans,
    Monospace,
    System(String),
}

impl FontChoice {
    pub fn pango_family(&self) -> &str {
        match self {
            FontChoice::Serif => "Serif",
            FontChoice::Sans => "Sans",
            FontChoice::Monospace => "Monospace",
            FontChoice::System(name) => name.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineSpacing {
    Compact,
    #[default]
    Normal,
    Relaxed,
    Spacious,
    Extra,
    Double,
}

impl LineSpacing {
    pub const ALL: [LineSpacing; 6] = [
        LineSpacing::Compact,
        LineSpacing::Normal,
        LineSpacing::Relaxed,
        LineSpacing::Spacious,
        LineSpacing::Extra,
        LineSpacing::Double,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineSpacing::Compact => "1.2×",
            LineSpacing::Normal => "1.4×",
            LineSpacing::Relaxed => "1.6×",
            LineSpacing::Spacious => "1.8×",
            LineSpacing::Extra => "2.0×",
            LineSpacing::Double => "2.5×",
        }
    }

    pub fn factor(self) -> f32 {
        match self {
            LineSpacing::Compact => 1.00,
            LineSpacing::Normal => 1.15,
            LineSpacing::Relaxed => 1.30,
            LineSpacing::Spacious => 1.45,
            LineSpacing::Extra => 1.62,
            LineSpacing::Double => 2.00,
        }
    }

    pub fn heading_factor(self) -> f32 {
        match self {
            LineSpacing::Compact => 1.05,
            LineSpacing::Normal => 1.15,
            LineSpacing::Relaxed => 1.25,
            LineSpacing::Spacious => 1.35,
            LineSpacing::Extra => 1.48,
            LineSpacing::Double => 1.70,
        }
    }

    pub fn css_factor(self) -> f32 {
        match self {
            LineSpacing::Compact => 1.25,
            LineSpacing::Normal => 1.40,
            LineSpacing::Relaxed => 1.60,
            LineSpacing::Spacious => 1.80,
            LineSpacing::Extra => 2.00,
            LineSpacing::Double => 2.50,
        }
    }

    pub fn css_heading_factor(self) -> f32 {
        match self {
            LineSpacing::Compact => 1.20,
            LineSpacing::Normal => 1.30,
            LineSpacing::Relaxed => 1.40,
            LineSpacing::Spacious => 1.50,
            LineSpacing::Extra => 1.65,
            LineSpacing::Double => 1.90,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AccentSwatch {
    #[default]
    Default,
    Blue,
    Pink,
    Green,
    Orange,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DefaultSaveFormat {
    #[default]
    Html,
    Markdown,
    PlainText,
}

impl DefaultSaveFormat {
    pub fn extension(self) -> &'static str {
        match self {
            DefaultSaveFormat::Html => "html",
            DefaultSaveFormat::Markdown => "md",
            DefaultSaveFormat::PlainText => "txt",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DefaultSaveFormat::Html => "Rich HTML (.html)",
            DefaultSaveFormat::Markdown => "Markdown (.md)",
            DefaultSaveFormat::PlainText => "Plain Text (.txt)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RendererBackend {
    #[default]
    Auto,
    Vulkan,
    Cairo,
}

impl RendererBackend {
    pub const ALL: [RendererBackend; 3] = [
        RendererBackend::Auto,
        RendererBackend::Vulkan,
        RendererBackend::Cairo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RendererBackend::Auto => "Auto",
            RendererBackend::Vulkan => "Vulkan",
            RendererBackend::Cairo => "Cairo",
        }
    }

    pub fn gsk_env_value(self) -> Option<&'static str> {
        match self {
            RendererBackend::Auto => None,
            RendererBackend::Vulkan => Some("vulkan"),
            RendererBackend::Cairo => Some("cairo"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GhostSuggestionMode {
    #[default]
    OnShortcut,
    AutoWhileTyping,
    Off,
}

impl GhostSuggestionMode {
    pub const ALL: [GhostSuggestionMode; 3] = [
        GhostSuggestionMode::OnShortcut,
        GhostSuggestionMode::AutoWhileTyping,
        GhostSuggestionMode::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GhostSuggestionMode::OnShortcut => "On Shortcut",
            GhostSuggestionMode::AutoWhileTyping => "Auto",
            GhostSuggestionMode::Off => "Off",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartQuotePair {
    pub double_open: char,
    pub double_close: char,
    pub single_open: char,
    pub single_close: char,
}

impl Default for SmartQuotePair {
    fn default() -> Self {
        Self {
            double_open: '\u{201C}',
            double_close: '\u{201D}',
            single_open: '\u{2018}',
            single_close: '\u{2019}',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: ThemeId,
    pub custom_theme_bg: String,
    pub text_width: TextWidth,
    pub font: FontChoice,
    pub font_size_percent: u16,
    pub text_color: AccentSwatch,
    pub custom_text_color: String,
    pub caret_color: AccentSwatch,
    pub paragraph_margins: bool,
    pub line_spacing: LineSpacing,

    pub focus_mode: bool,
    pub focus_mode_opacity: f64,
    pub typewriter_mode: bool,
    pub hide_chrome_while_typing: bool,
    pub hide_chrome_unless_hovered: bool,
    pub hide_caret_while_typing: bool,
    pub show_toc_button: bool,

    pub floating_toolbar: bool,
    pub smart_quotes: bool,
    pub smart_quote_pair: SmartQuotePair,
    pub smart_dashes: bool,
    pub smart_ellipses: bool,
    pub smart_arrows: bool,
    pub markdown_asterisk_lists: bool,
    pub markdown_ordered_lists: bool,
    pub markdown_inline_asterisks: bool,
    pub default_direction: TextDirection,
    pub zwnj_on_shift_space: bool,

    pub word_counter_always_visible: bool,
    pub show_character_count: bool,
    pub count_spaces_in_chars: bool,
    pub show_reading_time: bool,
    pub words_per_minute: u32,

    pub default_save_format: DefaultSaveFormat,
    pub default_save_folder: Option<PathBuf>,
    pub include_date_in_filenames: bool,

    pub renderer: RendererBackend,

    pub enabled_extensions: Vec<String>,
    pub ghost_suggestion_mode: GhostSuggestionMode,
    pub auto_inline_diagnostics: bool,
    pub dismissed_extension_hints: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemeId::Light,
            custom_theme_bg: "#FAFDFF".to_string(),
            text_width: TextWidth::Medium,
            font: FontChoice::Serif,
            font_size_percent: 100,
            text_color: AccentSwatch::Default,
            custom_text_color: "#3C3C3C".to_string(),
            caret_color: AccentSwatch::Blue,
            paragraph_margins: true,
            line_spacing: LineSpacing::Normal,

            focus_mode: false,
            focus_mode_opacity: 0.22,
            typewriter_mode: false,
            hide_chrome_while_typing: true,
            hide_chrome_unless_hovered: false,
            hide_caret_while_typing: false,
            show_toc_button: true,

            floating_toolbar: true,
            smart_quotes: true,
            smart_quote_pair: SmartQuotePair::default(),
            smart_dashes: true,
            smart_ellipses: true,
            smart_arrows: true,
            markdown_asterisk_lists: true,
            markdown_ordered_lists: true,
            markdown_inline_asterisks: true,
            default_direction: TextDirection::Auto,
            zwnj_on_shift_space: false,

            word_counter_always_visible: true,
            show_character_count: true,
            count_spaces_in_chars: false,
            show_reading_time: true,
            words_per_minute: 270,

            default_save_format: DefaultSaveFormat::Html,
            default_save_folder: None,
            include_date_in_filenames: false,

            renderer: RendererBackend::Auto,

            enabled_extensions: Vec::new(),
            ghost_suggestion_mode: GhostSuggestionMode::OnShortcut,
            auto_inline_diagnostics: true,
            dismissed_extension_hints: Vec::new(),
        }
    }
}

impl Preferences {
    pub fn font_size_pt(&self) -> f64 {
        let pct = self.font_size_percent.clamp(40, 600) as f64;
        13.5 * (pct / 100.0)
    }

    pub fn bump_font_size(&mut self, delta_percent: i16) {
        let next = (self.font_size_percent as i16 + delta_percent).clamp(40, 600);
        self.font_size_percent = next as u16;
    }
}
