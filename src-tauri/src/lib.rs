mod app;
mod commands;
mod idle;
mod errors;
mod library_commands;
mod log_commands;
mod maintenance;
mod menu;
mod metadata_commands;
mod photo_commands;
mod photos;
mod settings;
pub mod shell_thumb;
mod update;

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};

use app::AppState;
use settings::Settings;

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
            log_commands::recent_players,
            log_commands::search_players,
            log_commands::encounters,
            log_commands::visits,
            log_commands::visit_players,
            log_commands::visit_urls,
            log_commands::search_urls,
            log_commands::search_raw_events,
            log_commands::search_avatars,
            log_commands::search_notifications,
            log_commands::notification_kinds,
            log_commands::search_moderation,
            photo_commands::rescan_photos,
            photo_commands::photos,
            photo_commands::photo_days,
            photo_commands::photos_for_visit,
            photo_commands::photo_detail,
            photo_commands::set_photo_favorite,
            photo_commands::reveal_photo,
            photo_commands::open_photo,
            photo_commands::photo_file_info,
            photo_commands::copy_photo_image,
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
            library_commands::set_world_note,
            library_commands::timeline_day,
            library_commands::activity,
            library_commands::adjacent_active_day,
            library_commands::latest_active_day,
            library_commands::on_this_day,
            library_commands::stats,
            library_commands::years,
            library_commands::albums,
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
            metadata_commands::photo_meta_status,
        ])
        .setup(|tauri_app| {
            let handle = tauri_app.handle().clone();
            let (settings, settings_warning) = Settings::load_or_create();
            tauri_app.manage(AppState::new(settings));
            if let Some(msg) = settings_warning {
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                handle.dialog().message(msg).title("Yorimichi").kind(MessageDialogKind::Warning).show(|_| {});
            }

            if let Err(e) = app::reopen_query_db(&handle) {
                app::set_status(&handle, format!("データベースを開けません: {e}"));
            } else {
                // Recover avatar switches still present in raw_events.
                let st = handle.state::<AppState>();
                let guard = st.query.lock().unwrap();
                if let Some(conn) = guard.as_ref() {
                    let _ = yorimichi_core::data::import::ImportService::new(conn)
                        .backfill_avatar_events_from_raw_events();
                }
            }
            app::update_title(&handle);

            if let Some(window) = handle.get_webview_window("main") {
                window.set_menu(menu::build_window_menu(&handle)?)?;
            }
            menu::build_tray(&handle)?;
            app::apply_theme(&handle);
            // Global listener: receives both window-menu and tray-menu events (tray registers none itself, to avoid double handling).
            handle.on_menu_event(|app, event| menu::handle_menu(app, event.id().as_ref()));

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
        .expect("error while building Yorimichi")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                close_query_db(app);
            }
        });
}

/// `app.exit` ends the process without dropping managed state, so close the query
/// connections here; closing the last connection lets SQLite fold the WAL back and delete it.
pub(crate) fn close_query_db(app: &AppHandle) {
    let st = app.state::<AppState>();
    // The photo-lookup connection goes first, so the main one is the last open here.
    drop(st.photo_query.lock().unwrap().take());
    let Some(conn) = st.query.lock().unwrap().take() else {
        return;
    };
    // While an import holds the write lock the checkpoint would wait out the busy timeout.
    if !st.importing.load(Ordering::SeqCst) {
        use yorimichi_core::data::db;
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
