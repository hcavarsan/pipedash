use super::*;

const MANIFEST: &str = "[package]\nname = \"pipedash-plugins\"\n\n[dependencies]\npipedash-plugin-api.workspace = true\npipedash-plugin-alpha = { version = \"1.0.0\", path = \"../pipedash-plugin-alpha\" }\npipedash-plugin-gamma = { version = \"1.0.0\", path = \"../pipedash-plugin-gamma\" }\n\n[lints]\nworkspace = true\n";

#[test]
fn dependency_is_inserted_in_sorted_position() {
    let updated = insert_dependency(MANIFEST, "pipedash-plugin-beta", "1.0.0").unwrap();

    let lines: Vec<&str> = updated.lines().collect();
    assert_eq!(
        lines[5],
        "pipedash-plugin-alpha = { version = \"1.0.0\", path = \"../pipedash-plugin-alpha\" }"
    );
    assert_eq!(
        lines[6],
        "pipedash-plugin-beta = { version = \"1.0.0\", path = \"../pipedash-plugin-beta\" }"
    );
    assert_eq!(
        lines[7],
        "pipedash-plugin-gamma = { version = \"1.0.0\", path = \"../pipedash-plugin-gamma\" }"
    );
    assert!(updated.ends_with("workspace = true\n"));
}

#[test]
fn dependency_sorting_last_goes_after_the_last_plugin_line() {
    let updated = insert_dependency(MANIFEST, "pipedash-plugin-zeta", "1.0.0").unwrap();

    let lines: Vec<&str> = updated.lines().collect();
    assert!(lines[7].starts_with("pipedash-plugin-zeta = "));
    assert_eq!(lines[8], "");
    assert_eq!(lines[9], "[lints]");
}

#[test]
fn dependency_sorting_first_goes_before_other_plugins_but_after_the_api() {
    let updated = insert_dependency(MANIFEST, "pipedash-plugin-aaa", "1.0.0").unwrap();

    let lines: Vec<&str> = updated.lines().collect();
    assert_eq!(lines[4], "pipedash-plugin-api.workspace = true");
    assert!(lines[5].starts_with("pipedash-plugin-aaa = "));
}

#[test]
fn the_first_plugin_goes_right_after_the_last_dependency() {
    let manifest =
        "[dependencies]\npipedash-plugin-api.workspace = true\n\n[lints]\nworkspace = true\n";

    let updated = insert_dependency(manifest, "pipedash-plugin-beta", "1.0.0").unwrap();

    let lines: Vec<&str> = updated.lines().collect();
    assert_eq!(lines[1], "pipedash-plugin-api.workspace = true");
    assert!(lines[2].starts_with("pipedash-plugin-beta = "));
    assert_eq!(lines[3], "");
}

#[test]
fn duplicate_or_missing_section_is_an_error() {
    assert!(insert_dependency(MANIFEST, "pipedash-plugin-alpha", "1.0.0").is_err());
    assert!(
        insert_dependency("[package]\nname = \"x\"\n", "pipedash-plugin-beta", "1.0.0").is_err()
    );
}

#[test]
fn aliases_are_read_like_the_structure_audit() {
    let manifest = "[package]\nname = \"x\"\n\n[package.metadata.pipedash]\naliases = [\"delta\", \"epsilon\"]\n";

    assert_eq!(declared_aliases(manifest).unwrap(), ["delta", "epsilon"]);
    assert!(declared_aliases("[package]\n").unwrap().is_empty());
}

#[test]
fn workspace_version_comes_from_workspace_package() {
    let manifest = "[workspace]\nmembers = []\n\n[workspace.package]\nauthors = []\nversion = \"0.12.0\"\n\n[dependencies]\nversion = \"9\"\n";

    assert_eq!(workspace_version(manifest).unwrap(), "0.12.0");
    assert!(workspace_version("[workspace.package]\n\n[dependencies]\nversion = \"9\"\n").is_err());
}

fn repo_with_manifest() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join(PLUGINS_MANIFEST);
    fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    fs::write(&manifest, MANIFEST).unwrap();
    let repo = Repo {
        root: dir.path().to_path_buf(),
        taken: BTreeSet::new(),
        version: "1.0.0".to_string(),
    };
    (dir, repo)
}

fn spec(crate_name: &str) -> Spec {
    let args = crate::args::Args {
        id: crate_name.strip_prefix(PROVIDER_PREFIX).map(str::to_string),
        description: Some("x".to_string()),
        ..crate::args::Args::default()
    };
    crate::spec::from_args(&args, &BTreeSet::new()).unwrap()
}

#[test]
fn apply_writes_files_and_registers_the_crate_then_rollback_restores() {
    let (_dir, repo) = repo_with_manifest();
    let spec = spec("pipedash-plugin-beta");
    let files = vec![("src/lib.rs".to_string(), "x".to_string())];

    let original = repo.apply(&spec, &files).unwrap();

    assert_eq!(original, MANIFEST);
    assert_eq!(
        fs::read_to_string(repo.crate_dir(&spec).join("src/lib.rs")).unwrap(),
        "x"
    );
    let manifest = fs::read_to_string(repo.root.join(PLUGINS_MANIFEST)).unwrap();
    assert!(manifest.contains("pipedash-plugin-beta = "));

    repo.rollback(&spec, &original);
    assert!(!repo.crate_dir(&spec).exists());
    assert_eq!(
        fs::read_to_string(repo.root.join(PLUGINS_MANIFEST)).unwrap(),
        MANIFEST
    );
}

#[test]
fn apply_refuses_an_existing_crate_without_writing() {
    let (_dir, repo) = repo_with_manifest();
    let spec = spec("pipedash-plugin-beta");
    fs::create_dir_all(repo.crate_dir(&spec)).unwrap();

    let error = repo
        .apply(&spec, &[("a".to_string(), "b".to_string())])
        .unwrap_err();

    assert_eq!(error, "crates/pipedash-plugin-beta already exists");
    assert_eq!(
        fs::read_to_string(repo.root.join(PLUGINS_MANIFEST)).unwrap(),
        MANIFEST
    );
}
