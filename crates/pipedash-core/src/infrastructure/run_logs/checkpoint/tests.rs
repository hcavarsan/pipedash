use super::*;

#[test]
fn copied_prefix_can_be_truncated_and_extended_without_changing_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("base");
    let to = dir.path().join("resumed");
    fs::write(&from, b"prefix-stale-tail").unwrap();

    let mut file = copy_prefix(&from, &to, 6).unwrap();
    file.write_all(b"-new-tail").unwrap();
    file.sync_all().unwrap();

    assert_eq!(fs::read(&to).unwrap(), b"prefix-new-tail");
    assert_eq!(fs::read(&from).unwrap(), b"prefix-stale-tail");
}
