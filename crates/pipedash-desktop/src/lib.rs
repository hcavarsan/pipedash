#[cfg(target_os = "android")]
mod android;
mod app_lock;
#[cfg(target_os = "macos")]
mod app_menu;
mod build_watch;
mod commands;
mod core_boot;
mod credential;
#[cfg(desktop)]
mod desktop;
mod error;
mod export_file;
pub mod fallback_store;
mod host;
pub mod keyring_store;
mod local_notifications;
mod native;
mod pipeline_views;
mod plugin_registry;
mod push;
mod remote;
mod run_logs;
mod sign_in;
mod startup;
mod system_notifications;
pub mod tauri_event_bus;

use std::path::PathBuf;
use std::sync::Arc;

use pipedash_core::CoreContext;

pub struct AppDataDir(pub PathBuf);

#[derive(Default)]
pub struct MaybeCoreContext(pub Arc<tokio::sync::RwLock<Option<Arc<CoreContext>>>>);

impl MaybeCoreContext {
    pub async fn get(&self) -> Result<Arc<CoreContext>, String> {
        self.0
            .read()
            .await
            .clone()
            .ok_or_else(|| "Application not initialized. Please complete setup first.".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    startup::init_process();

    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(error) = desktop::show_main_window(app) {
                tracing::warn!(%error, "Couldn't restore the running Pipedash instance");
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(desktop::handle_window_event);
    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_pipedash_push::init())
        .plugin(tauri_plugin_pipedash_native::init())
        .manage(native::PendingIntent::default())
        .manage(build_watch::session::BuildWatchState::default())
        .manage(local_notifications::LocalNotificationsState::default());

    #[cfg(mobile)]
    let builder = builder
        .plugin(tauri_plugin_biometric::init())
        .plugin(tauri_plugin_barcode_scanner::init())
        .on_window_event(|window, event| {
            app_lock::handle_window_event(window, event);
            remote::handle_window_event(window, event);
        });

    let result = builder
        .setup(startup::setup)
        .invoke_handler(tauri::generate_handler![
            commands::setup::check_setup_status,
            commands::setup::bootstrap_app,
            commands::setup::prepare_setup_draft,
            commands::setup::apply_setup_draft,
            commands::providers::list_providers,
            commands::providers::get_provider,
            commands::providers::update_provider_refresh_interval,
            commands::providers::remove_provider,
            commands::providers::export_providers,
            commands::providers::import_providers,
            commands::provider_draft::validate_provider_draft,
            commands::provider_draft::save_provider_draft,
            commands::provider_draft::get_matching_pipeline_ids,
            commands::discovery::get_available_plugins,
            commands::discovery::get_provider_field_options,
            commands::discovery::fetch_provider_organizations,
            commands::discovery::preview_provider_pipelines,
            commands::discovery::check_provider_permissions,
            sign_in::start_sign_in,
            sign_in::start_provider_sign_in,
            sign_in::reconnect_provider,
            sign_in::sign_in_status,
            sign_in::complete_sign_in,
            sign_in::cancel_sign_in,
            commands::capabilities::get_provider_table_schema,
            commands::pipelines::get_cached_pipelines,
            commands::pipelines::fetch_run_history,
            commands::pipelines::get_workflow_run_details,
            build_watch::get_build_watch,
            build_watch::start_build_watch,
            build_watch::stop_build_watch,
            commands::pipelines::prepare_pipeline_trigger,
            commands::pipelines::trigger_pipeline,
            commands::pipelines::cancel_pipeline_run,
            commands::pipelines::get_cancellation_targets,
            commands::pipelines::cancel_pipeline_runs,
            commands::pipelines::force_cancel_pipeline_run,
            commands::pipelines::review_pipeline_run,
            commands::pipelines::rerun_failed_jobs,
            commands::pipelines::rerun_pipeline_run,
            commands::pipelines::enable_pipeline,
            commands::refresh::refresh_all,
            commands::refresh::refresh_provider,
            commands::refresh::set_refresh_mode,
            commands::settings::update_general_settings,
            commands::cache::get_cache_stats,
            commands::cache::clear_pipelines_cache,
            commands::cache::clear_all_run_history_caches,
            commands::cache::clear_workflow_params_cache,
            commands::cache::clear_all_caches,
            commands::metrics::get_global_metrics_config,
            commands::metrics::update_global_metrics_config,
            commands::metrics::get_pipeline_metrics_config,
            commands::metrics::update_pipeline_metrics_config,
            commands::metrics::get_metrics_dashboard,
            commands::metrics::get_metrics_storage_stats,
            commands::metrics::flush_pipeline_metrics,
            commands::preferences::get_table_preferences,
            commands::preferences::save_table_preferences,
            commands::preferences::get_default_table_preferences,
            commands::config::get_storage_config,
            commands::vault::get_vault_password_status,
            commands::vault::get_vault_status,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::config::get_config_content,
            commands::config::save_config_content,
            commands::config::analyze_config,
            commands::config::get_storage_paths,
            commands::config::get_default_data_dir,
            commands::migration::test_storage_connection,
            commands::migration::execute_storage_migration,
            commands::system::factory_reset,
            commands::system::restart_app,
            app_lock::commands::unlock_with_biometrics,
            app_lock::commands::set_app_lock,
            export_file::save_export_file,
            export_file::export_run_log,
            run_logs::get_run_log_tree,
            run_logs::open_run_log,
            run_logs::read_run_log_lines,
            run_logs::search_run_log,
            run_logs::clear_run_logs_cache,
            pipeline_views::list_pipeline_views,
            pipeline_views::get_pipeline_view_page,
            pipeline_views::create_pipeline_view,
            pipeline_views::update_pipeline_view,
            pipeline_views::delete_pipeline_view,
            pipeline_views::reorder_pipeline_views,
            pipeline_views::list_pipeline_view_members,
            pipeline_views::set_pipeline_view_membership,
            pipeline_views::list_pins,
            pipeline_views::set_pin,
            pipeline_views::set_pipeline_hidden,
            pipeline_views::import_legacy_pipeline_marks,
            pipeline_views::get_view_selection,
            pipeline_views::set_view_memberships,
            pipeline_views::get_view_editor_options,
            pipeline_views::get_pipeline_navigation,
            commands::applications::list_applications,
            commands::applications::get_application,
            commands::applications::get_application_changes,
            commands::applications::get_application_metrics,
            commands::applications::create_application,
            commands::applications::edit_application,
            commands::applications::delete_application,
            commands::applications::revert_application_override,
            commands::applications::list_application_suggestions,
            commands::applications::accept_application_suggestions,
            commands::applications::get_pipeline_applications,
            commands::applications::get_commit_locations,
            commands::applications::get_deploy_changes,
            push::send_test_alert,
            remote::commands::get_connection,
            remote::commands::run_on_this_device,
            remote::commands::pair_with_server,
            remote::commands::disconnect_server,
            remote::commands::reconnect_server,
            remote::commands::set_allow_private_http,
            remote::commands::remote_request,
            remote::commands::remote_save_download,
            host::get_sharing_status,
            host::set_sharing,
            host::reset_sharing_port,
            host::list_paired_devices,
            host::create_pairing_code,
            host::remove_paired_device,
            local_notifications::commands::get_local_notification_settings,
            local_notifications::commands::set_local_notifications_enabled,
            local_notifications::commands::pair_alert_host,
            local_notifications::commands::remove_alert_host,
            local_notifications::commands::set_notification_preferences,
            local_notifications::commands::change_notification_preference,
            local_notifications::commands::load_notification_preferences,
            remote::alerts::take_missed_alerts,
            system_notifications::show_system_notification,
            commands::notifications::update_notification_center,
            commands::notifications::get_push_relay,
            commands::notifications::set_push_relay,
            native::native_haptic,
            native::native_set_appearance,
            native::native_accessibility,
            native::native_open_settings,
            native::native_share_link,
            native::native_set_badge,
            native::native_release_privacy_cover,
            native::native_peek_pending_intent,
            native::native_ack_pending_intent,
            #[cfg(desktop)]
            desktop::settings::get_desktop_status,
            #[cfg(desktop)]
            desktop::settings::update_desktop_settings,
            #[cfg(desktop)]
            desktop::settings::import_desktop_notification_preference,
            #[cfg(desktop)]
            desktop::settings::test_desktop_notification,
            #[cfg(desktop)]
            desktop::notifications::get_desktop_notifications,
            #[cfg(desktop)]
            desktop::notifications::mark_desktop_notifications_read,
            #[cfg(desktop)]
            desktop::notifications::clear_desktop_notifications,
            #[cfg(desktop)]
            desktop::updater::get_update_status,
            #[cfg(desktop)]
            desktop::updater::check_for_update,
            #[cfg(desktop)]
            desktop::updater::install_update,
        ])
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|app, event| {
                native::on_run_event(app, &event);
                #[cfg(desktop)]
                desktop::on_run_event(app, &event);
            })
        });

    if let Err(error) = result {
        tracing::error!("error while building tauri application: {error}");
    }
}

#[cfg(test)]
mod tests;
