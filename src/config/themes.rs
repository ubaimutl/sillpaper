use crate::config::{AccentSwatch, Preferences, ThemeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePalette {
    pub is_dark: bool,
    pub background: String,
    pub text: String,
    pub muted_text: String,
    pub surface: String,
    pub surface_alt: String,
    pub surface_hover: String,
    pub border: String,
    pub accent: String,
    pub caret: String,
    pub selection_bg: String,
    pub tooltip_bg: String,
    pub tooltip_fg: String,
    pub code_bg: String,
    pub quote_bar: String,
    pub search_bg: String,
    pub search_current_bg: String,
    pub unsaved_dot: String,
}

pub fn resolve_palette(prefs: &Preferences) -> ThemePalette {
    let mut palette = match prefs.theme {
        ThemeId::Light => ThemePalette {
            is_dark: false,
            background: "#FAFAFA".into(),
            text: "#3C3C3C".into(),
            muted_text: "#767676".into(),
            surface: "#EBEBEB".into(),
            surface_alt: "#DFDFDF".into(),
            surface_hover: "rgba(50, 50, 50, 0.08)".into(),
            border: "rgba(0, 0, 0, 0.10)".into(),
            accent: "#2ABAFF".into(),
            caret: "#2ABAFF".into(),
            selection_bg: "rgba(168, 209, 255, 0.55)".into(),
            tooltip_bg: "#181818".into(),
            tooltip_fg: "#F5F5F5".into(),
            code_bg: "rgba(0, 0, 0, 0.04)".into(),
            quote_bar: "rgba(60, 60, 60, 0.25)".into(),
            search_bg: "rgba(255, 224, 102, 0.55)".into(),
            search_current_bg: "rgba(255, 168, 52, 0.80)".into(),
            unsaved_dot: "#BF4E4E".into(),
        },
        ThemeId::Dark => ThemePalette {
            is_dark: true,
            background: "#242424".into(),
            text: "#E6E6E6".into(),
            muted_text: "#989898".into(),
            surface: "#1B1B1B".into(),
            surface_alt: "#282828".into(),
            surface_hover: "rgba(255, 255, 255, 0.08)".into(),
            border: "rgba(255, 255, 255, 0.09)".into(),
            accent: "#2ABAFF".into(),
            caret: "#2ABAFF".into(),
            selection_bg: "rgba(42, 110, 168, 0.55)".into(),
            tooltip_bg: "#0F0F0F".into(),
            tooltip_fg: "#F5F5F5".into(),
            code_bg: "#1A1A1A".into(),
            quote_bar: "rgba(230, 230, 230, 0.30)".into(),
            search_bg: "rgba(230, 190, 60, 0.38)".into(),
            search_current_bg: "rgba(255, 155, 45, 0.68)".into(),
            unsaved_dot: "#D96666".into(),
        },
        ThemeId::Black => ThemePalette {
            is_dark: true,
            background: "#000000".into(),
            text: "#E6E6E6".into(),
            muted_text: "#8C8C8C".into(),
            surface: "#141414".into(),
            surface_alt: "#202020".into(),
            surface_hover: "rgba(255, 255, 255, 0.10)".into(),
            border: "rgba(255, 255, 255, 0.12)".into(),
            accent: "#2ABAFF".into(),
            caret: "#2ABAFF".into(),
            selection_bg: "rgba(42, 110, 168, 0.62)".into(),
            tooltip_bg: "#161616".into(),
            tooltip_fg: "#F5F5F5".into(),
            code_bg: "rgba(255, 255, 255, 0.08)".into(),
            quote_bar: "rgba(230, 230, 230, 0.32)".into(),
            search_bg: "rgba(230, 190, 60, 0.38)".into(),
            search_current_bg: "rgba(255, 155, 45, 0.72)".into(),
            unsaved_dot: "#D96666".into(),
        },
        ThemeId::DarkBlue => ThemePalette {
            is_dark: true,
            background: "#242931".into(),
            text: "#E6E6E6".into(),
            muted_text: "#98A3B3".into(),
            surface: "#1A1F26".into(),
            surface_alt: "#242B34".into(),
            surface_hover: "rgba(255, 255, 255, 0.08)".into(),
            border: "rgba(255, 255, 255, 0.10)".into(),
            accent: "#2ABAFF".into(),
            caret: "#2ABAFF".into(),
            selection_bg: "rgba(56, 118, 172, 0.58)".into(),
            tooltip_bg: "#0E1217".into(),
            tooltip_fg: "#F5F5F5".into(),
            code_bg: "#1A1A1A".into(),
            quote_bar: "rgba(230, 230, 230, 0.28)".into(),
            search_bg: "rgba(230, 190, 60, 0.38)".into(),
            search_current_bg: "rgba(255, 155, 45, 0.68)".into(),
            unsaved_dot: "#D96666".into(),
        },
        ThemeId::Dune => ThemePalette {
            is_dark: false,
            background: "#FCF6E4".into(),
            text: "#353025".into(),
            muted_text: "#7B725D".into(),
            surface: "#EBE2CC".into(),
            surface_alt: "#DDD2B8".into(),
            surface_hover: "rgba(54, 50, 46, 0.09)".into(),
            border: "rgba(53, 48, 37, 0.12)".into(),
            accent: "#2B7192".into(),
            caret: "#2B7192".into(),
            selection_bg: "rgba(132, 122, 96, 0.30)".into(),
            tooltip_bg: "#2A261F".into(),
            tooltip_fg: "#FCF6E4".into(),
            code_bg: "rgba(53, 48, 37, 0.06)".into(),
            quote_bar: "rgba(53, 48, 37, 0.28)".into(),
            search_bg: "rgba(235, 195, 70, 0.45)".into(),
            search_current_bg: "rgba(220, 130, 40, 0.68)".into(),
            unsaved_dot: "#B5453C".into(),
        },
        ThemeId::OldSchool => ThemePalette {
            is_dark: true,
            background: "#0000AA".into(),
            text: "#E6E6E6".into(),
            muted_text: "#A7B7E8".into(),
            surface: "#000080".into(),
            surface_alt: "#0C0C96".into(),
            surface_hover: "rgba(255, 255, 255, 0.12)".into(),
            border: "rgba(255, 255, 255, 0.20)".into(),
            accent: "#5CE1FF".into(),
            caret: "#FFFFFF".into(),
            selection_bg: "rgba(175, 168, 175, 0.65)".into(),
            tooltip_bg: "#0A0A0A".into(),
            tooltip_fg: "#F5F5F5".into(),
            code_bg: "rgba(0, 0, 0, 0.25)".into(),
            quote_bar: "rgba(230, 230, 230, 0.45)".into(),
            search_bg: "rgba(255, 230, 80, 0.45)".into(),
            search_current_bg: "rgba(255, 160, 40, 0.80)".into(),
            unsaved_dot: "#FF7070".into(),
        },
        ThemeId::Custom => derive_custom_palette(&prefs.custom_theme_bg),
    };

    palette.text = match prefs.text_color {
        AccentSwatch::Default => palette.text,
        AccentSwatch::Blue => match (prefs.theme, palette.is_dark) {
            (ThemeId::Dune, _) => "#124961".into(),
            (ThemeId::OldSchool, _) => "#A0D0E4".into(),
            (_, true) => "#4AB4E0".into(),
            (_, false) => "#2B7FA8".into(),
        },
        AccentSwatch::Pink => match (prefs.theme, palette.is_dark) {
            (ThemeId::Dune, _) => "#B73A64".into(),
            (ThemeId::OldSchool, _) => "#EAE417".into(),
            (_, true) => "#EF6E99".into(),
            (_, false) => "#C74370".into(),
        },
        AccentSwatch::Green => match (prefs.theme, palette.is_dark) {
            (ThemeId::Dune, _) => "#364C37".into(),
            (ThemeId::OldSchool, _) => "#57E389".into(),
            (_, true) => "#5FA864".into(),
            (_, false) => "#388E3C".into(),
        },
        AccentSwatch::Orange => match (prefs.theme, palette.is_dark) {
            (ThemeId::Dune, _) => "#7F5934".into(),
            (ThemeId::OldSchool, _) => "#ECBA6D".into(),
            (_, true) => "#DA7A5B".into(),
            (_, false) => "#C95532".into(),
        },
        AccentSwatch::Custom => sanitize_hex(&prefs.custom_text_color, &palette.text),
    };

    palette.caret = match prefs.caret_color {
        AccentSwatch::Default => palette.text.clone(),
        AccentSwatch::Blue => "#2ABAFF".into(),
        AccentSwatch::Pink => "#EC407A".into(),
        AccentSwatch::Green => "#4CAF50".into(),
        AccentSwatch::Orange => "#FF5722".into(),
        AccentSwatch::Custom => palette.accent.clone(),
    };

    palette
}

pub fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let trimmed = hex.trim().trim_start_matches('#');
    if trimmed.len() == 6 {
        let r = u8::from_str_radix(&trimmed[0..2], 16).ok()?;
        let g = u8::from_str_radix(&trimmed[2..4], 16).ok()?;
        let b = u8::from_str_radix(&trimmed[4..6], 16).ok()?;
        Some((r, g, b))
    } else if trimmed.len() == 3 {
        let r = u8::from_str_radix(&trimmed[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&trimmed[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&trimmed[2..3], 16).ok()? * 17;
        Some((r, g, b))
    } else {
        None
    }
}

fn sanitize_hex(hex: &str, fallback: &str) -> String {
    if let Some((r, g, b)) = parse_hex_rgb(hex) {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        fallback.to_string()
    }
}

fn derive_custom_palette(bg_hex: &str) -> ThemePalette {
    let (r, g, b) = parse_hex_rgb(bg_hex).unwrap_or((250, 253, 255));
    let luminance =
        0.2126 * (r as f64 / 255.0) + 0.7152 * (g as f64 / 255.0) + 0.0722 * (b as f64 / 255.0);
    let is_dark = luminance < 0.45;
    let delta: i16 = if is_dark { 18 } else { -18 };
    let delta2: i16 = if is_dark { 28 } else { -28 };
    let shift = |d: i16| {
        format!(
            "#{:02X}{:02X}{:02X}",
            (r as i16 + d).clamp(0, 255) as u8,
            (g as i16 + d).clamp(0, 255) as u8,
            (b as i16 + d).clamp(0, 255) as u8,
        )
    };
    let bg = format!("#{r:02X}{g:02X}{b:02X}");

    ThemePalette {
        is_dark,
        background: bg,
        text: if is_dark {
            "#E6E6E6".into()
        } else {
            "#3C3C3C".into()
        },
        muted_text: if is_dark {
            "#9E9E9E".into()
        } else {
            "#757575".into()
        },
        surface: shift(delta),
        surface_alt: shift(delta2),
        surface_hover: if is_dark {
            "rgba(255,255,255,0.09)".into()
        } else {
            "rgba(0,0,0,0.07)".into()
        },
        border: if is_dark {
            "rgba(255,255,255,0.12)".into()
        } else {
            "rgba(0,0,0,0.10)".into()
        },
        accent: "#2ABAFF".into(),
        caret: "#2ABAFF".into(),
        selection_bg: if is_dark {
            "rgba(42, 110, 160, 0.58)".into()
        } else {
            "rgba(168, 209, 255, 0.55)".into()
        },
        tooltip_bg: "#121212".into(),
        tooltip_fg: "#F5F5F5".into(),
        code_bg: if is_dark {
            "rgba(255,255,255,0.07)".into()
        } else {
            "rgba(0,0,0,0.05)".into()
        },
        quote_bar: if is_dark {
            "rgba(230,230,230,0.28)".into()
        } else {
            "rgba(60,60,60,0.25)".into()
        },
        search_bg: "rgba(245, 205, 70, 0.48)".into(),
        search_current_bg: "rgba(245, 145, 45, 0.78)".into(),
        unsaved_dot: if is_dark {
            "#D96666".into()
        } else {
            "#BF4E4E".into()
        },
    }
}

pub fn generate_app_css(prefs: &Preferences) -> String {
    let p = resolve_palette(prefs);
    let font_pt = prefs.font_size_pt();
    let font_family = prefs.font.pango_family().replace('"', "");
    let caret_color = if prefs.hide_caret_while_typing {
        "transparent".to_string()
    } else {
        p.caret.clone()
    };
    let control_bg = if p.is_dark {
        "rgba(255, 255, 255, 0.06)"
    } else {
        "rgba(0, 0, 0, 0.055)"
    };
    let control_bg_hover = if p.is_dark {
        "rgba(255, 255, 255, 0.13)"
    } else {
        "rgba(0, 0, 0, 0.10)"
    };
    let chrome_fg = if p.is_dark { "#F1F2F4" } else { "#24262B" };
    let chrome_muted = if p.is_dark { "#B5BBC4" } else { "#565C66" };
    let chrome_lift = if p.is_dark { "#444A55" } else { "#FFFFFF" };
    let chrome_rim = if p.is_dark {
        "rgba(255, 255, 255, 0.11)"
    } else {
        "rgba(25, 32, 46, 0.11)"
    };
    let chrome_accent = if p.is_dark { "#91C2FF" } else { "#1B5DC1" };
    let chrome_action = if p.is_dark { "#2863B3" } else { "#2164C9" };
    let (toolbar_bg, toolbar_fg) = if p.is_dark {
        ("#FFFFFF", "#000000")
    } else {
        ("#000000", "#FFFFFF")
    };

    format!(
        r#"
window.sillpaper-window,
window.sillpaper-window .sillpaper-push-drawer,
window.sillpaper-window tabview,
window.sillpaper-window .sillpaper-canvas-host {{
    background-color: {bg};
    background-image: none;
    color: {text};
}}


window.sillpaper-window .sillpaper-topbar,
window.sillpaper-window .sillpaper-sidebar-shell,
window.sillpaper-window .sillpaper-drawer,
window.sillpaper-window .sillpaper-settings-panel,
window.sillpaper-window .sillpaper-counter-pill,
window.sillpaper-window .sillpaper-shortcut-overlay,
window.sillpaper-window .sillpaper-search-bar,
window.sillpaper-window .sillpaper-toc-popover,
window.sillpaper-window .sillpaper-toolbar-bar,
window.sillpaper-window .sillpaper-toolbar-caption,
window.sillpaper-window .sillpaper-toast,
window.sillpaper-window .sillpaper-link-pill,
window.sillpaper-window .sillpaper-ext-card,
window.sillpaper-window .sillpaper-ext-preview,
window.sillpaper-window .sillpaper-ext-hint,
window.sillpaper-window popover.background {{
    font-family: "Roboto", "Inter", "Adwaita Sans", "Noto Sans", sans-serif;
}}


window.sillpaper-window .sillpaper-topbar-handle {{
    background-color: {bg};
    background-image: none;
    border: none;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-topbar {{
    background: transparent;
    color: {chrome_fg};
    border: none;
    box-shadow: none;
    min-height: 38px;
    padding: 0 10px;
    transition: opacity 220ms ease-out;
}}

window.sillpaper-window .sillpaper-topbar.chrome-hidden {{
    opacity: 0;
}}

window.sillpaper-window .sillpaper-topbar .sillpaper-tabbar-scroll,
window.sillpaper-window .sillpaper-topbar .sillpaper-tabbar-scroll > viewport {{
    background: transparent;
    border: none;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-topbar .sillpaper-tabbar {{
    background: transparent;
    padding: 1px 2px;
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-tab {{
    background: transparent;
    color: {chrome_muted};
    border: none;
    border-radius: 99px;
    min-height: 28px;
    min-width: 24px;
    padding: 0 10px;
    box-shadow: none;
    transition: background-color 140ms ease, color 140ms ease;
}}

window.sillpaper-window .sillpaper-topbar .sillpaper-tab-title {{
    color: inherit;
    font-size: 13px;
    font-weight: 500;
}}

window.sillpaper-window .sillpaper-topbar .sillpaper-tab-wrap:hover button.sillpaper-tab {{
    background-color: {control_bg};
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-tab.tab-active,
window.sillpaper-window .sillpaper-topbar .sillpaper-tab-wrap:hover button.sillpaper-tab.tab-active {{
    background-color: {control_bg_hover};
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-tab.tab-active .sillpaper-tab-title {{
    font-weight: 700;
}}

window.sillpaper-window .sillpaper-topbar .sillpaper-tab-dot {{
    min-width: 6px;
    min-height: 6px;
    border-radius: 99px;
    background-color: {chrome_accent};
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-tab-close {{
    background: transparent;
    color: {chrome_muted};
    border: none;
    border-radius: 6px;
    min-width: 18px;
    min-height: 18px;
    padding: 0;
    margin: 0;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-tab-close:hover {{
    background-color: {control_bg_hover};
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-tab-close image {{
    -gtk-icon-size: 12px;
    color: inherit;
}}


window.sillpaper-window button.sillpaper-edge-drawer-trigger,
window.sillpaper-window button.sillpaper-edge-drawer-trigger:hover,
window.sillpaper-window button.sillpaper-edge-drawer-trigger:active,
window.sillpaper-window button.sillpaper-edge-drawer-trigger:focus,
window.sillpaper-window button.sillpaper-edge-drawer-trigger:focus-visible {{
    background-color: transparent;
    background-image: none;
    border: none;
    border-radius: 0;
    padding: 0;
    margin: 0;
    min-width: 24px;
    min-height: 0;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window button.sillpaper-edge-drawer-trigger image {{
    color: {text};
    opacity: 0;
    transition: opacity 160ms ease;
}}

window.sillpaper-window button.sillpaper-edge-drawer-trigger:hover image {{
    opacity: 0.78;
}}

window.sillpaper-window .sillpaper-topbar windowcontrols {{
    margin: 0 4px 0 0;
}}

window.sillpaper-window .sillpaper-topbar windowcontrols button {{
    background-color: transparent;
    background-image: none;
    color: {chrome_muted};
    border: none;
    box-shadow: none;
    outline: none;
    min-width: 26px;
    min-height: 26px;
    padding: 0;
    border-radius: 8px;
    margin: 4px 1px;
}}

window.sillpaper-window .sillpaper-topbar windowcontrols button image {{
    color: {chrome_muted};
    opacity: 0.85;
}}

window.sillpaper-window .sillpaper-topbar windowcontrols button:hover {{
    background-color: {control_bg_hover};
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-topbar windowcontrols button:hover image {{
    color: {chrome_fg};
    opacity: 1.0;
}}

window.sillpaper-window .sillpaper-topbar windowcontrols button.close:hover {{
    background-color: rgba(217, 72, 72, 0.22);
}}

window.sillpaper-window .sillpaper-topbar windowcontrols button.close:hover image {{
    color: #F07070;
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-icon-btn {{
    min-width: 28px;
    min-height: 28px;
    padding: 0;
    margin: 4px 2px;
    border-radius: 9px;
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-icon-btn image {{
    color: {chrome_muted};
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-icon-btn:hover {{
    background-color: {control_bg_hover};
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-icon-btn:hover image {{
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-topbar button.sillpaper-menu-open image {{
    transform: scaleX(-1);
}}


window.sillpaper-window button.sillpaper-icon-btn,
window.sillpaper-window button.sillpaper-icon-btn:focus,
window.sillpaper-window button.sillpaper-icon-btn:focus-visible,
window.sillpaper-window menubutton.sillpaper-icon-btn > button,
window.sillpaper-window menubutton.sillpaper-icon-btn > button:focus,
window.sillpaper-window menubutton.sillpaper-icon-btn > button:focus-visible {{
    background-color: transparent;
    background-image: none;
    color: {muted};
    border: none;
    border-radius: 6px;
    min-width: 28px;
    min-height: 28px;
    padding: 4px;
    box-shadow: none;
    outline: none;
    outline-width: 0;
    transition: opacity 220ms ease-out, background-color 140ms ease, color 140ms ease;
}}

window.sillpaper-window button.sillpaper-icon-btn image,
window.sillpaper-window button.sillpaper-icon-btn label,
window.sillpaper-window menubutton.sillpaper-icon-btn > button image,
window.sillpaper-window menubutton.sillpaper-icon-btn > button label {{
    color: {muted};
}}

window.sillpaper-window button.sillpaper-icon-btn:hover,
window.sillpaper-window menubutton.sillpaper-icon-btn > button:hover {{
    background-color: {hover};
    color: {text};
}}

window.sillpaper-window button.sillpaper-icon-btn:hover image,
window.sillpaper-window button.sillpaper-icon-btn:hover label,
window.sillpaper-window menubutton.sillpaper-icon-btn > button:hover image,
window.sillpaper-window menubutton.sillpaper-icon-btn > button:hover label {{
    color: {text};
}}

window.sillpaper-window .sillpaper-icon-btn.chrome-hidden {{
    opacity: 0;
}}



window.sillpaper-window scrolledwindow.sillpaper-editor-scroll,
window.sillpaper-window scrolledwindow.sillpaper-editor-scroll > viewport,
window.sillpaper-window scrolledwindow.sillpaper-editor-scroll clamp-scrollable {{
    background-color: {bg};
    background-image: none;
    border: none;
}}

window.sillpaper-window textview.sillpaper-editor {{
    background-color: {bg};
    background-image: none;
    color: {text};
    font-family: "{font_family}";
    font-size: {font_pt:.2}pt;
    caret-color: {caret};
}}

window.sillpaper-window textview.sillpaper-editor.hide-caret-active {{
    caret-color: transparent;
}}

window.sillpaper-window textview.sillpaper-editor text {{
    background-color: {bg};
    background-image: none;
    color: {text};
}}


window.sillpaper-window textview.sillpaper-editor text selection,
window.sillpaper-window textview.sillpaper-editor text selection:focus-within,
window.sillpaper-window textview.sillpaper-editor text selection:backdrop {{
    background-color: transparent;
    background-image: none;
    color: {text};
}}



window.sillpaper-window .sillpaper-sidebar-shell {{
    background-color: {bg};
    background-image: none;
    color: {chrome_fg};
    border-right: 1px solid {chrome_rim};
}}

window.sillpaper-window .sillpaper-drawer {{
    background: transparent;
    color: {chrome_fg};
    padding: 12px 10px 14px 10px;
}}

window.sillpaper-window .sillpaper-drawer label {{
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-drawer label.sillpaper-brand-title {{
    color: {chrome_fg};
    font-size: 12.5px;
    font-weight: 700;
}}

window.sillpaper-window .sillpaper-drawer label.sillpaper-brand-ver {{
    color: {chrome_muted};
    font-size: 11.5px;
}}

window.sillpaper-window .sillpaper-drawer label.sillpaper-shortcut-hint {{
    color: {chrome_muted};
    font-size: 11px;
}}

window.sillpaper-window .sillpaper-drawer separator {{
    background-color: {chrome_rim};
    margin: 6px 10px;
}}

window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item,
window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item:focus,
window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item:focus-visible {{
    background-color: transparent;
    background-image: none;
    color: {chrome_fg};
    border: none;
    border-radius: 10px;
    padding: 10px 14px;
    box-shadow: none;
    outline: none;
    outline-width: 0;
}}

window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item image {{
    color: {chrome_muted};
    margin-right: 4px;
}}

window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item label {{
    color: {chrome_fg};
    font-size: 14px;
}}

window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item label.sillpaper-shortcut-hint {{
    color: {chrome_muted};
    font-size: 11px;
}}

window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item:hover {{
    background-color: {control_bg_hover};
}}

window.sillpaper-window .sillpaper-drawer button.sillpaper-drawer-item:hover image {{
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-target-btn {{
    background-color: {control_bg};
    background-image: none;
    color: {chrome_fg};
    border: 1px solid {chrome_rim};
    border-radius: 9px;
    padding: 6px 12px;
    font-size: 12.5px;
    font-weight: 600;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window .sillpaper-target-btn:hover {{
    background-color: {control_bg_hover};
}}

window.sillpaper-window .sillpaper-drawer-stats {{
    color: {chrome_muted};
    font-size: 13px;
}}


window.sillpaper-window .sillpaper-settings-panel,
window.sillpaper-window .sillpaper-settings-scroll,
window.sillpaper-window .sillpaper-settings-scroll > viewport,
window.sillpaper-window .sillpaper-settings-section {{
    background: transparent;
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-settings-panel label {{
    color: {chrome_fg};
    font-size: 14px;
}}

window.sillpaper-window .sillpaper-settings-header {{
    padding: 9px 12px;
    border-bottom: 1px solid {chrome_rim};
}}

window.sillpaper-window .sillpaper-settings-panel label.sillpaper-settings-title {{
    font-size: 16px;
    font-weight: 700;
}}

window.sillpaper-window .sillpaper-settings-panel button.sillpaper-settings-nav {{
    background: transparent;
    color: {chrome_muted};
    border: none;
    border-radius: 99px;
    min-width: 28px;
    min-height: 28px;
    padding: 0;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-settings-panel button.sillpaper-settings-nav:hover {{
    background-color: {control_bg_hover};
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-settings-category {{
    border-bottom: 1px solid {chrome_rim};
}}

window.sillpaper-window .sillpaper-settings-category > box > title {{
    padding: 10px 18px;
    min-height: 20px;
}}

window.sillpaper-window .sillpaper-settings-category > box > title:hover {{
    background-color: {control_bg_hover};
}}

window.sillpaper-window .sillpaper-settings-category > box > title > label {{
    font-weight: 600;
}}

window.sillpaper-window .sillpaper-settings-section {{
    padding: 12px 18px 20px;
}}

window.sillpaper-window .sillpaper-settings-panel label.sillpaper-sub-label {{
    color: {chrome_muted};
    font-size: 13px;
    font-weight: 600;
    margin-top: 4px;
}}

window.sillpaper-window .sillpaper-settings-panel label.sillpaper-hint-note {{
    color: {chrome_muted};
    font-size: 12px;
    margin-top: 4px;
}}

window.sillpaper-window .sillpaper-settings-panel label.sillpaper-setting-shortcut {{
    color: {chrome_muted};
    font-size: 11px;
    opacity: 0.85;
}}


window.sillpaper-window .sillpaper-seg-group {{
    background-color: {control_bg};
    border: 1px solid {chrome_rim};
    border-radius: 10px;
    padding: 3px;
}}

window.sillpaper-window .sillpaper-seg-group button,
window.sillpaper-window .sillpaper-seg-group button:focus,
window.sillpaper-window .sillpaper-seg-group button:focus-visible {{
    background-color: transparent;
    background-image: none;
    color: {chrome_muted};
    border: none;
    border-radius: 7px;
    min-height: 28px;
    min-width: 24px;
    padding: 2px 6px;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window .sillpaper-theme-selector button {{
    padding: 2px;
}}

window.sillpaper-window .sillpaper-seg-group.sillpaper-theme-selector button label {{
    font-size: 16px;
}}

window.sillpaper-window .sillpaper-seg-group button label,
window.sillpaper-window .sillpaper-seg-group button image {{
    color: {chrome_muted};
    font-size: 13px;
    font-weight: 600;
}}

window.sillpaper-window .sillpaper-seg-group button:hover {{
    background-color: {control_bg_hover};
}}

window.sillpaper-window .sillpaper-seg-group button:hover label,
window.sillpaper-window .sillpaper-seg-group button:hover image {{
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-seg-group button.seg-active {{
    background-color: {chrome_lift};
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.16);
}}

window.sillpaper-window .sillpaper-seg-group button.seg-active label,
window.sillpaper-window .sillpaper-seg-group button.seg-active image {{
    color: {chrome_fg};
}}


window.sillpaper-window button.sillpaper-swatch {{
    background-image: none;
    min-width: 30px;
    min-height: 30px;
    border-radius: 99px;
    padding: 0;
    border: 1px solid {chrome_rim};
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window button.sillpaper-swatch label {{
    font-size: 14px;
    font-weight: 800;
}}

window.sillpaper-window button.swatch-default {{
    background-color: {text};
}}
window.sillpaper-window button.swatch-default label {{
    color: {bg};
}}

window.sillpaper-window button.swatch-blue {{
    background-color: #2ABAFF;
}}
window.sillpaper-window button.swatch-blue label {{
    color: #FFFFFF;
}}

window.sillpaper-window button.swatch-pink {{
    background-color: #EC407A;
}}
window.sillpaper-window button.swatch-pink label {{
    color: #FFFFFF;
}}

window.sillpaper-window button.swatch-green {{
    background-color: #4CAF50;
}}
window.sillpaper-window button.swatch-green label {{
    color: #FFFFFF;
}}

window.sillpaper-window button.swatch-orange {{
    background-color: #FF6E40;
}}
window.sillpaper-window button.swatch-orange label {{
    color: #FFFFFF;
}}

window.sillpaper-window button.sillpaper-swatch.swatch-active {{
    outline: 2px solid {chrome_accent};
    outline-offset: 1px;
}}


window.sillpaper-window .sillpaper-settings-panel checkbutton {{
    margin: 3px 0;
    outline: none;
}}

window.sillpaper-window .sillpaper-settings-panel checkbutton label {{
    color: {chrome_fg};
    font-size: 14px;
    margin-left: 6px;
}}

window.sillpaper-window .sillpaper-settings-panel checkbutton > check {{
    background-color: {control_bg};
    background-image: none;
    border: 1px solid {chrome_rim};
    border-radius: 4px;
    min-width: 19px;
    min-height: 19px;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-settings-panel checkbutton > check:checked {{
    background-color: {chrome_action};
    border-color: {chrome_action};
    color: #FFFFFF;
}}

window.sillpaper-window .sillpaper-settings-panel dropdown > button,
window.sillpaper-window .sillpaper-settings-panel entry,
window.sillpaper-window .sillpaper-settings-panel passwordentry,
window.sillpaper-window .sillpaper-settings-panel spinbutton {{
    background-color: {control_bg};
    background-image: none;
    color: {chrome_fg};
    border: 1px solid {chrome_rim};
    border-radius: 10px;
    min-height: 36px;
    font-size: 13.5px;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window .sillpaper-settings-panel dropdown > button {{
    padding: 0 10px;
}}

window.sillpaper-window .sillpaper-settings-panel entry,
window.sillpaper-window .sillpaper-settings-panel passwordentry {{
    padding: 0 10px;
}}

window.sillpaper-window .sillpaper-settings-panel spinbutton {{
    padding: 0 2px 0 10px;
}}

window.sillpaper-window .sillpaper-settings-panel dropdown > button label,
window.sillpaper-window .sillpaper-settings-panel dropdown > button image {{
    color: {chrome_fg};
}}


window.sillpaper-window .sillpaper-settings-panel entry > text,
window.sillpaper-window .sillpaper-settings-panel passwordentry > text,
window.sillpaper-window .sillpaper-settings-panel spinbutton > text {{
    color: {chrome_fg};
    font-size: 15px;
    padding: 4px 0;
    margin: 0;
    min-height: 0;
}}

window.sillpaper-window .sillpaper-settings-panel spinbutton > text {{
    font-weight: 500;
}}

window.sillpaper-window .sillpaper-settings-panel spinbutton > button {{
    background-color: transparent;
    background-image: none;
    color: {chrome_muted};
    border: none;
    border-radius: 5px;
    min-width: 28px;
    min-height: 28px;
    margin: 2px;
    padding: 0;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window .sillpaper-settings-panel spinbutton > button:hover {{
    background-color: {control_bg_hover};
    color: {chrome_fg};
}}

window.sillpaper-window .sillpaper-settings-panel scale trough {{
    background-color: {control_bg_hover};
    min-height: 4px;
    border-radius: 2px;
}}

window.sillpaper-window .sillpaper-settings-panel scale highlight {{
    background-color: {chrome_accent};
    border-radius: 2px;
}}

window.sillpaper-window .sillpaper-settings-panel scale slider {{
    background-color: {chrome_lift};
    border: 1px solid {chrome_rim};
    border-radius: 99px;
    min-width: 14px;
    min-height: 14px;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.35);
}}



window.sillpaper-window .sillpaper-toolbar-bar {{
    background: transparent;
    border: none;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-toolbar-caption {{
    background-color: {toolbar_bg};
    background-image: none;
    color: {toolbar_fg};
    border: 1px solid mix({toolbar_bg}, {toolbar_fg}, 0.18);
    border-radius: 9px;
    padding: 4px 10px;
    box-shadow: none;
    opacity: 0;
    transition: opacity 120ms ease;
}}

window.sillpaper-window .sillpaper-toolbar-caption.caption-visible {{
    opacity: 1;
}}

window.sillpaper-window .sillpaper-toolbar-caption label {{
    color: {toolbar_fg};
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.1px;
}}

window.sillpaper-window .sillpaper-toolbar-bar button,
window.sillpaper-window .sillpaper-toolbar-bar button:focus,
window.sillpaper-window .sillpaper-toolbar-bar button:focus-visible {{
    background-color: transparent;
    background-image: none;
    color: {toolbar_fg};
    border: none;
    border-radius: 12px;
    min-width: 28px;
    min-height: 30px;
    padding: 0 3px;
    box-shadow: none;
    outline: none;
    transition: background-color 120ms ease, color 120ms ease;
}}

window.sillpaper-window .sillpaper-toolbar-bar button label,
window.sillpaper-window .sillpaper-toolbar-bar button image {{
    color: {toolbar_fg};
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-style-trigger {{
    padding: 0 9px 0 11px;
    min-height: 30px;
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-style-trigger label {{
    font-size: 13px;
    font-weight: 600;
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-style-trigger image.ftb-chevron {{
    color: mix({toolbar_fg}, {toolbar_bg}, 0.28);
}}

window.sillpaper-window .sillpaper-toolbar-bar button:hover {{
    background-color: mix({toolbar_bg}, {toolbar_fg}, 0.10);
}}

window.sillpaper-window .sillpaper-toolbar-bar button.active-format {{
    background-color: mix({toolbar_bg}, {toolbar_fg}, 0.24);
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-group,
window.sillpaper-window .sillpaper-toolbar-bar button.ftb-style-selector {{
    background-color: {toolbar_bg};
    background-image: none;
    color: {toolbar_fg};
    border: 1px solid mix({toolbar_bg}, {toolbar_fg}, 0.18);
    border-radius: 99px;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-group {{
    padding: 2px 4px;
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-style-selector {{
    min-height: 34px;
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-style-selector:hover {{
    background-color: mix({toolbar_bg}, {toolbar_fg}, 0.10);
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-style-selector.active-format {{
    background-color: mix({toolbar_bg}, {toolbar_fg}, 0.24);
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-extension label {{
    font-size: 17px;
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-primary-cap {{
    background-color: {toolbar_fg};
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-primary-cap image,
window.sillpaper-window .sillpaper-toolbar-bar button.ftb-primary-cap label {{
    color: {toolbar_bg};
}}

window.sillpaper-window .sillpaper-toolbar-bar button.ftb-primary-cap:hover {{
    background-color: mix({toolbar_fg}, {toolbar_bg}, 0.16);
}}

window.sillpaper-window .sillpaper-toolbar-bar button:disabled {{
    opacity: 0.30;
}}

window.sillpaper-window .sillpaper-toolbar-bar separator {{
    background-color: mix({toolbar_bg}, {toolbar_fg}, 0.18);
    min-width: 1px;
    margin: 8px 2px;
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-heading label {{
    font-weight: 800;
    font-size: 13px;
    letter-spacing: -0.3px;
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-bold label {{
    font-weight: 900;
    font-size: 14.5px;
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-italic label {{
    font-family: Serif;
    font-style: italic;
    font-weight: 700;
    font-size: 16.5px;
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-underline label {{
    text-decoration-line: underline;
    font-weight: 700;
    font-size: 13.5px;
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-strike label {{
    text-decoration-line: line-through;
    font-weight: 700;
    font-size: 13.5px;
}}

window.sillpaper-window .sillpaper-toolbar-bar .ftb-quote label {{
    font-family: Serif;
    font-weight: 900;
    font-size: 19px;
}}

window.sillpaper-window .sillpaper-toolbar-bar entry {{
    background-color: mix({toolbar_bg}, {toolbar_fg}, 0.10);
    background-image: none;
    color: {toolbar_fg};
    border: 1px solid mix({toolbar_bg}, {toolbar_fg}, 0.18);
    border-radius: 11px;
    min-height: 32px;
    padding: 0 12px;
    box-shadow: none;
}}

window.sillpaper-window .sillpaper-toolbar-bar entry text {{
    color: {toolbar_fg};
    caret-color: {toolbar_fg};
    padding: 0;
    margin: 0;
}}

window.sillpaper-window .sillpaper-toolbar-bar entry text placeholder {{
    color: mix({toolbar_fg}, {toolbar_bg}, 0.36);
}}

window.sillpaper-window .sillpaper-toolbar-bar entry text selection {{
    color: {toolbar_fg};
    background-color: mix({toolbar_bg}, {toolbar_fg}, 0.28);
}}



window.sillpaper-window button.sillpaper-counter-pill,
window.sillpaper-window button.sillpaper-counter-pill:focus,
window.sillpaper-window button.sillpaper-counter-pill:focus-visible {{
    background-color: {surface};
    background-image: none;
    color: {muted};
    border: none;
    border-radius: 6px 6px 0 0;
    min-height: 0;
    padding: 4px 14px 5px 14px;
    box-shadow: none;
    outline: none;
    transition: opacity 220ms ease-out, background-color 140ms ease;
}}

window.sillpaper-window button.sillpaper-counter-pill:hover {{
    background-color: {surface_alt};
}}

window.sillpaper-window button.sillpaper-counter-pill label.counter-main {{
    color: {muted};
    font-size: 12px;
    letter-spacing: 0.15px;
    padding: 0;
    margin: 0;
}}

window.sillpaper-window button.sillpaper-counter-pill label.counter-sub {{
    color: {muted};
    font-size: 10.5px;
    opacity: 0.80;
    padding: 0;
    margin: 0;
}}

window.sillpaper-window button.sillpaper-counter-pill.goal-reached label.counter-main {{
    color: {accent};
}}

window.sillpaper-window .sillpaper-toast {{
    background-color: {tip_bg};
    color: {tip_fg};
    border-radius: 20px;
    padding: 8px 18px;
    font-weight: 600;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.30);
}}



window.sillpaper-window .sillpaper-search-bar {{
    background-color: {surface};
    background-image: none;
    color: {text};
    border: 1px solid {border};
    border-radius: 10px;
    padding: 6px 8px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.24);
}}

window.sillpaper-window .sillpaper-search-bar entry {{
    background-color: {control_bg};
    background-image: none;
    color: {text};
    border: 1px solid {border};
    border-radius: 6px;
    min-height: 32px;
    padding: 0 8px;
}}

window.sillpaper-window .sillpaper-search-bar entry text {{
    color: {text};
    padding: 0;
    margin: 0;
}}

window.sillpaper-window .sillpaper-search-bar label {{
    color: {muted};
}}

window.sillpaper-window popover.sillpaper-toc-popover > contents,
window.sillpaper-window popover.sillpaper-toc-popover > arrow {{
    background-color: {surface};
    background-image: none;
    color: {text};
    border: 1px solid {border};
}}

window.sillpaper-window popover.sillpaper-toc-popover label {{
    color: {text};
}}

window.sillpaper-window popover.sillpaper-toc-popover button.sillpaper-toc-row {{
    background-color: transparent;
    background-image: none;
    color: {text};
    border: none;
    border-radius: 6px;
    padding: 6px 10px;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window popover.sillpaper-toc-popover button.sillpaper-toc-row:hover {{
    background-color: {hover};
}}

window.sillpaper-window popover.sillpaper-toc-popover button.sillpaper-toc-row.toc-active label {{
    color: {accent};
    font-weight: 600;
}}

window.sillpaper-window button.sillpaper-link-pill {{
    background-color: {tip_bg};
    background-image: none;
    color: {tip_fg};
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 12px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.28);
}}

window.sillpaper-window button.sillpaper-link-pill label {{
    color: {tip_fg};
}}

window.sillpaper-window button.sillpaper-comment-badge {{
    background-color: {surface};
    background-image: none;
    color: {accent};
    border: 1px solid {border};
    border-radius: 5px;
    padding: 0 6px;
    min-height: 20px;
    min-width: 22px;
    font-size: 11px;
}}

window.sillpaper-window .sillpaper-hr-line {{
    background-color: {border};
    min-height: 1px;
    margin: 12px 0;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay {{
    background-color: {tip_bg};
    background-image: none;
    color: {tip_fg};
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 5px;
    padding: 18px 26px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.65);
}}

window.sillpaper-window box.sillpaper-shortcut-overlay image.sillpaper-shortcut-icon {{
    color: #F2F2F2;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay label.sillpaper-shortcut-glyph {{
    color: #F2F2F2;
    font-size: 14px;
    font-weight: 700;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay label.sc-heading {{
    font-weight: 800;
    font-size: 13px;
    letter-spacing: -0.3px;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay label.sc-bold {{
    font-weight: 900;
    font-size: 14.5px;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay label.sc-italic {{
    font-family: Serif;
    font-style: italic;
    font-weight: 700;
    font-size: 15.5px;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay label.sc-underline {{
    text-decoration-line: underline;
    font-weight: 700;
    font-size: 13.5px;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay label.sc-strike {{
    text-decoration-line: line-through;
    font-weight: 700;
    font-size: 13.5px;
}}

window.sillpaper-window box.sillpaper-shortcut-overlay label.sillpaper-shortcut-keys {{
    color: rgba(255, 255, 255, 0.64);
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 1.6px;
}}

window.sillpaper-window popover.background > contents,
window.sillpaper-window popover.background > arrow {{
    background-color: {surface};
    background-image: none;
    color: {text};
    border: 1px solid {border};
}}

window.sillpaper-window popover.background label,
window.sillpaper-window popover.background modelbutton {{
    color: {text};
}}

window.sillpaper-window popover.background modelbutton:hover,
window.sillpaper-window popover.background listview row:hover {{
    background-color: {hover};
}}


window.sillpaper-window popover.background searchentry,
window.sillpaper-window popover.background entry {{
    background-color: {control_bg};
    background-image: none;
    color: {text};
    border: 1px solid {border};
    border-radius: 6px;
    min-height: 34px;
    padding: 0 8px;
    margin: 4px;
    font-size: 13.5px;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window popover.background searchentry > text,
window.sillpaper-window popover.background entry > text {{
    color: {text};
    padding: 0;
    margin: 0;
    min-height: 32px;
}}

window.sillpaper-window popover.background searchentry image,
window.sillpaper-window popover.background entry image {{
    color: {muted};
}}



window.sillpaper-window .sillpaper-ext-setting-card {{
    background-color: {control_bg};
    border: 1px solid {border};
    border-radius: 8px;
    padding: 10px 12px;
}}

window.sillpaper-window .sillpaper-ext-card,
window.sillpaper-window .sillpaper-ext-preview,
window.sillpaper-window .sillpaper-ext-hint {{
    background-color: {surface};
    background-image: none;
    color: {text};
    border: 1px solid {border};
    border-radius: 12px;
    padding: 12px 14px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.30);
}}

window.sillpaper-window .sillpaper-ext-hint label.ext-hint-title {{
    color: {text};
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.2px;
}}

window.sillpaper-window .sillpaper-ext-hint label.ext-hint-kbd {{
    background-color: {control_bg};
    color: {accent};
    border: 1px solid {border};
    border-radius: 5px;
    padding: 2px 6px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.3px;
}}

window.sillpaper-window .sillpaper-ext-hint label.ext-hint-desc {{
    color: {text};
    font-size: 12.5px;
}}

window.sillpaper-window .sillpaper-ext-hint label.ext-hint-foot {{
    color: {muted};
    font-size: 11.5px;
}}

window.sillpaper-window .sillpaper-ext-card label.ext-card-msg {{
    color: {muted};
    font-size: 12.5px;
}}

window.sillpaper-window .sillpaper-ext-card label.ext-card-fix {{
    color: {text};
    font-size: 13.5px;
    font-weight: 600;
}}

window.sillpaper-window .sillpaper-ext-preview label.ext-preview-title {{
    color: {muted};
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.4px;
}}

window.sillpaper-window .sillpaper-ext-preview label.ext-preview-body {{
    color: {text};
    font-size: 14px;
}}

window.sillpaper-window button.ext-btn-primary {{
    background-color: {accent};
    background-image: none;
    color: #FFFFFF;
    border: none;
    border-radius: 7px;
    min-height: 28px;
    padding: 4px 12px;
    font-size: 12.5px;
    font-weight: 600;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window button.ext-btn-secondary {{
    background-color: {control_bg};
    background-image: none;
    color: {text};
    border: 1px solid {border};
    border-radius: 7px;
    min-height: 28px;
    padding: 4px 12px;
    font-size: 12.5px;
    font-weight: 500;
    box-shadow: none;
    outline: none;
}}

window.sillpaper-window button.ext-btn-secondary:hover {{
    background-color: {control_bg_hover};
}}
"#,
        bg = p.background,
        text = p.text,
        muted = p.muted_text,
        surface = p.surface,
        surface_alt = p.surface_alt,
        hover = p.surface_hover,
        border = p.border,
        accent = p.accent,
        caret = caret_color,
        tip_bg = p.tooltip_bg,
        tip_fg = p.tooltip_fg,
        control_bg = control_bg,
        control_bg_hover = control_bg_hover,
        chrome_fg = chrome_fg,
        chrome_muted = chrome_muted,
        chrome_lift = chrome_lift,
        chrome_rim = chrome_rim,
        chrome_accent = chrome_accent,
        chrome_action = chrome_action,
        toolbar_bg = toolbar_bg,
        toolbar_fg = toolbar_fg,
        font_family = font_family,
        font_pt = font_pt,
    )
}
