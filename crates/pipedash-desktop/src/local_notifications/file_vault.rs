use std::fs::{
    self,
    File,
    OpenOptions,
};
use std::io::{
    self,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};

use aes_gcm::{
    aead::{
        Aead,
        KeyInit,
        Payload,
    },
    Aes256Gcm,
    Nonce,
};

const LOCK_FILE: &str = ".lock";
const GENERATION_FILE: &str = "generation";
const NONCE_LEN: usize = 12;

pub(super) struct FileVault {
    dir: PathBuf,
    cipher: Aes256Gcm,
}

pub(super) struct Exclusive {
    vault: FileVault,
    _lock: File,
}

impl Exclusive {
    pub fn get(&self, account: &str) -> io::Result<Option<String>> {
        self.vault.read(account)
    }

    pub fn set(&self, account: &str, value: &str) -> io::Result<()> {
        self.vault.write(account, value)
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)
}

impl FileVault {
    pub fn new(dir: PathBuf, key: [u8; 32]) -> Self {
        Self {
            dir,
            cipher: Aes256Gcm::new(&key.into()),
        }
    }

    pub fn get(&self, account: &str) -> io::Result<Option<String>> {
        let _lock = self.lock()?;
        self.read(account)
    }

    pub fn set(&self, account: &str, value: &str) -> io::Result<()> {
        let _lock = self.lock()?;
        self.write(account, value)
    }

    pub fn remove(&self, account: &str) -> io::Result<()> {
        let _lock = self.lock()?;
        match fs::remove_file(self.path(account)) {
            Ok(()) => self.bump(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn generation(&self) -> io::Result<u64> {
        let _lock = self.lock()?;
        self.current_generation()
    }

    pub fn set_if_generation(&self, account: &str, value: &str, expected: u64) -> io::Result<bool> {
        let _lock = self.lock()?;
        if self.current_generation()? != expected {
            return Ok(false);
        }
        self.write(account, value)?;
        Ok(true)
    }

    pub fn exclusive(self) -> io::Result<Exclusive> {
        let lock = self.lock()?;
        Ok(Exclusive {
            vault: self,
            _lock: lock,
        })
    }

    fn lock(&self) -> io::Result<File> {
        fs::create_dir_all(&self.dir)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.dir.join(LOCK_FILE))?;
        lock.lock()?;
        Ok(lock)
    }

    fn path(&self, account: &str) -> PathBuf {
        self.dir.join(format!("{account}.bin"))
    }

    fn read(&self, account: &str) -> io::Result<Option<String>> {
        let bytes = match fs::read(self.path(account)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        if bytes.len() < NONCE_LEN {
            return Err(invalid("Saved data is truncated"));
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| invalid("Invalid nonce"))?;
        let plaintext = self
            .cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: account.as_bytes(),
                },
            )
            .map_err(|_| invalid("Saved data could not be decrypted"))?;
        String::from_utf8(plaintext)
            .map(Some)
            .map_err(|_| invalid("Saved data is not valid text"))
    }

    fn write(&self, account: &str, value: &str) -> io::Result<()> {
        let nonce: [u8; NONCE_LEN] = rand::random();
        let ciphertext = self
            .cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: value.as_bytes(),
                    aad: account.as_bytes(),
                },
            )
            .map_err(|_| invalid("Data could not be encrypted"))?;
        let mut bytes = nonce.to_vec();
        bytes.extend(ciphertext);
        write_atomic(&self.path(account), &bytes)?;
        self.bump()
    }

    fn current_generation(&self) -> io::Result<u64> {
        match fs::read_to_string(self.dir.join(GENERATION_FILE)) {
            Ok(text) => text
                .trim()
                .parse()
                .map_err(|_| invalid("Generation is not a number")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e),
        }
    }

    fn bump(&self) -> io::Result<()> {
        let next = self.current_generation()?.saturating_add(1);
        write_atomic(&self.dir.join(GENERATION_FILE), next.to_string().as_bytes())
    }
}

#[cfg(test)]
mod tests {
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
                                .set_if_generation(
                                    "counter",
                                    &(current + 1).to_string(),
                                    generation,
                                )
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
}
