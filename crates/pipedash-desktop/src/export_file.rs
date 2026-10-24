use serde::Serialize;
use tauri::{
    AppHandle,
    Manager,
};

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
    pub location: &'static str,
}

fn export_error(message: String) -> ErrorResponse {
    ErrorResponse {
        error: message,
        details: None,
    }
}

#[tauri::command]
pub async fn save_export_file(
    app: AppHandle, file_name: String, contents: String,
) -> Result<ExportFileResult, ErrorResponse> {
    if !cfg!(mobile) {
        return Err(export_error(
            "Saving to the Documents folder is only available on mobile".to_string(),
        ));
    }

    let name = validate_file_name(&file_name).map_err(|e| export_error(e.to_string()))?;
    let dir = app
        .path()
        .document_dir()
        .map_err(|e| export_error(format!("Couldn't find the Documents folder: {}", e)))?;

    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| export_error(format!("Couldn't open the Documents folder: {}", e)))?;

    let path = dir.join(name);
    tokio::fs::write(&path, contents)
        .await
        .map_err(|e| export_error(format!("Couldn't save the file: {}", e)))?;

    Ok(ExportFileResult {
        path: path.display().to_string(),
        location: "documents",
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
}
