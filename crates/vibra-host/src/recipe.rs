//! Binding recipes, the provider table, and recipe identity.
//!
//! `docs/spec/04-programs-and-packages.md`, "Targets", fixes a binary
//! target's `grants`: one binding recipe for each grant leaf of the entry,
//! each naming a `provider`, a `resource`, an optional `policy`, optional
//! `limits`, and `after`. The provider table is closed: [`PROVIDERS`] holds
//! its nine rows, each with the grant types it issues and the resource kinds
//! it accepts.
//!
//! [`BindingPlan::validate`] checks a target's recipes against the table and
//! the rules of the chapter: the leaf's type is one the provider issues; the
//! resource is of a kind it accepts; a process read names a `source` leaf of
//! the type the read needs, and a variable read one whose `names` include the
//! variable; a policy is present exactly for `@host.fs.directory` and its
//! rules name the leaf as their root; `after` contains the `source`, names
//! leaves, repeats none, and is acyclic. A fault is data: the project checker
//! of `vibra-workspace` maps it to its diagnostic, and startup checks the
//! plan again before it acquires anything.
//!
//! [`BindingPlan::order`] is the one startup order of "Startup order", and
//! [`recipe_identity`] the `recipeIdentity` of `StartupRecord.bindings[]` in
//! `docs/spec/05-tooling.md`, "Command payloads".

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use vibra_diagnostics::ByteSpan;
use vibra_ir::external::host::HostInterface;
use vibra_syntax::canonical_string;

use crate::policy::{Policy, sha256};

/// The name of a grant leaf, as its full canonical atom spelling:
/// `@notes`, `@logs.reader`. Leaf names order by their UTF-8 bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LeafName(String);

impl LeafName {
    /// The leaf `atom`, `@` included.
    #[must_use]
    pub fn new(atom: impl Into<String>) -> Self {
        Self(atom.into())
    }

    /// The atom, `@` included.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LeafName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A registered issuer of grants: one row of the provider table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Provider {
    /// `@host.fs.directory`.
    FsDirectory,
    /// `@host.io.stdin`.
    IoStdin,
    /// `@host.io.stdout`.
    IoStdout,
    /// `@host.io.stderr`.
    IoStderr,
    /// `@host.env.vars`.
    EnvVars,
    /// `@host.env.args`.
    EnvArgs,
    /// `@host.time.wall`.
    TimeWall,
    /// `@host.time.monotonic`.
    TimeMonotonic,
    /// `@host.random`.
    Random,
}

impl Provider {
    /// The provider's atom, such as `@host.fs.directory`.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::FsDirectory => "@host.fs.directory",
            Self::IoStdin => "@host.io.stdin",
            Self::IoStdout => "@host.io.stdout",
            Self::IoStderr => "@host.io.stderr",
            Self::EnvVars => "@host.env.vars",
            Self::EnvArgs => "@host.env.args",
            Self::TimeWall => "@host.time.wall",
            Self::TimeMonotonic => "@host.time.monotonic",
            Self::Random => "@host.random",
        }
    }

    /// The provider whose atom is `atom`, when the table has one.
    #[must_use]
    pub fn from_atom(atom: &str) -> Option<Self> {
        PROVIDERS
            .iter()
            .map(|row| row.provider)
            .find(|provider| provider.atom() == atom)
    }

    /// The provider's row of the table.
    #[must_use]
    pub const fn row(self) -> &'static ProviderRow {
        match self {
            Self::FsDirectory => &PROVIDERS[0],
            Self::IoStdin => &PROVIDERS[1],
            Self::IoStdout => &PROVIDERS[2],
            Self::IoStderr => &PROVIDERS[3],
            Self::EnvVars => &PROVIDERS[4],
            Self::EnvArgs => &PROVIDERS[5],
            Self::TimeWall => &PROVIDERS[6],
            Self::TimeMonotonic => &PROVIDERS[7],
            Self::Random => &PROVIDERS[8],
        }
    }
}

/// The kind of a recipe's resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceKind {
    /// `@fixedPath`, with `path`.
    FixedPath,
    /// `@argumentPath`, with `index` and `source`.
    ArgumentPath,
    /// `@variablePath`, with `name` and `source`.
    VariablePath,
    /// `@processStream`.
    ProcessStream,
    /// `@variableSet`, with `names`.
    VariableSet,
    /// `@processArguments`.
    ProcessArguments,
    /// `@systemClock`.
    SystemClock,
    /// `@systemRandom`.
    SystemRandom,
}

impl ResourceKind {
    /// The kind's atom, such as `@fixedPath`.
    #[must_use]
    pub const fn atom(self) -> &'static str {
        match self {
            Self::FixedPath => "@fixedPath",
            Self::ArgumentPath => "@argumentPath",
            Self::VariablePath => "@variablePath",
            Self::ProcessStream => "@processStream",
            Self::VariableSet => "@variableSet",
            Self::ProcessArguments => "@processArguments",
            Self::SystemClock => "@systemClock",
            Self::SystemRandom => "@systemRandom",
        }
    }
}

/// One row of the provider table: the provider, the grant types it issues,
/// and the resource kinds it accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderRow {
    /// The provider.
    pub provider: Provider,
    /// The grant types it issues.
    pub grant_types: &'static [HostInterface],
    /// The resource kinds it accepts.
    pub resource_kinds: &'static [ResourceKind],
}

/// The closed provider table of the programs chapter, in its order. A
/// project cannot add a provider.
pub const PROVIDERS: [ProviderRow; 9] = [
    ProviderRow {
        provider: Provider::FsDirectory,
        grant_types: &[
            HostInterface::DirReadBytes,
            HostInterface::DirReadText,
            HostInterface::DirList,
            HostInterface::DirWriteBytes,
            HostInterface::DirWriteText,
            HostInterface::DirStat,
        ],
        resource_kinds: &[
            ResourceKind::FixedPath,
            ResourceKind::ArgumentPath,
            ResourceKind::VariablePath,
        ],
    },
    ProviderRow {
        provider: Provider::IoStdin,
        grant_types: &[HostInterface::StdinText, HostInterface::StdinBytes],
        resource_kinds: &[ResourceKind::ProcessStream],
    },
    ProviderRow {
        provider: Provider::IoStdout,
        grant_types: &[HostInterface::StdoutText, HostInterface::StdoutBytes],
        resource_kinds: &[ResourceKind::ProcessStream],
    },
    ProviderRow {
        provider: Provider::IoStderr,
        grant_types: &[HostInterface::StderrText, HostInterface::StderrBytes],
        resource_kinds: &[ResourceKind::ProcessStream],
    },
    ProviderRow {
        provider: Provider::EnvVars,
        grant_types: &[HostInterface::EnvReadVar],
        resource_kinds: &[ResourceKind::VariableSet],
    },
    ProviderRow {
        provider: Provider::EnvArgs,
        grant_types: &[HostInterface::EnvReadArgs],
        resource_kinds: &[ResourceKind::ProcessArguments],
    },
    ProviderRow {
        provider: Provider::TimeWall,
        grant_types: &[HostInterface::WallClock],
        resource_kinds: &[ResourceKind::SystemClock],
    },
    ProviderRow {
        provider: Provider::TimeMonotonic,
        grant_types: &[HostInterface::MonotonicClock],
        resource_kinds: &[ResourceKind::SystemClock],
    },
    ProviderRow {
        provider: Provider::Random,
        grant_types: &[HostInterface::RandomBytes],
        resource_kinds: &[ResourceKind::SystemRandom],
    },
];

/// The resource a recipe binds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource {
    /// `@fixedPath`: a path, absolute or relative to the working directory at
    /// startup.
    FixedPath {
        /// The path.
        path: String,
    },
    /// `@argumentPath`: the path in one argument, read at startup through
    /// the `source` leaf's grant.
    ArgumentPath {
        /// The argument's index, from zero, without the executable's path.
        index: u64,
        /// The `EnvReadArgs` leaf that performs the read.
        source: LeafName,
    },
    /// `@variablePath`: the path in one variable, read at startup through
    /// the `source` leaf's grant.
    VariablePath {
        /// The variable's name.
        name: String,
        /// The `EnvReadVar` leaf that performs the read.
        source: LeafName,
    },
    /// `@processStream`.
    ProcessStream,
    /// `@variableSet`: the variables an `EnvReadVar` grant may read.
    VariableSet {
        /// The exact names of the variables.
        names: Vec<String>,
    },
    /// `@processArguments`.
    ProcessArguments,
    /// `@systemClock`.
    SystemClock,
    /// `@systemRandom`.
    SystemRandom,
}

impl Resource {
    /// The resource's kind.
    #[must_use]
    pub const fn kind(&self) -> ResourceKind {
        match self {
            Self::FixedPath { .. } => ResourceKind::FixedPath,
            Self::ArgumentPath { .. } => ResourceKind::ArgumentPath,
            Self::VariablePath { .. } => ResourceKind::VariablePath,
            Self::ProcessStream => ResourceKind::ProcessStream,
            Self::VariableSet { .. } => ResourceKind::VariableSet,
            Self::ProcessArguments => ResourceKind::ProcessArguments,
            Self::SystemClock => ResourceKind::SystemClock,
            Self::SystemRandom => ResourceKind::SystemRandom,
        }
    }

    /// The `source` leaf of a resource read from the process.
    #[must_use]
    pub const fn source(&self) -> Option<&LeafName> {
        match self {
            Self::ArgumentPath { source, .. } | Self::VariablePath { source, .. } => {
                Some(source)
            }
            Self::FixedPath { .. }
            | Self::ProcessStream
            | Self::VariableSet { .. }
            | Self::ProcessArguments
            | Self::SystemClock
            | Self::SystemRandom => None,
        }
    }

    /// The resource record's canonical bytes: `kind` first, then the fields
    /// the provider table names, in its order.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::new();
        out.push_str("{kind: ");
        out.push_str(self.kind().atom());
        match self {
            Self::FixedPath { path } => {
                out.push_str(" path: ");
                out.push_str(&canonical_string(path));
            }
            Self::ArgumentPath { index, source } => {
                let _ = write!(out, " index: {index}u64 source: {source}");
            }
            Self::VariablePath { name, source } => {
                out.push_str(" name: ");
                out.push_str(&canonical_string(name));
                out.push_str(" source: ");
                out.push_str(source.as_str());
            }
            Self::VariableSet { names } => {
                out.push_str(" names: [");
                for (index, name) in names.iter().enumerate() {
                    if index > 0 {
                        out.push(' ');
                    }
                    out.push_str(&canonical_string(name));
                }
                out.push(']');
            }
            Self::ProcessStream
            | Self::ProcessArguments
            | Self::SystemClock
            | Self::SystemRandom => {}
        }
        out.push('}');
        out
    }
}

/// A recipe's `limits`: each an upper bound for the life of the process, and
/// an omitted bound unbounded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Limits {
    /// The most invocations.
    pub invocations: Option<u64>,
    /// The most input bytes.
    pub input_bytes: Option<u64>,
    /// The most output bytes.
    pub output_bytes: Option<u64>,
}

impl Limits {
    /// Whether no bound is set, so that a recipe omits `limits`.
    #[must_use]
    pub const fn is_unbounded(&self) -> bool {
        self.invocations.is_none()
            && self.input_bytes.is_none()
            && self.output_bytes.is_none()
    }

    /// The limits record's canonical bytes: the bounds that are set, in the
    /// order `invocations`, `inputBytes`, `outputBytes`.
    #[must_use]
    pub fn canonical_bytes(&self) -> String {
        let mut out = String::from("{");
        let bounds = [
            ("invocations", self.invocations),
            ("inputBytes", self.input_bytes),
            ("outputBytes", self.output_bytes),
        ];
        for (name, bound) in bounds {
            if let Some(bound) = bound {
                if out.len() > 1 {
                    out.push(' ');
                }
                let _ = write!(out, "{name}: {bound}u64");
            }
        }
        out.push('}');
        out
    }
}

/// One grant leaf of a target and its recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    /// The leaf's name, such as `@logs.reader`.
    pub name: LeafName,
    /// The operation interface of the leaf's type, or of the payload of its
    /// `Option`.
    pub interface: HostInterface,
    /// Whether the leaf's type is an `Option`.
    pub optional: bool,
    /// The recipe's issuer.
    pub provider: Provider,
    /// The resource the issuer binds.
    pub resource: Resource,
    /// The recipe's policy, in its normal form.
    pub policy: Option<Policy>,
    /// The recipe's limits.
    pub limits: Limits,
    /// The leaves this one is acquired after.
    pub after: Vec<LeafName>,
    /// The recipe identity, [`recipe_identity`] of the recipe's parts.
    pub recipe_identity: String,
    /// The recipe, in the project file.
    pub span: ByteSpan,
}

/// The field of a recipe a fault is at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// `provider`.
    Provider,
    /// `resource`.
    Resource,
    /// The `source` of the resource.
    Source,
    /// `policy`.
    Policy,
    /// `after`.
    After,
}

/// What is wrong with a recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The provider does not issue the leaf's type.
    TypeNotIssued,
    /// The provider does not accept the resource's kind.
    KindNotAccepted,
    /// The `source` names no leaf of the plan.
    UnknownSource,
    /// The `source` leaf's type is not the one the read needs: `EnvReadArgs`
    /// for an argument, `EnvReadVar` for a variable.
    WrongSourceType,
    /// The `source` leaf's `names` do not include the variable.
    VariableNotNamed,
    /// A policy on a provider other than `@host.fs.directory`.
    UnexpectedPolicy,
    /// `@host.fs.directory` with no policy.
    MissingPolicy,
    /// A rule of the policy names another root than the leaf: the policy is
    /// invalid.
    ForeignRoot(String),
    /// `after` does not contain the `source` leaf.
    AfterLacksSource,
    /// `after` names something that is not a leaf of the plan.
    UnknownAfter(LeafName),
    /// `after` names a leaf twice.
    RepeatedAfter(LeafName),
    /// This entry of `after` closes a cycle of the relation.
    Cycle(LeafName),
}

/// One fault of a binding plan: the leaf, the field, and the problem.
///
/// A [`Problem::Cycle`] is a cyclic `after` relation, `@grant.bindingCycle`;
/// a [`Problem::ForeignRoot`] an invalid policy, `@grant.invalidPolicy`; any
/// other problem an invalid binding, `@grant.invalidBinding`. The project
/// checker reports each at its field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingFault {
    /// The leaf whose recipe is wrong.
    pub leaf: LeafName,
    /// The field of the recipe.
    pub field: Field,
    /// What is wrong.
    pub problem: Problem,
}

/// The binding recipes of one binary target, one leaf each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingPlan {
    /// The target's name, as its atom.
    pub target: String,
    /// The leaves, keyed by distinct names.
    pub leaves: Vec<Leaf>,
}

impl BindingPlan {
    /// The leaves in the UTF-8 byte order of their names.
    fn sorted(&self) -> Vec<&Leaf> {
        let mut leaves = self.leaves.iter().collect::<Vec<_>>();
        leaves.sort_by(|left, right| left.name.cmp(&right.name));
        leaves
    }

    /// Checks every recipe against the provider table, the `source` rule,
    /// the policy rule, and the `after` relation.
    ///
    /// # Errors
    ///
    /// Every fault, leaf by leaf in the order of their names, then each
    /// cycle of `after`, at the entry that closes it.
    pub fn validate(&self) -> Result<(), Vec<BindingFault>> {
        let by_name = self
            .leaves
            .iter()
            .map(|leaf| (leaf.name.as_str(), leaf))
            .collect::<BTreeMap<_, _>>();
        let mut faults = Vec::new();
        for leaf in self.sorted() {
            check_leaf(leaf, &by_name, &mut faults);
        }
        cycles(&self.sorted(), &by_name, &mut faults);
        if faults.is_empty() {
            Ok(())
        } else {
            Err(faults)
        }
    }

    /// The one startup order: repeatedly, among the leaves not yet taken
    /// whose `after` leaves are all taken, the one whose name is smallest in
    /// UTF-8 byte order. Readiness is computed again after every leaf. The
    /// order stops early only where `after` is cyclic or names no leaf,
    /// which [`Self::validate`] rejects.
    #[must_use]
    pub fn order(&self) -> Vec<&Leaf> {
        let mut waiting = self.sorted();
        let mut taken: BTreeSet<&str> = BTreeSet::new();
        let mut order = Vec::with_capacity(waiting.len());
        loop {
            let ready = waiting.iter().position(|leaf| {
                leaf.after
                    .iter()
                    .all(|before| taken.contains(before.as_str()))
            });
            let Some(index) = ready else {
                return order;
            };
            let leaf = waiting.remove(index);
            taken.insert(leaf.name.as_str());
            order.push(leaf);
        }
    }
}

fn check_leaf(
    leaf: &Leaf,
    by_name: &BTreeMap<&str, &Leaf>,
    faults: &mut Vec<BindingFault>,
) {
    let mut fault = |field, problem| {
        faults.push(BindingFault {
            leaf: leaf.name.clone(),
            field,
            problem,
        });
    };
    let row = leaf.provider.row();
    if !row.grant_types.contains(&leaf.interface) {
        fault(Field::Provider, Problem::TypeNotIssued);
    }
    if !row.resource_kinds.contains(&leaf.resource.kind()) {
        fault(Field::Resource, Problem::KindNotAccepted);
    }
    if let Some(problem) = source_problem(&leaf.resource, by_name) {
        fault(Field::Source, problem);
    }
    match (leaf.provider, &leaf.policy) {
        (Provider::FsDirectory, None) => fault(Field::Policy, Problem::MissingPolicy),
        (Provider::FsDirectory, Some(policy)) => {
            if let Some(rule) = policy
                .expression()
                .rules()
                .find(|rule| rule.root() != leaf.name.as_str())
            {
                fault(Field::Policy, Problem::ForeignRoot(rule.root().to_owned()));
            }
        }
        (_, Some(_)) => fault(Field::Policy, Problem::UnexpectedPolicy),
        (_, None) => {}
    }
    if let Some(source) = leaf.resource.source()
        && !leaf.after.contains(source)
    {
        fault(Field::After, Problem::AfterLacksSource);
    }
    let mut seen = BTreeSet::new();
    for before in &leaf.after {
        if !by_name.contains_key(before.as_str()) {
            fault(Field::After, Problem::UnknownAfter(before.clone()));
        }
        if !seen.insert(before.as_str()) {
            fault(Field::After, Problem::RepeatedAfter(before.clone()));
        }
    }
}

/// What is wrong with the `source` of a resource read from the process.
fn source_problem(
    resource: &Resource,
    by_name: &BTreeMap<&str, &Leaf>,
) -> Option<Problem> {
    let (needed, variable) = match resource {
        Resource::ArgumentPath { .. } => (HostInterface::EnvReadArgs, None),
        Resource::VariablePath { name, .. } => (HostInterface::EnvReadVar, Some(name)),
        _ => return None,
    };
    let source = resource.source()?;
    let Some(source) = by_name.get(source.as_str()) else {
        return Some(Problem::UnknownSource);
    };
    if source.interface != needed {
        return Some(Problem::WrongSourceType);
    }
    let named = match (variable, &source.resource) {
        (Some(variable), Resource::VariableSet { names }) => names.contains(variable),
        (Some(_), _) => false,
        (None, _) => true,
    };
    (!named).then_some(Problem::VariableNotNamed)
}

/// Reports each edge of `after` that closes a cycle, found by a depth-first
/// walk with an explicit stack, from each leaf in name order and along each
/// `after` list in its order.
fn cycles(
    sorted: &[&Leaf],
    by_name: &BTreeMap<&str, &Leaf>,
    faults: &mut Vec<BindingFault>,
) {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Open,
        Done,
    }
    let mut marks: BTreeMap<&str, Mark> = BTreeMap::new();
    for start in sorted {
        if marks.contains_key(start.name.as_str()) {
            continue;
        }
        marks.insert(start.name.as_str(), Mark::Open);
        let mut stack: Vec<(&Leaf, usize)> = vec![(start, 0)];
        while let Some((leaf, next)) = stack.last_mut() {
            let leaf: &Leaf = leaf;
            let Some(before) = leaf.after.get(*next) else {
                marks.insert(leaf.name.as_str(), Mark::Done);
                stack.pop();
                continue;
            };
            *next += 1;
            let Some(target) = by_name.get(before.as_str()) else {
                continue;
            };
            match marks.get(before.as_str()) {
                Some(Mark::Open) => faults.push(BindingFault {
                    leaf: leaf.name.clone(),
                    field: Field::After,
                    problem: Problem::Cycle(before.clone()),
                }),
                Some(Mark::Done) => {}
                None => {
                    marks.insert(target.name.as_str(), Mark::Open);
                    stack.push((*target, 0));
                }
            }
        }
    }
}

/// The canonical bytes of a recipe with its policy normalized: the grant
/// binding record of the project file, its fields in canonical order
/// (`provider`, `resource`, `policy`, `limits`, `after`), with an absent
/// `policy` and unbounded `limits` omitted.
#[must_use]
pub fn recipe_bytes(
    provider: Provider,
    resource: &Resource,
    policy: Option<&Policy>,
    limits: &Limits,
    after: &[LeafName],
) -> String {
    let mut out = String::new();
    out.push_str("{provider: ");
    out.push_str(provider.atom());
    out.push_str(" resource: ");
    out.push_str(&resource.canonical_bytes());
    if let Some(policy) = policy {
        out.push_str(" policy: ");
        out.push_str(&policy.canonical_bytes());
    }
    if !limits.is_unbounded() {
        out.push_str(" limits: ");
        out.push_str(&limits.canonical_bytes());
    }
    out.push_str(" after: [");
    for (index, before) in after.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(before.as_str());
    }
    out.push_str("]}");
    out
}

/// The text that starts the hashed bytes of a recipe identity.
pub const RECIPE_IDENTITY_TAG: &str = "vibraRecipeIdentityV2";

/// The recipe identity: `sha256:` and the SHA-256 of
/// `vibraRecipeIdentityV2`, a zero byte, and [`recipe_bytes`]. It covers the
/// provider, the resource, the normalized policy, the limits, and `after`.
#[must_use]
pub fn recipe_identity(
    provider: Provider,
    resource: &Resource,
    policy: Option<&Policy>,
    limits: &Limits,
    after: &[LeafName],
) -> String {
    let mut bytes = Vec::from(RECIPE_IDENTITY_TAG.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(
        recipe_bytes(provider, resource, policy, limits, after).as_bytes(),
    );
    sha256(&bytes)
}
