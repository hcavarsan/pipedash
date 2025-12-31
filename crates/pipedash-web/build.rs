use std::path::Path;
use std::process::Command;

fn main() {
    let dist_path = Path::new("../../dist");
    let project_root = Path::new("../../");
    let skip_frontend_build = std::env::var("SKIP_FRONTEND_BUILD").is_ok();

    if skip_frontend_build {
        ensure_dist_exists(dist_path);
        return;
    }

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    if profile == "debug" {
        ensure_dist_exists(dist_path);
        return;
    }

    eprintln!("Building frontend...");

    let bun_check = Command::new("bun").arg("--version").output();
    if bun_check.is_err() {
        panic!("bun not found. Install: curl -fsSL https://bun.sh/install | bash");
    }

    let status = Command::new("bunx")
        .arg("vite")
        .arg("build")
        .current_dir(project_root)
        .status()
        .expect("Failed to execute frontend build");

    if !status.success() {
        panic!("Frontend build failed");
    }

    if !dist_path.exists() {
        panic!("dist/ not found after build");
    }

    println!("cargo:rerun-if-changed=../../src");
    println!("cargo:rerun-if-changed=../../package.json");
    println!("cargo:rerun-if-changed=../../vite.config.ts");
}

fn ensure_dist_exists(dist_path: &Path) {
    if !dist_path.exists() {
        std::fs::create_dir_all(dist_path).expect("Failed to create dist/");
        let index_html = dist_path.join("index.html");
        std::fs::write(&index_html, "<!DOCTYPE html><html><body><h1>Frontend not built</h1><p>Run: bun run build</p></body></html>")
            .expect("Failed to write placeholder");
    }
}
