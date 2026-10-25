use std::path::{
    Path,
    PathBuf,
};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_pipedash_native::PipedashNativeExt;

use crate::commands::ErrorResponse;

const MAX_FILE_NAME_BYTES: usize = 255;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum FileNameError {
    #[error("The file name is empty")]
    Empty,
    #[error("The file name is longer than 255 bytes")]
    TooLong,
    #[error("The file name can't contain path separators")]
    Separator,
    #[error("The file name can't be '.' or '..'")]
    Reserved,
    #[error("The file name can't contain control characters")]
    Control,
}

pub fn validate_file_name(name: &str) -> Result<&str, FileNameError> {
    if name.trim().is_empty() {
        return Err(FileNameError::Empty);
    }
    if name.len() > MAX_FILE_NAME_BYTES {
        return Err(FileNameError::TooLong);
    }
    if name.contains(['/', '\\']) {
        return Err(FileNameError::Separator);
    }
    if name.chars().any(char::is_control) {
        return Err(FileNameError::Control);
    }
    if name == "." || name == ".." {
        return Err(FileNameError::Reserved);
    }
    Ok(name)
}

#[derive(Debug, Serialize)]
pub struct ExportFileResult {
    pub path: String,
    pub shared: bool,
}

fn export_error(message: String) -> ErrorResponse {
    ErrorResponse {
        error: message,
        details: None,
    }
}

const EXPORT_DIR: &str = "pipedash-exports";
const CREATE_ATTEMPTS: usize = 8;

fn create_export_dir(base: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(base)?;
    for _ in 0..CREATE_ATTEMPTS {
        let dir = base.join(format!("{:016x}", rand::random::<u64>()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free export folder",
    ))
}

async fn write_export(base: &Path, name: &str, contents: &str) -> std::io::Result<PathBuf> {
    let dir = create_export_dir(base)?;
    let path = dir.join(name);
    tokio::fs::write(&path, contents).await?;

    Ok(path)
}

#[tauri::command]
pub async fn save_export_file(
    app: AppHandle, file_name: String, contents: String,
) -> Result<ExportFileResult, ErrorResponse> {
    if !cfg!(mobile) {
        return Err(export_error(
            "Sharing exports is only available on mobile".to_string(),
        ));
    }

    let name = validate_file_name(&file_name).map_err(|e| export_error(e.to_string()))?;
    let base = std::env::temp_dir().join(EXPORT_DIR);
    let path = write_export(&base, name, &contents)
        .await
        .map_err(|e| export_error(format!("Couldn't prepare the file: {}", e)))?;

    let shared = app.pipedash_native().share_file(&path).await;
    if let Some(dir) = path.parent() {
        if let Err(e) = tokio::fs::remove_dir_all(dir).await {
            tracing::warn!(error = %e, "Couldn't remove the temporary export");
        }
    }

    Ok(ExportFileResult {
        path: path.display().to_string(),
        shared: shared.map_err(|e| export_error(format!("Couldn't share the file: {}", e)))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_file_names_are_accepted() {
        for name in [
            "pipedash-export.json",
            "metrics 2026-10-24.csv",
            ".hidden",
            "a.b.c",
        ] {
            assert_eq!(validate_file_name(name), Ok(name));
        }
    }

    #[test]
    fn empty_and_blank_names_are_rejected() {
        assert_eq!(validate_file_name(""), Err(FileNameError::Empty));
        assert_eq!(validate_file_name("   "), Err(FileNameError::Empty));
    }

    #[test]
    fn path_separators_and_traversal_are_rejected() {
        for name in ["../x.json", "a/b.json", "/etc/passwd", "a\\b", "dir/"] {
            assert_eq!(
                validate_file_name(name),
                Err(FileNameError::Separator),
                "{name}"
            );
        }
        assert_eq!(validate_file_name(".."), Err(FileNameError::Reserved));
        assert_eq!(validate_file_name("."), Err(FileNameError::Reserved));
    }

    #[test]
    fn control_characters_and_overlong_names_are_rejected() {
        assert_eq!(validate_file_name("a\0b"), Err(FileNameError::Control));
        assert_eq!(validate_file_name("a\nb"), Err(FileNameError::Control));
        assert_eq!(
            validate_file_name(&"x".repeat(MAX_FILE_NAME_BYTES + 1)),
            Err(FileNameError::TooLong)
        );
        assert!(validate_file_name(&"x".repeat(MAX_FILE_NAME_BYTES)).is_ok());
    }

    #[tokio::test]
    async fn every_export_gets_its_own_folder_so_names_never_collide() {
        let base = tempfile::tempdir().unwrap();

        let first = write_export(base.path(), "export.json", "one")
            .await
            .unwrap();
        let second = write_export(base.path(), "export.json", "two")
            .await
            .unwrap();

        assert_ne!(first, second);
        assert_eq!(first.file_name(), second.file_name());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "two");
    }
}
