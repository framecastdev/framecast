//! Cryptographic utilities shared across Framecast crates
//!
//! Provides key hashing and verification using SHA-256 with random salts
//! and constant-time comparison to prevent timing attacks.

use sha2::{Digest, Sha256};

/// Verify an API key against a stored hash using constant-time comparison.
///
/// The stored hash format is `hex(salt):hex(sha256(key || salt))`.
pub fn verify_key_hash(candidate_key: &str, stored_hash: &str) -> bool {
    // Parse stored hash: salt:hash
    let parts: Vec<&str> = stored_hash.split(':').collect();
    if parts.len() != 2 {
        return false;
    }

    let salt = match hex::decode(parts[0]) {
        Ok(salt) => salt,
        Err(_) => return false,
    };

    let hash = match hex::decode(parts[1]) {
        Ok(hash) => hash,
        Err(_) => return false,
    };

    // Compute hash of candidate key with stored salt
    let mut hasher = Sha256::new();
    hasher.update(candidate_key.as_bytes());
    hasher.update(&salt);
    let candidate_hash = hasher.finalize();

    // Constant-time comparison to prevent timing attacks
    if hash.len() != candidate_hash.len() {
        return false;
    }

    let mut result = 0u8;
    for (a, b) in hash.iter().zip(candidate_hash.iter()) {
        result |= a ^ b;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_key_hash_valid() {
        // Create a known hash: sha256("test_key" || salt)
        let key = "test_key";
        let salt = b"test_salt_value_";
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.update(salt);
        let hash = hasher.finalize();
        let stored = format!("{}:{}", hex::encode(salt), hex::encode(hash));

        assert!(verify_key_hash(key, &stored));
    }

    #[test]
    fn test_verify_key_hash_wrong_key() {
        let key = "test_key";
        let salt = b"test_salt_value_";
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.update(salt);
        let hash = hasher.finalize();
        let stored = format!("{}:{}", hex::encode(salt), hex::encode(hash));

        assert!(!verify_key_hash("wrong_key", &stored));
    }

    #[test]
    fn test_verify_key_hash_malformed_no_colon() {
        assert!(!verify_key_hash("key", "nocolonshere"));
    }

    #[test]
    fn test_verify_key_hash_malformed_invalid_hex_salt() {
        assert!(!verify_key_hash("key", "zzzz:abcd"));
    }

    #[test]
    fn test_verify_key_hash_malformed_invalid_hex_hash() {
        assert!(!verify_key_hash("key", "abcd:zzzz"));
    }

    #[test]
    fn test_verify_key_hash_empty_key() {
        let key = "";
        let salt = b"salt";
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.update(salt);
        let hash = hasher.finalize();
        let stored = format!("{}:{}", hex::encode(salt), hex::encode(hash));

        assert!(verify_key_hash(key, &stored));
        assert!(!verify_key_hash("notempty", &stored));
    }
}
