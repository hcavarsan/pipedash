use std::fs;

use super::write_atomic;

#[test]
fn a_write_replaces_the_file_and_leaves_nothing_next_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "old = true\n").unwrap();

    write_atomic(&path, b"new = true\n").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "new = true\n");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn a_read_only_file_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "old = true\n").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();

    assert!(write_atomic(&path, b"new = true\n").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "old = true\n");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_keeps_its_link_and_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.toml");
    let link = dir.path().join("config.toml");
    fs::write(&real, "old = true\n").unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    write_atomic(&link, b"new = true\n").unwrap();

    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(&real).unwrap(), "new = true\n");
    assert_eq!(
        fs::metadata(&real).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn a_link_to_a_missing_file_creates_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink("real.toml", &link).unwrap();

    write_atomic(&link, b"new = true\n").unwrap();

    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(dir.path().join("real.toml")).unwrap(),
        "new = true\n"
    );
}
