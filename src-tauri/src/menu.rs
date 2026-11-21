//! The window menu bar, the tray icon and its menu, and the handling of their events.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::app::{self, AppState};
use crate::settings::ThemeSetting;
use crate::{maintenance, update};

const ID_MENU_IMPORT: &str = "menu_import";
const ID_MENU_EXPORT: &str = "menu_export";
const ID_MENU_BACKUP: &str = "menu_backup";
const ID_MENU_METADATA: &str = "menu_metadata";
const ID_MENU_SETTINGS: &str = "menu_settings";
const ID_MENU_UPDATE: &str = "menu_update";
const ID_MENU_EXIT: &str = "menu_exit";
const ID_MENU_THEME_AUTO: &str = "menu_theme_auto";
const ID_MENU_THEME_LIGHT: &str = "menu_theme_light";
const ID_MENU_THEME_DARK: &str = "menu_theme_dark";
const ID_MENU_PERIOD: &str = "menu_period";
const ID_TRAY_OPEN: &str = "tray_open";
const ID_TRAY_IMPORT: &str = "tray_import";
const ID_TRAY_EXPORT: &str = "tray_export";
const ID_TRAY_SETTINGS: &str = "tray_settings";
const ID_TRAY_EXIT: &str = "tray_exit";

pub(crate) fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        ID_MENU_EXPORT | ID_TRAY_EXPORT => {
            tauri::async_runtime::spawn(app::export_viewer(app.clone()));
        }
        ID_MENU_BACKUP => {
            tauri::async_runtime::spawn(maintenance::backup_now(app.clone(), false));
        }
        ID_MENU_METADATA => {
            // Only opens the confirmation dialog; nothing is written until the user confirms there.
            app::show_main(app);
            let _ = app.emit("open-metadata-dialog", ());
        }
        ID_MENU_PERIOD => {
            // The date range lives in the UI; the menu only asks it to open its dialog.
            app::show_main(app);
            let _ = app.emit("open-period-dialog", ());
        }
        ID_MENU_SETTINGS | ID_TRAY_SETTINGS => app::open_settings(app),
        ID_MENU_UPDATE => {
            tauri::async_runtime::spawn(update::check(app.clone(), true));
        }
        ID_MENU_THEME_AUTO => app::choose_theme(app, ThemeSetting::Auto),
        ID_MENU_THEME_LIGHT => app::choose_theme(app, ThemeSetting::Light),
        ID_MENU_THEME_DARK => app::choose_theme(app, ThemeSetting::Dark),
        ID_MENU_EXIT | ID_TRAY_EXIT => app::exit_app(app),
        ID_TRAY_OPEN => app::show_main(app),
        ID_MENU_IMPORT | ID_TRAY_IMPORT => {
            tauri::async_runtime::spawn(app::run_import(app.clone(), false));
        }
        _ => {}
    }
}

pub(crate) fn build_window_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let import = MenuItem::with_id(app, ID_MENU_IMPORT, "今すぐ取り込み", true, None::<&str>)?;
    let export = MenuItem::with_id(app, ID_MENU_EXPORT, "閲覧用 HTML を書き出す…", true, None::<&str>)?;
    let backup = MenuItem::with_id(app, ID_MENU_BACKUP, "今すぐバックアップ", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, ID_MENU_SETTINGS, "設定(&S)…", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, ID_MENU_EXIT, "終了(&X)", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep0 = PredefinedMenuItem::separator(app)?;
    let metadata =
        MenuItem::with_id(app, ID_MENU_METADATA, "書き込めるすべての写真にメタデータを書き込む…", true, None::<&str>)?;
    let file =
        Submenu::with_items(app, "ファイル(&F)", true, &[&import, &sep0, &export, &backup, &metadata, &sep1, &settings, &sep2, &exit])?;
    let current = app.state::<AppState>().settings.lock().unwrap().theme;
    let check = |id: &str, text: &str, t: ThemeSetting| CheckMenuItem::with_id(app, id, text, true, current == t, None::<&str>);
    let theme_items = vec![
        (ThemeSetting::Auto, check(ID_MENU_THEME_AUTO, "自動", ThemeSetting::Auto)?),
        (ThemeSetting::Light, check(ID_MENU_THEME_LIGHT, "ライト", ThemeSetting::Light)?),
        (ThemeSetting::Dark, check(ID_MENU_THEME_DARK, "ダーク", ThemeSetting::Dark)?),
    ];
    let theme_menu = Submenu::with_items(app, "テーマ", true, &[&theme_items[0].1, &theme_items[1].1, &theme_items[2].1])?;
    let sep_view = PredefinedMenuItem::separator(app)?;
    let period = MenuItem::with_id(app, ID_MENU_PERIOD, "期間で絞り込む…", true, None::<&str>)?;
    let view = Submenu::with_items(app, "表示(&V)", true, &[&theme_menu, &sep_view, &period])?;
    *app.state::<AppState>().theme_items.lock().unwrap() = theme_items;
    let update = MenuItem::with_id(app, ID_MENU_UPDATE, "更新を確認", true, None::<&str>)?;
    let help = Submenu::with_items(app, "ヘルプ(&H)", true, &[&update])?;
    Menu::with_items(app, &[&file, &view, &help])
}

pub(crate) fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, ID_TRAY_OPEN, "開く", true, None::<&str>)?;
    let import = MenuItem::with_id(app, ID_TRAY_IMPORT, "今すぐ取り込み", true, None::<&str>)?;
    let export = MenuItem::with_id(app, ID_TRAY_EXPORT, "閲覧用 HTML を書き出す…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, ID_TRAY_SETTINGS, "設定…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let exit = MenuItem::with_id(app, ID_TRAY_EXIT, "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &import, &export, &settings, &sep, &exit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Yorimichi")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                app::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
