//! Independently authored, fixed SCC address vectors.
//!
//! The frame literals and expected addresses below were derived in the
//! companion `docs/roadmap/v0.0.2/m1/address-vectors.md` fixture with a small
//! standalone SHA-256 reference calculation. They are deliberately not
//! generated from `encode_candidates`. `SIMPLE_FRAME` is a complete checked
//! function projection; the remaining frames are explicitly wire-level SCC
//! fixtures and do not claim to be accepted source projections.

use sha2::{Digest, Sha256};

use super::encoding::{
    Candidate, CandidateReference, CandidateTarget, DefinitionNode, EncodedMembers,
    encode_candidates,
};
use super::{EdgeKind, ObjectKey, ObjectKind};

const SIMPLE_FRAME: &str = concat!(
    r#"(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.simple" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "function-part") (record kind: @list items: (array (record kind: @atom value: "name") (record kind: @string value: "simple"))) (record kind: @list items: (array (record kind: @atom value: "attributes") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "body") (record kind: @list items: (array (record kind: @atom value: "checked-function") (record kind: @list items: (array (record kind: @atom value: "signature") (record kind: @list items: (array (record kind: @atom value: "signature") (record kind: @list items: (array (record kind: @atom value: "positional") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "labelled") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "variadic") (record kind: @list items: (array (record kind: @atom value: "absent"))))) (record kind: @list items: (array (record kind: @atom value: "result") (record kind: @list items: (array (record kind: @atom value: "primitive") (record kind: @list items: (array (record kind: @atom value: "name") (record kind: @atom value: "u32"))))))) (record kind: @list items: (array (record kind: @atom value: "effects") (record kind: @list items: (array (record kind: @atom value: "effects") (record kind: @list items: (array (record kind: @atom value: "roots") (record kind: @list items: (array)))) (record kind: @list items: (array (record kind: @atom value: "tail") (record kind: @atom value: "none"))))))))))) (record kind: @list items: (array (record kind: @atom value: "body") (record kind: @list items: (array (record kind: @atom value: "expr") (record kind: @atom value: "sequence") (record kind: @list items: (array (record kind: @atom value: "expressions") (record kind: @list items: (array (record kind: @list items: (array (record kind: @atom value: "expr") (record kind: @atom value: "literal") (record kind: @list items: (array (record kind: @atom value: "value") (record kind: @integer type: @u32 value: "7"))))))))))))) (record kind: @list items: (array (record kind: @atom value: "lowered-body") (record kind: @void))) (record kind: @list items: (array (record kind: @atom value: "native-body") (record kind: @void))) (record kind: @list items: (array (record kind: @atom value: "implements") (record kind: @void))) (record kind: @list items: (array (record kind: @atom value: "effect-owner") (record kind: @void)))))))))) references: (array))))"#,
    "\n"
);
const SIMPLE_COMPONENT: &str =
    "sha256:007833aa1b06fbade3d0e75485111bc6ebc39fbe1859f4959ce2161feac3aa03";
const SIMPLE_OBJECT: &str =
    "sha256:e961613b65f10343533506c1add52cdb85098bab87c31fcfbf1958ce3cd95613";

const SELF_FRAME: &str = concat!(
    r#"(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.loop" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.loop")))) references: (array (record part: (array) kind: @call target: (record kind: @component-member index: 0u64))))))"#,
    "\n"
);
const SELF_COMPONENT: &str =
    "sha256:8130d40260251f9dbfd50e21c3472a85051ab965d8580f70fda4b8291787e64d";
const SELF_OBJECT: &str =
    "sha256:cf09809d2ed35d14728846c1f67bc028be7582026f736470fe2e42ccb03192e0";

const MUTUAL_FRAME: &str = concat!(
    r#"(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.alpha" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.beta")))) references: (array (record part: (array) kind: @call target: (record kind: @component-member index: 1u64)))) (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.beta" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.alpha")))) references: (array (record part: (array) kind: @call target: (record kind: @component-member index: 0u64))))))"#,
    "\n"
);
const MUTUAL_COMPONENT: &str =
    "sha256:5410603ea9a48013bfb58a5690772dc3319a9a0b8b80ddec265d523337f304b3";
const MUTUAL_ALPHA: &str =
    "sha256:3c04ec7243737de86c0594671c76dab58fa04fe36ee85a94545b94a9c61725b7";
const MUTUAL_BETA: &str =
    "sha256:0398214f07198493243573b82e17ec590a2311d8fcf96eda5edf105ba66e8860";

const DEPENDENCY_FRAME: &str = concat!(
    r#"(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.z-library" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "body") (record kind: @integer type: @u32 value: "9")))) references: (array))))"#,
    "\n"
);
const DEPENDENCY_COMPONENT: &str =
    "sha256:962b4e74a3342bf941323d295a287021eff4712cafe143b7684d81b9b4256e80";
const DEPENDENCY_OBJECT: &str =
    "sha256:3a3a15f1cb4ccfb35d934db447887e6694a3fcce03df87384b2894f07658a956";

const CLIENT_FRAME: &str = concat!(
    r#"(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.a-client" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@demo@0.1.0/app.main.z-library")))) references: (array (record part: (array) kind: @call target: (record kind: @object address: "sha256:3a3a15f1cb4ccfb35d934db447887e6694a3fcce03df87384b2894f07658a956"))))))"#,
    "\n"
);
const CLIENT_COMPONENT: &str =
    "sha256:415658873a6bd2644666d1cb1cb05004f612f92f82787b5d658160c794b3dc48";
const CLIENT_OBJECT: &str =
    "sha256:bb49699bb68f60e7a38c8cfa82588baeaf0150e6e15fde6618e8ee6478b72387";

const TRUSTED_FRAME: &str = concat!(
    r#"(record format: @checked-component.v1 members: (array (record key: (record kind: @declaration identity: "@demo@0.1.0/app.main.external-call" entity-kind: @function) kind: @defn definition: (record format: @checked-definition.v1 kind: @defn form: (record kind: @list items: (array (record kind: @atom value: "call") (record kind: @reference identity: "@host:io.stdout-text")))) references: (array (record part: (array) kind: @call target: (record kind: @trusted-external identity: "@host:io.stdout-text" registry: "sha256:7467d79727468361e01e8a44289492103b6a482078624662c69a7999b09ef4e4"))))))"#,
    "\n"
);
const TRUSTED_COMPONENT: &str =
    "sha256:732e7da21f6e3811f918f21eccc1f7c825383e11201197d5ad98f1a6ebb58ec7";
const TRUSTED_OBJECT: &str =
    "sha256:5d9f91cb7e1b8c1cfdfee99a45cffb6c2d232d6a9a404ee53476bbe9583d8bf8";

fn declaration(name: &str) -> ObjectKey {
    ObjectKey::Declaration {
        identity: format!("@demo@0.1.0/app.main.{name}"),
        entity_kind: "function".to_owned(),
    }
}

fn definition(items: Vec<DefinitionNode>) -> DefinitionNode {
    DefinitionNode::List(items)
}

fn field(name: &str, value: DefinitionNode) -> DefinitionNode {
    definition(vec![atom(name), value])
}

fn tagged(name: &str, fields: Vec<(&str, DefinitionNode)>) -> DefinitionNode {
    let mut values = vec![atom(name)];
    values.extend(fields.into_iter().map(|(name, value)| field(name, value)));
    definition(values)
}

fn array(values: Vec<DefinitionNode>) -> DefinitionNode {
    definition(values)
}

fn atom(value: &str) -> DefinitionNode {
    DefinitionNode::Atom(value.to_owned())
}

fn reference(identity: &str) -> DefinitionNode {
    DefinitionNode::Reference(identity.to_owned())
}

fn u32(value: &str) -> DefinitionNode {
    DefinitionNode::Integer {
        ty: "u32".to_owned(),
        value: value.to_owned(),
    }
}

fn candidate(name: &str, form: DefinitionNode) -> Candidate {
    Candidate::new(declaration(name), ObjectKind::Defn, &form)
        .expect("fixed vector candidate is well-formed")
}

fn complete_simple_function() -> DefinitionNode {
    let primitive_u32 = tagged("primitive", vec![("name", atom("u32"))]);
    let signature = tagged(
        "signature",
        vec![
            ("positional", array(vec![])),
            ("labelled", array(vec![])),
            ("variadic", tagged("absent", vec![])),
            ("result", primitive_u32),
            (
                "effects",
                tagged(
                    "effects",
                    vec![("roots", array(vec![])), ("tail", atom("none"))],
                ),
            ),
        ],
    );
    let literal = definition(vec![
        atom("expr"),
        atom("literal"),
        field("value", u32("7")),
    ]);
    let body = definition(vec![
        atom("expr"),
        atom("sequence"),
        field("expressions", array(vec![literal])),
    ]);
    let checked = tagged(
        "checked-function",
        vec![
            ("signature", signature),
            ("body", body),
            ("lowered-body", DefinitionNode::Void),
            ("native-body", DefinitionNode::Void),
            ("implements", DefinitionNode::Void),
            ("effect-owner", DefinitionNode::Void),
        ],
    );
    tagged(
        "function-part",
        vec![
            ("name", DefinitionNode::String("simple".to_owned())),
            ("attributes", array(vec![])),
            ("body", checked),
        ],
    )
}

fn call(target: CandidateTarget) -> CandidateReference {
    CandidateReference {
        part: Vec::new(),
        kind: EdgeKind::Call,
        target,
        dispatch: None,
    }
}

fn assert_vector(
    encoded: &EncodedMembers,
    component_index: usize,
    expected_frame: &str,
    expected_component: &str,
    expected_members: &[&str],
) {
    let component = &encoded.components[component_index];
    assert_eq!(component.payload, expected_frame.as_bytes());
    assert_eq!(component.address.as_str(), expected_component);

    let digest = reference_component_digest(expected_frame.as_bytes());
    assert_eq!(component.digest, digest);
    assert_eq!(reference_address(&digest), expected_component);
    let object_addresses = expected_members
        .iter()
        .enumerate()
        .map(|(index, _)| reference_object_address(&digest, index as u64))
        .collect::<Vec<_>>();
    assert_eq!(
        component
            .members
            .iter()
            .map(|member| member.as_str().to_owned())
            .collect::<Vec<_>>(),
        object_addresses
    );

    for (identity, expected_address) in expected_members.iter().zip(object_addresses) {
        let encoded_object = encoded
            .objects
            .iter()
            .find(|object| match &object.key {
                ObjectKey::Declaration {
                    identity: actual, ..
                } => actual.as_str() == *identity,
                _ => false,
            })
            .expect("fixed vector object exists");
        assert_eq!(encoded_object.address.as_str(), expected_address);
    }
}

fn assert_exact_frame(actual: &[u8], expected: &[u8]) {
    if actual == expected {
        return;
    }
    let offset = actual
        .iter()
        .zip(expected)
        .position(|(actual, expected)| actual != expected)
        .unwrap_or(actual.len().min(expected.len()));
    let start = offset.saturating_sub(80);
    let end = offset
        .saturating_add(80)
        .min(actual.len().max(expected.len()));
    let actual_end = end.min(actual.len());
    let expected_end = end.min(expected.len());
    panic!(
        "component frame differs at byte {offset}; expected {:?}, actual {:?}",
        String::from_utf8_lossy(&expected[start.min(expected.len())..expected_end]),
        String::from_utf8_lossy(&actual[start.min(actual.len())..actual_end]),
    );
}

// This is intentionally a separate reference calculation over authored
// payload bytes; it does not call the encoder's hash or address helpers.
fn reference_component_digest(payload: &[u8]) -> [u8; 32] {
    let mut sha256 = Sha256::new();
    sha256.update(b"vibra-codebase-component-v1\0");
    sha256.update(payload);
    sha256.finalize().into()
}

fn reference_address(digest: &[u8; 32]) -> String {
    format!("sha256:{}", hex(digest))
}

fn reference_object_address(digest: &[u8; 32], index: u64) -> String {
    let mut sha256 = Sha256::new();
    sha256.update(b"vibra-codebase-object-v1\0");
    sha256.update(digest);
    sha256.update(index.to_be_bytes());
    format!("sha256:{}", hex(&sha256.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

struct TemporaryProject(std::path::PathBuf);

impl TemporaryProject {
    fn simple_function() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory");
        let root = loop {
            let serial = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!(
                "vibra-m1-simple-vector-{}-{serial}",
                std::process::id()
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    continue;
                }
                Err(error) => panic!("create vector project directory: {error}"),
            }
        };
        let source_dir = root.join("src/app");
        std::fs::create_dir_all(&source_dir).expect("create vector source directory");
        std::fs::write(
            root.join("project.vibon"),
            "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @lib root: \"src/app\")) dependencies: (dict))\n",
        )
        .expect("write vector project manifest");
        std::fs::write(source_dir.join("main.vib"), "(defn simple () u32 7)\n")
            .expect("write vector source");
        Self(root)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TemporaryProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn source_simple_function_projects_to_fixed_authored_frame() {
    let project = TemporaryProject::simple_function();
    let prepared = crate::codebase::import::prepare_import(project.path())
        .expect("the simple source project prepares without execution");
    let object = prepared
        .graph
        .objects
        .iter()
        .find(|object| {
            matches!(
                &object.key,
                ObjectKey::Declaration { identity, entity_kind }
                    if identity == "@demo@0.1.0/app.main.simple" && entity_kind == "function"
            )
        })
        .expect("source projection includes the simple function object");
    let component = prepared
        .graph
        .components
        .iter()
        .find(|component| component.address == object.component_address)
        .expect("simple function component exists");
    assert_exact_frame(&component.payload, SIMPLE_FRAME.as_bytes());
    assert_eq!(component.address.as_str(), SIMPLE_COMPONENT);
    assert_eq!(object.address.as_str(), SIMPLE_OBJECT);
    assert_eq!(
        component.digest,
        reference_component_digest(SIMPLE_FRAME.as_bytes())
    );
    assert_eq!(component.members, vec![object.address.clone()]);
}

#[test]
fn fixed_address_vectors_match_authored_frames_and_independent_sha256() {
    let simple = candidate("simple", complete_simple_function());
    let simple_graph = encode_candidates(vec![simple]).expect("simple vector encodes");
    assert_eq!(simple_graph.components.len(), 1);
    assert_vector(
        &simple_graph,
        0,
        SIMPLE_FRAME,
        SIMPLE_COMPONENT,
        &["@demo@0.1.0/app.main.simple"],
    );
    assert_eq!(simple_graph.objects[0].address.as_str(), SIMPLE_OBJECT);

    let loop_key = declaration("loop");
    let mut self_recursive = candidate(
        "loop",
        definition(vec![atom("call"), reference("@demo@0.1.0/app.main.loop")]),
    );
    self_recursive
        .references
        .push(call(CandidateTarget::Object(loop_key)));
    let self_graph =
        encode_candidates(vec![self_recursive]).expect("self vector encodes");
    assert_eq!(self_graph.components.len(), 1);
    assert_vector(
        &self_graph,
        0,
        SELF_FRAME,
        SELF_COMPONENT,
        &["@demo@0.1.0/app.main.loop"],
    );
    assert_eq!(self_graph.objects[0].address.as_str(), SELF_OBJECT);

    let alpha_key = declaration("alpha");
    let beta_key = declaration("beta");
    let mut alpha = candidate(
        "alpha",
        definition(vec![atom("call"), reference("@demo@0.1.0/app.main.beta")]),
    );
    let mut beta = candidate(
        "beta",
        definition(vec![atom("call"), reference("@demo@0.1.0/app.main.alpha")]),
    );
    alpha
        .references
        .push(call(CandidateTarget::Object(beta_key)));
    beta.references
        .push(call(CandidateTarget::Object(alpha_key)));
    let mutual_graph =
        encode_candidates(vec![beta, alpha]).expect("mutual vector encodes");
    assert_eq!(mutual_graph.components.len(), 1);
    assert_vector(
        &mutual_graph,
        0,
        MUTUAL_FRAME,
        MUTUAL_COMPONENT,
        &["@demo@0.1.0/app.main.alpha", "@demo@0.1.0/app.main.beta"],
    );
    assert_eq!(mutual_graph.objects[0].address.as_str(), MUTUAL_ALPHA);
    assert_eq!(mutual_graph.objects[1].address.as_str(), MUTUAL_BETA);

    let library_key = declaration("z-library");
    let mut client = candidate(
        "a-client",
        definition(vec![
            atom("call"),
            reference("@demo@0.1.0/app.main.z-library"),
        ]),
    );
    client
        .references
        .push(call(CandidateTarget::Object(library_key)));
    let library = candidate("z-library", definition(vec![atom("body"), u32("9")]));
    let dependency_graph =
        encode_candidates(vec![client, library]).expect("dependency vector encodes");
    assert_eq!(dependency_graph.components.len(), 2);
    assert_vector(
        &dependency_graph,
        0,
        DEPENDENCY_FRAME,
        DEPENDENCY_COMPONENT,
        &["@demo@0.1.0/app.main.z-library"],
    );
    assert_eq!(
        dependency_graph
            .objects
            .iter()
            .find(|object| matches!(&object.key, ObjectKey::Declaration { identity, .. } if identity.ends_with(".z-library")))
            .expect("library object")
            .address
            .as_str(),
        DEPENDENCY_OBJECT
    );
    assert_vector(
        &dependency_graph,
        1,
        CLIENT_FRAME,
        CLIENT_COMPONENT,
        &["@demo@0.1.0/app.main.a-client"],
    );
    assert_eq!(
        dependency_graph
            .objects
            .iter()
            .find(|object| matches!(&object.key, ObjectKey::Declaration { identity, .. } if identity.ends_with(".a-client")))
            .expect("client object")
            .address
            .as_str(),
        CLIENT_OBJECT
    );

    const HOST_REGISTRY: &str =
        "sha256:7467d79727468361e01e8a44289492103b6a482078624662c69a7999b09ef4e4";
    let external_identity = "@host:io.stdout-text";
    let mut trusted = candidate(
        "external-call",
        definition(vec![atom("call"), reference(external_identity)]),
    );
    trusted
        .references
        .push(call(CandidateTarget::TrustedExternal {
            identity: external_identity.to_owned(),
            registry: HOST_REGISTRY.to_owned(),
        }));
    let trusted_graph =
        encode_candidates(vec![trusted]).expect("trusted vector encodes");
    assert_eq!(trusted_graph.components.len(), 1);
    assert_vector(
        &trusted_graph,
        0,
        TRUSTED_FRAME,
        TRUSTED_COMPONENT,
        &["@demo@0.1.0/app.main.external-call"],
    );
    assert_eq!(trusted_graph.objects[0].address.as_str(), TRUSTED_OBJECT);
}
