use std::collections::BTreeSet;
use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use regex::Regex;

use crate::spec::Spec;

const PROVIDER_PREFIX: &str = "pipedash-plugin-";
pub(crate) const PLUGINS_MANIFEST: &str = "crates/pipedash-plugins/Cargo.toml";

pub(crate) struct Repo {
    pub root: PathBuf,
    pub taken: BTreeSet<String>,
    pub version: String,
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

pub(crate) fn declared_aliases(manifest: &str) -> Result<Vec<String>, String> {
    let list = Regex::new(r"(?m)^\s*aliases\s*=\s*\[([^\]]*)\]").map_err(|e| e.to_string())?;
    let quoted = Regex::new(r#""([^"]+)""#).map_err(|e| e.to_string())?;
    let Some(declared) = list.captures(manifest) else {
        return Ok(Vec::new());
    };
    Ok(quoted
        .captures_iter(&declared[1])
        .map(|alias| alias[1].to_string())
        .collect())
}

pub(crate) fn workspace_version(manifest: &str) -> Result<String, String> {
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[workspace.package]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| {
            let value = line.trim().strip_prefix("version")?.trim_start();
            Some(
                value
                    .strip_prefix('=')?
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            )
        })
        .ok_or_else(|| "no version in [workspace.package] of Cargo.toml".to_string())
}

fn taken_names(crates: &Path) -> Result<BTreeSet<String>, String> {
    let entries = fs::read_dir(crates).map_err(|error| format!("cannot list crates: {error}"))?;
    let mut taken = BTreeSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_prefix(PROVIDER_PREFIX) else {
            continue;
        };
        taken.insert(id.to_string());
        let lib = fs::read_to_string(entry.path().join("src/lib.rs")).unwrap_or_default();
        if lib.contains("register_plugin!") {
            taken.extend(declared_aliases(&read(&entry.path().join("Cargo.toml"))?)?);
        }
    }
    Ok(taken)
}

impl Repo {
    pub(crate) fn discover() -> Result<Self, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("hacks has no parent directory")?
            .to_path_buf();
        if !root.join(PLUGINS_MANIFEST).is_file() {
            return Err(format!(
                "run from the pipedash repository ({PLUGINS_MANIFEST} not found)"
            ));
        }
        Ok(Self {
            taken: taken_names(&root.join("crates"))?,
            version: workspace_version(&read(&root.join("Cargo.toml"))?)?,
            root,
        })
    }

    pub(crate) fn crate_dir(&self, spec: &Spec) -> PathBuf {
        self.root.join("crates").join(&spec.crate_name)
    }

    pub(crate) fn apply(&self, spec: &Spec, files: &[(String, String)]) -> Result<String, String> {
        let crate_dir = self.crate_dir(spec);
        if crate_dir.exists() {
            return Err(format!("crates/{} already exists", spec.crate_name));
        }
        let manifest_path = self.root.join(PLUGINS_MANIFEST);
        let original = read(&manifest_path)?;
        let updated = insert_dependency(&original, &spec.crate_name, &self.version)?;
        let written = write_all(&crate_dir, files)
            .and_then(|()| fs::write(&manifest_path, updated).map_err(|e| e.to_string()));
        if let Err(error) = written {
            self.rollback(spec, &original);
            return Err(format!("writing the crate failed: {error}"));
        }
        Ok(original)
    }

    pub(crate) fn rollback(&self, spec: &Spec, original_manifest: &str) {
        let crate_dir = self.crate_dir(spec);
        match fs::remove_dir_all(&crate_dir) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                eprintln!("warning: could not remove {}: {error}", crate_dir.display());
            }
            _ => {}
        }
        if let Err(error) = fs::write(self.root.join(PLUGINS_MANIFEST), original_manifest) {
            eprintln!("warning: could not restore {PLUGINS_MANIFEST}: {error}");
        }
    }
}

fn write_all(crate_dir: &Path, files: &[(String, String)]) -> Result<(), String> {
    for (relative, content) in files {
        let path = crate_dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn dependency_line(crate_name: &str, version: &str) -> String {
    format!("{crate_name} = {{ version = \"{version}\", path = \"../{crate_name}\" }}")
}

fn dependency_key(line: &str) -> Option<&str> {
    let key = line.split(['=', '.', ' ']).next()?;
    (key.starts_with(PROVIDER_PREFIX) && key != "pipedash-plugin-api").then_some(key)
}

pub(crate) fn insert_dependency(
    manifest: &str, crate_name: &str, version: &str,
) -> Result<String, String> {
    let lines: Vec<&str> = manifest.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim() == "[dependencies]")
        .ok_or("no [dependencies] section in the plugins manifest")?
        + 1;
    let end = lines[start..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + offset);
    let section = &lines[start..end];
    let prefix = |line: &&str| {
        line.starts_with(&format!("{crate_name} ")) || line.starts_with(&format!("{crate_name}."))
    };
    if section.iter().any(prefix) {
        return Err(format!(
            "{crate_name} is already a dependency of pipedash-plugins"
        ));
    }
    let keyed = section
        .iter()
        .enumerate()
        .filter_map(|(index, line)| Some((start + index, dependency_key(line)?)));
    let mut at = None;
    for (index, key) in keyed {
        if key > crate_name {
            at = Some(index);
            break;
        }
        at = Some(index + 1);
    }
    let at = at.unwrap_or_else(|| {
        (start..end)
            .rev()
            .find(|&index| !lines[index].trim().is_empty())
            .map_or(start, |index| index + 1)
    });
    let line = dependency_line(crate_name, version);
    let mut output: Vec<&str> = lines[..at].to_vec();
    output.push(&line);
    output.extend(&lines[at..]);
    Ok(output.join("\n") + if manifest.ends_with('\n') { "\n" } else { "" })
}

#[cfg(test)]
mod tests;
