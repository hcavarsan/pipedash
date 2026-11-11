use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    collect_controller_health,
    ComponentHealth,
    PermissionCheck,
    PermissionStatus,
    PluginError,
    PluginResult,
};

use super::{
    FluxcdPlugin,
    CONCURRENCY,
};
use crate::{
    config,
    metadata::PERMISSIONS,
};

impl FluxcdPlugin {
    pub(super) fn scopes(&self) -> PluginResult<Vec<Option<String>>> {
        let namespaces = self.settings()?.monitored_namespaces();

        Ok(if namespaces.is_empty() {
            vec![None]
        } else {
            namespaces.into_iter().map(Some).collect()
        })
    }

    pub(super) async fn check_permissions_impl(&self) -> PluginResult<PermissionStatus> {
        let client = self.client().await?;
        let scopes = self.scopes()?;
        let flux_scope = [Some(self.settings()?.flux_namespace.clone())];
        let cluster_scope: [Option<String>; 1] = [None];
        let mut checks = Vec::new();
        for (index, spec) in PERMISSIONS.iter().enumerate() {
            let (spec_scopes, names): (&[Option<String>], Vec<Option<&str>>) =
                if !spec.names.is_empty() {
                    (
                        flux_scope.as_slice(),
                        spec.names.iter().copied().map(Some).collect(),
                    )
                } else if spec.cluster_wide {
                    (cluster_scope.as_slice(), vec![None])
                } else {
                    (scopes.as_slice(), vec![None])
                };
            for resource in spec.resources {
                for verb in spec.verbs {
                    for scope in spec_scopes {
                        for name in &names {
                            checks.push((index, spec, *resource, *verb, scope.as_deref(), *name));
                        }
                    }
                }
            }
        }
        let mut reviews = Vec::with_capacity(checks.len());
        for batch in checks.chunks(CONCURRENCY) {
            let results = futures::future::try_join_all(batch.iter().map(
                |(index, spec, resource, verb, scope, name)| async move {
                    let allowed = client
                        .can_i(spec.group, resource, *scope, verb, *name)
                        .await?;
                    Ok::<_, PluginError>((*index, allowed))
                },
            ))
            .await?;
            reviews.extend(results);
        }

        let permissions: Vec<PermissionCheck> = PERMISSIONS
            .iter()
            .enumerate()
            .map(|(index, spec)| PermissionCheck {
                permission: spec.permission(),
                granted: reviews
                    .iter()
                    .filter(|(reviewed, _)| *reviewed == index)
                    .all(|(_, allowed)| *allowed),
            })
            .collect();
        let all_granted = permissions
            .iter()
            .filter(|check| check.permission.required)
            .all(|check| check.granted);
        let scope = match self.settings()?.monitored_namespaces().as_slice() {
            [] => "all namespaces".to_string(),
            namespaces => format!("namespaces: {}", namespaces.join(", ")),
        };

        Ok(PermissionStatus {
            permissions,
            all_granted,
            checked_at: Utc::now(),
            metadata: HashMap::from([("scope".to_string(), scope)]),
            credential_label: None,
        })
    }

    pub(super) async fn check_health_impl(&self) -> PluginResult<Vec<ComponentHealth>> {
        let client = self.client().await?;
        let namespace = self.settings()?.flux_namespace.as_str();
        let checks = futures::future::join_all(config::CONTROLLERS.iter().map(|name| async move {
            client
                .deployment(namespace, name)
                .await
                .map(|deployment| deployment.health(name))
        }))
        .await;

        collect_controller_health(checks)
    }
}
