use std::{
    env,
    fs,
    io,
    process::{Command, ExitCode},
};

use glob::glob;
use regex::Regex;
use serde_json::Value;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 || !matches!(args[1].as_str(), "patch" | "minor" | "major") {
        eprintln!("Usage: {} <patch|minor|major>", args[0]);
        return ExitCode::from(1);
    }

    match bump_version(&args[1]) {
        Ok(version) => {
            println!("✓ Version bumped to {}", version);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("✗ Failed: {}", e);
            ExitCode::from(1)
        }
    }
}

fn bump_version(bump_type: &str) -> io::Result<String> {
    let root = env::current_dir()?.join("..");

    let output = Command::new("npm")
        .args(["version", bump_type, "--no-git-tag-version"])
        .current_dir(&root)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .strip_prefix('v')
        .unwrap_or("")
        .to_string();

    update_workspace_version(&root.join("Cargo.toml"), &version)?;

    // Dynamically find and update all tauri.conf.json files
    let pattern = root.join("**/tauri.conf.json");
    let pattern_str = pattern.to_string_lossy();
    for entry in glob(&pattern_str).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))? {
        match entry {
            Ok(path) => {
                println!("  ✓ {}", path.strip_prefix(&root).unwrap_or(&path).display());
                update_json_version(&path, &version)?;
            }
            Err(e) => eprintln!("  ⚠ Warning: {}", e),
        }
    }

    Ok(version)
}

fn update_workspace_version(path: &std::path::Path, version: &str) -> io::Result<()> {
    let content = fs::read_to_string(path)?;

    // Update [workspace.package] version
    let workspace_re = Regex::new(r"(?m)^\[workspace\.package\][\s\S]*?^version = .+$")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let updated = workspace_re.replace(&content, |caps: &regex::Captures| {
        caps[0].replacen(
            &caps[0].lines().last().unwrap(),
            &format!(r#"version = "{}""#, version),
            1,
        )
    });

    // Update all pipedash-* workspace dependencies
    // Matches: pipedash-xxx = { version = "x.y.z", path = "..." }
    let deps_re = Regex::new(r#"(pipedash-[\w-]+\s*=\s*\{\s*version\s*=\s*)"[^"]+""#)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let updated = deps_re.replace_all(&updated, |caps: &regex::Captures| {
        format!(r#"{}"{}""#, &caps[1], version)
    });

    fs::write(path, updated.as_ref())
}

fn update_json_version(path: &std::path::Path, version: &str) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let mut json: Value =
        serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    json["version"] = Value::String(version.to_string());

    fs::write(path, serde_json::to_string_pretty(&json)?)
}
