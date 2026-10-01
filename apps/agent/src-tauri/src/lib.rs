mod agent;
mod agent_pure;
mod anonymous_analytics;
mod auth;
mod coach_chat;
pub mod context;
mod crash_guard;
mod desktop_presence;
mod entitlements;
mod focus_alerts;
mod focus_semantics;
mod insights_local;
mod jira;
mod linear;
mod llama_port;
mod llama_windows_job;
mod local_agent;
pub mod mcp;
mod model_assets;
mod oauth_env;
pub mod paths;
mod privacy;
mod report_schedule;
mod screenshot_disk;
mod secure_config;
mod sync;
mod sync_env;
mod sync_pure;
mod telemetry;
mod user_preferences;
mod vision_model;

use tauri::Manager;

use agent::{
    capture_screen_command, check_local_server, check_ollama, get_activity_log, get_config,
    get_status, get_today_history, get_week_summary, initialize_agent,
    llama_managed_process_status, llama_server_log_tail, restart_llama_server_cpu_only,
    save_activity, set_task_context, start_monitoring, stop_monitoring, update_config, AgentState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must be the very first thing `run()` does: installs the Vectored Exception
    // Handler that contains hardware faults from third-party DLLs (e.g. a broken
    // Winsock LSP such as an old VPN's network-intercept driver) so they can
    // only ever take down the specific background thread that touched them,
    // never the whole process. See `crash_guard` module docs.
    crash_guard::install();

    tauri::Builder::default()
        .manage(AgentState::default())
        .invoke_handler(tauri::generate_handler![
            initialize_agent,
            local_agent::get_local_agent_data,
            local_agent::session_plan::propose_session_plan,
            local_agent::session_plan::confirm_session_plan,
            local_agent::session_plan::cancel_session_plan,
            desktop_presence::get_desktop_preferences,
            desktop_presence::set_focus_alerts_enabled,
            desktop_presence::set_contextual_focus_alerts_enabled,
            report_schedule::get_weekly_report_schedule,
            report_schedule::save_weekly_report_schedule,
            report_schedule::save_scheduled_report_pdf,
            get_config,
            update_config,
            get_status,
            start_monitoring,
            stop_monitoring,
            capture_screen_command,
            save_activity,
            get_activity_log,
            check_ollama,
            check_local_server,
            agent::capture_context_snapshot,
            jira::fetch_jira_tasks,
            jira::start_jira_oauth,
            jira::fetch_jira_profile,
            sync::force_sync_now,
            sync::save_user_session,
            sync::clear_user_session,
            sync::get_current_user,
            sync::upload_activity_report,
            sync::join_team,
            sync::get_user_teams,
            sync::set_active_team,
            entitlements::get_entitlements,
            entitlements::save_entitlements_command,
            entitlements::refresh_entitlements,
            entitlements::fetch_cloud_insights,
            entitlements::request_cloud_insights,
            coach_chat::get_coach_chat_messages,
            coach_chat::clear_coach_chat,
            coach_chat::get_coach_chat_usage,
            coach_chat::send_coach_chat_message,
            insights_local::generate_local_status_report,
            mcp::get_mcp_connection_info,
            user_preferences::get_user_preferences,
            user_preferences::save_user_preferences_command,
            anonymous_analytics::get_analytics_consent,
            anonymous_analytics::set_analytics_consent,
            anonymous_analytics::sync_anonymous_analytics,
            anonymous_analytics::submit_product_feedback,
            agent::start_server,
            agent::stop_server,
            model_assets::local_model_status,
            model_assets::download_local_model,
            llama_managed_process_status,
            llama_server_log_tail,
            restart_llama_server_cpu_only,
            // Auth commands
            auth::start_auth,
            auth::get_auth_session,
            auth::logout,
            auth::is_logged_in,
            auth::login_with_code,
            // Linear commands
            linear::fetch_linear_tasks,
            linear::fetch_linear_profile,
            // History commands
            get_today_history,
            get_week_summary,
            set_task_context,
            paths::get_flowsight_user_paths,
            paths::save_pdf_to_downloads,
            paths::open_path_in_file_manager,
        ])
        .setup(|app| {
            // Self-update support (GitHub Releases). Desktop-only; mobile targets skip it.
            // `process` provides relaunch() so the frontend can restart after installing.
            #[cfg(desktop)]
            {
                app.handle().plugin(tauri_plugin_dialog::init())?;
                app.handle().plugin(tauri_plugin_notification::init())?;
                app.handle().plugin(tauri_plugin_process::init())?;
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            // Log a archivo en TODOS los builds. En release el usuario no ve stderr,
            // así que sin esto no hay forma de diagnosticar crashes post-login.
            // Logs: ~/Library/Application Support/ai.flowsight.agent/logs/ (macOS)
            //       %LOCALAPPDATA%\ai.flowsight.agent\logs\ (Windows)
            // o equivalente del OS según tauri-plugin-log.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .targets([
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                            file_name: None,
                        }),
                    ])
                    .build(),
            )?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
