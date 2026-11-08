mod args;
mod prompt;
mod render;
mod repo;
mod spec;

use std::io::IsTerminal;
use std::process::{
    Command,
    ExitCode,
};

use clap::Parser;

use crate::args::Args;
use crate::repo::{
    Repo,
    PLUGINS_MANIFEST,
};
use crate::spec::Spec;

enum Audit {
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

fn generate(repo: &Repo, spec: &Spec, files: &[(String, String)]) -> Result<(), String> {
    let baseline = audit(repo);
    let original = repo.apply(spec, files)?;
    if let Err(error) = format_crate(repo, spec) {
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

fn run(args: &Args) -> Result<(), String> {
    let repo = Repo::discover()?;
    let interactive =
        !args.no_input && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let spec = if interactive {
        prompt::fill(args, &repo.taken)?
    } else {
        spec::from_args(args, &repo.taken)?
    };
    let files = render::files(&spec)?;
    if args.dry_run {
        for (path, _) in &files {
            println!("would create crates/{}/{path}", spec.crate_name);
        }
        let line = repo::dependency_line(&spec.crate_name, &repo.version);
        println!("would add to {PLUGINS_MANIFEST}: {line}");
        return Ok(());
    }
    generate(&repo, &spec, &files)?;
    println!(
        "created crates/{} ({} files) and registered it in {PLUGINS_MANIFEST}",
        spec.crate_name,
        files.len()
    );
    println!("next steps:");
    println!("  cargo test -p {} -p pipedash-plugins", spec.crate_name);
    println!("  grep -rn 'todo!' crates/{}/src", spec.crate_name);
    Ok(())
}

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
