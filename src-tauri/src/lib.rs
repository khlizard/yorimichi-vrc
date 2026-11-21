mod app;
mod commands;
mod idle;
mod library_commands;
mod maintenance;
mod metadata_commands;
mod photos;
mod settings;
pub mod shell_thumb;
mod update;

use std::sync::atomic::Ordering;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};

use app::AppState;
use settings::{Settings, ThemeSetting};

const ID_MENU_EXPORT: &str = "menu_export";
const ID_MENU_BACKUP: &str = "menu_backup";
const ID_MENU_METADATA: &str = "menu_metadata";
const ID_MENU_METADATA_ALL: &str = "menu_metadata_all";
const ID_MENU_SETTINGS: &str = "menu_settings";
const ID_MENU_UPDATE: &str = "menu_update";
const ID_MENU_EXIT: &str = "menu_exit";
const ID_MENU_THEME_AUTO: &str = "menu_theme_auto";
const ID_MENU_THEME_LIGHT: &str = "menu_theme_light";
const ID_MENU_THEME_DARK: &str = "menu_theme_dark";
const ID_TRAY_OPEN: &str = "tray_open";
const ID_TRAY_IMPORT: &str = "tray_import";
const ID_TRAY_EXPORT: &str = "tray_export";
const ID_TRAY_SETTINGS: &str = "tray_settings";
const ID_TRAY_EXIT: &str = "tray_exit";

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        ID_MENU_EXPORT | ID_TRAY_EXPORT => {
            tauri::async_runtime::spawn(app::export_viewer(app.clone()));
        }
        ID_MENU_BACKUP => {
            tauri::async_runtime::spawn(maintenance::backup_now(app.clone(), false));
        }
        ID_MENU_METADATA | ID_MENU_METADATA_ALL => {
            // Only opens the confirmation dialog; nothing is written until the user confirms there.
            app::show_main(app);
            let kind = if id == ID_MENU_METADATA_ALL { "all" } else { "range" };
            let _ = app.emit("open-metadata-dialog", kind);
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
        ID_TRAY_IMPORT => {
            tauri::async_runtime::spawn(app::run_import(app.clone(), false));
        }
        _ => {}
    }
}

fn build_window_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let export = MenuItem::with_id(app, ID_MENU_EXPORT, "閲覧用 HTML を書き出す…", true, None::<&str>)?;
    let backup = MenuItem::with_id(app, ID_MENU_BACKUP, "今すぐバックアップ", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, ID_MENU_SETTINGS, "設定(&S)…", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, ID_MENU_EXIT, "終了(&X)", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let metadata = MenuItem::with_id(app, ID_MENU_METADATA, "写真にメタデータを書き込む…", true, None::<&str>)?;
    let metadata_all =
        MenuItem::with_id(app, ID_MENU_METADATA_ALL, "書き込めるすべての写真にメタデータを書き込む…", true, None::<&str>)?;
    let file = Submenu::with_items(
        app,
        "ファイル(&F)",
        true,
        &[&export, &backup, &metadata, &metadata_all, &sep1, &settings, &sep2, &exit],
    )?;
    let current = app.state::<AppState>().settings.lock().unwrap().theme;
    let check = |id: &str, text: &str, t: ThemeSetting| CheckMenuItem::with_id(app, id, text, true, current == t, None::<&str>);
    let theme_items = vec![
        (ThemeSetting::Auto, check(ID_MENU_THEME_AUTO, "自動", ThemeSetting::Auto)?),
        (ThemeSetting::Light, check(ID_MENU_THEME_LIGHT, "ライト", ThemeSetting::Light)?),
        (ThemeSetting::Dark, check(ID_MENU_THEME_DARK, "ダーク", ThemeSetting::Dark)?),
    ];
    let theme_menu = Submenu::with_items(app, "テーマ", true, &[&theme_items[0].1, &theme_items[1].1, &theme_items[2].1])?;
    let view = Submenu::with_items(app, "表示(&V)", true, &[&theme_menu])?;
    *app.state::<AppState>().theme_items.lock().unwrap() = theme_items;
    let update = MenuItem::with_id(app, ID_MENU_UPDATE, "更新を確認", true, None::<&str>)?;
    let help = Submenu::with_items(app, "ヘルプ(&H)", true, &[&update])?;
    Menu::with_items(app, &[&file, &view, &help])
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, ID_TRAY_OPEN, "開く", true, None::<&str>)?;
    let import = MenuItem::with_id(app, ID_TRAY_IMPORT, "今すぐ取り込み", true, None::<&str>)?;
    let export = MenuItem::with_id(app, ID_TRAY_EXPORT, "閲覧用 HTML を書き出す…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, ID_TRAY_SETTINGS, "設定…", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let exit = MenuItem::with_id(app, ID_TRAY_EXIT, "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &import, &export, &settings, &sep, &exit])?;

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
        .register_asynchronous_uri_scheme_protocol("vrcphoto", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(photos::handle_request(app, request).await);
            });
        })
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
                if st.exit_requested.load(Ordering::SeqCst) {
                    return;
                }
                let (minimize, choice_made) = {
                    let s = st.settings.lock().unwrap();
                    (s.minimize_to_tray, s.tray_choice_made)
                };
                api.prevent_close();
                if !choice_made {
                    // Never stay resident without having asked: the UI shows the choice dialog.
                    let _ = window.emit("ask-tray", ());
                } else if minimize {
                    let _ = window.hide();
                } else {
                    // The tray icon / settings window would otherwise keep the process alive.
                    app::exit_app(window.app_handle());
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
            commands::answer_tray_choice,
            commands::backup_now,
            commands::check_update,
            commands::apply_update,
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
            commands::rescan_photos,
            commands::photos,
            commands::photo_days,
            commands::photos_for_visit,
            commands::photo_detail,
            commands::set_photo_favorite,
            commands::photo_count,
            commands::reveal_photo,
            commands::open_photo,
            library_commands::list_tags,
            library_commands::create_tag,
            library_commands::update_tag,
            library_commands::delete_tag,
            library_commands::set_player_tags,
            library_commands::set_player_memo,
            library_commands::player_profile,
            library_commands::players_by_tag,
            library_commands::player_marks,
            library_commands::visit_player_badges,
            library_commands::worlds,
            library_commands::world,
            library_commands::set_world_note,
            library_commands::timeline_day,
            library_commands::activity,
            library_commands::adjacent_active_day,
            library_commands::latest_active_day,
            library_commands::on_this_day,
            library_commands::stats,
            library_commands::years,
            library_commands::albums,
            library_commands::album,
            library_commands::create_album,
            library_commands::update_album,
            library_commands::delete_album,
            library_commands::album_add_photos,
            library_commands::album_remove_photos,
            library_commands::album_reorder,
            library_commands::album_photos,
            library_commands::albums_of_photo,
            library_commands::create_album_from_visit,
            library_commands::export_album_html,
            library_commands::export_day_html,
            library_commands::open_world_page,
            library_commands::open_user_page,
            library_commands::open_instance,
            metadata_commands::metadata_plan,
            metadata_commands::metadata_run,
            metadata_commands::metadata_cancel,
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
            app::apply_theme(&handle);
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
            maintenance::start(&handle);
            update::start(&handle);
            photos::start_pregen_delayed(&handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building VrcLogger")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                close_query_db(app);
            }
        });
}

/// `app.exit` ends the process without dropping managed state, so close the query
/// connection here; closing the last connection lets SQLite fold the WAL back and delete it.
pub(crate) fn close_query_db(app: &AppHandle) {
    let st = app.state::<AppState>();
    let Some(conn) = st.query.lock().unwrap().take() else {
        return;
    };
    // While an import holds the write lock the checkpoint would wait out the busy timeout.
    if !st.importing.load(Ordering::SeqCst) {
        use vrclogger_core::data::db;
        let min_mb = st.settings.lock().unwrap().auto_vacuum_min_db_mb;
        let auto = st.settings.lock().unwrap().auto_vacuum;
        if auto {
            let size = db::file_size_bytes(&conn).unwrap_or(0);
            let reclaimable = db::reclaimable_bytes(&conn).unwrap_or(0);
            if maintenance::should_vacuum(size, reclaimable, min_mb) {
                let _ = db::vacuum(&conn);
            }
        }
        let _ = db::checkpoint_truncate(&conn);
    }
    drop(conn);
}
