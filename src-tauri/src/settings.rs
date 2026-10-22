use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub ui_language: String,
    pub theme: String,
    pub shortcut_capture: String,
    pub shortcut_clipboard: String,
    /// Optional show/hide-main-window shortcut. Empty = disabled.
    pub shortcut_main: String,

    /// Popup stays above other windows. Never affects whether the popup closes.
    pub popup_pinned: bool,
    /// "s" | "m" | "l"
    pub popup_font_size: String,
    /// 60..=100 (percent)
    pub popup_opacity: u8,
    pub popup_remember_position: bool,
    /// Last dragged position (physical pixels). Owned by the backend.
    pub popup_pos: Option<(i32, i32)>,

    /// History is OFF by default; nothing is written to disk while it is off.
    pub history_enabled: bool,

    /// Default / last-used translation direction: "en-ar" | "he-ar".
    pub direction: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_language: "en".into(),
            theme: "dark".into(),
            shortcut_capture: "Ctrl+Alt+G".into(),
            shortcut_clipboard: "Ctrl+Alt+V".into(),
            shortcut_main: String::new(),
            popup_pinned: true,
            popup_font_size: "m".into(),
            popup_opacity: 100,
            popup_remember_position: false,
            popup_pos: None,
            history_enabled: false,
            direction: "en-ar".into(),
        }
    }
}

impl Settings {
    pub fn load(path: &PathBuf) -> Self {
        let mut s: Settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        s.sanitize();
        s
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn sanitize(&mut self) {
        if self.ui_language != "ar" {
            self.ui_language = "en".into();
        }
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            self.theme = "dark".into();
        }
        if !matches!(self.popup_font_size.as_str(), "s" | "m" | "l") {
            self.popup_font_size = "m".into();
        }
        self.popup_opacity = self.popup_opacity.clamp(60, 100);
        self.shortcut_main = self.shortcut_main.trim().to_string();
        if !matches!(self.direction.as_str(), "en-ar" | "he-ar") {
            self.direction = "en-ar".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps_values() {
        let mut s = Settings { popup_opacity: 10, popup_font_size: "xl".into(), theme: "x".into(), ..Default::default() };
        s.sanitize();
        assert_eq!(s.popup_opacity, 60);
        assert_eq!(s.popup_font_size, "m");
        assert_eq!(s.theme, "dark");
    }

    #[test]
    fn old_v1_settings_file_loads_with_defaults() {
        let s: Settings = serde_json::from_str(r#"{"uiLanguage":"ar","theme":"light"}"#).unwrap();
        assert_eq!(s.ui_language, "ar");
        assert!(s.popup_pinned);
        assert!(!s.history_enabled);
        assert_eq!(s.shortcut_capture, "Ctrl+Alt+G");
        assert_eq!(s.direction, "en-ar");
    }

    #[test]
    fn direction_is_sanitized_and_persists() {
        let mut s: Settings = serde_json::from_str(r#"{"direction":"he-ar"}"#).unwrap();
        s.sanitize();
        assert_eq!(s.direction, "he-ar");
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.direction, "he-ar");
        let mut bad: Settings = serde_json::from_str(r#"{"direction":"fr-ar"}"#).unwrap();
        bad.sanitize();
        assert_eq!(bad.direction, "en-ar");
    }
}
