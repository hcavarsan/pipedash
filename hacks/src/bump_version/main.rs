use std::{
    env,
    fs,
    path::{
        Path,
        PathBuf,
    },
    process::{
        Child,
        Command,
        ExitCode,
        Stdio,
    },
};

use glob::glob;
use regex::Regex;
use serde_json::Value;

const USAGE: &str = "Usage: bump_version <patch|minor|major>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [bump] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(1);
    };

    match bump_version(bump) {
        Ok(version) => {
            println!("✓ Version bumped to {version}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("✗ Failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn bump_version(bump: &str) -> Result<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let root_manifest = root.join("Cargo.toml");
    let (current, next, root_content) = bump_workspace_version(&read(&root_manifest)?, bump)?;

    let manifests = files(&root, "crates/*/Cargo.toml")?;
    check_published(&manifests)?;

    // Build every edit before writing any, so a mismatch leaves the tree
    // untouched.
    let mut edits = vec![(root_manifest, pin_internal_versions(&root_content, &next))];
    for manifest in manifests {
        let content = pin_internal_versions(&read(&manifest)?, &next);
        edits.push((manifest, content));
    }
    let json_files =
        std::iter::once(root.join("package.json")).chain(files(&root, "crates/*/tauri.conf.json")?);
    for path in json_files {
        let content = set_json_version(&read(&path)?, &current, &next)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        edits.push((path, content));
    }

    for (path, content) in edits {
        if read(&path)? == content {
            continue;
        }
        fs::write(&path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!(
            "  ✓ {}",
            path.strip_prefix(&root).unwrap_or(&path).display()
        );
    }

    // The maintainer wants the full update, not just the workspace crates.
    let status = Command::new("cargo")
        .arg("update")
        .current_dir(&root)
        .status()
        .map_err(|e| format!("cannot run cargo update: {e}"))?;
    if !status.success() {
        return Err(format!("cargo update failed: {status}"));
    }

    Ok(next)
}

/// Bumps `version` under `[workspace.package]`; returns the current and next
/// versions and the new manifest.
fn bump_workspace_version(manifest: &str, bump: &str) -> Result<(String, String, String), String> {
    let mut section = "";
    let mut versions = None;
    let mut content = String::with_capacity(manifest.len());
    for line in manifest.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed;
        }
        let value = trimmed
            .strip_prefix("version")
            .and_then(|rest| rest.trim_start().strip_prefix('='))
            .filter(|_| section == "[workspace.package]" && versions.is_none());
        let Some(value) = value else {
            content.push_str(line);
            continue;
        };
        let current = value.trim().trim_matches('"').to_string();
        let next = next_version(&current, bump)?;
        let newline = if line.ends_with('\n') { "\n" } else { "" };
        content.push_str(&format!("version = \"{next}\"{newline}"));
        versions = Some((current, next));
    }
    let (current, next) = versions.ok_or("no version in [workspace.package] of Cargo.toml")?;
    Ok((current, next, content))
}

fn next_version(current: &str, bump: &str) -> Result<String, String> {
    let parts: Vec<u64> = current
        .split('.')
        .map(|part| {
            part.parse()
                .map_err(|_| format!("unsupported version {current}, expected X.Y.Z"))
        })
        .collect::<Result<_, _>>()?;
    let [major, minor, patch] = parts[..] else {
        return Err(format!("unsupported version {current}, expected X.Y.Z"));
    };
    match bump {
        "patch" => Ok(format!("{major}.{minor}.{}", patch + 1)),
        "minor" => Ok(format!("{major}.{}.0", minor + 1)),
        "major" => Ok(format!("{}.0.0", major + 1)),
        _ => Err(USAGE.to_string()),
    }
}

/// Points every inline `pipedash-*` dependency that carries a `version` at
/// `next`.
fn pin_internal_versions(manifest: &str, next: &str) -> String {
    let pin = Regex::new(
        r#"(?m)^((?:tauri-plugin-)?pipedash-[\w-]+\s*=\s*\{[^}\n]*\bversion\s*=\s*)"[^"]*""#,
    )
    .expect("valid regex");
    pin.replace_all(manifest, |caps: &regex::Captures| {
        format!(r#"{}"{next}""#, &caps[1])
    })
    .into_owned()
}

/// Rewrites only the top-level `"version": "<current>"` line, keeping the
/// file's own formatting.
fn set_json_version(content: &str, current: &str, next: &str) -> Result<String, String> {
    let line = Regex::new(&format!(
        r#"(?m)^(\s*"version"\s*:\s*)"{}""#,
        regex::escape(current)
    ))
    .expect("valid regex");
    if !line.is_match(content) {
        return Err(format!(r#"no "version": "{current}""#));
    }
    let updated = line
        .replacen(content, 1, |caps: &regex::Captures| {
            format!(r#"{}"{next}""#, &caps[1])
        })
        .into_owned();
    let json: Value = serde_json::from_str(&updated).map_err(|e| e.to_string())?;
    if json["version"] != next {
        return Err(format!(
            r#"the first "version": "{current}" is not the top-level one"#
        ));
    }
    Ok(updated)
}

/// release.yml publishes through crates.io trusted publishing, which cannot
/// create a crate, so a publishable crate missing from crates.io would fail the
/// release after the tag.
fn check_published(manifests: &[PathBuf]) -> Result<(), String> {
    let publish_false = Regex::new(r"(?m)^publish\s*=\s*false").expect("valid regex");
    let name = Regex::new(r#"(?m)^name\s*=\s*"([^"]+)""#).expect("valid regex");

    let mut lookups: Vec<(String, Child)> = Vec::new();
    for manifest in manifests {
        let content = read(manifest)?;
        if publish_false.is_match(&content) {
            continue;
        }
        let crate_name = name
            .captures(&content)
            .ok_or_else(|| format!("no package name in {}", manifest.display()))?[1]
            .to_string();
        let child = Command::new("curl")
            .args([
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "--max-time",
                "30",
            ])
            .arg(format!(
                "https://index.crates.io/{}",
                index_path(&crate_name)
            ))
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run curl: {e}"))?;
        lookups.push((crate_name, child));
    }

    let mut missing = Vec::new();
    for (crate_name, child) in lookups {
        let output = child
            .wait_with_output()
            .map_err(|e| format!("curl failed: {e}"))?;
        match String::from_utf8_lossy(&output.stdout).trim() {
            "200" => {}
            "404" => missing.push(crate_name),
            code => {
                return Err(format!(
                    "cannot check {crate_name} on crates.io (HTTP {code:?})"
                ))
            }
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "not on crates.io yet: {}. Trusted publishing in release.yml cannot create a crate: publish each \
         once by hand (`cargo publish -p <crate>`), add the trusted publisher hcavarsan/pipedash \
         release.yml on crates.io, then bump again",
        missing.join(", ")
    ))
}

/// Path of a crate in the crates.io sparse index.
fn index_path(crate_name: &str) -> String {
    let name = crate_name.to_lowercase();
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

fn files(root: &Path, pattern: &str) -> Result<Vec<PathBuf>, String> {
    glob(&root.join(pattern).to_string_lossy())
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map_err(|e| e.to_string()))
        .collect()
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

#[cfg(test)]
mod tests;
