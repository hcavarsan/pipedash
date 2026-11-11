use pipedash_plugin_api::{
    Permission,
    PermissionStatus,
};
use tracing::debug;

use super::{
    PermissionChecker,
    ScopeSet,
};

impl PermissionChecker {
    const SCOPE_HIERARCHIES: &'static [(&'static str, &'static [&'static str])] = &[
        ("admin:org", &["admin:org"]),
        ("write:org", &["write:org", "admin:org"]),
        ("read:org", &["read:org", "write:org", "admin:org"]),
        ("admin:repo_hook", &["admin:repo_hook"]),
        ("write:repo_hook", &["write:repo_hook", "admin:repo_hook"]),
        (
            "read:repo_hook",
            &["read:repo_hook", "write:repo_hook", "admin:repo_hook"],
        ),
        ("admin:public_key", &["admin:public_key"]),
        (
            "write:public_key",
            &["write:public_key", "admin:public_key"],
        ),
        (
            "read:public_key",
            &["read:public_key", "write:public_key", "admin:public_key"],
        ),
        ("admin:gpg_key", &["admin:gpg_key"]),
        ("write:gpg_key", &["write:gpg_key", "admin:gpg_key"]),
        (
            "read:gpg_key",
            &["read:gpg_key", "write:gpg_key", "admin:gpg_key"],
        ),
        ("repo", &["repo", "public_repo"]),
        ("public_repo", &["public_repo", "repo"]),
        ("workflow", &["workflow"]),
    ];

    fn get_classic_pat_permissions() -> Vec<Permission> {
        vec![
            Permission {
                name: "repo".to_string(),
                description: "Repository access - 'repo' scope for private repositories, or 'public_repo' scope for public repositories only".to_string(),
                required: true,
            },
            Permission {
                name: "workflow".to_string(),
                description: "Trigger workflow dispatches and cancel running workflows".to_string(),
                required: false,
            },
            Permission {
                name: "read:org".to_string(),
                description: "List organizations and their repositories. Without this, only your personal repositories will be available.".to_string(),
                required: false,
            },
        ]
    }

    fn get_fine_grained_permissions() -> Vec<Permission> {
        vec![
            Permission {
                name: "Repository Metadata".to_string(),
                description: "Read repository metadata (name, description, visibility). Required as a base permission to access repository data.".to_string(),
                required: true,
            },
            Permission {
                name: "Organization members and teams (Read)".to_string(),
                description: "List organization members and teams. Required to access organization repositories. Without this, only your personal repositories will be available.".to_string(),
                required: false,
            },
            Permission {
                name: "Actions (Read)".to_string(),
                description: "View workflow runs, logs, and workflow definitions".to_string(),
                required: true,
            },
            Permission {
                name: "Contents (Read)".to_string(),
                description: "Read workflow files and dispatch inputs. Required for the Run and Rerun forms on private repositories.".to_string(),
                required: true,
            },
            Permission {
                name: "Actions (Write)".to_string(),
                description: "Trigger workflow dispatches and cancel running workflows".to_string(),
                required: false,
            },
        ]
    }

    fn get_acceptable_scopes(minimal_permission: &str) -> Vec<String> {
        Self::SCOPE_HIERARCHIES
            .iter()
            .find(|(perm, _)| *perm == minimal_permission)
            .map(|(_, scopes)| scopes.iter().map(|s| s.to_string()).collect())
            .unwrap_or_else(|| vec![minimal_permission.to_string()])
    }

    fn has_permission(minimal_permission: &str, granted_scopes: &ScopeSet) -> bool {
        let acceptable_scopes = Self::get_acceptable_scopes(minimal_permission);
        acceptable_scopes
            .iter()
            .any(|scope| granted_scopes.contains(scope))
    }

    pub(super) fn build_classic_pat_status(granted_scopes: &ScopeSet) -> PermissionStatus {
        Self::build_status(
            "classic_pat",
            &Self::get_classic_pat_permissions(),
            |perm| Self::has_permission(&perm.name, granted_scopes),
        )
    }

    pub(super) fn build_fine_grained_status(capabilities: &ScopeSet) -> PermissionStatus {
        Self::build_status(
            "fine_grained",
            &Self::get_fine_grained_permissions(),
            |perm| capabilities.contains(&perm.name),
        )
    }

    fn build_status(
        token_type: &str, permissions: &[Permission], granted: impl Fn(&Permission) -> bool,
    ) -> PermissionStatus {
        let mut status = PermissionStatus::evaluate(permissions, token_type, |perm| {
            let granted = granted(perm);
            debug!(
                "Permission '{}': {}",
                perm.name,
                if granted { "granted" } else { "denied" }
            );
            granted
        });
        let label = if token_type == "classic_pat" {
            "Classic PAT"
        } else {
            "Fine-grained token"
        };
        status.credential_label = Some(label.to_string());
        status
    }
}
