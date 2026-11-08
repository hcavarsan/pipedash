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

pub(crate) fn derive_key(
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

pub(crate) async fn derive_key_blocking(
    password: &str, salt: &'static [u8], memory_kib: u32, iterations: u32,
) -> DomainResult<[u8; 32]> {
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || derive_key(&password, salt, memory_kib, iterations))
        .await
        .map_err(|e| DomainError::InternalError(format!("Key derivation task failed: {e}")))?
}

fn cipher(key: &[u8; 32]) -> DomainResult<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key)
        .map_err(|e| DomainError::InternalError(format!("Failed to create cipher: {e}")))
}

pub(super) fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> DomainResult<([u8; 12], Vec<u8>)> {
    let nonce_bytes: [u8; 12] = rand::random();
    let ciphertext = cipher(key)?
        .encrypt(&Nonce::from(nonce_bytes), plaintext)
        .map_err(|e| DomainError::InternalError(format!("Encryption failed: {e}")))?;
    Ok((nonce_bytes, ciphertext))
}

pub(super) fn decrypt(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> DomainResult<Vec<u8>> {
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| DomainError::InvalidConfig("Invalid nonce length".into()))?;
    cipher(key)?
        .decrypt(&Nonce::from(nonce), ciphertext)
        .map_err(|_| {
            DomainError::AuthenticationFailed("Decryption failed (wrong password?)".into())
        })
}

pub(super) fn encode_backup(
    tokens: &HashMap<i64, String>, key: &[u8; 32],
) -> DomainResult<Vec<u8>> {
    let json = serde_json::to_vec(tokens)
        .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize tokens: {e}")))?;
    let (nonce, ciphertext) = encrypt(key, &json)?;
    let mut result = nonce.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

pub(super) fn decode_backup(data: &[u8], key: &[u8; 32]) -> DomainResult<HashMap<i64, String>> {
    let Some((nonce, ciphertext)) = data.split_at_checked(12) else {
        return Err(DomainError::InvalidConfig("Invalid backup data".into()));
    };
    let plaintext = decrypt(key, nonce, ciphertext)?;
    serde_json::from_slice(&plaintext)
        .map_err(|e| DomainError::DatabaseError(format!("Failed to parse tokens: {e}")))
}
