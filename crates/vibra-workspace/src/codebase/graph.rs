//! Pure projection and canonical addressing of an accepted checked workspace.

use sha2::{Digest, Sha256};
use vibra_resolve::ResolvedSnapshot;

mod encoding;
mod model;
mod projection;
mod validation;
#[cfg(test)]
mod vectors;

use crate::{
    WorkspaceSnapshot, semantic::WorkspaceCheckResult, test_runner::TestCheckResult,
};

pub use model::*;
pub use validation::{
    parse_canonical_frame, validate_component_payload, validate_encoded_graph,
    validate_revision_payload,
};

/// Immutable inputs to the checked graph projection. All checking and test
/// checking must have completed before this value is constructed.
pub(crate) struct CheckedGraphInput<'a> {
    /// Captured project data and exact source documents.
    pub workspace: &'a WorkspaceSnapshot,
    /// Resolver facts for the same captured project.
    pub resolved: &'a ResolvedSnapshot,
    /// Accepted check facts for ordinary workspace units.
    pub checked: &'a WorkspaceCheckResult,
    /// Optional accepted test-scope facts. Tests are checked and never run.
    pub tests: Option<&'a TestCheckResult>,
    /// Already verified embedded standard-library and registry provenance.
    pub stdlib: &'a vibra_types::Stdlib,
}

/// Encodes an accepted workspace and its optional checked tests into a
/// deterministic addressed graph without acquiring inputs or running code.
pub(crate) fn encode_checked_graph(
    input: CheckedGraphInput<'_>,
) -> Result<EncodedGraph, GraphEncodingError> {
    if input.checked.status() != crate::semantic::CheckStatus::Accepted
        || input.tests.is_some_and(|tests| {
            tests.status() != crate::semantic::CheckStatus::Accepted
        })
        || !input.resolved.accepted()
        || input.checked.resolved().is_none()
        || input.checked.checked().is_none()
        || input.tests.is_some_and(|tests| tests.checked().is_none())
    {
        return Err(GraphEncodingError::UncheckedInput);
    }
    let sources = encode_sources(input.workspace)?;
    let package = input.resolved.package();
    let (trusted, compiler_registry, host_registry) = trusted_records(input.stdlib)?;
    let candidates =
        projection::collect_candidates(&input, &compiler_registry, &host_registry)?;
    let encoded = encoding::encode_candidates(candidates)?;
    let revision_payload = revision_frame(
        package.name(),
        package.version(),
        &trusted,
        &encoded.entities,
        &sources,
    )?;
    let address = address_from_digest(&hash_domain(
        "vibra-codebase-revision-v1\0",
        &revision_payload,
    ));
    let graph = EncodedGraph {
        revision: EncodedRevision {
            address: RevisionAddress(address),
            payload: revision_payload,
        },
        objects: encoded.objects,
        components: encoded.components,
        sources,
        edges: encoded.edges,
        entities: encoded.entities,
    };
    validate_encoded_graph(&graph)?;
    Ok(graph)
}

fn encode_sources(
    workspace: &WorkspaceSnapshot,
) -> Result<Vec<SourceAttachment>, GraphEncodingError> {
    let mut attachments = Vec::new();
    for document in workspace.source().documents() {
        let text = std::str::from_utf8(document.bytes())
            .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
        attachments.push(source_attachment(
            document.source_id(),
            document.relative_path(),
            document.bytes(),
            text,
        )?);
    }
    let project_bytes = workspace.project().project_bytes();
    let project_text = std::str::from_utf8(project_bytes)
        .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
    attachments.push(source_attachment(
        workspace.project().project_source_id(),
        workspace.project().project_source_id(),
        project_bytes,
        project_text,
    )?);
    attachments.sort_by(|left, right| {
        left.source_id.as_bytes().cmp(right.source_id.as_bytes())
    });
    Ok(attachments)
}

fn source_attachment(
    source_id: &str,
    origin: &str,
    bytes: &[u8],
    text: &str,
) -> Result<SourceAttachment, GraphEncodingError> {
    let record = format!(
        "(record format: @source.v1 text: {})",
        vibra_syntax::canonical_string_spelling(text),
    );
    let payload = encoding::canonical_frame(&record)?;
    let raw = Sha256::digest(bytes);
    let raw_hash = address_from_digest(&raw.into());
    let address =
        address_from_digest(&hash_domain("vibra-codebase-source-v1\0", &payload));
    Ok(SourceAttachment {
        source_id: source_id.to_owned(),
        origin: origin.to_owned(),
        address: SourceAddress(address),
        raw_hash: SourceHash(raw_hash),
        payload,
        text: bytes.to_vec(),
    })
}

fn trusted_records(
    stdlib: &vibra_types::Stdlib,
) -> Result<(Vec<String>, String, String), GraphEncodingError> {
    let mut host_payload = format!(
        "(record format: @codebase-registry.v1 provider: @host version: \"vibra_v1\" minor: {}u32 snapshot: {})",
        vibra_ir::external::host::HOST_REGISTRY_MINOR,
        vibra_syntax::canonical_string_spelling(
            &vibra_ir::external::host::registry_snapshot()
        ),
    );
    let host_bytes = encoding::canonical_frame(&host_payload)?;
    host_payload = String::from_utf8(host_bytes.clone())
        .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
    let host_fingerprint =
        address_from_digest(&hash_domain("vibra-codebase-registry-v1\0", &host_bytes));
    let host = format!(
        "(record kind: @registry fingerprint: {} registry: {})",
        vibra_syntax::canonical_string_spelling(&host_fingerprint),
        host_payload.trim_end_matches('\n'),
    );

    let mut compiler_rows = vibra_ir::external::CompilerIntrinsic::all()
        .into_iter()
        .map(|intrinsic| {
            let parameters = intrinsic.type_parameters();
            let signature =
                vibra_ir::canonical_type(&vibra_ir::Type::Function(Box::new(
                    intrinsic.signature(&vibra_ir::external::RoleTypes::new(None)),
                )));
            (
                intrinsic.symbol().to_owned(),
                parameters,
                signature,
                intrinsic.is_native(),
            )
        })
        .collect::<Vec<_>>();
    compiler_rows.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let rows = compiler_rows.iter().map(|(symbol, parameters, signature, native)| {
        let parameters = parameters.iter().map(|value| vibra_syntax::canonical_string_spelling(value)).collect::<Vec<_>>();
        format!(
            "(record symbol: {} type-parameters: (array{}) signature: {} native: {})",
            vibra_syntax::canonical_string_spelling(symbol),
            if parameters.is_empty() { String::new() } else { format!(" {}", parameters.join(" ")) },
            vibra_syntax::canonical_string_spelling(signature),
            native,
        )
    }).collect::<Vec<_>>();
    let compiler_payload = format!(
        "(record format: @codebase-registry.v1 provider: @compiler version: \"compiler-v1\" rows: (array {}))",
        rows.join(" "),
    );
    let compiler_bytes = encoding::canonical_frame(&compiler_payload)?;
    let compiler_frame = String::from_utf8(compiler_bytes.clone())
        .map_err(|error| GraphEncodingError::MalformedInput(error.to_string()))?;
    let compiler_fingerprint = address_from_digest(&hash_domain(
        "vibra-codebase-registry-v1\0",
        &compiler_bytes,
    ));
    let compiler = format!(
        "(record kind: @registry fingerprint: {} registry: {})",
        vibra_syntax::canonical_string_spelling(&compiler_fingerprint),
        compiler_frame.trim_end_matches('\n'),
    );

    let package = stdlib.package();
    let mut modules = stdlib
        .modules()
        .iter()
        .map(|module| {
            let module_id = format!(
                "@{}@{}/{}",
                package.name(),
                package.version(),
                module.atom(),
            );
            let raw = Sha256::digest(module.bytes());
            format!(
                "(record module: {} source-hash: {})",
                vibra_syntax::canonical_string_spelling(&module_id),
                vibra_syntax::canonical_string_spelling(&address_from_digest(
                    &raw.into()
                )),
            )
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let stdlib_record = format!(
        "(record kind: @stdlib package-name: {} package-version: {} manifest: {} modules: (array {}))",
        vibra_syntax::canonical_string_spelling(package.name()),
        vibra_syntax::canonical_string_spelling(package.version()),
        vibra_syntax::canonical_string_spelling(stdlib.manifest_hash()),
        modules.join(" "),
    );
    // Trusted records order by kind then provider/package identity:
    // compiler registry, host registry, standard library.
    Ok((
        vec![compiler, host, stdlib_record],
        compiler_fingerprint,
        host_fingerprint,
    ))
}

fn revision_frame(
    package_name: &str,
    package_version: &str,
    trusted: &[String],
    entities: &[EntityMapping],
    sources: &[SourceAttachment],
) -> Result<Vec<u8>, GraphEncodingError> {
    let objects = entities
        .iter()
        .map(|entity| {
            let part = entity
                .part
                .iter()
                .map(|value| vibra_syntax::canonical_string_spelling(value))
                .collect::<Vec<_>>();
            format!(
                "(record key: {} object: {} part: (array{}))",
                entity.key.canonical(),
                vibra_syntax::canonical_string_spelling(entity.object.as_str()),
                if part.is_empty() {
                    String::new()
                } else {
                    format!(" {}", part.join(" "))
                },
            )
        })
        .collect::<Vec<_>>();
    let sources = sources
        .iter()
        .map(|source| {
            format!(
                "(record source-id: {} attachment: {} source-hash: {} origin: {})",
                vibra_syntax::canonical_string_spelling(&source.source_id),
                vibra_syntax::canonical_string_spelling(source.address.as_str()),
                vibra_syntax::canonical_string_spelling(source.raw_hash.as_str()),
                vibra_syntax::canonical_string_spelling(&source.origin),
            )
        })
        .collect::<Vec<_>>();
    let record = format!(
        "(record format: @codebase-revision.v1 package: (record name: {} version: {}) trusted: (array {}) objects: (array{}) sources: (array{}))",
        vibra_syntax::canonical_string_spelling(package_name),
        vibra_syntax::canonical_string_spelling(package_version),
        trusted.join(" "),
        if objects.is_empty() {
            String::new()
        } else {
            format!(" {}", objects.join(" "))
        },
        if sources.is_empty() {
            String::new()
        } else {
            format!(" {}", sources.join(" "))
        },
    );
    encoding::canonical_frame(&record)
}

fn hash_domain(domain: &str, payload: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update(payload);
    hash.finalize().into()
}

fn address_from_digest(digest: &[u8; 32]) -> String {
    let mut address = String::with_capacity(71);
    address.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(address, "{byte:02x}");
    }
    address
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest_address(domain: &str, payload: &[u8]) -> ([u8; 32], String) {
        let digest = hash_domain(domain, payload);
        (digest, address_from_digest(&digest))
    }

    #[test]
    fn graph_type_contract_is_closed_and_has_stable_edge_names() {
        let edges = [
            EdgeKind::ValueReference,
            EdgeKind::TypeReference,
            EdgeKind::Contract,
            EdgeKind::Implementation,
            EdgeKind::EffectOwner,
            EdgeKind::Initializer,
            EdgeKind::Call,
        ];
        assert_eq!(
            edges.map(EdgeKind::as_str),
            [
                "value-reference",
                "type-reference",
                "contract",
                "implementation",
                "effect-owner",
                "initializer",
                "call",
            ]
        );
    }

    #[test]
    fn canonical_record_validators_require_one_trailing_lf_and_matching_hashes() {
        let revision = b"(record format: @codebase-revision.v1)\n";
        let (_, revision_address) =
            digest_address("vibra-codebase-revision-v1\0", revision);
        assert!(validate_revision_payload(&revision_address, revision).is_ok());
        assert!(
            validate_revision_payload(
                &revision_address,
                b"(record format: @codebase-revision.v1)\n\n"
            )
            .is_err()
        );

        let component =
            b"(record format: @checked-component.v1 members: (array @empty))\n";
        let (digest, address) =
            digest_address("vibra-codebase-component-v1\0", component);
        assert!(validate_component_payload(&address, &digest, component).is_ok());
        assert!(
            validate_component_payload(&revision_address, &digest, component).is_err()
        );
    }

    #[test]
    fn deeply_nested_canonical_vibon_is_moved_out_without_recursive_clone() {
        // The shared decoder retains a raw source slice copy for every node,
        // so a single-child chain has quadratic input-copy cost. A small stack
        // exercises the move-out boundary directly without a huge fixture.
        let depth: usize = 768;
        let mut text = String::with_capacity(depth.saturating_mul(9).saturating_add(3));
        for _ in 0..depth {
            text.push_str("(array ");
        }
        text.push_str("@leaf");
        text.extend(std::iter::repeat_n(')', depth));
        text.push('\n');

        let parsed = std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(move || {
                let parsed = parse_canonical_frame(text.as_bytes());
                drop(parsed);
            })
            .expect("small-stack thread")
            .join();

        assert!(parsed.is_ok());
    }
}
