use aes_gcm::{
    aead::{
        Aead,
        KeyInit,
    },
    Aes256Gcm,
    Nonce,
};
use base64::{
    engine::general_purpose::STANDARD as BASE64,
    Engine as _,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};

const CONFIG_ENCRYPTION_SALT: &[u8] = b"pipedash-config-encrypt-v1";

#[derive(Debug, Clone, Default)]
pub struct EncryptedValue {
    pub plaintext: Option<String>,
    pub encrypted_repr: Option<String>,
}

impl EncryptedValue {
    pub fn from_plaintext(value: String) -> Self {
        Self {
            plaintext: if value.is_empty() { None } else { Some(value) },
            encrypted_repr: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            plaintext: None,
            encrypted_repr: None,
        }
    }

    pub fn is_set(&self) -> bool {
        self.plaintext.is_some() || self.encrypted_repr.is_some()
    }

    pub fn is_encrypted(&self) -> bool {
        self.encrypted_repr.is_some()
    }

    pub fn get(&self, password: &str) -> Result<Option<String>, String> {
        if let Some(ref plain) = self.plaintext {
            return Ok(Some(plain.clone()));
        }

        if let Some(ref encrypted) = self.encrypted_repr {
            return Self::decrypt_value(encrypted, password).map(Some);
        }

        Ok(None)
    }

    pub fn get_plaintext(&self) -> Option<&str> {
        self.plaintext.as_deref()
    }

    pub fn encrypt(plaintext: &str, password: &str) -> Result<String, String> {
        let key = derive_config_key(password);
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("Failed to create cipher: {}", e))?;

        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failed: {}", e))?;

        Ok(format!(
            "enc:v1:{}:{}",
            BASE64.encode(nonce_bytes),
            BASE64.encode(ciphertext)
        ))
    }

    pub fn encrypt_value(plaintext: &str, password: &str) -> Result<Self, String> {
        let encrypted_repr = Self::encrypt(plaintext, password)?;
        Ok(Self {
            plaintext: None,
            encrypted_repr: Some(encrypted_repr),
        })
    }

    fn decrypt_value(encrypted: &str, password: &str) -> Result<String, String> {
        let parts: Vec<&str> = encrypted.split(':').collect();
        if parts.len() != 4 {
            return Err("Invalid encrypted format: expected 4 parts".into());
        }
        if parts[0] != "enc" {
            return Err("Invalid encrypted format: must start with 'enc'".into());
        }
        if parts[1] != "v1" {
            return Err(format!(
                "Unsupported encryption version: {}. Only v1 is supported.",
                parts[1]
            ));
        }

        let nonce_bytes = BASE64
            .decode(parts[2])
            .map_err(|e| format!("Invalid nonce encoding: {}", e))?;
        let ciphertext = BASE64
            .decode(parts[3])
            .map_err(|e| format!("Invalid ciphertext encoding: {}", e))?;

        if nonce_bytes.len() != 12 {
            return Err(format!(
                "Invalid nonce length: expected 12 bytes, got {}",
                nonce_bytes.len()
            ));
        }

        let key = derive_config_key(password);
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("Failed to create cipher: {}", e))?;

        let nonce_array: [u8; 12] = nonce_bytes
            .try_into()
            .map_err(|_| "Invalid nonce length".to_string())?;
        let nonce = Nonce::from(nonce_array);

        let plaintext = cipher
            .decrypt(&nonce, ciphertext.as_ref())
            .map_err(|_| "Decryption failed - wrong password or corrupted data".to_string())?;

        String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8 in decrypted value: {}", e))
    }
}

fn derive_config_key(password: &str) -> [u8; 32] {
    use argon2::{
        Argon2,
        ParamsBuilder,
    };

    let mut output = [0u8; 32];

    let params = ParamsBuilder::new()
        .m_cost(4096) // 4 MiB memory
        .t_cost(1) // 1 iteration
        .p_cost(1) // 1 thread
        .output_len(32)
        .build()
        .expect("Invalid Argon2 parameters");

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    argon2
        .hash_password_into(password.as_bytes(), CONFIG_ENCRYPTION_SALT, &mut output)
        .expect("Failed to derive config encryption key");

    output
}

impl Serialize for EncryptedValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(ref encrypted) = self.encrypted_repr {
            return serializer.serialize_str(encrypted);
        }
        match &self.plaintext {
            Some(s) => serializer.serialize_str(s),
            None => serializer.serialize_str(""),
        }
    }
}

impl<'de> Deserialize<'de> for EncryptedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        if s.starts_with("enc:v1:") {
            Ok(Self {
                plaintext: None,
                encrypted_repr: Some(s),
            })
        } else {
            Ok(Self {
                plaintext: if s.is_empty() { None } else { Some(s) },
                encrypted_repr: None,
            })
        }
    }
}

pub fn is_encrypted_format(s: &str) -> bool {
    s.starts_with("enc:v1:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let plaintext = "postgres://user:pass@localhost/db";
        let password = "my-secure-password";

        let encrypted = EncryptedValue::encrypt(plaintext, password).unwrap();
        assert!(encrypted.starts_with("enc:v1:"));

        let value = EncryptedValue {
            plaintext: None,
            encrypted_repr: Some(encrypted),
        };

        let decrypted = value.get(password).unwrap().unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_password() {
        let plaintext = "secret-value";
        let encrypted = EncryptedValue::encrypt(plaintext, "correct-password").unwrap();

        let value = EncryptedValue {
            plaintext: None,
            encrypted_repr: Some(encrypted),
        };

        let result = value.get("wrong-password");
        assert!(result.is_err());
    }

    #[test]
    fn test_plaintext_passthrough() {
        let value = EncryptedValue::from_plaintext("plain-value".to_string());
        let result = value.get("any-password").unwrap().unwrap();
        assert_eq!(result, "plain-value");
    }

    #[test]
    fn test_empty_value() {
        let value = EncryptedValue::empty();
        assert!(!value.is_set());
        assert!(value.get("password").unwrap().is_none());
    }

    #[test]
    fn test_serde_plaintext() {
        let value = EncryptedValue::from_plaintext("my-value".to_string());
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, "\"my-value\"");

        let deserialized: EncryptedValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.plaintext.as_deref(), Some("my-value"));
    }

    #[test]
    fn test_serde_encrypted() {
        let encrypted_str = "enc:v1:AAAAAAAAAAAAAAAA:BBBBBBBB";
        let serialized = format!("\"{}\"", encrypted_str);

        let deserialized: EncryptedValue = serde_json::from_str(&serialized).unwrap();
        assert!(deserialized.is_encrypted());
        assert_eq!(deserialized.encrypted_repr.as_deref(), Some(encrypted_str));
    }

    #[test]
    fn test_is_encrypted_format() {
        assert!(is_encrypted_format("enc:v1:nonce:ciphertext"));
        assert!(!is_encrypted_format("plain-value"));
        assert!(!is_encrypted_format("enc:v2:something"));
    }

    #[test]
    fn test_encrypt_value() {
        let value = EncryptedValue::encrypt_value("my-secret", "my-password").unwrap();
        assert!(value.is_encrypted());
        assert!(value.encrypted_repr.is_some());
        assert!(value.plaintext.is_none());

        let decrypted = value.get("my-password").unwrap().unwrap();
        assert_eq!(decrypted, "my-secret");
    }
}
