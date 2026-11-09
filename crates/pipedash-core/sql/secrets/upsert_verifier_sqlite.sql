INSERT INTO vault_verifier (id, nonce, ciphertext, kdf_salt)
VALUES (1, ?1, ?2, ?3)
ON CONFLICT (id) DO UPDATE SET
    nonce = excluded.nonce,
    ciphertext = excluded.ciphertext,
    kdf_salt = excluded.kdf_salt
WHERE vault_verifier.kdf_salt IS NULL
