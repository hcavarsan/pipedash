use std::path::Path;
use std::process::Command;

fn main() {
    let dist = Path::new("dist");
    let repo_root = Path::new("../..");
    let in_repo = repo_root.join("package.json").exists();
    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");
    let skip = std::env::var("SKIP_FRONTEND_BUILD").is_ok();

    if !in_repo {
        assert!(
            dist.join("index.html").exists(),
            "dist/ is missing: this package was built without frontend assets"
        );
        return;
    }

    println!("cargo:rerun-if-env-changed=SKIP_FRONTEND_BUILD");
    println!("cargo:rerun-if-changed=../../src");
    println!("cargo:rerun-if-changed=../../package.json");
    println!("cargo:rerun-if-changed=../../vite.config.ts");

    if skip || !release {
        ensure_dist_exists(dist);
        return;
    }

    eprintln!("Building frontend...");
    let status = Command::new("bunx")
        .args(["vite", "build"])
        .current_dir(repo_root)
        .status()
        .expect("bun not found. Install: curl -fsSL https://bun.sh/install | bash");
    assert!(status.success(), "Frontend build failed");
    assert!(
        dist.join("index.html").exists(),
        "dist/ not found after build"
    );
}

fn ensure_dist_exists(dist: &Path) {
    if dist.join("index.html").exists() {
        return;
    }
    std::fs::create_dir_all(dist).expect("Failed to create dist/");
    std::fs::write(
        dist.join("index.html"),
        "<!DOCTYPE html><html><body><h1>Frontend not built</h1><p>Run: bun run build</p></body></html>",
    )
    .expect("Failed to write placeholder");
}
