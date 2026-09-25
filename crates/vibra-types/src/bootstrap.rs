//! Verification of the signed M2 standard-library bootstrap.
//!
//! This module performs no I/O. The toolchain embeds the reviewed bootstrap
//! files at build time through [`BootstrapInputs::embedded`], so an installed
//! or relocated binary never depends on the checkout it was built from.
//! [`verify_bootstrap_bytes`] is a pure function over explicit byte inputs;
//! it decodes the manifest and the signed artifact into typed records and
//! compares them structurally before any module is admitted.

use std::fmt;
use std::path::Path;

use base64::Engine;
use ring::signature;
use sha2::{Digest, Sha256};
use vibra_syntax::{DataField, DataNode, DataValue, Literal, NameKind};

/// The canonical path of the signed M2 text bootstrap module.
pub const BOOTSTRAP_TEXT_SOURCE_ID: &str = "stdlib/m2/src/std/text.vib";
/// The canonical path of the signed M2 assertion bootstrap module.
pub const BOOTSTRAP_ASSERT_SOURCE_ID: &str = "stdlib/m2/src/std/assert.vib";

const MANIFEST_PATH: &str = "stdlib/m2/bootstrap-manifest.vibon";
const ARTIFACT_PATH: &str = "stdlib/m2/bootstrap.vibon";
const SIGNATURE_PATH: &str = "stdlib/m2/bootstrap.vibon.sig";
const PUBLIC_KEY_PATH: &str = "stdlib/m2/toolchain-ed25519.pub";

const PACKAGE_NAME: &str = "vibra-stdlib";
const PACKAGE_VERSION: &str = "0.1.0";

const ARTIFACT_SHA256: &str =
    "8dd00d7ecbe068205775cd74a0fdf54ffd32f8c0710da362ab938edee567e103";
const SIGNATURE_SHA256: &str =
    "f6bad514c77cf8dac2dc2309df174cb3f25425c681258db276e240a4af2a5e63";
const PUBLIC_KEY_SHA256: &str =
    "fe5736bd57729053562bf6617fbe0acd1d81f66e9cb930341556c4808f3b1509";
const TEXT_SHA256: &str =
    "c796489f44636b7856c6ce21a12a753f95e59a5204a028e1ce2697afa6a28e44";
const ASSERT_SHA256: &str =
    "746e2f3152caf3f80026531385d7364a8457e5310cbc599e15b7fdbf3bc65007";

/// The DER `SubjectPublicKeyInfo` prefix of every Ed25519 public key
/// (RFC 8410): `SEQUENCE { SEQUENCE { OID 1.3.101.112 } BIT STRING(33) }`.
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

const EMBEDDED_MANIFEST: &[u8] =
    include_bytes!("../../../stdlib/m2/bootstrap-manifest.vibon");
const EMBEDDED_ARTIFACT: &[u8] = include_bytes!("../../../stdlib/m2/bootstrap.vibon");
const EMBEDDED_SIGNATURE: &[u8] =
    include_bytes!("../../../stdlib/m2/bootstrap.vibon.sig");
const EMBEDDED_PUBLIC_KEY: &[u8] =
    include_bytes!("../../../stdlib/m2/toolchain-ed25519.pub");
pub(crate) const EMBEDDED_TEXT_MODULE: &[u8] =
    include_bytes!("../../../stdlib/m2/src/std/text.vib");
const EMBEDDED_ASSERT_MODULE: &[u8] =
    include_bytes!("../../../stdlib/m2/src/std/assert.vib");

/// The exact byte inputs of one bootstrap verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapInputs<'bytes> {
    /// `stdlib/m2/bootstrap-manifest.vibon`.
    pub manifest: &'bytes [u8],
    /// `stdlib/m2/bootstrap.vibon`, the signed artifact.
    pub artifact: &'bytes [u8],
    /// `stdlib/m2/bootstrap.vibon.sig`, a base64 detached Ed25519 signature.
    pub signature: &'bytes [u8],
    /// `stdlib/m2/toolchain-ed25519.pub`, a PEM Ed25519 public key.
    pub public_key: &'bytes [u8],
    /// `stdlib/m2/src/std/text.vib`.
    pub text_module: &'bytes [u8],
    /// `stdlib/m2/src/std/assert.vib`.
    pub assert_module: &'bytes [u8],
}

impl BootstrapInputs<'static> {
    /// The reviewed bootstrap files embedded into the toolchain at build time.
    #[must_use]
    pub const fn embedded() -> Self {
        Self {
            manifest: EMBEDDED_MANIFEST,
            artifact: EMBEDDED_ARTIFACT,
            signature: EMBEDDED_SIGNATURE,
            public_key: EMBEDDED_PUBLIC_KEY,
            text_module: EMBEDDED_TEXT_MODULE,
            assert_module: EMBEDDED_ASSERT_MODULE,
        }
    }
}

/// One entry of the closed bootstrap import map.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModuleEntry {
    atom: String,
    path: String,
    sha256: String,
    role: String,
}

/// The decoded `@stdlib-bootstrap-manifest.v1` record.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BootstrapManifest {
    package_name: String,
    package_version: String,
    artifact: String,
    artifact_sha256: String,
    signature: String,
    signature_encoding: String,
    signature_sha256: String,
    public_key: String,
    public_key_sha256: String,
    modules: Vec<ModuleEntry>,
    compiler: Vec<String>,
    assertions: Vec<String>,
}

/// The decoded `@stdlib-bootstrap.v1` signed artifact record.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BootstrapArtifact {
    package: String,
    modules: Vec<ModuleEntry>,
    compiler: Vec<String>,
    assertions: Vec<String>,
}

/// The result of verifying the signed M2 bootstrap input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapVerification {
    artifact: Vec<u8>,
    package: vibra_resolve::PackageId,
    modules: Vec<ModuleEntry>,
    text_module: Vec<u8>,
    assert_module: Vec<u8>,
}

impl BootstrapVerification {
    /// The exact signed artifact bytes.
    #[must_use]
    pub fn artifact(&self) -> &[u8] {
        &self.artifact
    }

    /// The exact package identity selected by the pinned bootstrap manifest.
    #[must_use]
    pub const fn package(&self) -> &vibra_resolve::PackageId {
        &self.package
    }

    /// Verified source modules for the resolver's separate bootstrap overlay.
    #[must_use]
    pub fn resolver_overlay(
        &self,
    ) -> (vibra_resolve::PackageId, Vec<vibra_resolve::SourceModule>) {
        (
            self.package.clone(),
            vec![
                vibra_resolve::SourceModule::new(
                    "std",
                    ["text"],
                    BOOTSTRAP_TEXT_SOURCE_ID,
                    &self.text_module,
                ),
                vibra_resolve::SourceModule::new(
                    "std",
                    ["assert"],
                    BOOTSTRAP_ASSERT_SOURCE_ID,
                    &self.assert_module,
                ),
            ],
        )
    }

    /// Whether the signed import map maps `atom` to `path`.
    pub(crate) fn maps(&self, atom: &str, path: &str) -> bool {
        self.modules
            .iter()
            .any(|entry| entry.atom == atom && entry.path == path)
    }

    pub(crate) fn trusts_module(&self, module: &vibra_resolve::ModuleRecord) -> bool {
        module.package() == &self.package
            && ((module.source_id() == BOOTSTRAP_TEXT_SOURCE_ID
                && module.bytes() == self.text_module)
                || (module.source_id() == BOOTSTRAP_ASSERT_SOURCE_ID
                    && module.bytes() == self.assert_module))
    }
}

/// A failure while checking the fixed offline bootstrap provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapVerificationError(String);

impl BootstrapVerificationError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for BootstrapVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BootstrapVerificationError {}

/// Verifies the bootstrap embedded into this toolchain.
pub fn verify_bootstrap() -> Result<BootstrapVerification, BootstrapVerificationError> {
    verify_bootstrap_bytes(&BootstrapInputs::embedded())
}

/// Verifies one set of bootstrap bytes without touching the filesystem.
///
/// The manifest is decoded into a typed record and must equal the reviewed
/// manifest bytes. Every digest is checked before the artifact is parsed, the
/// Ed25519 signature is verified over the exact artifact bytes, and the signed
/// artifact's package and import map must equal the manifest's structurally.
pub fn verify_bootstrap_bytes(
    inputs: &BootstrapInputs<'_>,
) -> Result<BootstrapVerification, BootstrapVerificationError> {
    let manifest = decode_manifest(inputs.manifest)?;
    if inputs.manifest != EMBEDDED_MANIFEST {
        return Err(BootstrapVerificationError::new(
            "M2 bootstrap manifest bytes do not match the reviewed manifest",
        ));
    }
    check_manifest_identity(&manifest)?;

    check_digest("artifact", inputs.artifact, &manifest.artifact_sha256)?;
    check_digest("signature", inputs.signature, &manifest.signature_sha256)?;
    check_digest("public key", inputs.public_key, &manifest.public_key_sha256)?;
    verify_signature(inputs.artifact, inputs.signature, inputs.public_key)?;

    let artifact = decode_artifact(inputs.artifact)?;
    if artifact.package != manifest.package_name {
        return Err(BootstrapVerificationError::new(format!(
            "signed bootstrap package `{}` does not match manifest package `{}`",
            artifact.package, manifest.package_name
        )));
    }
    if artifact.modules != manifest.modules {
        return Err(BootstrapVerificationError::new(
            "signed bootstrap import map does not match the manifest import map",
        ));
    }
    if artifact.compiler != manifest.compiler
        || artifact.assertions != manifest.assertions
    {
        return Err(BootstrapVerificationError::new(
            "signed bootstrap symbol lists do not match the manifest",
        ));
    }

    for (label, bytes, source_id) in [
        ("text module", inputs.text_module, BOOTSTRAP_TEXT_SOURCE_ID),
        (
            "assertion module",
            inputs.assert_module,
            BOOTSTRAP_ASSERT_SOURCE_ID,
        ),
    ] {
        let entry = artifact
            .modules
            .iter()
            .find(|entry| entry.path == source_id)
            .ok_or_else(|| {
                BootstrapVerificationError::new(format!(
                    "signed bootstrap import map has no entry for `{source_id}`"
                ))
            })?;
        check_digest(label, bytes, &entry.sha256)?;
    }

    Ok(BootstrapVerification {
        artifact: inputs.artifact.to_vec(),
        package: vibra_resolve::PackageId::new(
            manifest.package_name,
            manifest.package_version,
        ),
        modules: artifact.modules,
        text_module: inputs.text_module.to_vec(),
        assert_module: inputs.assert_module.to_vec(),
    })
}

/// Checks the manifest fields that the M2 contract fixes exactly.
fn check_manifest_identity(
    manifest: &BootstrapManifest,
) -> Result<(), BootstrapVerificationError> {
    let fixed = [
        ("package-name", manifest.package_name.as_str(), PACKAGE_NAME),
        (
            "package-version",
            manifest.package_version.as_str(),
            PACKAGE_VERSION,
        ),
        ("artifact", manifest.artifact.as_str(), ARTIFACT_PATH),
        ("signature", manifest.signature.as_str(), SIGNATURE_PATH),
        (
            "signature-encoding",
            manifest.signature_encoding.as_str(),
            "base64",
        ),
        ("public-key", manifest.public_key.as_str(), PUBLIC_KEY_PATH),
        (
            "artifact-sha256",
            manifest.artifact_sha256.as_str(),
            ARTIFACT_SHA256,
        ),
        (
            "signature-sha256",
            manifest.signature_sha256.as_str(),
            SIGNATURE_SHA256,
        ),
        (
            "public-key-sha256",
            manifest.public_key_sha256.as_str(),
            PUBLIC_KEY_SHA256,
        ),
    ];
    for (field, actual, expected) in fixed {
        if actual != expected {
            return Err(BootstrapVerificationError::new(format!(
                "bootstrap manifest `{field}` is `{actual}`, expected `{expected}`"
            )));
        }
    }
    let expected_modules = [
        ("std.text", BOOTSTRAP_TEXT_SOURCE_ID, TEXT_SHA256, "source"),
        (
            "std.assert",
            BOOTSTRAP_ASSERT_SOURCE_ID,
            ASSERT_SHA256,
            "test-registry",
        ),
    ];
    let closed = manifest.modules.len() == expected_modules.len()
        && manifest.modules.iter().zip(expected_modules).all(
            |(entry, (atom, path, sha256, role))| {
                entry.atom == atom
                    && entry.path == path
                    && entry.sha256 == sha256
                    && entry.role == role
            },
        );
    if !closed {
        return Err(BootstrapVerificationError::new(
            "bootstrap manifest import map is not the closed M2 map",
        ));
    }
    Ok(())
}

fn verify_signature(
    artifact: &[u8],
    signature: &[u8],
    public_key: &[u8],
) -> Result<(), BootstrapVerificationError> {
    let signature_text = std::str::from_utf8(signature).map_err(|_| {
        BootstrapVerificationError::new("bootstrap signature is not UTF-8")
    })?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature_text.trim())
        .map_err(|_| {
            BootstrapVerificationError::new("bootstrap signature is not base64")
        })?;
    let key = ed25519_public_key(public_key)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(artifact, &signature)
        .map_err(|_| {
            BootstrapVerificationError::new("M2 bootstrap Ed25519 signature is invalid")
        })
}

/// Extracts the raw key from a PEM Ed25519 `SubjectPublicKeyInfo`.
fn ed25519_public_key(pem: &[u8]) -> Result<[u8; 32], BootstrapVerificationError> {
    let pem = std::str::from_utf8(pem).map_err(|_| {
        BootstrapVerificationError::new("bootstrap public key is not UTF-8")
    })?;
    let mut lines = pem.lines();
    if lines.next() != Some("-----BEGIN PUBLIC KEY-----") {
        return Err(BootstrapVerificationError::new(
            "bootstrap public key is not a PEM public key",
        ));
    }
    let mut body = String::new();
    let mut terminated = false;
    for line in lines.by_ref() {
        if line == "-----END PUBLIC KEY-----" {
            terminated = true;
            break;
        }
        body.push_str(line);
    }
    if !terminated || lines.any(|line| !line.is_empty()) {
        return Err(BootstrapVerificationError::new(
            "bootstrap public key is not a single PEM public key",
        ));
    }
    let der = base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|_| {
            BootstrapVerificationError::new("bootstrap public key is not base64")
        })?;
    der.strip_prefix(&ED25519_SPKI_PREFIX)
        .and_then(|key| <[u8; 32]>::try_from(key).ok())
        .ok_or_else(|| {
            BootstrapVerificationError::new(
                "bootstrap public key is not an Ed25519 SubjectPublicKeyInfo",
            )
        })
}

fn check_digest(
    label: &str,
    bytes: &[u8],
    expected: &str,
) -> Result<(), BootstrapVerificationError> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected {
        return Err(BootstrapVerificationError::new(format!(
            "M2 bootstrap {label} digest mismatch: expected sha256:{expected}, got sha256:{actual}"
        )));
    }
    Ok(())
}

fn decode_manifest(
    bytes: &[u8],
) -> Result<BootstrapManifest, BootstrapVerificationError> {
    let root = decode_document("manifest", MANIFEST_PATH, bytes)?;
    let mut record = RecordReader::new("manifest", &root)?;
    record.expect_atom("format", "stdlib-bootstrap-manifest.v1")?;
    let manifest = BootstrapManifest {
        package_name: record.string("package-name")?,
        package_version: record.string("package-version")?,
        artifact: record.string("artifact")?,
        artifact_sha256: record.digest("artifact-sha256")?,
        signature: record.string("signature")?,
        signature_encoding: record.atom("signature-encoding")?,
        signature_sha256: record.digest("signature-sha256")?,
        public_key: record.string("public-key")?,
        public_key_sha256: record.digest("public-key-sha256")?,
        modules: record.modules("modules")?,
        compiler: record.strings("compiler")?,
        assertions: record.strings("assertions")?,
    };
    record.finish()?;
    Ok(manifest)
}

fn decode_artifact(
    bytes: &[u8],
) -> Result<BootstrapArtifact, BootstrapVerificationError> {
    let root = decode_document("artifact", ARTIFACT_PATH, bytes)?;
    let mut record = RecordReader::new("artifact", &root)?;
    record.expect_atom("format", "stdlib-bootstrap.v1")?;
    let artifact = BootstrapArtifact {
        package: record.string("package")?,
        modules: record.modules("modules")?,
        compiler: record.strings("compiler")?,
        assertions: record.strings("assertions")?,
    };
    record.finish()?;
    Ok(artifact)
}

fn decode_document(
    label: &str,
    path: &str,
    bytes: &[u8],
) -> Result<DataNode, BootstrapVerificationError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        BootstrapVerificationError::new(format!("bootstrap {label} is not UTF-8"))
    })?;
    let document =
        vibra_syntax::parse_data(Path::new(path), text).map_err(|error| {
            BootstrapVerificationError::new(format!("bootstrap {label}: {error}"))
        })?;
    if !document.accepted() || document.recovered() {
        return Err(BootstrapVerificationError::new(format!(
            "bootstrap {label} is not valid VIBON data"
        )));
    }
    document.data().cloned().ok_or_else(|| {
        BootstrapVerificationError::new(format!("bootstrap {label} has no data value"))
    })
}

/// Reads a record's fields in their exact, closed order.
struct RecordReader<'node> {
    label: &'static str,
    fields: std::slice::Iter<'node, DataField>,
}

impl<'node> RecordReader<'node> {
    fn new(
        label: &'static str,
        node: &'node DataNode,
    ) -> Result<Self, BootstrapVerificationError> {
        match node.value() {
            DataValue::Record(fields) => Ok(Self {
                label,
                fields: fields.iter(),
            }),
            _ => Err(shape(label, "the root", "a record")),
        }
    }

    fn field(
        &mut self,
        name: &str,
    ) -> Result<&'node DataNode, BootstrapVerificationError> {
        match self.fields.next() {
            Some(field) if field.label().value() == name => Ok(field.value()),
            Some(field) => Err(BootstrapVerificationError::new(format!(
                "bootstrap {} field `{}` appears where `{name}` is required",
                self.label,
                field.label().value()
            ))),
            None => Err(BootstrapVerificationError::new(format!(
                "bootstrap {} is missing field `{name}`",
                self.label
            ))),
        }
    }

    fn string(&mut self, name: &str) -> Result<String, BootstrapVerificationError> {
        let node = self.field(name)?;
        string_value(node).ok_or_else(|| shape(self.label, name, "a string"))
    }

    fn digest(&mut self, name: &str) -> Result<String, BootstrapVerificationError> {
        let value = self.string(name)?;
        digest_value(&value)
            .ok_or_else(|| shape(self.label, name, "a `sha256:` digest"))
    }

    fn atom(&mut self, name: &str) -> Result<String, BootstrapVerificationError> {
        let node = self.field(name)?;
        atom_value(node).ok_or_else(|| shape(self.label, name, "an atom"))
    }

    fn expect_atom(
        &mut self,
        name: &str,
        expected: &str,
    ) -> Result<(), BootstrapVerificationError> {
        let actual = self.atom(name)?;
        if actual == expected {
            Ok(())
        } else {
            Err(BootstrapVerificationError::new(format!(
                "bootstrap {} `{name}` is `@{actual}`, expected `@{expected}`",
                self.label
            )))
        }
    }

    fn strings(
        &mut self,
        name: &str,
    ) -> Result<Vec<String>, BootstrapVerificationError> {
        let label = self.label;
        match self.field(name)?.value() {
            DataValue::Array(items) => items
                .iter()
                .map(|item| {
                    string_value(item).ok_or_else(|| shape(label, name, "strings"))
                })
                .collect(),
            _ => Err(shape(label, name, "an array")),
        }
    }

    fn modules(
        &mut self,
        name: &str,
    ) -> Result<Vec<ModuleEntry>, BootstrapVerificationError> {
        let label = self.label;
        let DataValue::Map(entries) = self.field(name)?.value() else {
            return Err(shape(label, name, "a map"));
        };
        entries
            .iter()
            .map(|(key, value)| {
                let atom =
                    atom_value(key).ok_or_else(|| shape(label, name, "atom keys"))?;
                let mut entry = RecordReader::new(label, value)
                    .map_err(|_| shape(label, name, "record values"))?;
                let module = ModuleEntry {
                    atom,
                    path: entry.string("path")?,
                    sha256: entry.digest("sha256")?,
                    role: entry.atom("role")?,
                };
                entry.finish()?;
                Ok(module)
            })
            .collect()
    }

    fn finish(mut self) -> Result<(), BootstrapVerificationError> {
        match self.fields.next() {
            None => Ok(()),
            Some(field) => Err(BootstrapVerificationError::new(format!(
                "bootstrap {} has unexpected field `{}`",
                self.label,
                field.label().value()
            ))),
        }
    }
}

fn shape(label: &str, field: &str, expected: &str) -> BootstrapVerificationError {
    BootstrapVerificationError::new(format!(
        "bootstrap {label} `{field}` must be {expected}"
    ))
}

fn string_value(node: &DataNode) -> Option<String> {
    match node.value() {
        DataValue::Literal(Literal::String(literal)) => {
            Some(literal.value().to_owned())
        }
        _ => None,
    }
}

fn atom_value(node: &DataNode) -> Option<String> {
    match node.value() {
        DataValue::Atom(name) if name.kind() == NameKind::Atom => {
            Some(name.value().to_owned())
        }
        _ => None,
    }
}

/// Strips the `sha256:` prefix from a lowercase 64-digit hex digest.
fn digest_value(value: &str) -> Option<String> {
    let hex = value.strip_prefix("sha256:")?;
    (hex.len() == 64
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')))
    .then(|| hex.to_owned())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::{BootstrapInputs, verify_bootstrap, verify_bootstrap_bytes};

    fn rejects(inputs: &BootstrapInputs<'_>, fragment: &str) {
        let error =
            verify_bootstrap_bytes(inputs).expect_err("bootstrap must be rejected");
        assert!(
            error.to_string().contains(fragment),
            "`{error}` does not mention `{fragment}`"
        );
    }

    #[test]
    fn embedded_bootstrap_verifies() {
        let verification = verify_bootstrap().expect("embedded bootstrap");
        assert_eq!(
            verification.package(),
            &vibra_resolve::PackageId::new("vibra-stdlib", "0.1.0")
        );
        assert!(verification.maps("std.text", super::BOOTSTRAP_TEXT_SOURCE_ID));
        assert!(verification.maps("std.assert", super::BOOTSTRAP_ASSERT_SOURCE_ID));
        assert!(!verification.maps("std.text", super::BOOTSTRAP_ASSERT_SOURCE_ID));
    }

    #[test]
    fn rejects_tampered_module_bytes() {
        let mut inputs = BootstrapInputs::embedded();
        inputs.text_module = b"; modified trusted module\n";
        rejects(&inputs, "text module digest mismatch");
        let mut inputs = BootstrapInputs::embedded();
        inputs.assert_module = b"";
        rejects(&inputs, "assertion module digest mismatch");
    }

    #[test]
    fn rejects_tampered_artifact_signature_and_key_bytes() {
        let mut inputs = BootstrapInputs::embedded();
        inputs.artifact = b"tampered";
        rejects(&inputs, "artifact digest mismatch");
        let mut inputs = BootstrapInputs::embedded();
        inputs.signature = b"AAAA";
        rejects(&inputs, "signature digest mismatch");
        let mut inputs = BootstrapInputs::embedded();
        inputs.public_key = b"-----BEGIN PUBLIC KEY-----\n-----END PUBLIC KEY-----\n";
        rejects(&inputs, "public key digest mismatch");
    }

    #[test]
    fn rejects_malformed_manifests_by_shape() {
        let embedded = std::str::from_utf8(BootstrapInputs::embedded().manifest)
            .expect("manifest text");
        let cases = [
            ("not data at all", "not valid VIBON data"),
            ("(array 1)", "must be a record"),
            (
                &embedded
                    .replace("format: @stdlib-bootstrap-manifest.v1", "format: @other"),
                "expected `@stdlib-bootstrap-manifest.v1`",
            ),
            (
                &embedded.replace("package-version: \"0.1.0\"", "package-version: 1"),
                "`package-version` must be a string",
            ),
            (
                &embedded.replace(
                    "signature-encoding: @base64",
                    "signature-encoding: \"base64\"",
                ),
                "`signature-encoding` must be an atom",
            ),
            (
                &embedded
                    .replace("artifact-sha256: \"sha256:", "artifact-sha256: \"md5:"),
                "`artifact-sha256` must be a `sha256:` digest",
            ),
            (
                &embedded.replace("  compiler:", "  extra: 1\n  compiler:"),
                "`extra` appears where `compiler` is required",
            ),
            (
                &embedded.replace("modules: (map", "modules: (array"),
                "`modules` must be a map",
            ),
        ];
        for (manifest, fragment) in cases {
            let mut inputs = BootstrapInputs::embedded();
            inputs.manifest = manifest.as_bytes();
            rejects(&inputs, fragment);
        }
    }

    #[test]
    fn rejects_a_well_formed_but_unreviewed_manifest() {
        let embedded = std::str::from_utf8(BootstrapInputs::embedded().manifest)
            .expect("manifest text");
        // Swapping the two digests keeps every fragment present but places
        // each in the wrong entry; a substring check would accept it.
        let swapped = embedded
            .replace(super::TEXT_SHA256, "PLACEHOLDER")
            .replace(super::ASSERT_SHA256, super::TEXT_SHA256)
            .replace("PLACEHOLDER", super::ASSERT_SHA256);
        let mut inputs = BootstrapInputs::embedded();
        inputs.manifest = swapped.as_bytes();
        rejects(&inputs, "do not match the reviewed manifest");
    }

    #[test]
    fn typed_map_comparison_rejects_misplaced_digests() {
        let manifest = super::decode_manifest(BootstrapInputs::embedded().manifest)
            .expect("manifest");
        let mut swapped = manifest.clone();
        let text = swapped.modules[0].sha256.clone();
        swapped.modules[0].sha256 = swapped.modules[1].sha256.clone();
        swapped.modules[1].sha256 = text;
        let error =
            super::check_manifest_identity(&swapped).expect_err("misplaced digest");
        assert!(error.to_string().contains("closed M2 map"));
        let artifact = super::decode_artifact(BootstrapInputs::embedded().artifact)
            .expect("artifact");
        assert_eq!(artifact.modules, manifest.modules);
        assert_ne!(artifact.modules, swapped.modules);
    }

    #[test]
    fn public_key_requires_the_exact_ed25519_spki_prefix() {
        use base64::Engine;
        let encode = |der: &[u8]| {
            format!(
                "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
                base64::engine::general_purpose::STANDARD.encode(der)
            )
        };
        let mut der = super::ED25519_SPKI_PREFIX.to_vec();
        der.extend([7u8; 32]);
        assert_eq!(
            super::ed25519_public_key(encode(&der).as_bytes()),
            Ok([7u8; 32])
        );

        let mut wrong_oid = der.clone();
        wrong_oid[8] = 0x71; // Ed448
        let mut longer = der.clone();
        longer.push(0);
        let mut padded = vec![0u8; 4];
        padded.extend(&der);
        for key in [wrong_oid, longer, padded, vec![1u8; 32]] {
            let error = super::ed25519_public_key(encode(&key).as_bytes())
                .expect_err("bad key");
            assert!(
                error.to_string().contains("SubjectPublicKeyInfo"),
                "{error}"
            );
        }
    }
}
