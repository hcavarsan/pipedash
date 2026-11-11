#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{
    Path,
    PathBuf,
};
use std::process::Command;

const MANIFEST: &str = r#"[package]
name = "dist-placeholder-fixture"
version = "0.0.0"
edition = "2021"

[features]
embedded-frontend = []

[workspace]
"#;

fn checkout(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    let package = repo.join("crates").join("pipedash-web");
    fs::create_dir_all(package.join("src")).unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("package.json"), "{}").unwrap();
    fs::write(repo.join("vite.config.ts"), "").unwrap();
    fs::write(package.join("Cargo.toml"), MANIFEST).unwrap();
    fs::write(package.join("src").join("lib.rs"), "").unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"),
        package.join("build.rs"),
    )
    .unwrap();
    package
}

fn check(package: &Path, target: &Path) {
    let output = Command::new(env!("CARGO"))
        .args(["check", "--offline", "--features", "embedded-frontend"])
        .current_dir(package)
        .env("CARGO_TARGET_DIR", target)
        .env_remove("SKIP_FRONTEND_BUILD")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo check failed in {}: {}",
        package.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn second_checkout_sharing_a_target_dir_gets_its_dist_placeholder() {
    let root = std::env::temp_dir().join(format!("pipedash-dist-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let first = checkout(&root, "first");
    let second = checkout(&root, "second");
    let target = root.join("target");

    check(&first, &target);
    check(&second, &target);
    check(&second, &target);

    let placeholder = second.join("dist").join("index.html");
    let written = placeholder.exists();
    let _ = fs::remove_dir_all(&root);
    assert!(written, "{} was not written", placeholder.display());
}
