use std::io::Write;

use super::*;

fn taken() -> BTreeSet<String> {
    ["api", "actions", "alpha", "beta"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn args(id: &str, description: &str) -> Args {
    Args {
        id: Some(id.to_string()),
        description: Some(description.to_string()),
        ..Args::default()
    }
}

#[test]
fn ids_must_be_kebab_case_and_free() {
    for id in ["gamma", "my-ci", "ci2", "a-1-b"] {
        assert_eq!(validate_id(id, &taken()), Ok(()), "{id}");
    }
    for id in ["", "Gamma", "my_ci", "-ci", "ci-", "my--ci", "2ci", "my ci"] {
        assert_eq!(
            validate_id(id, &taken()),
            Err("use lowercase kebab-case, e.g. my-ci".to_string()),
            "{id:?}"
        );
    }
    assert_eq!(
        validate_id("api", &taken()),
        Err("api is already a provider id, alias or plugin crate".to_string())
    );
}

#[test]
fn aliases_must_be_valid_free_distinct_and_unique() {
    let list = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        validate_aliases(&list(&["delta", "epsilon"]), "gamma", &taken()),
        Ok(())
    );
    assert!(validate_aliases(&list(&["Delta"]), "gamma", &taken()).is_err());
    assert!(validate_aliases(&list(&["beta"]), "gamma", &taken()).is_err());
    assert!(validate_aliases(&list(&["gamma"]), "gamma", &taken()).is_err());
    assert!(validate_aliases(&list(&["delta", "delta"]), "gamma", &taken()).is_err());
}

#[test]
fn text_rejects_blank_and_literal_breaking_characters() {
    assert_eq!(validate_text("name", "Gamma CI"), Ok(()));
    for value in ["", "  ", "a\"b", "a\\b", "a{b", "a}b", "a\nb"] {
        assert!(validate_text("name", value).is_err(), "{value:?}");
    }
}

#[test]
fn token_prefixes_and_ping_paths_are_checked() {
    assert_eq!(validate_token_prefixes(&["gm_".to_string()]), Ok(()));
    for prefix in ["", "a b", "a\"", "a\\"] {
        assert!(
            validate_token_prefixes(&[prefix.to_string()]).is_err(),
            "{prefix:?}"
        );
    }
    assert_eq!(validate_ping_path("/api/v4/user"), Ok(()));
    for path in ["api/user", "/a b", "/a\"", "/api/\\user"] {
        assert!(validate_ping_path(path).is_err(), "{path:?}");
    }
}

#[test]
fn icons_must_be_readable_svg_files() {
    let mut svg = tempfile::NamedTempFile::new().unwrap();
    write!(svg, "<svg viewBox=\"0 0 1 1\"/>").unwrap();
    assert_eq!(
        validate_icon(svg.path()),
        Ok("<svg viewBox=\"0 0 1 1\"/>".to_string())
    );

    let mut text = tempfile::NamedTempFile::new().unwrap();
    write!(text, "hello").unwrap();
    assert!(validate_icon(text.path()).is_err());
    assert!(validate_icon(Path::new("/nonexistent/icon.svg")).is_err());
}

#[test]
fn names_derive_from_the_id() {
    assert_eq!(title_case("my-ci"), "My Ci");
    assert_eq!(pascal_case("my-ci"), "MyCi");
    assert_eq!(pascal_case("gamma"), "Gamma");
}

#[test]
fn from_args_lists_only_the_missing_required_flags() {
    let error = from_args(&Args::default(), &taken()).unwrap_err();
    assert!(
        error.starts_with("missing required flags: --id, --description ("),
        "{error}"
    );

    let only_id = Args {
        id: Some("gamma".to_string()),
        ..Args::default()
    };
    let error = from_args(&only_id, &taken()).unwrap_err();
    assert!(
        error.starts_with("missing required flags: --description ("),
        "{error}"
    );
}

#[test]
fn read_only_disables_every_capability() {
    let spec = from_args(
        &Args {
            read_only: true,
            ..args("gamma", "Monitor Gamma workloads")
        },
        &taken(),
    )
    .unwrap();

    assert_eq!(
        spec.capabilities,
        Capabilities {
            trigger: false,
            cancel: false,
            logs: false
        }
    );
}

#[test]
fn rest_only_flags_are_rejected_for_kubernetes() {
    let error = from_args(
        &Args {
            archetype: Some(Archetype::Kubernetes),
            ping_path: Some("/x".to_string()),
            ..args("gamma", "x")
        },
        &taken(),
    )
    .unwrap_err();

    assert_eq!(error, "--ping-path only applies to --archetype rest");
}

#[test]
fn from_args_runs_every_validator() {
    let bad = [
        Args {
            id: Some("Bad_Id".to_string()),
            ..args("", "x")
        },
        Args {
            aliases: Some(vec!["alpha".to_string()]),
            ..args("gamma", "x")
        },
        Args {
            name: Some("a\"b".to_string()),
            ..args("gamma", "x")
        },
        Args {
            token_prefixes: Some(vec!["a b".to_string()]),
            ..args("gamma", "x")
        },
        Args {
            ping_path: Some("x".to_string()),
            ..args("gamma", "x")
        },
        Args {
            icon: Some("/nonexistent/icon.svg".into()),
            ..args("gamma", "x")
        },
        args("gamma", ""),
    ];
    for args in bad {
        assert!(from_args(&args, &taken()).is_err());
    }
}
