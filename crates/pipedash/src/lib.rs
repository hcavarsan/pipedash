mod application;
mod domain;
mod infrastructure;

use std::sync::Arc;

use application::commands::{
    add_provider,
    cancel_pipeline_run,
    clear_all_caches,
    clear_all_run_history_caches,
    clear_pipelines_cache,
    clear_run_history_cache,
    clear_workflow_params_cache,
    fetch_pipelines,
    fetch_run_history,
    flush_pipeline_metrics,
    get_available_plugins,
    get_cache_stats,
    get_cached_pipelines,
    get_global_metrics_config,
    get_metrics_storage_stats,
    get_pipeline_metrics_config,
    get_provider,
    get_refresh_mode,
    get_workflow_parameters,
    get_workflow_run_details,
    list_providers,
    preview_provider_pipelines,
    query_aggregated_metrics,
    query_pipeline_metrics,
    refresh_all,
    remove_provider,
    set_refresh_mode,
    trigger_pipeline,
    update_global_metrics_config,
    update_pipeline_metrics_config,
    update_provider,
    update_provider_refresh_interval,
    AppState,
};
use application::services::{
    MetricsService,
    PipelineService,
    ProviderService,
};
use application::RefreshManager;
use infrastructure::database::{
    init_database,
    init_metrics_database,
    MetricsRepository,
    Repository,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");

            let db_path = app_data_dir.join("pipedash.db");
            let pool = tauri::async_runtime::block_on(async {
                init_database(db_path)
                    .await
                    .expect("Failed to initialize database")
            });

            let metrics_db_path = app_data_dir.join("metrics.db");
            let metrics_pool = tauri::async_runtime::block_on(async {
                init_metrics_database(metrics_db_path)
                    .await
                    .expect("Failed to initialize metrics database")
            });

            let repository = Arc::new(Repository::new(pool));
            let metrics_repository = Arc::new(MetricsRepository::new(metrics_pool));
            let metrics_service = Arc::new(MetricsService::new(Arc::clone(&metrics_repository)));

            let provider_service = Arc::new(ProviderService::new(Arc::clone(&repository)));
            let pipeline_service = Arc::new(PipelineService::new(
                Arc::clone(&repository),
                Arc::clone(&provider_service),
                Some(Arc::clone(&metrics_service)),
            ));
            let refresh_manager = Arc::new(RefreshManager::new(
                Arc::clone(&pipeline_service),
                Some(Arc::clone(&metrics_service)),
            ));

            let app_state = AppState {
                provider_service: Arc::clone(&provider_service),
                pipeline_service: Arc::clone(&pipeline_service),
                metrics_service: Some(Arc::clone(&metrics_service)),
                refresh_manager: Arc::clone(&refresh_manager),
                app: app.handle().clone(),
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
            get_global_metrics_config,
            update_global_metrics_config,
            get_pipeline_metrics_config,
            update_pipeline_metrics_config,
            query_pipeline_metrics,
            query_aggregated_metrics,
            get_metrics_storage_stats,
            flush_pipeline_metrics,
            get_cache_stats,
            clear_pipelines_cache,
            clear_all_run_history_caches,
            clear_workflow_params_cache,
            clear_all_caches,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
