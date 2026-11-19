mod app;
mod commands;
mod settings;

use std::sync::atomic::Ordering;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};

use app::AppState;
use settings::Settings;

const ID_MENU_EXPORT: &str = "menu_export";
const ID_MENU_EXIT: &str = "menu_exit";
const ID_TRAY_OPEN: &str = "tray_open";
const ID_TRAY_IMPORT: &str = "tray_import";
const ID_TRAY_EXPORT: &str = "tray_export";
const ID_TRAY_EXIT: &str = "tray_exit";

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        ID_MENU_EXPORT | ID_TRAY_EXPORT => {
            tauri::async_runtime::spawn(app::export_viewer(app.clone()));
        }
        ID_MENU_EXIT | ID_TRAY_EXIT => app::exit_app(app),
        ID_TRAY_OPEN => app::show_main(app),
        ID_TRAY_IMPORT => {
            tauri::async_runtime::spawn(app::run_import(app.clone(), false));
        }
        _ => {}
    }
}

fn build_window_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let export = MenuItem::with_id(app, ID_MENU_EXPORT, "閲覧用 HTML を書き出す…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let exit = MenuItem::with_id(app, ID_MENU_EXIT, "終了(&X)", true, None::<&str>)?;
    let file = Submenu::with_items(app, "ファイル(&F)", true, &[&export, &sep, &exit])?;
    Menu::with_items(app, &[&file])
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, ID_TRAY_OPEN, "開く", true, None::<&str>)?;
    let import = MenuItem::with_id(app, ID_TRAY_IMPORT, "今すぐ取り込み", true, None::<&str>)?;
    let export = MenuItem::with_id(app, ID_TRAY_EXPORT, "閲覧用 HTML を書き出す…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let exit = MenuItem::with_id(app, ID_TRAY_EXIT, "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &import, &export, &sep, &exit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("VrcLogger")
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--tray"])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    return;
                }
                let st = window.app_handle().state::<AppState>();
                let minimize = st.settings.lock().unwrap().minimize_to_tray;
                if !st.exit_requested.load(Ordering::SeqCst) && minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::import_now,
            commands::reimport_all,
            commands::vacuum,
            commands::export_viewer,
            commands::get_settings,
            commands::save_settings,
            commands::default_noise_patterns,
            commands::pick_folder,
            commands::autostart_get,
            commands::copy_text,
            commands::open_url,
            commands::recent_players,
            commands::search_players,
            commands::encounters,
            commands::visits,
            commands::visit_players,
            commands::visit_urls,
            commands::search_urls,
            commands::search_raw_events,
            commands::search_avatars,
            commands::search_notifications,
            commands::notification_kinds,
            commands::search_moderation,
        ])
        .setup(|tauri_app| {
            let handle = tauri_app.handle().clone();
            tauri_app.manage(AppState::new(Settings::load_or_create()));

            if let Err(e) = app::reopen_query_db(&handle) {
                app::set_status(&handle, format!("データベースを開けません: {e}"));
            } else {
                // Like the C# MainViewModel ctor: recover avatar switches still present in raw_events.
                let st = handle.state::<AppState>();
                let guard = st.query.lock().unwrap();
                if let Some(conn) = guard.as_ref() {
                    let _ = vrclogger_core::data::import::ImportService::new(conn)
                        .backfill_avatar_events_from_raw_events();
                }
            }
            app::update_title(&handle);

            if let Some(window) = handle.get_webview_window("main") {
                window.set_menu(build_window_menu(&handle)?)?;
            }
            build_tray(&handle)?;
            // Global listener: receives both window-menu and tray-menu events (tray registers none itself, to avoid double handling).
            handle.on_menu_event(|app, event| handle_menu(app, event.id().as_ref()));

            app::restart_timer(&handle);

            if !std::env::args().any(|a| a.eq_ignore_ascii_case("--tray")) {
                app::show_main(&handle);
            }

            // Refresh the registered exe path (move / update) when autostart is enabled.
            let al = handle.autolaunch();
            if al.is_enabled().unwrap_or(false) {
                let _ = al.enable();
            }

            tauri::async_runtime::spawn(app::run_import(handle.clone(), false));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running VrcLogger");
}
