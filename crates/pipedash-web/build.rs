use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("CARGO_FEATURE_EMBEDDED_FRONTEND").is_none() {
        return Ok(());
    }

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
        return Ok(());
    }

    println!("cargo:rerun-if-env-changed=SKIP_FRONTEND_BUILD");
    println!("cargo:rerun-if-changed=../../src");
    println!("cargo:rerun-if-changed=../../package.json");
    println!("cargo:rerun-if-changed=../../vite.config.ts");

    if skip || !release {
        return ensure_dist_exists(dist);
    }

    eprintln!("Building frontend...");
    let status = Command::new("bunx")
        .args(["vite", "build"])
        .current_dir(repo_root)
        .status()
        .map_err(|_| "bun not found. Install: curl -fsSL https://bun.sh/install | bash")?;
    assert!(status.success(), "Frontend build failed");
    assert!(
        dist.join("index.html").exists(),
        "dist/ not found after build"
    );
    Ok(())
}

fn ensure_dist_exists(dist: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if dist.join("index.html").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dist)?;
    std::fs::write(
        dist.join("index.html"),
        "<!DOCTYPE html><html><body><h1>Frontend not built</h1><p>Run: bun run build</p></body></html>",
    )?;
    Ok(())
}
