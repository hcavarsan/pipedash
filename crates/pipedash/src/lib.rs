mod application;
mod domain;
mod infrastructure;

use std::sync::Arc;

use application::commands::{
    add_provider,
    cancel_pipeline_run,
    clear_run_history_cache,
    fetch_pipelines,
    fetch_run_history,
    get_available_plugins,
    get_cached_pipelines,
    get_provider,
    get_refresh_mode,
    get_workflow_parameters,
    get_workflow_run_details,
    list_providers,
    preview_provider_pipelines,
    refresh_all,
    remove_provider,
    set_refresh_mode,
    trigger_pipeline,
    update_provider,
    update_provider_refresh_interval,
    AppState,
};
use application::services::{
    PipelineService,
    ProviderService,
};
use application::RefreshManager;
use infrastructure::database::{
    init_database,
    Repository,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");

            let db_path = app_data_dir.join("pipedash.db");
            let conn = init_database(db_path).expect("Failed to initialize database");

            let repository = Arc::new(Repository::new(conn));
            let provider_service = Arc::new(ProviderService::new(Arc::clone(&repository)));
            let pipeline_service = Arc::new(PipelineService::new(
                Arc::clone(&repository),
                Arc::clone(&provider_service),
            ));
            let refresh_manager = Arc::new(RefreshManager::new(Arc::clone(&pipeline_service)));

            let app_state = AppState {
                provider_service: Arc::clone(&provider_service),
                pipeline_service: Arc::clone(&pipeline_service),
                refresh_manager: Arc::clone(&refresh_manager),
            };

            app.manage(app_state);

            let app_handle = app.handle().clone();
            let refresh_manager_clone = Arc::clone(&refresh_manager);

            tauri::async_runtime::spawn(async move {
                refresh_manager_clone.start(app_handle).await;
            });

            let provider_service_clone = Arc::clone(&provider_service);
            tauri::async_runtime::spawn(async move {
                let _ = provider_service_clone.load_all_providers().await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_provider,
            list_providers,
            get_provider,
            update_provider,
            update_provider_refresh_interval,
            remove_provider,
            fetch_pipelines,
            get_cached_pipelines,
            fetch_run_history,
            trigger_pipeline,
            get_workflow_parameters,
            refresh_all,
            clear_run_history_cache,
            set_refresh_mode,
            get_refresh_mode,
            get_workflow_run_details,
            cancel_pipeline_run,
            get_available_plugins,
            preview_provider_pipelines,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
