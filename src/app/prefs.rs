//! Per-user preferences (not per project): text size, interface zoom, theme.
//! Stored at `$XDG_CONFIG_HOME/projectfcg/prefs.json` (default `~/.config`)
//! on Linux, `%APPDATA%\\ProjectFCG\\prefs.json` on Windows and
//! `~/Library/Application Support/ProjectFCG/prefs.json` on macOS.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Table and chart text size in points (rows grow with it).
    pub text_size: f32,
    /// Whole-interface zoom (ribbon, dialogs, everything), 1.0 = 100%.
    pub ui_zoom: f32,
    /// View ▸ Theme.
    pub theme_mode: super::theme::ThemeMode,
    pub accent: super::theme::Accent,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            text_size: super::theme::DEFAULT_TEXT_SIZE,
            ui_zoom: 1.0,
            theme_mode: Default::default(),
            accent: Default::default(),
        }
    }
}

pub const ZOOM_STEPS: [f32; 9] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5];

pub fn default_path() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("ProjectFCG").join("prefs.json"));
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/ProjectFCG/prefs.json"));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("projectfcg").join("prefs.json"))
}

/// Where earlier versions kept preferences, newest first: 0.3 was named
/// "Ganttry", 0.1–0.2 "Gantry".
pub fn legacy_paths(current: &std::path::Path) -> Vec<PathBuf> {
    let Some(base) = current.parent().and_then(|d| d.parent()) else { return vec![] };
    ["ganttry", "gantry"].iter().map(|d| base.join(d).join("prefs.json")).collect()
}

/// Load preferences, falling back to an older name's location the first
/// time. Saving always writes the new location, so the migration is one-way.
pub fn load_with_migration(path: &std::path::Path) -> Prefs {
    if !path.exists() {
        if let Some(old) = legacy_paths(path).into_iter().find(|o| o.exists()) {
            return Prefs::load(&old);
        }
    }
    Prefs::load(path)
}

impl Prefs {
    pub fn load(path: &std::path::Path) -> Prefs {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Prefs>(&s).ok())
            .map(|p| p.sanitized())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn sanitized(mut self) -> Prefs {
        use super::theme::{MAX_TEXT_SIZE, MIN_TEXT_SIZE};
        if !self.text_size.is_finite() {
            self.text_size = super::theme::DEFAULT_TEXT_SIZE;
        }
        self.text_size = self.text_size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
        if !self.ui_zoom.is_finite() {
            self.ui_zoom = 1.0;
        }
        self.ui_zoom = self.ui_zoom.clamp(0.5, 3.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_sanitize() {
        let dir = std::env::temp_dir().join(format!("projectfcg-prefs-{}", std::process::id()));
        let path = dir.join("projectfcg/prefs.json");
        let p = Prefs { text_size: 18.0, ui_zoom: 1.25, ..Default::default() };
        p.save(&path).unwrap();
        assert_eq!(Prefs::load(&path), p);
        std::fs::write(&path, r#"{"text_size": 500, "ui_zoom": -3}"#).unwrap();
        let q = Prefs::load(&path);
        assert_eq!(q.text_size, super::super::theme::MAX_TEXT_SIZE);
        assert_eq!(q.ui_zoom, 0.5);
        std::fs::write(&path, "garbage").unwrap();
        assert_eq!(Prefs::load(&path), Prefs::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migrates_from_both_older_names() {
        use super::super::theme::{Accent, ThemeMode};
        let base = std::env::temp_dir().join(format!("projectfcg-migrate-{}", std::process::id()));
        let oldest = base.join("gantry/prefs.json");
        let older = base.join("ganttry/prefs.json");
        let new = base.join("projectfcg/prefs.json");
        assert_eq!(legacy_paths(&new), vec![older.clone(), oldest.clone()]);
        let a = Prefs { text_size: 20.0, ui_zoom: 1.5, ..Default::default() };
        a.save(&oldest).unwrap();
        assert_eq!(load_with_migration(&new), a, "0.1–0.2 prefs are picked up");
        let b = Prefs { text_size: 18.0, ui_zoom: 1.0, theme_mode: ThemeMode::Dark, accent: Accent::Green };
        b.save(&older).unwrap();
        assert_eq!(load_with_migration(&new), b, "0.3 prefs win over 0.1–0.2");
        let c = Prefs { text_size: 16.0, ..Default::default() };
        c.save(&new).unwrap();
        assert_eq!(load_with_migration(&new), c, "the current location wins once it exists");
        std::fs::remove_dir_all(&base).ok();
    }
}
