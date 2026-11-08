mod args;
mod generate;
mod prompt;
mod render;
mod repo;
mod spec;
#[cfg(test)]
mod test_support;

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;

use crate::args::Args;
use crate::generate::generate;
use crate::repo::{
    Repo,
    PLUGINS_MANIFEST,
};

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
