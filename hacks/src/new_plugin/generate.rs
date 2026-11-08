use std::process::Command;

use crate::repo::Repo;
use crate::spec::Spec;

pub(crate) enum Audit {
    Passed,
    Failed(String),
    Skipped,
}

fn audit(repo: &Repo) -> Audit {
    let Ok(output) = Command::new("python3")
        .arg("scripts/audit_structure.py")
        .current_dir(&repo.root)
        .output()
    else {
        return Audit::Skipped;
    };
    if output.status.success() {
        return Audit::Passed;
    }
    Audit::Failed(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn format_crate(repo: &Repo, spec: &Spec) -> Result<(), String> {
    let output = Command::new("cargo")
        .args(["fmt", "-p", &spec.crate_name])
        .current_dir(&repo.root)
        .output()
        .map_err(|error| format!("cannot run cargo fmt: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    Err("generated code failed to format; please report this template bug".to_string())
}

pub(crate) fn generate(repo: &Repo, spec: &Spec, files: &[(String, String)]) -> Result<(), String> {
    generate_with(repo, spec, files, format_crate, audit)
}

fn generate_with(
    repo: &Repo, spec: &Spec, files: &[(String, String)],
    format: impl FnOnce(&Repo, &Spec) -> Result<(), String>, audit: impl Fn(&Repo) -> Audit,
) -> Result<(), String> {
    let baseline = audit(repo);
    let original = repo.apply(spec, files)?;
    if let Err(error) = format(repo, spec) {
        repo.rollback(spec, &original);
        return Err(error);
    }
    match (baseline, audit(repo)) {
        (Audit::Passed, Audit::Failed(report)) => {
            repo.rollback(spec, &original);
            let names = std::iter::once(spec.id.as_str())
                .chain(spec.aliases.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" or ");
            Err(format!(
                "{report}\n{names} clashes with names used outside provider crates; choose another id or alias"
            ))
        }
        (Audit::Passed, _) => Ok(()),
        _ => {
            eprintln!(
                "warning: lint:structure was already failing (or python3 is missing); name-collision check skipped"
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
