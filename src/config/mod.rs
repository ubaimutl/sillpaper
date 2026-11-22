pub mod preferences;
pub mod themes;

pub use preferences::{
    AccentSwatch, DefaultSaveFormat, FontChoice, GhostSuggestionMode, LineSpacing, Preferences,
    RendererBackend, SmartQuotePair, TextWidth, ThemeId,
};
pub use themes::{ThemePalette, generate_app_css, resolve_palette};
