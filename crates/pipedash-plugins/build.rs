use std::fmt::Write as _;
use std::path::PathBuf;

const SHARED: &[&str] = &["pipedash-plugin-api", "pipedash-plugin-actions"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=Cargo.toml");
    let manifest = std::fs::read_to_string("Cargo.toml")?;
    let mut source = String::from(
        "pub fn registry() -> pipedash_plugin_api::PluginRegistry {\n    let mut registry = pipedash_plugin_api::PluginRegistry::new();\n",
    );
    for name in provider_dependencies(&manifest) {
        writeln!(
            source,
            "    {}::register(&mut registry);",
            name.replace('-', "_")
        )?;
    }
    source.push_str("    registry\n}\n");
    let out = PathBuf::from(std::env::var("OUT_DIR")?).join("registry.rs");
    std::fs::write(out, source)?;
    Ok(())
}

fn provider_dependencies(manifest: &str) -> Vec<&str> {
    let mut in_dependencies = false;
    let mut names = Vec::new();
    for line in manifest.lines().map(str::trim) {
        let name = if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']');
            in_dependencies = header == "dependencies";
            match header.strip_prefix("dependencies.") {
                Some(name) => name.trim_matches('"'),
                None => continue,
            }
        } else if in_dependencies {
            line.split(['=', '.']).next().unwrap_or_default().trim()
        } else {
            continue;
        };
        if name.starts_with("pipedash-plugin-") && !SHARED.contains(&name) {
            names.push(name);
        }
    }
    names.sort_unstable();
    names
}
