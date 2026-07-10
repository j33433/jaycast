//! Light/dark theme preference with localStorage persistence.

use web_sys::window;

const THEME_PREF_KEY: &str = "jaycast:theme";
const THEME_COLOR_DARK: &str = "#1a1712";
const THEME_COLOR_LIGHT: &str = "#f3ebe0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn attr(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    pub fn theme_color(self) -> &'static str {
        match self {
            Theme::Light => THEME_COLOR_LIGHT,
            Theme::Dark => THEME_COLOR_DARK,
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            _ => None,
        }
    }
}

/// Read saved theme override from localStorage (`None` = follow OS).
pub fn load_theme_pref() -> Option<Theme> {
    let storage = window()?.local_storage().ok().flatten()?;
    let val = storage.get_item(THEME_PREF_KEY).ok().flatten()?;
    Theme::from_str(&val)
}

/// Persist theme override to localStorage.
pub fn save_theme_pref(theme: Theme) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(THEME_PREF_KEY, theme.attr());
    }
}

/// Detect OS light/dark preference via `prefers-color-scheme`.
pub fn detect_os_theme() -> Theme {
    let Some(window) = window() else {
        return Theme::Dark;
    };
    match window.match_media("(prefers-color-scheme: dark)") {
        Ok(Some(mq)) if mq.matches() => Theme::Dark,
        Ok(Some(_)) => Theme::Light,
        _ => Theme::Dark,
    }
}

/// Apply `data-theme` on `<html>`.
pub fn apply_theme(theme: Theme) {
    if let Some(document) = window().and_then(|w| w.document()) {
        if let Some(el) = document.document_element() {
            let _ = el.set_attribute("data-theme", theme.attr());
        }
    }
}

/// Update the mobile browser chrome color meta tag.
pub fn apply_theme_color(theme: Theme) {
    let Some(document) = window().and_then(|w| w.document()) else {
        return;
    };
    if let Ok(Some(meta)) = document.query_selector("meta[name=\"theme-color\"]") {
        let _ = meta.set_attribute("content", theme.theme_color());
    }
}
