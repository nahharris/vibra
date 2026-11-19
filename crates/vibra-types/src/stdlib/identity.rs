//! Declaration identities (`docs/spec/02-type-system.md`, "Declared types").

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// The identity of the declaration whose logical anchor is the package
/// `package` at `version`, the module path atom `module` (with its `@`), the
/// dotted `owner` path within the module (empty for a top-level declaration),
/// the kind atom `kind` (with its `@`), and the name `name`.
///
/// The identity is `sha256:` followed by the lower-case hexadecimal SHA-256
/// of the text `vibraDeclarationIdentityV2`, one zero byte, and the six
/// strings, each preceded by its length as a big-endian 64-bit integer.
#[must_use]
pub fn declaration_identity(
    package: &str,
    version: &str,
    module: &str,
    owner: &str,
    kind: &str,
    name: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"vibraDeclarationIdentityV2");
    hasher.update([0u8]);
    for part in [package, version, module, owner, kind, name] {
        let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
        hasher.update(length.to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("sha256:{}", hex(&hasher.finalize()))
}

/// The SHA-256 of `bytes` as a digest string: `sha256:` and 64 lower-case
/// hexadecimal digits.
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{declaration_identity, digest};

    #[test]
    fn the_chapter_test_vector_holds() {
        assert_eq!(
            declaration_identity("hello", "0.1.0", "@hello.main", "", "@proc", "main"),
            "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
        );
    }

    #[test]
    fn the_digest_of_nothing_is_the_empty_hash() {
        assert_eq!(
            digest(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
