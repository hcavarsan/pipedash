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

    let first = write_export(base.path(), "export.json", b"one")
        .await
        .unwrap();
    let second = write_export(base.path(), "export.json", b"two")
        .await
        .unwrap();

    assert_ne!(first, second);
    assert_eq!(first.file_name(), second.file_name());
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "one");
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "two");
}

#[tokio::test]
async fn binary_exports_preserve_every_byte() {
    let base = tempfile::tempdir().unwrap();
    let bytes = [0x89, b'P', b'N', b'G', 0, 0xff];
    let path = write_export(base.path(), "chart.png", &bytes)
        .await
        .unwrap();

    assert_eq!(std::fs::read(path).unwrap(), bytes);
}
