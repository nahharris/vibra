//! Shared fixtures: the embedded inputs with one module or the manifest
//! replaced, and a manifest rebuilt for them.

use sha2::{Digest, Sha256};
use vibra_types::{StdlibError, StdlibInputs, load_stdlib_from};

/// The embedded source of the module `path`, as text.
pub(crate) fn embedded_source(path: &str) -> String {
    StdlibInputs::embedded()
        .modules
        .iter()
        .find(|(module, _)| *module == path)
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

/// The embedded manifest, as text.
pub(crate) fn embedded_manifest() -> String {
    String::from_utf8_lossy(StdlibInputs::embedded().manifest).into_owned()
}

/// `sha256:` and the lower-case hex SHA-256 of `bytes`.
pub(crate) fn digest(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let hex = hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

/// The embedded manifest with the digest of `path` replaced by the digest of
/// `bytes`.
pub(crate) fn manifest_for(path: &str, bytes: &[u8]) -> String {
    let manifest = embedded_manifest();
    let old = digest(
        StdlibInputs::embedded()
            .modules
            .iter()
            .find(|(module, _)| *module == path)
            .map_or(&[][..], |(_, bytes)| *bytes),
    );
    manifest.replace(&old, &digest(bytes))
}

/// Loads the embedded library with the module `path` replaced by `source`
/// and the manifest digest updated to match, and gives the error.
pub(crate) fn load_with_module(path: &str, source: &str) -> Option<StdlibError> {
    let manifest = manifest_for(path, source.as_bytes());
    let inputs = StdlibInputs::embedded()
        .with_module(path, source.as_bytes())
        .with_manifest(manifest.as_bytes());
    load_stdlib_from(&inputs).err()
}

/// Loads the embedded modules with `manifest`, and gives the error.
pub(crate) fn load_with_manifest(manifest: &str) -> Option<StdlibError> {
    let inputs = StdlibInputs::embedded().with_manifest(manifest.as_bytes());
    load_stdlib_from(&inputs).err()
}

/// The notes of `error`, joined by newlines, or an empty string.
pub(crate) fn notes(error: Option<&StdlibError>) -> String {
    error.map_or_else(String::new, |error| error.diagnostic().notes().join("\n"))
}

/// Replaces exactly one occurrence of `old` in `text` by `new`.
///
/// # Panics
///
/// `old` does not occur exactly once, which makes the fixture wrong.
pub(crate) fn replace_once(text: &str, old: &str, new: &str) -> String {
    assert_eq!(
        text.matches(old).count(),
        1,
        "the fixture needle `{old}` must occur once"
    );
    text.replacen(old, new, 1)
}
