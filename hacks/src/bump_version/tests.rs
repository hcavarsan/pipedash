use super::*;

#[test]
fn bump_types_follow_semver() {
    assert_eq!(next_version("0.12.3", "patch").unwrap(), "0.12.4");
    assert_eq!(next_version("0.12.3", "minor").unwrap(), "0.13.0");
    assert_eq!(next_version("0.12.3", "major").unwrap(), "1.0.0");
    assert!(next_version("0.12.3-beta.1", "patch").is_err());
    assert!(next_version("0.12", "patch").is_err());
}

#[test]
fn workspace_version_changes_only_under_workspace_package() {
    let manifest = "[workspace]\nmembers = []\n\n[workspace.package]\nauthors = []\nversion = \"0.12.0\"\n\n[workspace.dependencies]\nversion_check = \"0.9\"\n";

    let (current, next, updated) = bump_workspace_version(manifest, "minor").unwrap();

    assert_eq!((current.as_str(), next.as_str()), ("0.12.0", "0.13.0"));
    assert_eq!(
        updated,
        manifest.replace("version = \"0.12.0\"", "version = \"0.13.0\"")
    );
    assert!(bump_workspace_version("[package]\nversion = \"1.0.0\"\n", "patch").is_err());
}

#[test]
fn internal_pins_move_and_external_dependencies_stay() {
    let manifest = r#"[dependencies]
pipedash-core = { version = "0.12.0", path = "crates/pipedash-core" }
pipedash-web = { path = "crates/pipedash-web", version = "0.11.0", default-features = false }
pipedash-plugin-api.workspace = true
tauri-plugin-pipedash-push = { version = "0.12.0", path = "../tauri-plugin-pipedash-push" }
octocrab = { version = "0.12.0", default-features = false }
"#;

    let updated = pin_internal_versions(manifest, "0.13.0");

    assert_eq!(
        updated,
        r#"[dependencies]
pipedash-core = { version = "0.13.0", path = "crates/pipedash-core" }
pipedash-web = { path = "crates/pipedash-web", version = "0.13.0", default-features = false }
pipedash-plugin-api.workspace = true
tauri-plugin-pipedash-push = { version = "0.13.0", path = "../tauri-plugin-pipedash-push" }
octocrab = { version = "0.12.0", default-features = false }
"#
    );
}

#[test]
fn json_version_keeps_formatting_and_rejects_mismatches() {
    let conf = "{\n  \"build\": { \"plugin\": { \"version\": \"0.12.0\" } },\n  \"version\": \"0.12.0\"\n}";
    let package = "{\n  \"name\": \"pipedash\",\n  \"version\": \"0.12.0\",\n  \"dependencies\": { \"x\": \"0.12.0\" }\n}\n";

    assert_eq!(
        set_json_version(package, "0.12.0", "0.13.0").unwrap(),
        package.replacen("\"0.12.0\"", "\"0.13.0\"", 1)
    );
    assert!(set_json_version(package, "0.11.0", "0.13.0").is_err());
    // A nested "version" on its own line before the top-level one must not be
    // taken for it.
    let nested_first =
        "{\n  \"plugin\": {\n    \"version\": \"0.12.0\"\n  },\n  \"version\": \"0.12.0\"\n}";
    assert!(set_json_version(nested_first, "0.12.0", "0.13.0").is_err());
    assert!(set_json_version(conf, "0.12.0", "0.13.0")
        .unwrap()
        .ends_with("\"version\": \"0.13.0\"\n}"));
}

#[test]
fn sparse_index_paths() {
    assert_eq!(index_path("a"), "1/a");
    assert_eq!(index_path("ab"), "2/ab");
    assert_eq!(index_path("abc"), "3/a/abc");
    assert_eq!(index_path("Pipedash-Core"), "pi/pe/pipedash-core");
}
