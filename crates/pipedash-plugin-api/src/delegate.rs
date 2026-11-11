#[macro_export]
macro_rules! plugin_struct {
    ($name:ident, $client:ty, $meta:expr $(; $($field:ident: $fty:ty = $init:expr),+ $(,)?)?) => {
        $crate::plugin_struct!(@define $name, $meta;
            client: Option<$client> = None,
            config: ::std::collections::HashMap<String, String> = ::std::collections::HashMap::new()
            $(, $($field: $fty = $init),+)?
        );

        impl $name {
            fn client(&self) -> $crate::PluginResult<&$client> {
                self.client.as_ref().ok_or_else(|| {
                    $crate::PluginError::Internal("Plugin not initialized".to_string())
                })
            }
        }
    };
    (custom $name:ident, $meta:expr; $($field:ident: $fty:ty = $init:expr),+ $(,)?) => {
        $crate::plugin_struct!(@define $name, $meta; $($field: $fty = $init),+);
    };
    (@define $name:ident, $meta:expr; $($field:ident: $fty:ty = $init:expr),*) => {
        pub struct $name {
            metadata: $crate::PluginMetadata,
            $($field: $fty,)*
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    metadata: $meta,
                    $($field: $init,)*
                }
            }
        }
    };
}

#[macro_export]
macro_rules! plugin_impl {
    ($ty:ident [$($methods:tt)*] { $($items:tt)* }) => {
        $crate::plugin_impl!(@munch $ty { $($items)* } [$($methods)*] []);
    };
    (@munch $ty:ident { $($items:tt)* } [] [$($out:tt)*]) => {
        #[::async_trait::async_trait]
        impl $crate::Plugin for $ty {
            $($items)*
            $($out)*
        }
    };
    (@munch $ty:ident $items:tt [metadata $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            fn metadata(&self) -> &$crate::PluginMetadata {
                &self.metadata
            }
        ]);
    };
    (@munch $ty:ident $items:tt [status_source $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            fn status_source(&self) -> Option<&$crate::StatusSource> {
                self.status_source.as_ref()
            }
        ]);
    };
    (@munch $ty:ident $items:tt [available_from_filtered $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_available_pipelines(
                &self, params: Option<$crate::PaginationParams>,
            ) -> $crate::PluginResult<$crate::PaginatedAvailablePipelines> {
                self.fetch_available_pipelines_filtered(None, None, params).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_available_pipelines $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_available_pipelines(
                &self, params: Option<$crate::PaginationParams>,
            ) -> $crate::PluginResult<$crate::PaginatedAvailablePipelines> {
                self.fetch_available_pipelines_impl(params).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_available_pipelines_filtered $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_available_pipelines_filtered(
                &self, org: Option<String>, search: Option<String>,
                params: Option<$crate::PaginationParams>,
            ) -> $crate::PluginResult<$crate::PaginatedAvailablePipelines> {
                self.fetch_available_pipelines_filtered_impl(org, search, params).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_organizations $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_organizations(&self) -> $crate::PluginResult<Vec<$crate::Organization>> {
                self.fetch_organizations_impl().await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [ids_with_ci $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn ids_with_ci(&self, ids: Vec<String>) -> $crate::PluginResult<Vec<String>> {
                self.ids_with_ci_impl(ids).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_pipelines $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_pipelines(&self) -> $crate::PluginResult<Vec<$crate::Pipeline>> {
                self.fetch_pipelines_impl().await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_run_history $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_run_history(
                &self, pipeline_id: &str, limit: usize,
            ) -> $crate::PluginResult<Vec<$crate::PipelineRun>> {
                self.fetch_run_history_impl(pipeline_id, limit).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_run_details $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_run_details(
                &self, pipeline_id: &str, run_number: i64,
            ) -> $crate::PluginResult<$crate::PipelineRun> {
                self.fetch_run_details_impl(pipeline_id, run_number).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_recent_runs $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_recent_runs(
                &self, pipeline_ids: &[String], limit: usize,
            ) -> $crate::PluginResult<
                ::std::collections::HashMap<String, Vec<$crate::PipelineRun>>,
            > {
                self.fetch_recent_runs_impl(pipeline_ids, limit).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_log_nodes $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_log_nodes(
                &self, run: $crate::RunRef<'_>,
            ) -> $crate::PluginResult<Vec<$crate::LogNode>> {
                self.fetch_log_nodes_impl(run).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [read_log $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn read_log(
                &self, run: $crate::RunRef<'_>, node_id: &str, from: Option<&$crate::LogCursor>,
                sink: &mut $crate::LogSink,
            ) -> $crate::PluginResult<$crate::LogRead> {
                self.read_log_impl(run, node_id, from, sink).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [fetch_workflow_parameters $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn fetch_workflow_parameters(
                &self, workflow_id: &str,
            ) -> $crate::PluginResult<Vec<$crate::WorkflowParameter>> {
                self.fetch_workflow_parameters_impl(workflow_id).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [trigger_pipeline $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn trigger_pipeline(
                &self, params: $crate::TriggerParams,
            ) -> $crate::PluginResult<$crate::TriggerResult> {
                self.trigger_pipeline_impl(params).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [cancel_run $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn cancel_run(
                &self, pipeline_id: &str, run_number: i64,
            ) -> $crate::PluginResult<()> {
                self.cancel_run_impl(pipeline_id, run_number).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [force_cancel_run $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn force_cancel_run(
                &self, pipeline_id: &str, run_number: i64,
            ) -> $crate::PluginResult<()> {
                self.force_cancel_run_impl(pipeline_id, run_number).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [review_run $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn review_run(
                &self, pipeline_id: &str, run_number: i64, decision: $crate::ReviewDecision,
            ) -> $crate::PluginResult<()> {
                self.review_run_impl(pipeline_id, run_number, decision).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [rerun_run $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn rerun_run(
                &self, pipeline_id: &str, run_number: i64,
            ) -> $crate::PluginResult<()> {
                self.rerun(pipeline_id, run_number, false).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [rerun_failed_jobs $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn rerun_failed_jobs(
                &self, pipeline_id: &str, run_number: i64,
            ) -> $crate::PluginResult<()> {
                self.rerun(pipeline_id, run_number, true).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [enable_pipeline $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn enable_pipeline(&self, pipeline_id: &str) -> $crate::PluginResult<()> {
                self.enable_pipeline_impl(pipeline_id).await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [check_permissions $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn check_permissions(&self) -> $crate::PluginResult<$crate::PermissionStatus> {
                self.check_permissions_impl().await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [check_health $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn check_health(
                &self,
            ) -> $crate::PluginResult<Vec<$crate::ComponentHealth>> {
                self.check_health_impl().await
            }
        ]);
    };
    (@munch $ty:ident $items:tt [get_feature_availability $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            fn get_feature_availability(
                &self, status: &$crate::PermissionStatus,
            ) -> Vec<$crate::FeatureAvailability> {
                self.get_feature_availability_impl(status)
            }
        ]);
    };
    (@munch $ty:ident $items:tt [get_field_options $(, $($rest:tt)*)?] [$($out:tt)*]) => {
        $crate::plugin_impl!(@munch $ty $items [$($($rest)*)?] [$($out)*
            async fn get_field_options(
                &self, field_key: &str, config: &::std::collections::HashMap<String, String>,
            ) -> $crate::PluginResult<Vec<String>> {
                self.get_field_options_impl(field_key, config).await
            }
        ]);
    };
}

#[cfg(test)]
mod tests;
