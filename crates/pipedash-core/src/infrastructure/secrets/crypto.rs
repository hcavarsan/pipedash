use std::collections::HashMap;

use aes_gcm::aead::{
    Aead,
    KeyInit,
};
use aes_gcm::{
    Aes256Gcm,
    Nonce,
};
use argon2::{
    Algorithm,
    Argon2,
    ParamsBuilder,
    Version,
};

use crate::domain::{
    DomainError,
    DomainResult,
};

pub(super) const KDF_MEMORY_KIB: u32 = 65536;
pub(super) const KDF_ITERATIONS: u32 = 3;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

pub(super) fn random_salt() -> Vec<u8> {
    rand::random::<[u8; SALT_LEN]>().to_vec()
}

fn derive_key(
    password: &str, salt: &[u8], memory_kib: u32, iterations: u32,
) -> DomainResult<[u8; 32]> {
    let params = ParamsBuilder::new()
        .m_cost(memory_kib)
        .t_cost(iterations)
        .p_cost(1)
        .output_len(32)
        .build()
        .map_err(|e| DomainError::InternalError(format!("Invalid Argon2 parameters: {e}")))?;

    let mut output = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut output)
        .map_err(|e| DomainError::InternalError(format!("Failed to derive key: {e}")))?;

    Ok(output)
}

pub(super) async fn derive_key_blocking(
    password: &str, salt: Vec<u8>, memory_kib: u32, iterations: u32,
) -> DomainResult<[u8; 32]> {
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || derive_key(&password, &salt, memory_kib, iterations))
        .await
        .map_err(|e| DomainError::InternalError(format!("Key derivation task failed: {e}")))?
}

fn cipher(key: &[u8; 32]) -> DomainResult<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key)
        .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {e}")))
}

pub(super) fn encrypt(
    key: &[u8; 32], plaintext: &[u8],
) -> DomainResult<([u8; NONCE_LEN], Vec<u8>)> {
    let nonce_bytes: [u8; NONCE_LEN] = rand::random();
    let ciphertext = cipher(key)?
        .encrypt(&Nonce::from(nonce_bytes), plaintext)
        .map_err(|e| DomainError::InternalError(format!("Encryption failed: {e}")))?;
    Ok((nonce_bytes, ciphertext))
}

pub(super) fn decrypt(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> DomainResult<Vec<u8>> {
    let nonce: [u8; NONCE_LEN] = nonce
        .try_into()
        .map_err(|_| DomainError::InvalidConfig("Invalid nonce length".into()))?;
    cipher(key)?
        .decrypt(&Nonce::from(nonce), ciphertext)
        .map_err(|_| {
            DomainError::AuthenticationFailed("Decryption failed (wrong password?)".into())
        })
}

pub fn seal_backup(tokens: &HashMap<i64, String>, password: &str) -> DomainResult<Vec<u8>> {
    let json = serde_json::to_vec(tokens)
        .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize tokens: {e}")))?;
    let salt = random_salt();
    let key = derive_key(password, &salt, KDF_MEMORY_KIB, KDF_ITERATIONS)?;
    let (nonce, ciphertext) = encrypt(&key, &json)?;
    let mut result = salt;
    result.extend(nonce);
    result.extend(ciphertext);
    Ok(result)
}

pub fn open_backup(data: &[u8], password: &str) -> DomainResult<HashMap<i64, String>> {
    let Some((salt, rest)) = data.split_at_checked(SALT_LEN) else {
        return Err(DomainError::InvalidConfig("Invalid backup data".into()));
    };
    let Some((nonce, ciphertext)) = rest.split_at_checked(NONCE_LEN) else {
        return Err(DomainError::InvalidConfig("Invalid backup data".into()));
    };
    let key = derive_key(password, salt, KDF_MEMORY_KIB, KDF_ITERATIONS)?;
    let plaintext = decrypt(&key, nonce, ciphertext)?;
    serde_json::from_slice(&plaintext)
        .map_err(|e| DomainError::DatabaseError(format!("Failed to parse tokens: {e}")))
}
