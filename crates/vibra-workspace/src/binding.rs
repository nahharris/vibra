//! The validation of a target's recipes into a binding plan.
//!
//! `docs/spec/04-programs-and-packages.md`, "Targets", and
//! `docs/spec/03-effects.md`, "Target admission". Every grant leaf of the
//! entry MUST have a binding recipe, and project checking validates every
//! recipe. This module does it in four steps:
//!
//! 1. **Keys.** The keys of `grants` MUST be exactly the entry's leaves: a
//!    missing or extra key is `@grant.invalidBinding` at the `grants` field,
//!    related to the parameter, for a required and an optional leaf alike.
//! 2. **Recipes.** Each recipe of a leaf becomes a [`Leaf`]: its `provider`
//!    MUST be in the closed provider table and the leaf's interface one that
//!    some provider issues, or the recipe is `@grant.invalidBinding` at
//!    `provider` (so a `TestWorld` leaf, which no provider issues, is
//!    refused there); its `policy` is decoded as a `@scopePolicy.v1`
//!    document, and a document that is not one is `@grant.invalidPolicy`,
//!    one over the limits `@grant.policyTooComplex`, both at the policy
//!    value.
//! 3. **The plan.** [`BindingPlan::validate`] holds the provider table, the
//!    `source` rule, the policy rule, and the `after` relation; this module
//!    holds no copy of them. Each [`BindingFault`] is reported at its field:
//!    a cycle is `@grant.bindingCycle` at the entry of `after` that closes
//!    it, related along the cycle; a policy naming another root is
//!    `@grant.invalidPolicy` at the policy value; a missing policy is
//!    `@grant.invalidBinding` at the binding; a variable the `source` leaf's
//!    `names` do not include is `@grant.invalidBinding` at `name`; any other
//!    fault is `@grant.invalidBinding` at its field. A fault that only
//!    repeats one reported in steps 1 and 2 is not reported again.
//! 4. **Startup steps.** An `@argumentPath` or `@variablePath` recipe is a
//!    startup step whose row is its `source` leaf's interface
//!    ([`StartupStep`]); [`crate::entry`] holds the steps to the admission
//!    ceiling.
//!
//! A target whose recipes all check has a validated [`BindingPlan`], which
//! its [`EntryFacts`](crate::entry::EntryFacts) hold.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_host::policy::{self, Policy, PolicyError};
use vibra_host::recipe::{
    BindingFault, BindingPlan, Field, Leaf, LeafName, Limits, Problem, Provider,
    Resource, recipe_identity,
};
use vibra_ir::EntityRef;
use vibra_ir::external::host::HostInterface;
use vibra_syntax::{TypedNode, TypedValue};
use vibra_types::{EntryLeaf, EntrySignature};

use crate::project::{Project, Recipe, Target};

/// One startup step: the read of a path from the process, through the
/// grant of the recipe's `source` leaf, before the entry runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupStep {
    leaf: String,
    interface: EntityRef,
    recipe: ByteSpan,
}

impl StartupStep {
    /// The leaf whose recipe reads the path, `@` included.
    #[must_use]
    pub fn leaf(&self) -> &str {
        &self.leaf
    }

    /// The step's row: the interface of the `source` leaf.
    #[must_use]
    pub const fn interface(&self) -> &EntityRef {
        &self.interface
    }

    /// Where the recipe is written in the project file.
    #[must_use]
    pub const fn recipe(&self) -> ByteSpan {
        self.recipe
    }
}

/// The outcome of [`check`] for one binary target.
#[derive(Clone, Debug)]
pub struct BindingCheck {
    diagnostics: Vec<Diagnostic>,
    plan: Option<BindingPlan>,
    startup: Vec<StartupStep>,
}

impl BindingCheck {
    /// The faults of the keys and the recipes, in the project file.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The validated plan, when no recipe has a fault.
    #[must_use]
    pub const fn plan(&self) -> Option<&BindingPlan> {
        self.plan.as_ref()
    }

    /// The startup steps of the recipes whose `source` names a leaf of the
    /// entry, in the order of the recipes' keys.
    #[must_use]
    pub fn startup(&self) -> &[StartupStep] {
        &self.startup
    }
}

/// Where the parts of one recipe are written.
struct Spans {
    recipe: ByteSpan,
    provider: ByteSpan,
    resource: ByteSpan,
    source: Option<ByteSpan>,
    name: Option<ByteSpan>,
    policy: Option<ByteSpan>,
    after: ByteSpan,
    after_entries: Vec<ByteSpan>,
}

impl Spans {
    fn of(recipe: &Recipe) -> Self {
        let binding = recipe.binding();
        let at = |node: Option<&TypedNode>| node.map_or(recipe.span(), TypedNode::span);
        let resource = binding.get("resource");
        let after = binding.get("after");
        Self {
            recipe: recipe.span(),
            provider: at(binding.get("provider")),
            resource: at(resource),
            source: resource
                .and_then(|resource| resource.get("source"))
                .map(TypedNode::span),
            name: resource
                .and_then(|resource| resource.get("name"))
                .map(TypedNode::span),
            policy: binding.get("policy").map(TypedNode::span),
            after: at(after),
            after_entries: after
                .and_then(TypedNode::as_list)
                .unwrap_or_default()
                .iter()
                .map(TypedNode::span)
                .collect(),
        }
    }

    /// The span of `field`.
    fn field(&self, field: Field) -> ByteSpan {
        match field {
            Field::Provider => self.provider,
            Field::Resource => self.resource,
            Field::Source => self.source.unwrap_or(self.resource),
            Field::Policy => self.policy.unwrap_or(self.recipe),
            Field::After => self.after,
        }
    }
}

fn integer(node: Option<&TypedNode>) -> Option<u64> {
    node.and_then(TypedNode::as_integer)
        .and_then(|value| u64::try_from(value).ok())
}

/// The resource of a validated `resource` record.
fn resource_of(node: &TypedNode) -> Option<Resource> {
    let text = |field: &str| {
        node.get(field)
            .and_then(TypedNode::as_str)
            .map(str::to_owned)
    };
    let source = || {
        node.get("source")
            .and_then(TypedNode::as_atom)
            .map(LeafName::new)
    };
    Some(match node.get("kind").and_then(TypedNode::as_atom)? {
        "@fixedPath" => Resource::FixedPath {
            path: text("path")?,
        },
        "@argumentPath" => Resource::ArgumentPath {
            index: integer(node.get("index"))?,
            source: source()?,
        },
        "@variablePath" => Resource::VariablePath {
            name: text("name")?,
            source: source()?,
        },
        "@processStream" => Resource::ProcessStream,
        "@variableSet" => Resource::VariableSet {
            names: node
                .get("names")?
                .as_list()?
                .iter()
                .map(|name| name.as_str().map(str::to_owned))
                .collect::<Option<_>>()?,
        },
        "@processArguments" => Resource::ProcessArguments,
        "@systemClock" => Resource::SystemClock,
        "@systemRandom" => Resource::SystemRandom,
        _ => return None,
    })
}

/// The limits of a validated grant binding.
fn limits_of(binding: &TypedNode) -> Limits {
    let limits = binding.get("limits");
    let bound = |name: &str| integer(limits.and_then(|limits| limits.get(name)));
    Limits {
        invocations: bound("invocations"),
        input_bytes: bound("inputBytes"),
        output_bytes: bound("outputBytes"),
    }
}

/// The `@scopePolicy.v1` document of an opaque `policy` value.
fn policy_of(node: &TypedNode) -> Result<Policy, PolicyError> {
    match node.value() {
        TypedValue::Opaque(bytes) => policy::decode(bytes.as_bytes()),
        _ => Err(PolicyError::Malformed {
            span: node.span(),
            reason: "a policy is a `@scopePolicy.v1` document".to_owned(),
        }),
    }
}

/// The parameter a leaf or key `@p.f…` names: `p`.
fn parameter_of(name: &str) -> &str {
    let name = name.strip_prefix('@').unwrap_or(name);
    name.split('.').next().unwrap_or(name)
}

/// The faults, the plan, and the startup steps of what is being checked.
struct Checker<'a> {
    source_id: &'a str,
    diagnostics: Vec<Diagnostic>,
    /// The recipe fields already reported, whose plan faults would repeat it.
    reported: HashSet<(String, Field)>,
}

impl Checker<'_> {
    fn push(&mut self, code: DiagnosticCode, span: ByteSpan, message: String) {
        self.diagnostics
            .push(Diagnostic::new(code, span, message).with_source_id(self.source_id));
    }

    fn report(
        &mut self,
        leaf: &str,
        field: Field,
        code: DiagnosticCode,
        span: ByteSpan,
        message: String,
    ) {
        self.reported.insert((leaf.to_owned(), field));
        self.push(code, span, message);
    }
}

/// Checks the `grants` of the binary target `target` of `project` against
/// the checked entry `signature`.
#[must_use]
pub fn check(
    project: &Project,
    target: &Target,
    signature: &EntrySignature,
) -> BindingCheck {
    let mut checker = Checker {
        source_id: project.source_id(),
        diagnostics: Vec::new(),
        reported: HashSet::new(),
    };
    let leaves = signature.leaves();
    let by_leaf = leaves
        .iter()
        .map(|leaf| (leaf.name(), leaf))
        .collect::<BTreeMap<_, _>>();
    let recipes = target
        .recipes()
        .iter()
        .map(|recipe| (recipe.key(), recipe))
        .collect::<BTreeMap<_, _>>();
    check_keys(&mut checker, target, signature, &by_leaf, &recipes);

    let mut built = Vec::new();
    let mut spans = BTreeMap::new();
    for recipe in target.recipes() {
        let Some(entry_leaf) = by_leaf.get(recipe.key()) else {
            continue;
        };
        let recipe_spans = Spans::of(recipe);
        if let Some(leaf) = build_leaf(&mut checker, recipe, entry_leaf, &recipe_spans)
        {
            built.push(leaf);
        }
        spans.insert(recipe.key().to_owned(), recipe_spans);
    }
    let startup = built
        .iter()
        .filter_map(|leaf| {
            let source = by_leaf.get(leaf.resource.source()?.as_str())?;
            Some(StartupStep {
                leaf: leaf.name.as_str().to_owned(),
                interface: source.leaf().interface().clone(),
                recipe: leaf.span,
            })
        })
        .collect();
    let plan = BindingPlan {
        target: target.name().to_owned(),
        leaves: built,
    };
    if let Err(faults) = plan.validate() {
        report_faults(&mut checker, &plan, &spans, &recipes, &faults);
    }
    let plan = checker.diagnostics.is_empty().then_some(plan);
    BindingCheck {
        diagnostics: checker.diagnostics,
        plan,
        startup,
    }
}

/// Reports every leaf of the entry with no key, and every key that is no
/// leaf, at the `grants` field, in the UTF-8 byte order of their names.
fn check_keys(
    checker: &mut Checker<'_>,
    target: &Target,
    signature: &EntrySignature,
    by_leaf: &BTreeMap<&str, &EntryLeaf>,
    recipes: &BTreeMap<&str, &Recipe>,
) {
    let grants = target.grants_span().unwrap_or(target.span());
    let names = by_leaf
        .keys()
        .chain(recipes.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    for name in names {
        let message = match (by_leaf.contains_key(name), recipes.contains_key(name)) {
            (true, false) => {
                format!("the entry's grant leaf `{name}` has no binding recipe")
            }
            (false, true) => format!("`{name}` is not a grant leaf of the entry"),
            _ => continue,
        };
        let parameter = parameter_of(name);
        let mut diagnostic =
            Diagnostic::new(DiagnosticCode::GrantInvalidBinding, grants, message)
                .with_source_id(checker.source_id);
        if let Some(span) = signature.parameter_span(parameter) {
            diagnostic = diagnostic.with_related_source(
                signature.source_id(),
                span,
                format!("the parameter `{parameter}`"),
            );
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// The leaf of one recipe of an entry leaf, when its provider and resource
/// can be read; a policy that does not decode is reported and left out.
fn build_leaf(
    checker: &mut Checker<'_>,
    recipe: &Recipe,
    entry_leaf: &EntryLeaf,
    spans: &Spans,
) -> Option<Leaf> {
    let name = recipe.key();
    let binding = recipe.binding();
    let atom = binding
        .get("provider")
        .and_then(TypedNode::as_atom)
        .unwrap_or_default();
    let Some(provider) = Provider::from_atom(atom) else {
        checker.report(
            name,
            Field::Provider,
            DiagnosticCode::GrantInvalidBinding,
            spans.provider,
            format!("`{atom}` is no provider: the provider table is closed"),
        );
        return None;
    };
    let entity = entry_leaf.leaf().interface();
    let Some(interface) = HostInterface::by_owner_path(&format!("@{}", entity.path()))
    else {
        checker.report(
            name,
            Field::Provider,
            DiagnosticCode::GrantInvalidBinding,
            spans.provider,
            format!(
                "no provider issues `@{}`, so no recipe can bind `{name}`",
                entity.path()
            ),
        );
        return None;
    };
    let Some(resource) = binding.get("resource").and_then(resource_of) else {
        checker.report(
            name,
            Field::Resource,
            DiagnosticCode::GrantInvalidBinding,
            spans.resource,
            "the resource is not one of the provider table's kinds".to_owned(),
        );
        return None;
    };
    let policy = match binding.get("policy").map(policy_of) {
        None => None,
        Some(Ok(policy)) => Some(policy),
        Some(Err(error)) => {
            let code = if error == PolicyError::TooComplex {
                DiagnosticCode::GrantPolicyTooComplex
            } else {
                DiagnosticCode::GrantInvalidPolicy
            };
            checker.report(
                name,
                Field::Policy,
                code,
                spans.field(Field::Policy),
                format!("the policy of `{name}`: {error}"),
            );
            None
        }
    };
    let limits = limits_of(binding);
    let after = binding
        .get("after")
        .and_then(TypedNode::as_list)
        .unwrap_or_default()
        .iter()
        .map(|before| LeafName::new(before.as_atom().unwrap_or_default()))
        .collect::<Vec<_>>();
    let recipe_identity =
        recipe_identity(provider, &resource, policy.as_ref(), &limits, &after);
    Some(Leaf {
        name: LeafName::new(name),
        interface,
        optional: entry_leaf.leaf().optional(),
        provider,
        resource,
        policy,
        limits,
        after,
        recipe_identity,
        span: recipe.span(),
    })
}

/// Reports the faults of [`BindingPlan::validate`] at their fields.
fn report_faults(
    checker: &mut Checker<'_>,
    plan: &BindingPlan,
    spans: &BTreeMap<String, Spans>,
    recipes: &BTreeMap<&str, &Recipe>,
    faults: &[BindingFault],
) {
    let by_name = plan
        .leaves
        .iter()
        .map(|leaf| (leaf.name.as_str(), leaf))
        .collect::<BTreeMap<_, _>>();
    // How many cycle faults each `(leaf, entry)` pair has had, so that a
    // repeated entry's second fault lands on its second occurrence.
    let mut closed: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for fault in faults {
        let name = fault.leaf.as_str();
        let (Some(leaf), Some(at)) = (by_name.get(name), spans.get(name)) else {
            continue;
        };
        if checker.reported.contains(&(name.to_owned(), fault.field)) {
            continue;
        }
        let mut code = DiagnosticCode::GrantInvalidBinding;
        let mut span = at.field(fault.field);
        let mut related = Vec::new();
        let message = match &fault.problem {
            Problem::TypeNotIssued => format!(
                "`{}` does not issue `{}`, the type of `{name}`",
                leaf.provider.atom(),
                leaf.interface.name()
            ),
            Problem::KindNotAccepted => format!(
                "`{}` does not accept a resource of kind `{}`",
                leaf.provider.atom(),
                leaf.resource.kind().atom()
            ),
            Problem::UnknownSource => {
                let source = leaf.resource.source().map(LeafName::as_str);
                if source.is_some_and(|source| recipes.contains_key(source)) {
                    continue;
                }
                format!(
                    "the `source` `{}` is not a grant leaf of the entry",
                    source.unwrap_or_default()
                )
            }
            Problem::WrongSourceType => format!(
                "the `source` of `{name}` is not a leaf of the type its read needs: \
                 `EnvReadArgs` for an argument, `EnvReadVar` for a variable"
            ),
            Problem::VariableNotNamed => {
                span = at.name.unwrap_or(span);
                format!(
                    "the `source` leaf's `names` do not include the variable of `{name}`"
                )
            }
            Problem::UnexpectedPolicy => format!(
                "`{}` takes no policy: a policy is for `@host.fs.directory` only",
                leaf.provider.atom()
            ),
            Problem::MissingPolicy => {
                span = at.recipe;
                format!("the `@host.fs.directory` recipe `{name}` requires a policy")
            }
            Problem::ForeignRoot(root) => {
                code = DiagnosticCode::GrantInvalidPolicy;
                format!("a rule of the policy names the root `{root}`, not `{name}`")
            }
            Problem::AfterLacksSource => {
                format!("the `after` of `{name}` does not contain its `source` leaf")
            }
            Problem::UnknownAfter(before) => {
                if recipes.contains_key(before.as_str()) {
                    continue;
                }
                format!("`after` names `{before}`, which is not a key of `grants`")
            }
            Problem::RepeatedAfter(before) => {
                format!("`after` names `{before}` more than once")
            }
            Problem::Cycle(before) => {
                code = DiagnosticCode::GrantBindingCycle;
                let seen = closed.entry((name, before.as_str())).or_default();
                if let Some(entry) = leaf
                    .after
                    .iter()
                    .zip(&at.after_entries)
                    .filter(|(entry, _)| *entry == before)
                    .nth(*seen)
                    .map(|(_, entry)| *entry)
                {
                    span = entry;
                }
                *seen += 1;
                related = cycle(&by_name, spans, before.as_str(), name);
                format!(
                    "the entry `{before}` of the `after` of `{name}` closes a cycle"
                )
            }
        };
        let mut diagnostic =
            Diagnostic::new(code, span, message).with_source_id(checker.source_id);
        for edge in related {
            diagnostic =
                diagnostic.with_related(edge, "an entry of `after` on the cycle");
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// The entries of `after` that lead from the leaf `from` back to the leaf
/// `to`, the shortest such chain, each list followed in written order: the
/// rest of the cycle an entry of `to`'s list closes.
fn cycle(
    by_name: &BTreeMap<&str, &Leaf>,
    spans: &BTreeMap<String, Spans>,
    from: &str,
    to: &str,
) -> Vec<ByteSpan> {
    let mut parents: BTreeMap<&str, (&str, ByteSpan)> = BTreeMap::new();
    let mut seen = BTreeSet::from([from]);
    let mut queue = VecDeque::from([from]);
    while let Some(node) = queue.pop_front() {
        if node == to {
            break;
        }
        let (Some(leaf), Some(at)) = (by_name.get(node), spans.get(node)) else {
            continue;
        };
        for (before, span) in leaf.after.iter().zip(&at.after_entries) {
            if seen.insert(before.as_str()) {
                parents.insert(before.as_str(), (node, *span));
                queue.push_back(before.as_str());
            }
        }
    }
    let mut chain = Vec::new();
    let mut at = to;
    while at != from {
        let Some((parent, span)) = parents.get(at) else {
            return Vec::new();
        };
        chain.push(*span);
        at = parent;
    }
    chain.reverse();
    chain
}
