use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    Light,
    Dark,
}

impl AppTheme {
    pub fn to_class(self) -> &'static str {
        match self {
            AppTheme::Light => "theme-light",
            AppTheme::Dark => "theme-dark",
        }
    }
}

pub fn get_saved_theme() -> AppTheme {
    if let Some(w) = web_sys::window() {
        if let Ok(Some(ls)) = w.local_storage() {
            if let Ok(Some(val)) = ls.get_item("chatui_theme") {
                match val.as_str() {
                    "light" => return AppTheme::Light,
                    "dark" => return AppTheme::Dark,
                    _ => {}
                }
            }
        }
    }
    AppTheme::Dark // Default to Dark
}

pub fn save_theme(theme: AppTheme) {
    if let Some(w) = web_sys::window() {
        if let Ok(Some(ls)) = w.local_storage() {
            let val = match theme {
                AppTheme::Light => "light",
                AppTheme::Dark => "dark",
            };
            let _ = ls.set_item("chatui_theme", val);
        }
    }
}
