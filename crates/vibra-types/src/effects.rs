//! Performed effect rows over the function-call graph
//! (`docs/spec/03-effects.md`, "Function effect rows", "Performed rows are
//! transitively closed", and "The witness of `@effect.outside-ceiling`").
//!
//! The graph is built once from the checked IR, so both checkers and every
//! later query share one result. A node is a module-level function, an
//! operation, a test, a module value's initializer, or a `lambda`; an edge is
//! an application, classified by what it applies:
//!
//! - a module-level function contributes its performed row;
//! - an effect operation contributes its owner root together with its additive
//!   row, whatever its body performs;
//! - a function value contributes the row of its type; and
//! - an interface contract member contributes the contract ceiling, never the
//!   row of an implementation.
//!
//! Creating a function value performs nothing, so the body of a `lambda` is a
//! node of its own and not part of the function that writes it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    CallTarget, CheckedFunction, CheckedGlobal, EffectRoot, EffectRow, Expr,
    FunctionSignature, SourceOrigin, Type,
};

use crate::nominal::TypeNames;

/// What an application applies.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Callee {
    /// A function or operation of the checked program, by node index.
    Node(usize),
    /// A function value, whose callee is unknown: its type's row.
    Value { description: String, row: EffectRow },
    /// A contract member: the contract's ceiling.
    Contract {
        description: String,
        identity: String,
        row: EffectRow,
    },
}

/// One application in a body.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Site {
    origin: SourceOrigin,
    callee: Callee,
}

/// What a node of the function-call graph is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectNodeKind {
    /// A module-level function or a method.
    Function,
    /// An effect operation.
    Operation,
    /// A `test` declaration.
    Test,
    /// A `lambda`, which has no identity.
    Lambda,
    /// A module value's initializer.
    Value,
}

/// One node of the function-call graph: a function, an operation, a test, a
/// module value's initializer, or a `lambda`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectNode {
    index: usize,
    /// The canonical declaration identity, empty for a `lambda`.
    name: String,
    /// The identity a note names this node by.
    label: String,
    kind: EffectNodeKind,
    origin: SourceOrigin,
    /// Whether the body of this declaration checked.
    checked: bool,
    /// The exact row the declaration writes, which the performed row must be
    /// contained in. For an operation it already holds the owner root.
    ceiling: EffectRow,
    /// The signature of a function, method, operation, `lambda`, or test.
    signature: Option<FunctionSignature>,
    /// The root that owns an operation.
    owner: Option<EffectRoot>,
    /// Whether the checker reads the body for applications: a host
    /// operation has none.
    has_body: bool,
    /// Whether the declaration is a member written in an `impl` block, which
    /// has no atom identity.
    impl_member: bool,
    sites: Vec<Site>,
    /// Whether the body of this function did not check, so its performed row
    /// is not known.
    unknown: bool,
}

impl EffectNode {
    /// The position of this node in [`EffectGraph::nodes`].
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// The canonical declaration identity, empty for a `lambda`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The canonical identity the node is reported by. A `lambda` and a
    /// member written in an `impl` block have no atom identity.
    #[must_use]
    pub fn identity(&self) -> Option<&str> {
        (!self.name.is_empty() && !self.impl_member).then_some(self.name.as_str())
    }

    /// What the node is.
    #[must_use]
    pub const fn kind(&self) -> EffectNodeKind {
        self.kind
    }

    /// The source document that holds the node.
    #[must_use]
    pub fn source_id(&self) -> &str {
        self.origin.source_id()
    }

    /// The span of the whole declaration, `test`, or `lambda`.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.origin.span()
    }

    /// The written or default-empty ceiling. For an operation it is the
    /// complete row: the owner root together with the additive row.
    #[must_use]
    pub const fn ceiling(&self) -> &EffectRow {
        &self.ceiling
    }

    /// The signature of a function, method, operation, `lambda`, or test.
    #[must_use]
    pub const fn signature(&self) -> Option<&FunctionSignature> {
        self.signature.as_ref()
    }

    /// Whether the body of the declaration checked.
    #[must_use]
    pub const fn checked(&self) -> bool {
        self.checked
    }

    /// Whether the declaration is an operation bound to the host, which has
    /// no source-level body.
    #[must_use]
    pub const fn is_external_operation(&self) -> bool {
        self.owner.is_some() && !self.has_body
    }
}

struct Closure<'a> {
    body: &'a Expr,
    ceiling: EffectRow,
    signature: FunctionSignature,
    origin: SourceOrigin,
}

/// Checks every written ceiling against the performed row of its body and
/// reports each root a ceiling does not contain with its witness, and each
/// written root that is never performed. The graph that produced the
/// diagnostics is returned with them: it is the one result both the checker
/// and a query read.
pub(crate) fn check(
    functions: &[Option<CheckedFunction>],
    globals: &[Option<CheckedGlobal>],
    headers: &[crate::FunctionHeader],
    spans: &[Option<ByteSpan>],
    types: &TypeNames,
    entries: &[EntryConsent],
) -> (Vec<Diagnostic>, EffectGraph) {
    let mut graph = EffectGraph::build(functions, globals, headers, spans);
    let diagnostics = graph.diagnostics(types, entries);
    graph.entries = entries
        .iter()
        .map(|entry| EffectEntry {
            unit: entry.unit.clone(),
            identity: entry.identity.clone(),
            array: EffectRow::new(entry.array.iter().map(|(root, _)| root.clone())),
        })
        .collect();
    (diagnostics, graph)
}

/// The consent of one binary target, to be admitted against the performed
/// row of its entry (`docs/spec/03-effects.md`, "Target consent").
pub(crate) struct EntryConsent {
    /// The target unit that owns the entry.
    pub(crate) unit: String,
    /// The canonical identity of the entry declaration.
    pub(crate) identity: String,
    /// The index of the entry declaration in the checked functions.
    pub(crate) function: usize,
    /// The project document and span of the `entry:` value.
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
    /// The roots of the target `effects` array and the span of each atom.
    pub(crate) array: Vec<(EffectRoot, ByteSpan)>,
}

/// The consent of one binary target: its entry and the `effects` array that
/// admitted it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectEntry {
    unit: String,
    identity: String,
    array: EffectRow,
}

impl EffectEntry {
    /// The target unit that owns the entry.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// The canonical identity of the entry declaration.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The target `effects` array as a row: the consent that admitted the
    /// entry, not the entry's own ceiling.
    #[must_use]
    pub const fn array(&self) -> &EffectRow {
        &self.array
    }
}

/// What a witness hop applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WitnessCallee {
    /// A function, by canonical identity; a member written in an `impl` block
    /// has none.
    Function(Option<String>),
    /// An effect operation, by canonical identity.
    Operation(String),
    /// A function value, which has no identity.
    FunctionValue,
    /// An interface contract member, by canonical identity.
    ContractMember(String),
}

/// One hop of a witness: a call site and what it applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessHop {
    source_id: String,
    span: ByteSpan,
    callee: WitnessCallee,
}

impl WitnessHop {
    /// The source document of the call site.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The span of the call site.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// What the call site applies.
    #[must_use]
    pub const fn callee(&self) -> &WitnessCallee {
        &self.callee
    }
}

/// The chain that carries one performed root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootWitness {
    root: EffectRoot,
    hops: Vec<WitnessHop>,
}

impl RootWitness {
    /// The performed root.
    #[must_use]
    pub const fn root(&self) -> &EffectRoot {
        &self.root
    }

    /// The call sites from the queried body down to the declaration that
    /// introduces the root, in the order of the checker's witness.
    #[must_use]
    pub fn hops(&self) -> &[WitnessHop] {
        &self.hops
    }
}

/// The function-call graph and the performed row of every node: the one
/// result the checker's diagnostics and every query read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectGraph {
    /// Whether a graph was built, as opposed to a source that stopped before
    /// it could be checked.
    available: bool,
    nodes: Vec<EffectNode>,
    performed: Vec<EffectRow>,
    /// Whether the performed row of a node is a lower bound only, because
    /// its body, or the body of a function it reaches, did not check. An
    /// error can still be reported from a lower bound, but "never
    /// performed" cannot.
    partial: Vec<bool>,
    entries: Vec<EffectEntry>,
}

impl EffectGraph {
    fn build(
        functions: &[Option<CheckedFunction>],
        globals: &[Option<CheckedGlobal>],
        headers: &[crate::FunctionHeader],
        spans: &[Option<ByteSpan>],
    ) -> Self {
        let mut nodes = Vec::with_capacity(functions.len() + globals.len());
        let mut bodies = Vec::<Option<&Expr>>::new();
        let kind_of = |index: usize, owner: Option<&EffectRoot>| {
            if owner.is_some() {
                EffectNodeKind::Operation
            } else if headers
                .get(index)
                .is_some_and(|header| header.test.is_some())
            {
                EffectNodeKind::Test
            } else {
                EffectNodeKind::Function
            }
        };
        for (index, function) in functions.iter().enumerate() {
            match function {
                Some(function) => {
                    let owner = function.effect_owner().cloned();
                    let has_body = !function.is_external_wrapper();
                    nodes.push(EffectNode {
                        index: nodes.len(),
                        name: function.name().to_owned(),
                        label: identity(function.name()),
                        kind: kind_of(index, owner.as_ref()),
                        origin: function.origin().clone(),
                        checked: true,
                        ceiling: function.signature().effects().clone(),
                        signature: Some(function.signature().clone()),
                        owner,
                        has_body,
                        impl_member: headers
                            .get(index)
                            .is_some_and(|header| header.impl_member.is_some()),
                        sites: Vec::new(),
                        unknown: false,
                    });
                    bodies.push(has_body.then(|| function.body()));
                }
                // A function that did not check still has its declared
                // signature, which is all a caller needs of an operation. An
                // ordinary function's performed row is then unknown.
                None => {
                    let header = headers.get(index);
                    let owner = header.and_then(|header| header.effect_owner.clone());
                    let origin = SourceOrigin::new(
                        header.map_or("", |header| header.source_id.as_str()),
                        spans
                            .get(index)
                            .copied()
                            .flatten()
                            .unwrap_or_else(|| ByteSpan::empty_at(0)),
                    );
                    nodes.push(EffectNode {
                        index: nodes.len(),
                        name: header.map_or_else(String::new, |header| header.name.clone()),
                        label: header
                            .map_or_else(String::new, |header| identity(&header.name)),
                        kind: kind_of(index, owner.as_ref()),
                        origin,
                        checked: false,
                        ceiling: header.map_or_else(EffectRow::empty, |header| {
                            header.signature.effects().clone()
                        }),
                        signature: header.map(|header| header.signature.clone()),
                        has_body: false,
                        impl_member: header
                            .is_some_and(|header| header.impl_member.is_some()),
                        unknown: owner.is_none(),
                        owner,
                        sites: Vec::new(),
                    });
                    bodies.push(None);
                }
            }
        }
        for global in globals.iter().flatten() {
            nodes.push(EffectNode {
                index: nodes.len(),
                name: global.name().to_owned(),
                label: identity(global.name()),
                kind: EffectNodeKind::Value,
                origin: global.origin().clone(),
                checked: true,
                // A module value has no `effects:` attribute, so its
                // initializer has the default-empty ceiling.
                ceiling: EffectRow::empty(),
                signature: None,
                owner: None,
                has_body: true,
                impl_member: false,
                sites: Vec::new(),
                unknown: false,
            });
            bodies.push(Some(global.initializer()));
        }
        let function_count = functions.len();
        let mut cursor = 0;
        while cursor < nodes.len() {
            if let Some(body) = bodies.get(cursor).copied().flatten() {
                let (mut sites, closures) = collect(body, function_count);
                sites.sort_by(|left, right| {
                    (
                        left.origin.source_id(),
                        left.origin.span().start(),
                        left.origin.span().end(),
                    )
                        .cmp(&(
                            right.origin.source_id(),
                            right.origin.span().start(),
                            right.origin.span().end(),
                        ))
                });
                if let Some(node) = nodes.get_mut(cursor) {
                    node.sites = sites;
                }
                for closure in closures {
                    nodes.push(EffectNode {
                        index: nodes.len(),
                        name: String::new(),
                        label: format!(
                            "a lambda at byte {}",
                            closure.origin.span().start()
                        ),
                        kind: EffectNodeKind::Lambda,
                        origin: closure.origin,
                        checked: true,
                        ceiling: closure.ceiling,
                        signature: Some(closure.signature),
                        owner: None,
                        has_body: true,
                        impl_member: false,
                        sites: Vec::new(),
                        unknown: false,
                    });
                    bodies.push(Some(closure.body));
                }
            }
            cursor += 1;
        }
        let mut graph = Self {
            available: true,
            performed: vec![EffectRow::empty(); nodes.len()],
            partial: vec![false; nodes.len()],
            nodes,
            entries: Vec::new(),
        };
        graph.close();
        graph.taint();
        graph
    }

    /// Whether a graph was built. A source that stopped before it could be
    /// checked has none, and a query then reports its effects as unavailable.
    #[must_use]
    pub const fn available(&self) -> bool {
        self.available
    }

    /// Every node: the functions, operations, and tests in checked order, the
    /// module values, and then the `lambda` bodies.
    #[must_use]
    pub fn nodes(&self) -> &[EffectNode] {
        &self.nodes
    }

    /// The node of the declaration with canonical identity `name`.
    #[must_use]
    pub fn node_named(&self, name: &str) -> Option<&EffectNode> {
        if name.is_empty() {
            return None;
        }
        self.nodes.iter().find(|node| node.name == name)
    }

    /// The performed row of `node`: the least row containing the complete row
    /// of every application of its body. For a host operation, which has no
    /// body, it is the complete row the host performs.
    #[must_use]
    pub fn performed<'g>(&'g self, node: &'g EffectNode) -> &'g EffectRow {
        if node.is_external_operation() {
            return &node.ceiling;
        }
        self.performed.get(node.index).unwrap_or(&EMPTY)
    }

    /// Whether the performed row of `node` is exact. It is a lower bound
    /// only when the body of the node, or of a function it reaches, did not
    /// check.
    #[must_use]
    pub fn performed_known(&self, node: &EffectNode) -> bool {
        node.checked && !self.partial.get(node.index).copied().unwrap_or(true)
    }

    /// The complete row of the application at `span` of `source_id`, as the
    /// checker's admission reads it, or `None` when no call is applied
    /// there.
    #[must_use]
    pub fn application_row(&self, source_id: &str, span: ByteSpan) -> Option<&EffectRow> {
        self.nodes
            .iter()
            .flat_map(|node| node.sites.iter())
            .find(|site| {
                site.origin.source_id() == source_id && site.origin.span() == span
            })
            .map(|site| self.complete(&site.callee))
    }

    /// The consent of every binary target whose entry was checked.
    #[must_use]
    pub fn entries(&self) -> &[EffectEntry] {
        &self.entries
    }

    /// The witness of each root `node` performs, in canonical root order: the
    /// chain the checker reports for that root when the ceiling omits it.
    #[must_use]
    pub fn witnesses(&self, node: &EffectNode) -> Vec<RootWitness> {
        let Some(performed) = self.performed.get(node.index) else {
            return Vec::new();
        };
        performed
            .roots()
            .iter()
            .map(|root| {
                let distances = self.distances(root);
                let hops = self
                    .witness(node.index, root, &distances)
                    .into_iter()
                    .map(|site| WitnessHop {
                        source_id: site.origin.source_id().to_owned(),
                        span: site.origin.span(),
                        callee: self.witness_callee(&site.callee),
                    })
                    .collect();
                RootWitness {
                    root: root.clone(),
                    hops,
                }
            })
            .collect()
    }

    fn witness_callee(&self, callee: &Callee) -> WitnessCallee {
        match callee {
            Callee::Node(index) => match self.nodes.get(*index) {
                Some(node) if node.owner.is_some() => {
                    WitnessCallee::Operation(node.name.clone())
                }
                Some(node) => WitnessCallee::Function(node.identity().map(str::to_owned)),
                None => WitnessCallee::Function(None),
            },
            Callee::Value { .. } => WitnessCallee::FunctionValue,
            Callee::Contract { identity, .. } => {
                WitnessCallee::ContractMember(identity.clone())
            }
        }
    }

    /// Marks every node whose performed row is a lower bound because it
    /// reaches a function whose body did not check. An operation is a
    /// boundary: its callers see its declared ceiling.
    fn taint(&mut self) {
        let mut callers = vec![Vec::<usize>::new(); self.nodes.len()];
        for (caller, node) in self.nodes.iter().enumerate() {
            for site in &node.sites {
                if let Callee::Node(callee) = site.callee
                    && let Some(list) = callers.get_mut(callee)
                {
                    list.push(caller);
                }
            }
        }
        let mut queue = VecDeque::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if node.unknown {
                if let Some(slot) = self.partial.get_mut(index) {
                    *slot = true;
                }
                queue.push_back(index);
            }
        }
        while let Some(index) = queue.pop_front() {
            for caller in callers.get(index).into_iter().flatten() {
                if let Some(slot) = self.partial.get_mut(*caller)
                    && !*slot
                {
                    *slot = true;
                    queue.push_back(*caller);
                }
            }
        }
    }

    /// The row an application of `callee` contributes to its body.
    fn complete<'g>(&'g self, callee: &'g Callee) -> &'g EffectRow {
        match callee {
            Callee::Node(index) => match self.nodes.get(*index) {
                Some(node) if node.owner.is_some() => &node.ceiling,
                _ => self.performed.get(*index).unwrap_or(&EMPTY),
            },
            Callee::Value { row, .. } | Callee::Contract { row, .. } => row,
        }
    }

    /// Computes the least performed row of every node to a fixed point, so
    /// recursion and mutual recursion close.
    fn close(&mut self) {
        let mut callers = vec![Vec::<usize>::new(); self.nodes.len()];
        for (caller, node) in self.nodes.iter().enumerate() {
            for site in &node.sites {
                if let Callee::Node(callee) = site.callee
                    && let Some(list) = callers.get_mut(callee)
                {
                    list.push(caller);
                }
            }
        }
        let mut queue = (0..self.nodes.len()).collect::<VecDeque<_>>();
        let mut queued = vec![true; self.nodes.len()];
        while let Some(index) = queue.pop_front() {
            if let Some(flag) = queued.get_mut(index) {
                *flag = false;
            }
            let Some(node) = self.nodes.get(index) else {
                continue;
            };
            let mut row = EffectRow::empty();
            for site in &node.sites {
                row = row.union(self.complete(&site.callee));
            }
            if self.performed.get(index) == Some(&row) {
                continue;
            }
            if let Some(slot) = self.performed.get_mut(index) {
                *slot = row;
            }
            // An operation's callers see its ceiling, not its body.
            if node.owner.is_some() {
                continue;
            }
            for caller in callers.get(index).into_iter().flatten() {
                if let Some(flag) = queued.get_mut(*caller)
                    && !*flag
                {
                    *flag = true;
                    queue.push_back(*caller);
                }
            }
        }
    }

    fn diagnostics(
        &self,
        types: &TypeNames,
        entries: &[EntryConsent],
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut seen = BTreeSet::new();
        let mut distances = BTreeMap::<EffectRoot, Distances>::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let Some(performed) = self.performed.get(index) else {
                continue;
            };
            if !node.has_body {
                continue;
            }
            for root in performed.difference(&node.ceiling) {
                let distance = distances
                    .entry(root.clone())
                    .or_insert_with(|| self.distances(&root));
                let diagnostic = self.outside_ceiling(index, &root, distance);
                if seen.insert(identity_of(&diagnostic)) {
                    diagnostics.push(diagnostic);
                }
            }
            // A written root that the body never performs weakens local
            // reasoning. The owner root of an operation is not written. A
            // body that reaches a function which did not check may still
            // perform it there.
            if self.partial.get(index).copied().unwrap_or(false) {
                continue;
            }
            for (root, span) in
                types.written_roots(node.origin.source_id(), node.origin.span())
            {
                if performed.contains(&root) {
                    continue;
                }
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::ContractUnusedEffect,
                    span,
                    format!("{} is declared but never performed", root.atom()),
                )
                .with_source_id(node.origin.source_id());
                if seen.insert(identity_of(&diagnostic)) {
                    diagnostics.push(diagnostic);
                }
            }
        }
        self.admit(entries, &mut diagnostics, &mut distances);
        // Source order, as every other checker diagnostic is reported. The
        // sort is stable, so the diagnostics of one function that share a
        // primary span keep the canonical order of their roots.
        diagnostics.sort_by(|left, right| {
            (
                left.source_id(),
                left.primary_span().start(),
                left.primary_span().end(),
                left.code(),
            )
                .cmp(&(
                    right.source_id(),
                    right.primary_span().start(),
                    right.primary_span().end(),
                    right.code(),
                ))
        });
        diagnostics
    }

    /// Admits each binary target: the performed row of its entry must be
    /// contained in the target array, one `@effect.outside-ceiling` for each
    /// root the array omits, at the `entry:` value with the witness of the
    /// root as related spans; and a root of the array that the entry never
    /// performs is the warning `@contract.unused-effect` at its atom.
    fn admit(
        &self,
        entries: &[EntryConsent],
        diagnostics: &mut Vec<Diagnostic>,
        distances: &mut BTreeMap<EffectRoot, Distances>,
    ) {
        for entry in entries {
            let Some(performed) = self.performed.get(entry.function) else {
                continue;
            };
            let array =
                EffectRow::new(entry.array.iter().map(|(root, _)| root.clone()));
            for root in performed.difference(&array) {
                let distance = distances
                    .entry(root.clone())
                    .or_insert_with(|| self.distances(&root));
                let hops = self.witness(entry.function, &root, distance);
                let atom = root.atom();
                let mut diagnostic = Diagnostic::new(
                    DiagnosticCode::EffectOutsideCeiling,
                    entry.span,
                    format!(
                        "the entry performs {atom}, which the target `effects` array does not contain"
                    ),
                )
                .with_source_id(entry.source_id.clone());
                for site in hops {
                    diagnostic = diagnostic.with_related_source(
                        site.origin.source_id(),
                        site.origin.span(),
                        format!("performs {atom} via {}", self.describe(&site.callee)),
                    );
                }
                diagnostics.push(diagnostic);
            }
            // A body that reaches a function which did not check may still
            // perform the root there.
            if self.partial.get(entry.function).copied().unwrap_or(false) {
                continue;
            }
            for (root, span) in &entry.array {
                if !performed.contains(root) {
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::ContractUnusedEffect,
                            *span,
                            format!(
                                "{} is in the target `effects` array but the entry never performs it",
                                root.atom()
                            ),
                        )
                        .with_source_id(entry.source_id.clone()),
                    );
                }
            }
        }
    }

    /// The sites of `node` whose callee's complete row contains `root`, in
    /// source order.
    fn qualifying<'g>(
        &'g self,
        node: usize,
        root: &'g EffectRoot,
    ) -> impl Iterator<Item = &'g Site> {
        self.nodes
            .get(node)
            .into_iter()
            .flat_map(|node| node.sites.iter())
            .filter(move |site| self.complete(&site.callee).contains(root))
    }

    /// Whether a chain stops at `site`: it applies a function value or a
    /// contract member, or an operation that introduces the root because no
    /// call in its body carries it deeper.
    fn terminal(&self, site: &Site, distances: &Distances) -> bool {
        match site.callee {
            Callee::Value { .. } | Callee::Contract { .. } => true,
            Callee::Node(callee) => {
                self.nodes
                    .get(callee)
                    .is_some_and(|node| node.owner.is_some())
                    && distances.forced.contains(&callee)
            }
        }
    }

    /// The number of hops of the shortest chain from each node to the
    /// declaration that introduces `root`, by breadth-first search over the
    /// reverse call edges, so a deep call chain needs no host recursion.
    fn distances(&self, root: &EffectRoot) -> Distances {
        let mut distances = Distances {
            forced: BTreeSet::new(),
            hops: vec![usize::MAX; self.nodes.len()],
        };
        // An operation whose body has no call that performs the root
        // introduces it.
        for (index, node) in self.nodes.iter().enumerate() {
            if node.owner.is_some() && self.qualifying(index, root).next().is_none() {
                distances.forced.insert(index);
            }
        }
        loop {
            let mut hops = vec![usize::MAX; self.nodes.len()];
            let mut predecessors = vec![Vec::<usize>::new(); self.nodes.len()];
            let mut queue = VecDeque::new();
            for index in 0..self.nodes.len() {
                for site in self.qualifying(index, root) {
                    if self.terminal(site, &distances) {
                        if let Some(slot) = hops.get_mut(index)
                            && *slot == usize::MAX
                        {
                            *slot = 1;
                            queue.push_back(index);
                        }
                    } else if let Callee::Node(callee) = site.callee
                        && let Some(list) = predecessors.get_mut(callee)
                    {
                        list.push(index);
                    }
                }
            }
            while let Some(index) = queue.pop_front() {
                let next = hops
                    .get(index)
                    .copied()
                    .unwrap_or(usize::MAX)
                    .saturating_add(1);
                for predecessor in predecessors.get(index).into_iter().flatten() {
                    if let Some(slot) = hops.get_mut(*predecessor)
                        && *slot == usize::MAX
                    {
                        *slot = next;
                        queue.push_back(*predecessor);
                    }
                }
            }
            // An operation that only reaches itself through its own body has
            // no deeper call that performs the root, so it introduces it.
            let stuck = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(index, node)| {
                    node.owner.is_some()
                        && !distances.forced.contains(index)
                        && hops.get(*index) == Some(&usize::MAX)
                        && self.qualifying(*index, root).next().is_some()
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            distances.hops = hops;
            if stuck.is_empty() {
                return distances;
            }
            distances.forced.extend(stuck);
        }
    }

    /// The remaining hops cost of applying `site` from a body.
    fn cost(&self, site: &Site, distances: &Distances) -> usize {
        if self.terminal(site, distances) {
            return 1;
        }
        match site.callee {
            Callee::Node(callee) => distances
                .hops
                .get(callee)
                .copied()
                .unwrap_or(usize::MAX)
                .saturating_add(1),
            _ => 1,
        }
    }

    /// The `@effect.outside-ceiling` diagnostic of `node` for `root`, with its
    /// witness: the first call site in source order, then the shortest chain
    /// by hops to the declaration that introduces the root, the earliest call
    /// sites first among equal chains.
    fn outside_ceiling(
        &self,
        node: usize,
        root: &EffectRoot,
        distances: &Distances,
    ) -> Diagnostic {
        let atom = root.atom();
        let hops = self.witness(node, root, distances);
        self.outside_ceiling_with(node, &atom, &hops)
    }

    /// The chain of call sites that carries `root` from the body of `node`
    /// down to the declaration that introduces it.
    fn witness<'g>(
        &'g self,
        node: usize,
        root: &'g EffectRoot,
        distances: &Distances,
    ) -> Vec<&'g Site> {
        let mut hops = Vec::<&Site>::new();
        let mut current = node;
        let mut first = true;
        // Bounded by the node count: every step after the first takes a site
        // of strictly smaller remaining cost.
        for _ in 0..=self.nodes.len() {
            let chosen = if first {
                self.qualifying(current, root).next()
            } else {
                self.qualifying(current, root)
                    .min_by_key(|site| self.cost(site, distances))
            };
            first = false;
            let Some(site) = chosen else {
                break;
            };
            hops.push(site);
            if self.terminal(site, distances) {
                break;
            }
            let Callee::Node(callee) = site.callee else {
                break;
            };
            current = callee;
        }
        hops
    }

    fn outside_ceiling_with(
        &self,
        node: usize,
        atom: &str,
        hops: &[&Site],
    ) -> Diagnostic {
        let owner = self.nodes.get(node);
        let source_id = owner.map_or("", |owner| owner.origin.source_id());
        let span = hops.first().map_or_else(
            || owner.map_or(ByteSpan::empty_at(0), |owner| owner.origin.span()),
            |site| site.origin.span(),
        );
        let primary_source = hops
            .first()
            .map_or(source_id, |site| site.origin.source_id());
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::EffectOutsideCeiling,
            span,
            format!(
                "this body performs {atom}, which its written ceiling does not contain"
            ),
        )
        .with_source_id(primary_source);
        for (position, site) in hops.iter().enumerate() {
            let note = format!("performs {atom} via {}", self.describe(&site.callee));
            if position == 0 {
                diagnostic = diagnostic.with_note(note);
            } else {
                diagnostic = diagnostic.with_related_source(
                    site.origin.source_id(),
                    site.origin.span(),
                    note,
                );
            }
        }
        diagnostic
    }

    fn describe(&self, callee: &Callee) -> String {
        match callee {
            Callee::Node(index) => self
                .nodes
                .get(*index)
                .map_or_else(String::new, |node| node.label.clone()),
            Callee::Value { description, .. }
            | Callee::Contract { description, .. } => description.clone(),
        }
    }
}

static EMPTY: EffectRow = EffectRow::empty();

/// The shortest-chain table of one root.
struct Distances {
    /// Operations that introduce the root: no deeper call carries it.
    forced: BTreeSet<usize>,
    /// Hops from the body of each node to the introducing declaration.
    hops: Vec<usize>,
}

fn identity_of(diagnostic: &Diagnostic) -> (DiagnosticCode, String, ByteSpan, String) {
    (
        diagnostic.code(),
        diagnostic.source_id().unwrap_or_default().to_owned(),
        diagnostic.primary_span(),
        diagnostic.message().to_owned(),
    )
}

/// The canonical atom path of a declaration identity, without its package:
/// the form the specification's notes use.
fn identity(name: &str) -> String {
    match name.split_once('/') {
        Some((package, path)) if package.starts_with('@') => format!("@{path}"),
        _ => format!("@{name}"),
    }
}

/// The applications of one body in evaluation-independent order, and the
/// `lambda` bodies it creates. A `lambda` body is not part of this body: only
/// its captures are evaluated here.
fn collect(body: &Expr, function_count: usize) -> (Vec<Site>, Vec<Closure<'_>>) {
    let mut sites = Vec::new();
    let mut closures = Vec::new();
    let mut stack = vec![body];
    while let Some(expression) = stack.pop() {
        match expression {
            Expr::Literal { .. }
            | Expr::Default { .. }
            | Expr::Variable { .. }
            | Expr::Global { .. }
            | Expr::Function { .. }
            | Expr::Captured { .. } => {}
            Expr::External { arguments, .. } | Expr::Host { arguments, .. } => {
                stack.extend(arguments);
            }
            Expr::Sequence { expressions, .. } => stack.extend(expressions),
            Expr::Closure {
                signature,
                captures,
                body,
                origin,
                ..
            } => {
                stack.extend(captures);
                closures.push(Closure {
                    body,
                    ceiling: signature.effects().clone(),
                    signature: signature.clone(),
                    origin: origin.clone(),
                });
            }
            Expr::Let { value, body, .. } => {
                stack.push(value);
                stack.push(body);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                stack.push(scrutinee);
                stack.extend(arms.iter().map(|arm| &arm.body));
            }
            Expr::Widen { value, .. }
            | Expr::Try { value, .. }
            | Expr::Return { value, .. }
            | Expr::Wrap { value, .. } => stack.push(value),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                stack.push(condition);
                stack.push(then_branch);
                stack.push(else_branch);
            }
            Expr::Call {
                target,
                arguments,
                origin,
                ..
            } => {
                stack.extend(arguments);
                let callee = match target {
                    CallTarget::Direct(index) if *index < function_count => {
                        Callee::Node(*index)
                    }
                    CallTarget::Direct(_) => continue,
                    CallTarget::Indirect { callee, .. } => {
                        stack.push(callee);
                        let value_type = callee.result_type();
                        let row = match &value_type {
                            Type::Function(signature) => signature.effects().clone(),
                            _ => EffectRow::empty(),
                        };
                        Callee::Value {
                            description: format!(
                                "a function value of type {value_type}"
                            ),
                            row,
                        }
                    }
                    CallTarget::Contract {
                        interface,
                        member,
                        signature,
                        ..
                    } => Callee::Contract {
                        description: format!(
                            "contract member @{}.{member}",
                            interface.path()
                        ),
                        identity: format!("{}.{member}", interface.id()),
                        row: signature.effects().clone(),
                    },
                };
                sites.push(Site {
                    origin: origin.clone(),
                    callee,
                });
            }
            Expr::Record { fields, .. } => {
                stack.extend(fields.iter().map(|(_, value)| value));
            }
            Expr::Variant { payload, .. } => {
                stack.extend(payload.iter().map(|payload| &**payload));
            }
            Expr::Project { record, .. } => stack.push(record),
            Expr::Tuple { components, .. } => stack.extend(components),
            Expr::TupleProject { tuple, .. } => stack.push(tuple),
            Expr::Array { elements, .. } => stack.extend(elements),
            Expr::Dict { entries, .. } => {
                for (key, value) in entries {
                    stack.push(key);
                    stack.push(value);
                }
            }
            Expr::Lookup {
                collection, key, ..
            } => {
                stack.push(collection);
                stack.push(key);
            }
        }
    }
    (sites, closures)
}
