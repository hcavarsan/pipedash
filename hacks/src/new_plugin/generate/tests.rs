use super::*;
use crate::test_support::{
    manifest,
    spec,
    temp_repo,
    MANIFEST,
};

fn files() -> Vec<(String, String)> {
    vec![("src/lib.rs".to_string(), "x".to_string())]
}

#[test]
fn a_formatting_failure_removes_the_crate_and_restores_the_manifest() {
    let (_dir, repo) = temp_repo();
    let spec = spec("beta");

    let error = generate_with(
        &repo,
        &spec,
        &files(),
        |_, _| Err("fmt failed".to_string()),
        |_| Audit::Passed,
    )
    .unwrap_err();

    assert_eq!(error, "fmt failed");
    assert!(!repo.crate_dir(&spec).exists());
    assert_eq!(manifest(&repo), MANIFEST);
}

#[test]
fn a_new_audit_failure_rolls_back_and_names_the_clashing_id() {
    let (_dir, repo) = temp_repo();
    let spec = spec("beta");
    let runs = std::cell::Cell::new(0);
    let audit = |_: &Repo| {
        runs.set(runs.get() + 1);
        match runs.get() {
            1 => Audit::Passed,
            _ => Audit::Failed("src/x.rs:1: provider-name".to_string()),
        }
    };

    let error = generate_with(&repo, &spec, &files(), |_, _| Ok(()), audit).unwrap_err();

    assert!(
        error.starts_with("src/x.rs:1: provider-name\nbeta clashes"),
        "{error}"
    );
    assert!(!repo.crate_dir(&spec).exists());
    assert_eq!(manifest(&repo), MANIFEST);
}

#[test]
fn an_audit_that_was_already_failing_keeps_the_generated_crate() {
    let (_dir, repo) = temp_repo();
    let spec = spec("beta");

    generate_with(
        &repo,
        &spec,
        &files(),
        |_, _| Ok(()),
        |_| Audit::Failed("old".to_string()),
    )
    .unwrap();

    assert!(repo.crate_dir(&spec).join("src/lib.rs").is_file());
    assert!(manifest(&repo).contains("pipedash-plugin-beta = "));
}
