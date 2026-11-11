use std::thread;

use super::*;

fn vault(dir: &Path, key: u8) -> FileVault {
    FileVault::new(dir.to_path_buf(), [key; 32])
}

#[test]
fn values_round_trip_and_missing_accounts_are_empty() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path(), 1);
    assert_eq!(vault.get("token").unwrap(), None);
    vault.set("token", "secret").unwrap();
    assert_eq!(vault.get("token").unwrap().as_deref(), Some("secret"));
    vault.set("token", "replaced").unwrap();
    assert_eq!(vault.get("token").unwrap().as_deref(), Some("replaced"));
    vault.remove("token").unwrap();
    assert_eq!(vault.get("token").unwrap(), None);
}

#[test]
fn a_wrong_key_is_rejected_as_invalid_data() {
    let dir = tempfile::tempdir().unwrap();
    vault(dir.path(), 1).set("token", "secret").unwrap();
    let error = vault(dir.path(), 2).get("token").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn a_flipped_byte_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path(), 1);
    vault.set("token", "secret").unwrap();
    let path = dir.path().join("token.bin");
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&path, bytes).unwrap();
    assert_eq!(
        vault.get("token").unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn a_value_moved_to_another_account_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path(), 1);
    vault.set("one", "secret").unwrap();
    fs::copy(dir.path().join("one.bin"), dir.path().join("two.bin")).unwrap();
    assert_eq!(
        vault.get("two").unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn every_change_bumps_the_generation() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path(), 1);
    assert_eq!(vault.generation().unwrap(), 0);
    vault.set("token", "a").unwrap();
    assert_eq!(vault.generation().unwrap(), 1);
    vault.set("token", "b").unwrap();
    assert_eq!(vault.generation().unwrap(), 2);
    vault.remove("token").unwrap();
    assert_eq!(vault.generation().unwrap(), 3);
    vault.remove("token").unwrap();
    assert_eq!(vault.generation().unwrap(), 3);
}

#[test]
fn a_stale_generation_does_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path(), 1);
    vault.set("token", "a").unwrap();
    let seen = vault.generation().unwrap();
    vault.set("token", "b").unwrap();
    assert!(!vault.set_if_generation("token", "stale", seen).unwrap());
    assert_eq!(vault.get("token").unwrap().as_deref(), Some("b"));
    let current = vault.generation().unwrap();
    assert!(vault.set_if_generation("token", "c", current).unwrap());
    assert_eq!(vault.get("token").unwrap().as_deref(), Some("c"));
}

#[test]
fn concurrent_compare_and_set_loses_no_update() {
    let dir = tempfile::tempdir().unwrap();
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let path = dir.path().to_path_buf();
            thread::spawn(move || {
                let vault = FileVault::new(path, [7; 32]);
                for _ in 0..50 {
                    loop {
                        let generation = vault.generation().unwrap();
                        let current: u64 = vault
                            .get("counter")
                            .unwrap()
                            .map_or(0, |text| text.parse().unwrap());
                        if vault
                            .set_if_generation("counter", &(current + 1).to_string(), generation)
                            .unwrap()
                        {
                            break;
                        }
                    }
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let vault = FileVault::new(dir.path().to_path_buf(), [7; 32]);
    assert_eq!(vault.get("counter").unwrap().as_deref(), Some("400"));
}

#[test]
fn an_exclusive_session_holds_off_stale_compare_and_set() {
    let dir = tempfile::tempdir().unwrap();
    vault(dir.path(), 3).set("state", "start").unwrap();
    let seen = vault(dir.path(), 3).generation().unwrap();
    let session = vault(dir.path(), 3).exclusive().unwrap();
    let path = dir.path().to_path_buf();
    let contender = thread::spawn(move || {
        FileVault::new(path, [3; 32]).set_if_generation("state", "background", seen)
    });
    thread::sleep(std::time::Duration::from_millis(100));
    assert!(!contender.is_finished());
    assert_eq!(session.get("state").unwrap().as_deref(), Some("start"));
    session.set("state", "app").unwrap();
    drop(session);
    assert!(!contender.join().unwrap().unwrap());
    assert_eq!(
        vault(dir.path(), 3).get("state").unwrap().as_deref(),
        Some("app")
    );
}
