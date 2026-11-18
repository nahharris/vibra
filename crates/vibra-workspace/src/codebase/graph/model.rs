//! Public checked graph records, keys, and closed vocabularies.

use vibra_diagnostics::ByteSpan;

/// A completely encoded addressed graph and revision manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedGraph {
    /// Canonical revision manifest and its address.
    pub revision: EncodedRevision,
    /// Unique checked objects, sorted by canonical key bytes.
    pub objects: Vec<EncodedObject>,
    /// SCC component records, sorted by canonical member key sequence.
    pub components: Vec<EncodedComponent>,
    /// Exact project source attachments, sorted by source ID.
    pub sources: Vec<SourceAttachment>,
    /// Resolved semantic reference and call edges, in canonical edge order.
    pub edges: Vec<EncodedEdge>,
    /// Resolver entities and owned parts mapped to object addresses.
    pub entities: Vec<EntityMapping>,
}

/// Canonical manifest for one complete immutable codebase revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedRevision {
    /// Domain-separated revision address.
    pub address: RevisionAddress,
    /// Canonical `@codebase-revision.v1` bytes, with one trailing LF.
    pub payload: Vec<u8>,
}

/// One canonical strongly connected component frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedComponent {
    /// Domain-separated component address.
    pub address: ComponentAddress,
    /// Raw SHA-256 digest used to derive member addresses.
    pub digest: [u8; 32],
    /// Canonical `@checked-component.v1` bytes, with one trailing LF.
    pub payload: Vec<u8>,
    /// Member object addresses in their canonical key order.
    pub members: Vec<ObjectAddress>,
}

/// One object in an encoded revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedObject {
    /// Canonical nominal object key.
    pub key: ObjectKey,
    /// Closed checked object kind.
    pub kind: ObjectKind,
    /// Domain-separated address for this component member.
    pub address: ObjectAddress,
    /// Address of the SCC frame containing this object.
    pub component_address: ComponentAddress,
    /// Zero-based ordinal within its SCC.
    pub member_index: u64,
    /// Source locations owned by this definition, excluded from its hash.
    pub origins: Vec<ObjectOrigin>,
}

/// A source file attachment retained separately from checked objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAttachment {
    /// Stable project-relative source identity.
    pub source_id: String,
    /// Project-relative origin path using `/` separators.
    pub origin: String,
    /// Address of the canonical `@source.v1` wrapper.
    pub address: SourceAddress,
    /// Raw SHA-256 of the exact UTF-8 source bytes.
    pub raw_hash: SourceHash,
    /// Canonical source wrapper bytes.
    pub payload: Vec<u8>,
    /// Exact attached UTF-8 bytes.
    pub text: Vec<u8>,
}

/// A resolver entity-to-object and owned-part mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityMapping {
    /// Canonical resolver entity key, including kind where applicable.
    pub key: EntityKey,
    /// Address of the top-level owner object.
    pub object: ObjectAddress,
    /// Exact canonical owner-relative path; empty for the owner itself.
    pub part: Vec<String>,
}

/// One resolved semantic dependency edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedEdge {
    /// Address of the checked owning object.
    pub owner: ObjectAddress,
    /// Owner-relative path of the reference occurrence.
    pub part: Vec<String>,
    /// Closed semantic edge category.
    pub kind: EdgeKind,
    /// Resolved object or closed trusted registry identity.
    pub target: EdgeTarget,
}

/// A checked object key, preserving nominal identity independently of content.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectKey {
    /// Top-level declaration, including resolver entity kind.
    Declaration {
        /// Full canonical resolver identity.
        identity: String,
        /// Canonical resolver entity-kind atom.
        entity_kind: String,
    },
    /// Test identity, including its canonical module and decoded name.
    Test {
        /// Full canonical identity of the test module.
        module: String,
        /// Decoded test name.
        name: String,
    },
    /// Explicit implementation block keyed by normalized receiver/interface.
    Implementation {
        /// Canonical normalized receiver type text.
        receiver: String,
        /// Canonical normalized applied interface type text.
        interface: String,
    },
}

impl ObjectKey {
    /// Canonical VIBON encoding used for component member ordering.
    #[must_use]
    pub fn canonical(&self) -> String {
        use vibra_syntax::canonical_string_spelling as quote;
        match self {
            Self::Declaration {
                identity,
                entity_kind,
            } => format!(
                "(record kind: @declaration identity: {} entity-kind: @{})",
                quote(identity),
                entity_kind
            ),
            Self::Test { module, name } => format!(
                "(record kind: @test module: {} name: {})",
                quote(module),
                quote(name)
            ),
            Self::Implementation {
                receiver,
                interface,
            } => format!(
                "(record kind: @implementation receiver: {} interface: {})",
                quote(receiver),
                quote(interface)
            ),
        }
    }
}

/// Revision membership key for an entity or implementation member.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKey {
    /// A top-level declaration identity and kind.
    Declaration {
        /// Full canonical resolver identity.
        identity: String,
        /// Canonical resolver entity-kind atom.
        entity_kind: String,
    },
    /// A test identity.
    Test {
        /// Full canonical identity of the test module.
        module: String,
        /// Decoded test name.
        name: String,
    },
    /// The explicit implementation block.
    Implementation {
        /// Canonical normalized receiver type text.
        receiver: String,
        /// Canonical normalized applied interface type text.
        interface: String,
    },
    /// One implementation member owned by an implementation block.
    ImplementationMember {
        /// Canonical resolver identity of the implemented contract member.
        contract: String,
        /// Canonical normalized receiver type text.
        receiver: String,
        /// Canonical normalized applied interface type text.
        interface: String,
    },
}

impl EntityKey {
    /// Canonical VIBON encoding used as the revision membership selector.
    #[must_use]
    pub fn canonical(&self) -> String {
        use vibra_syntax::canonical_string_spelling as quote;
        match self {
            Self::Declaration {
                identity,
                entity_kind,
            } => format!(
                "(record kind: @declaration identity: {} entity-kind: @{})",
                quote(identity),
                entity_kind
            ),
            Self::Test { module, name } => format!(
                "(record kind: @test module: {} name: {})",
                quote(module),
                quote(name)
            ),
            Self::Implementation {
                receiver,
                interface,
            } => format!(
                "(record kind: @implementation receiver: {} interface: {})",
                quote(receiver),
                quote(interface)
            ),
            Self::ImplementationMember {
                contract,
                receiver,
                interface,
            } => format!(
                "(record kind: @implementation-member contract: {} receiver: {} interface: {})",
                quote(contract),
                quote(receiver),
                quote(interface)
            ),
        }
    }
}

/// Closed object kinds admitted by the checked graph contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectKind {
    /// Checked value declaration.
    Def,
    /// Checked function declaration.
    Defn,
    /// Nominal type declaration.
    Deftype,
    /// Interface declaration.
    Defint,
    /// Nominal effect declaration.
    Deffect,
    /// Explicit implementation block.
    Implementation,
    /// Checked test declaration.
    Test,
}

impl ObjectKind {
    /// Canonical VIBON atom spelling without the `@` marker.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Def => "def",
            Self::Defn => "defn",
            Self::Deftype => "deftype",
            Self::Defint => "defint",
            Self::Deffect => "deffect",
            Self::Implementation => "implementation",
            Self::Test => "test",
        }
    }
}

/// Closed reference edge kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    /// A resolved value use.
    ValueReference,
    /// A resolved type use.
    TypeReference,
    /// An interface contract edge.
    Contract,
    /// An implementation relationship.
    Implementation,
    /// A resolved effect owner.
    EffectOwner,
    /// A checked initializer dependency.
    Initializer,
    /// A checked function call.
    Call,
}

impl EdgeKind {
    /// Canonical closed edge atom spelling, without the `@` marker.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ValueReference => "value-reference",
            Self::TypeReference => "type-reference",
            Self::Contract => "contract",
            Self::Implementation => "implementation",
            Self::EffectOwner => "effect-owner",
            Self::Initializer => "initializer",
            Self::Call => "call",
        }
    }
}

/// A resolved graph edge target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeTarget {
    /// Another checked object in this revision.
    Object(ObjectAddress),
    /// A closed compiler or host registry identity pinned to its registry.
    TrustedExternal {
        /// Exact `@compiler:<symbol>` or `@host:<symbol>` identity.
        identity: String,
        /// Revision-pinned provider registry fingerprint.
        registry: String,
    },
}

impl EdgeTarget {
    /// Canonical VIBON target value.
    #[must_use]
    pub fn canonical(&self) -> String {
        use vibra_syntax::canonical_string_spelling as quote;
        match self {
            Self::Object(address) => format!(
                "(record kind: @object address: {})",
                quote(address.as_str())
            ),
            Self::TrustedExternal { identity, registry } => format!(
                "(record kind: @trusted-external identity: {} registry: {})",
                quote(identity),
                quote(registry)
            ),
        }
    }
}

/// Source provenance for an object or an owned part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectOrigin {
    /// Project-relative source identity.
    pub source_id: String,
    /// Half-open UTF-8 byte span in the attached source document.
    pub span: ByteSpan,
    /// Owner-relative path; empty for a top-level declaration span.
    pub part: Vec<String>,
}

/// Domain-separated lowercase SHA-256 address.
macro_rules! address_type {
    ($name:ident) => {
        #[doc = concat!("A canonical SHA-256 address for ", stringify!($name), ".")]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub(super) String);

        impl $name {
            /// Address in canonical `sha256:<64 lowercase hex>` form.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

address_type!(RevisionAddress);
address_type!(ComponentAddress);
address_type!(ObjectAddress);
address_type!(SourceAddress);
address_type!(SourceHash);

/// Failure while projecting or addressing accepted checked facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphEncodingError {
    /// A workspace or test scope was not fully accepted.
    UncheckedInput,
    /// A resolved entity needed to close the graph is unavailable.
    MissingResolution(String),
    /// Checked facts violate a single-checker invariant.
    InconsistentCheckedFacts(String),
    /// An invalid source document or malformed graph frame was supplied.
    MalformedInput(String),
    /// A source construct currently lacks immutable checker projection data.
    IncompleteProjection(String),
}
