use std::future::Future;

use tempfile::TempDir;

use super::*;
use crate::domain::setup::{
    is_postgres_url,
    PASSWORD_LENGTH_ERROR,
    PASSWORD_MISMATCH_ERROR,
    POSTGRES_URL_ERROR,
};
use crate::test_env::EnvGuard;

fn draft() -> SetupDraft {
    SetupDraft {
        backend: "sqlite".into(),
        data_dir: String::new(),
        postgres_url: String::new(),
        vault_password: String::new(),
        vault_password_confirm: String::new(),
        transfer_data: true,
    }
}

fn keys(errors: &std::collections::BTreeMap<String, String>) -> Vec<&str> {
    errors.keys().map(String::as_str).collect()
}

fn run<F: Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new()
        .map(|rt| rt.block_on(future))
        .unwrap_or_else(|_| unreachable!("runtime creation failed"))
}

fn env_guard(data_dir: &TempDir, vault: Option<&str>) -> EnvGuard {
    EnvGuard::set(&[
        ("PIPEDASH_DATA_DIR", data_dir.path().to_str()),
        ("PIPEDASH_VAULT_PASSWORD", vault),
    ])
}

#[test]
fn postgres_urls_need_scheme_and_host() {
    assert!(is_postgres_url("postgres://user@localhost:5432/db"));
    assert!(is_postgres_url("  postgresql://localhost/db  "));
    assert!(!is_postgres_url("nope"));
    assert!(!is_postgres_url("postgres://"));
    assert!(!is_postgres_url("mysql://localhost/db"));
}

#[test]
fn postgres_draft_without_valid_url_is_rejected_even_without_vault_password() {
    let errors = SetupDraft {
        backend: "postgres".into(),
        postgres_url: "nope".into(),
        ..draft()
    }
    .validate(false);

    assert_eq!(keys(&errors), ["postgres_url"]);
    assert_eq!(errors["postgres_url"], POSTGRES_URL_ERROR);
}

#[test]
fn postgres_url_is_ignored_for_sqlite() {
    let errors = SetupDraft {
        postgres_url: "nope".into(),
        ..draft()
    }
    .validate(false);

    assert!(errors.is_empty());
}

#[test]
fn required_vault_password_needs_length_and_confirmation() {
    let short = SetupDraft {
        vault_password: "short".into(),
        vault_password_confirm: "short".into(),
        ..draft()
    }
    .validate(true);
    assert_eq!(keys(&short), ["vault_password"]);
    assert_eq!(short["vault_password"], PASSWORD_LENGTH_ERROR);

    let mismatch = SetupDraft {
        vault_password: "long enough".into(),
        vault_password_confirm: "different".into(),
        ..draft()
    }
    .validate(true);
    assert_eq!(keys(&mismatch), ["vault_password_confirm"]);
    assert_eq!(mismatch["vault_password_confirm"], PASSWORD_MISMATCH_ERROR);

    let valid = SetupDraft {
        vault_password: "long enough".into(),
        vault_password_confirm: "long enough".into(),
        ..draft()
    }
    .validate(true);
    assert!(valid.is_empty());
}

#[test]
fn vault_password_is_not_validated_when_not_needed() {
    let errors = SetupDraft {
        vault_password: "x".into(),
        vault_password_confirm: "y".into(),
        ..draft()
    }
    .validate(false);

    assert!(errors.is_empty());
}

#[test]
fn password_warnings_start_at_twelve_characters() {
    let with = |password: &str| SetupDraft {
        vault_password: password.into(),
        ..draft()
    };

    assert_eq!(with("short11chars").password_warnings().len(), 3);
    assert!(with("elevenchars").password_warnings().is_empty());
    assert_eq!(
        with("abcdefghijkl").password_warnings(),
        [
            "Consider using 16+ characters",
            "Consider adding numbers",
            "Consider adding special characters",
            "Consider mixing uppercase and lowercase",
        ]
    );
    assert!(with("Abcdefghijkl1234!").password_warnings().is_empty());
}

#[test]
fn initial_draft_uses_the_default_directory_on_first_run() {
    let initial = SetupDraft::initial(None, "/default");

    assert_eq!(initial.backend, "sqlite");
    assert_eq!(initial.data_dir, "/default");
    assert!(initial.postgres_url.is_empty());
    assert!(initial.transfer_data);
}

#[test]
fn initial_draft_reflects_the_current_config() {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = "/current".into();
    config.storage.backend = StorageBackendType::Postgres;
    config.storage.postgres.connection_string = "postgres://localhost/db".into();

    let initial = SetupDraft::initial(Some(&config), "/default");

    assert_eq!(initial.backend, "postgres");
    assert_eq!(initial.data_dir, "/current");
    assert_eq!(initial.postgres_url, "postgres://localhost/db");
}

#[test]
fn debug_output_hides_passwords() {
    let secret = SetupDraft {
        vault_password: "hunter2-secret".into(),
        vault_password_confirm: "hunter2-secret".into(),
        ..draft()
    };

    assert!(!format!("{secret:?}").contains("hunter2"));
}

#[test]
fn prepare_requires_a_vault_password_only_without_one_in_the_environment() {
    let dir = TempDir::new().unwrap_or_else(|_| unreachable!());
    let service = SetupService::new(dir.path().join("config.toml"), None, false);
    let short = SetupDraft {
        vault_password: "x".into(),
        vault_password_confirm: "x".into(),
        ..draft()
    };

    {
        let _env = env_guard(&dir, None);
        let result = run(service.prepare(&short)).unwrap_or_else(|_| unreachable!());
        if !is_mobile() {
            assert_eq!(keys(&result.errors), ["vault_password"]);
        }
        assert!(!result.needs_migration);
    }
    {
        let _env = env_guard(&dir, Some("x"));
        let result = run(service.prepare(&short)).unwrap_or_else(|_| unreachable!());
        assert!(result.errors.is_empty());
    }
}

#[test]
fn apply_creates_the_config_for_a_fresh_setup() {
    let dir = TempDir::new().unwrap_or_else(|_| unreachable!());
    let config_path = dir.path().join("nested").join("config.toml");
    let _env = env_guard(&dir, Some("x"));
    let service = SetupService::new(config_path.clone(), None, false);
    let body = SetupDraft {
        data_dir: dir.path().to_string_lossy().into_owned(),
        vault_password: "x".into(),
        vault_password_confirm: "x".into(),
        ..draft()
    };

    let applied = run(service.apply(body)).unwrap_or_else(|_| unreachable!());
    let saved = ConfigLoader::load(&config_path).unwrap_or_else(|_| unreachable!());

    assert!(applied.saved);
    assert!(!applied.restart_required);
    assert!(applied.migration_stats.is_none());
    assert!(saved.storage.data_dir.is_empty());
    assert!(saved.storage.backend.is_sqlite());
}

#[test]
fn apply_rejects_an_invalid_draft_with_the_first_error() {
    let dir = TempDir::new().unwrap_or_else(|_| unreachable!());
    let config_path = dir.path().join("config.toml");
    let _env = env_guard(&dir, Some("x"));
    let service = SetupService::new(config_path.clone(), None, false);
    let body = SetupDraft {
        backend: "postgres".into(),
        postgres_url: "nope".into(),
        ..draft()
    };

    let error = run(service.apply(body)).err();

    assert!(matches!(
        error,
        Some(DomainError::InvalidConfig(message)) if message == POSTGRES_URL_ERROR
    ));
    assert!(!config_path.exists());
}

#[test]
fn apply_rejects_an_unknown_backend() {
    let dir = TempDir::new().unwrap_or_else(|_| unreachable!());
    let _env = env_guard(&dir, Some("x"));
    let service = SetupService::new(dir.path().join("config.toml"), None, false);
    let body = SetupDraft {
        backend: "mysql".into(),
        ..draft()
    };

    assert!(matches!(
        run(service.apply(body)).err(),
        Some(DomainError::InvalidConfig(_))
    ));
}

#[test]
fn reconfiguring_keeps_general_settings_and_asks_for_a_restart() {
    let dir = TempDir::new().unwrap_or_else(|_| unreachable!());
    let config_path = dir.path().join("config.toml");
    let _env = env_guard(&dir, Some("x"));
    let mut current = PipedashConfig::default();
    current.general.default_refresh_interval = 77;
    ConfigLoader::save(&current, &config_path).unwrap_or_else(|_| unreachable!());
    let service = SetupService::new(config_path.clone(), None, false);
    let other_dir = dir.path().join("other");
    let body = SetupDraft {
        data_dir: other_dir.to_string_lossy().into_owned(),
        transfer_data: false,
        ..draft()
    };

    let prepared = run(service.prepare(&body)).unwrap_or_else(|_| unreachable!());
    assert!(prepared.needs_migration);

    let applied = run(service.apply(body)).unwrap_or_else(|_| unreachable!());
    let saved = ConfigLoader::load(&config_path).unwrap_or_else(|_| unreachable!());

    assert!(applied.restart_required);
    assert!(applied.migration_stats.is_none());
    assert_eq!(saved.general.default_refresh_interval, 77);
    assert_eq!(saved.storage.data_dir, other_dir.to_string_lossy());
}
