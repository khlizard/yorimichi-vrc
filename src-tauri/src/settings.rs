//! Settings file: `%APPDATA%\Yorimichi\yorimichi.settings.json` (PascalCase keys).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// VRChat's data folder: the logs and its `config.json`.
pub const DEFAULT_LOG_FOLDER: &str = "%USERPROFILE%\\AppData\\LocalLow\\VRChat\\VRChat";
/// Last resort for the photo folder; see [`first_run_photo_folder`].
const DEFAULT_PHOTO_FOLDER: &str = "%USERPROFILE%\\Pictures\\VRChat";

/// Display theme. Stored as `"auto" | "light" | "dark"`; any other value reads as auto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSetting {
    #[default]
    Auto,
    Light,
    Dark,
}

impl<'de> Deserialize<'de> for ThemeSetting {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        Ok(match v.as_str() {
            Some("light") => ThemeSetting::Light,
            Some("dark") => ThemeSetting::Dark,
            _ => ThemeSetting::Auto,
        })
    }
}

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
    /// False until the user has answered the "stay in the tray?" question (or saved settings).
    pub tray_choice_made: bool,
    pub photo_folders: Vec<String>,
    pub day_start_hour: u32,
    pub show_first_met: bool,
    pub reunion_days: i32,
    pub auto_vacuum: bool,
    pub auto_vacuum_min_db_mb: i64,
    pub backup_folder: String,
    pub backup_interval_days: i32,
    pub backup_keep_count: i32,
    /// Local time, `YYYY-MM-DD HH:MM:SS`.
    pub last_backup_at: Option<String>,
    pub update_feed: String,
    pub theme: ThemeSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            db_path: "yorimichi.db".into(),
            log_folders: Vec::new(),
            auto_import_interval_minutes: 1,
            minimize_to_tray: false,
            raw_log_retention_days: 30,
            raw_log_noise_patterns: yorimichi_core::parsing::noise::default_patterns(),
            last_viewer_export_folder: None,
            tray_choice_made: false,
            photo_folders: Vec::new(),
            day_start_hour: 6,
            show_first_met: true,
            reunion_days: 180,
            auto_vacuum: true,
            auto_vacuum_min_db_mb: 200,
            backup_folder: "backups".into(),
            backup_interval_days: 7,
            backup_keep_count: 3,
            last_backup_at: None,
            update_feed: String::new(),
            theme: ThemeSetting::Auto,
        }
    }
}

/// `%APPDATA%\Yorimichi` (also the base for relative paths).
pub fn settings_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Yorimichi")
}

pub fn settings_path() -> PathBuf {
    settings_dir().join("yorimichi.settings.json")
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

    pub fn resolved_photo_folders(&self) -> Vec<PathBuf> {
        self.photo_folders.iter().map(|f| resolve(f)).collect()
    }

    pub fn resolved_backup_folder(&self) -> PathBuf {
        resolve(&self.backup_folder)
    }

    /// Loads the settings file, creating it with first-run defaults when it does not exist.
    /// The second value is a message for the user when an existing file could not be read.
    pub fn load_or_create() -> (Settings, Option<String>) {
        let config = std::fs::read_to_string(resolve(DEFAULT_LOG_FOLDER).join("config.json")).ok();
        let photo_folder = first_run_photo_folder(config.as_deref(), dirs::picture_dir());
        load_or_create_at(&settings_path(), photo_folder)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        write_file(&path, self).map_err(|e| e.to_string())
    }
}

/// Where VRChat saves photos on this PC: the folder set in VRChat's `config.json`
/// (`picture_output_folder`), else `VRChat` in the user's Pictures folder (which OneDrive may
/// have moved), else the classic location.
fn first_run_photo_folder(vrchat_config: Option<&str>, pictures: Option<PathBuf>) -> String {
    let configured = vrchat_config
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        .and_then(|v| v.get("picture_output_folder").and_then(|f| f.as_str()).map(str::trim).map(String::from))
        .filter(|f| !f.is_empty());
    configured
        .or_else(|| pictures.map(|p| p.join("VRChat").display().to_string()))
        .unwrap_or_else(|| DEFAULT_PHOTO_FOLDER.into())
}

fn load_or_create_at(path: &Path, first_run_photo_folder: String) -> (Settings, Option<String>) {
    if !path.exists() {
        let s = Settings {
            log_folders: vec![DEFAULT_LOG_FOLDER.into()],
            photo_folders: vec![first_run_photo_folder],
            ..Settings::default()
        };
        let _ = write_file(path, &s);
        return (s, None);
    }
    let read = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str::<Settings>(&t).map_err(|e| e.to_string()));
    match read {
        Ok(s) => (s, None),
        Err(e) => {
            // Keep what the user had: the next save would otherwise replace it with defaults.
            let backup = path.with_extension("json.bak");
            let kept = std::fs::copy(path, &backup).is_ok();
            let mut msg = format!("設定ファイルを読めなかったため、既定の設定で起動しました。\n{}\n\n原因: {e}", path.display());
            if kept {
                msg.push_str(&format!("\n\n元のファイルは次の名前で残してあります。\n{}", backup.display()));
            }
            let s = Settings { log_folders: vec![DEFAULT_LOG_FOLDER.into()], ..Settings::default() };
            (s, Some(msg))
        }
    }
}

/// Writes to a temporary file and renames it over the target, so a crash cannot leave half a file.
fn write_file(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_file_gets_defaults_for_new_fields() {
        let s: Settings = serde_json::from_str(r#"{"DbPath":"x.db","MinimizeToTray":true}"#).unwrap();
        assert_eq!(s.db_path, "x.db");
        assert!(s.minimize_to_tray);
        assert!(!s.tray_choice_made);
        assert!(s.photo_folders.is_empty());
        assert_eq!(s.day_start_hour, 6);
        assert!(s.show_first_met);
        assert_eq!(s.reunion_days, 180);
        assert!(s.auto_vacuum);
        assert_eq!(s.auto_vacuum_min_db_mb, 200);
        assert_eq!(s.backup_folder, "backups");
        assert_eq!(s.backup_interval_days, 7);
        assert_eq!(s.backup_keep_count, 3);
        assert_eq!(s.last_backup_at, None);
        assert_eq!(s.update_feed, "");
        assert_eq!(s.theme, ThemeSetting::Auto);
    }

    #[test]
    fn first_run_photo_folder_prefers_vrchat_config_then_pictures() {
        let pics = Some(PathBuf::from("X:\\Pics"));
        let cfg = r#"{"picture_output_folder":" Y:\\Shots ","camera_res_height":2160}"#;
        assert_eq!(first_run_photo_folder(Some(cfg), pics.clone()), "Y:\\Shots");
        for no_folder in [r#"{"picture_output_folder":""}"#, r#"{"other":1}"#, "not json", r#"{"picture_output_folder":3}"#] {
            assert_eq!(first_run_photo_folder(Some(no_folder), pics.clone()), "X:\\Pics\\VRChat", "{no_folder}");
        }
        assert_eq!(first_run_photo_folder(None, pics), "X:\\Pics\\VRChat");
        assert_eq!(first_run_photo_folder(None, None), DEFAULT_PHOTO_FOLDER);
    }

    #[test]
    fn first_run_creates_the_file_with_the_found_folders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("yorimichi.settings.json");
        let (s, warning) = load_or_create_at(&path, "P:\\Photos".into());
        assert_eq!(warning, None);
        assert_eq!(s.photo_folders, ["P:\\Photos"]);
        assert_eq!(s.log_folders, [DEFAULT_LOG_FOLDER]);
        // The second start reads what the first one wrote (and leaves no temporary file).
        let (again, warning) = load_or_create_at(&path, "ignored".into());
        assert_eq!(warning, None);
        assert_eq!(again.photo_folders, ["P:\\Photos"]);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn unreadable_file_is_kept_as_bak_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yorimichi.settings.json");
        let broken = r#"{"DbPath":"mine.db","DayStartHour":-1}"#;
        std::fs::write(&path, broken).unwrap();
        let (s, warning) = load_or_create_at(&path, "P:\\Photos".into());
        assert_eq!(s.db_path, "yorimichi.db");
        assert!(s.photo_folders.is_empty());
        let warning = warning.expect("the user must be told");
        assert!(warning.contains("yorimichi.settings.json.bak"), "{warning}");
        assert_eq!(std::fs::read_to_string(dir.path().join("yorimichi.settings.json.bak")).unwrap(), broken);
        // A later save replaces the broken file; the copy stays.
        write_file(&path, &s).unwrap();
        assert!(serde_json::from_str::<Settings>(&std::fs::read_to_string(&path).unwrap()).is_ok());
        assert_eq!(std::fs::read_to_string(dir.path().join("yorimichi.settings.json.bak")).unwrap(), broken);
    }

    #[test]
    fn theme_round_trips_and_unknown_is_auto() {
        let s = Settings { theme: ThemeSetting::Dark, ..Settings::default() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"Theme\":\"dark\""));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, ThemeSetting::Dark);
        let light: Settings = serde_json::from_str(r#"{"Theme":"light"}"#).unwrap();
        assert_eq!(light.theme, ThemeSetting::Light);
        for bad in [r#"{"Theme":"sepia"}"#, r#"{"Theme":7}"#, r#"{"Theme":null}"#] {
            let s: Settings = serde_json::from_str(bad).unwrap();
            assert_eq!(s.theme, ThemeSetting::Auto, "{bad}");
        }
    }

    #[test]
    fn round_trip_keeps_pascal_case_keys() {
        let s = Settings { last_backup_at: Some("2026-01-02 03:04:05".into()), ..Settings::default() };
        let json = serde_json::to_string(&s).unwrap();
        for key in ["\"TrayChoiceMade\"", "\"PhotoFolders\"", "\"DayStartHour\"", "\"BackupFolder\"", "\"LastBackupAt\"", "\"UpdateFeed\""] {
            assert!(json.contains(key), "missing {key}");
        }
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.last_backup_at.as_deref(), Some("2026-01-02 03:04:05"));
        assert!(!back.minimize_to_tray);
    }
}
