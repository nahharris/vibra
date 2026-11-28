//! Content digests.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// The SHA-256 of `bytes` as a digest string: `sha256:` and 64 lower-case
/// hexadecimal digits.
pub(crate) fn digest(bytes: &[u8]) -> String {
    let mut text = String::from("sha256:");
    for byte in Sha256::digest(bytes) {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::digest;

    #[test]
    fn the_digest_of_nothing_is_the_empty_hash() {
        assert_eq!(
            digest(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
