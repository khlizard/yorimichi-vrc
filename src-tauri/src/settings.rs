//! Settings file: `%APPDATA%\VrcLogger\vrclogger.settings.json` (PascalCase keys, as in the WPF app).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_LOG_FOLDER: &str = "%USERPROFILE%\\AppData\\LocalLow\\VRChat\\VRChat";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Settings {
    pub db_path: String,
    pub log_folders: Vec<String>,
    pub auto_import_interval_minutes: i32,
    pub minimize_to_tray: bool,
    pub raw_log_retention_days: i32,
    pub raw_log_noise_patterns: Vec<String>,
    pub last_viewer_export_folder: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            db_path: "vrclog.db".into(),
            log_folders: Vec::new(),
            auto_import_interval_minutes: 1,
            minimize_to_tray: true,
            raw_log_retention_days: 30,
            raw_log_noise_patterns: vrclogger_core::parsing::noise::default_patterns(),
            last_viewer_export_folder: None,
        }
    }
}

/// `%APPDATA%\VrcLogger` (also the base for relative paths).
pub fn settings_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("VrcLogger")
}

pub fn settings_path() -> PathBuf {
    settings_dir().join("vrclogger.settings.json")
}

/// Expands `%NAME%` using environment variables; unknown names are left as written.
pub fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match (!name.is_empty()).then(|| std::env::var(name).ok()).flatten() {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn resolve(p: &str) -> PathBuf {
    let expanded = PathBuf::from(expand_env(p));
    if expanded.is_absolute() {
        expanded
    } else {
        settings_dir().join(expanded)
    }
}

impl Settings {
    pub fn resolved_db_path(&self) -> PathBuf {
        resolve(&self.db_path)
    }

    pub fn resolved_log_folders(&self) -> Vec<PathBuf> {
        self.log_folders.iter().map(|f| resolve(f)).collect()
    }

    /// Loads the file; creates it with defaults when missing (or falls back to defaults when unreadable).
    pub fn load_or_create() -> Settings {
        let path = settings_path();
        if path.exists() {
            return std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
                .unwrap_or_else(|| Settings {
                    log_folders: vec![DEFAULT_LOG_FOLDER.into()],
                    ..Settings::default()
                });
        }
        let s = Settings { log_folders: vec![DEFAULT_LOG_FOLDER.into()], ..Settings::default() };
        let _ = s.save();
        s
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        write_file(&path, self).map_err(|e| e.to_string())
    }
}

fn write_file(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}
