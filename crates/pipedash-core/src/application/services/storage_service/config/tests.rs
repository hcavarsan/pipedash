use super::*;
use crate::application::test_support::plugins::registry;

fn content_for(data_dir: &Path) -> String {
    format!(
        "[general]\nstatus_pages_enabled = false\n[storage]\nbackend = \"sqlite\"\ndata_dir = {:?}\n",
        data_dir.to_string_lossy()
    )
}

#[tokio::test]
async fn a_missing_config_file_asks_for_the_setup_wizard() {
    let dir = tempfile::tempdir().unwrap();

    let error = load_config(dir.path()).await.unwrap_err();

    assert!(matches!(error, DomainError::ConfigNotFound));
}

#[tokio::test]
async fn a_broken_config_file_reports_its_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "[storage\n").unwrap();

    let error = load_config(dir.path()).await.unwrap_err();

    assert!(matches!(error, DomainError::InvalidConfigFile { .. }));
    assert!(error.to_string().contains("config.toml"));
}

#[tokio::test]
async fn saving_content_writes_the_original_text_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let content = format!("# kept\n{}", content_for(dir.path()));

    save_config_content(&path, &content, &registry(), None)
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
}

#[tokio::test]
async fn invalid_content_is_rejected_without_touching_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "original").unwrap();

    let error = save_config_content(&path, "[storage\n", &registry(), None)
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::InvalidConfig(_)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
}

#[tokio::test]
async fn a_validation_failure_names_the_field_in_plain_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let content = format!(
        "{}[views.team]\nname = \"\"\nname_contains = \"api\"\n",
        content_for(dir.path())
    );

    let error = save_config_content(&path, &content, &registry(), None)
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("views.team.name: View name is required"),
        "{error}"
    );
    assert!(!error.contains("ConfigError {"), "{error}");
}

#[tokio::test]
async fn config_content_is_read_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "body").unwrap();

    let response = read_config_content(&path).await.unwrap();

    assert_eq!(response.content, "body");
    assert_eq!(response.path, path.display().to_string());
    assert!(read_config_content(&dir.path().join("none.toml"))
        .await
        .is_err());
}

#[tokio::test]
async fn the_effective_data_dir_follows_the_config_or_the_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let custom = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    let without_config = effective_data_dir(&path, dir.path()).await.unwrap();
    std::fs::write(&path, content_for(custom.path())).unwrap();
    let with_config = effective_data_dir(&path, dir.path()).await.unwrap();

    assert_eq!(without_config, dir.path());
    assert_eq!(with_config, custom.path());
}

#[test]
fn storage_paths_hang_off_the_data_dir() {
    let data_dir = Path::new("/d");
    let paths = StoragePathsResponse::new(Path::new("/c/config.toml"), data_dir);
    let under = |name: &str| data_dir.join(name).display().to_string();

    assert_eq!(
        paths.config_file,
        Path::new("/c/config.toml").display().to_string()
    );
    assert_eq!(paths.pipedash_db, under("pipedash.db"));
    assert_eq!(paths.metrics_db, under("metrics.db"));
    assert_eq!(paths.cache_dir, under("cache"));
    assert_eq!(paths.vault_path, under("vault"));
    assert_eq!(paths.data_dir, data_dir.display().to_string());
}
