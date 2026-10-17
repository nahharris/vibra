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
    SourceOrigin, Type,
};

use crate::nominal::TypeNames;

/// What an application applies.
enum Callee {
    /// A function or operation of the checked program, by node index.
    Node(usize),
    /// A function value, whose callee is unknown: its type's row.
    Value { description: String, row: EffectRow },
    /// A contract member: the contract's ceiling.
    Contract { description: String, row: EffectRow },
}

/// One application in a body.
struct Site {
    origin: SourceOrigin,
    callee: Callee,
}

struct Node<'a> {
    /// The identity a note names this node by.
    label: String,
    origin: SourceOrigin,
    body: Option<&'a Expr>,
    /// The exact row the declaration writes, which the performed row must be
    /// contained in. For an operation it already holds the owner root.
    ceiling: EffectRow,
    /// The root that owns an operation.
    owner: Option<EffectRoot>,
    sites: Vec<Site>,
    /// Whether the body of this function did not check, so its performed row
    /// is not known.
    unknown: bool,
}

struct Closure<'a> {
    body: &'a Expr,
    ceiling: EffectRow,
    origin: SourceOrigin,
}

/// Checks every written ceiling against the performed row of its body and
/// reports each root a ceiling does not contain with its witness, and each
/// written root that is never performed.
pub(crate) fn check(
    functions: &[Option<CheckedFunction>],
    globals: &[Option<CheckedGlobal>],
    headers: &[crate::FunctionHeader],
    types: &TypeNames,
) -> Vec<Diagnostic> {
    let graph = Graph::build(functions, globals, headers);
    graph.diagnostics(types)
}

struct Graph<'a> {
    nodes: Vec<Node<'a>>,
    performed: Vec<EffectRow>,
    /// Whether the performed row of a node is a lower bound only, because
    /// its body, or the body of a function it reaches, did not check. An
    /// error can still be reported from a lower bound, but "never
    /// performed" cannot.
    partial: Vec<bool>,
}

impl<'a> Graph<'a> {
    fn build(
        functions: &'a [Option<CheckedFunction>],
        globals: &'a [Option<CheckedGlobal>],
        headers: &[crate::FunctionHeader],
    ) -> Self {
        let mut nodes = Vec::with_capacity(functions.len() + globals.len());
        for (index, function) in functions.iter().enumerate() {
            nodes.push(match function {
                Some(function) if !function.is_external_wrapper() => Node {
                    label: identity(function.name()),
                    origin: function.origin().clone(),
                    body: Some(function.body()),
                    ceiling: function.signature().effects().clone(),
                    owner: function.effect_owner().cloned(),
                    sites: Vec::new(),
                    unknown: false,
                },
                Some(function) => Node {
                    label: identity(function.name()),
                    origin: function.origin().clone(),
                    body: None,
                    ceiling: function.signature().effects().clone(),
                    owner: function.effect_owner().cloned(),
                    sites: Vec::new(),
                    unknown: false,
                },
                // A function that did not check still has its declared
                // signature, which is all a caller needs of an operation. An
                // ordinary function's performed row is then unknown.
                None => {
                    let header = headers.get(index);
                    let owner = header.and_then(|header| header.effect_owner.clone());
                    Node {
                        label: header
                            .map_or_else(String::new, |header| identity(&header.name)),
                        origin: SourceOrigin::new("", ByteSpan::empty_at(0)),
                        body: None,
                        ceiling: header.map_or_else(EffectRow::empty, |header| {
                            header.signature.effects().clone()
                        }),
                        unknown: owner.is_none(),
                        owner,
                        sites: Vec::new(),
                    }
                }
            });
        }
        for global in globals.iter().flatten() {
            nodes.push(Node {
                label: identity(global.name()),
                origin: global.origin().clone(),
                body: Some(global.initializer()),
                // A module value has no `effects:` attribute, so its
                // initializer has the default-empty ceiling.
                ceiling: EffectRow::empty(),
                owner: None,
                sites: Vec::new(),
                unknown: false,
            });
        }
        let function_count = functions.len();
        let mut cursor = 0;
        while cursor < nodes.len() {
            if let Some(body) = nodes.get(cursor).and_then(|node| node.body) {
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
                    nodes.push(Node {
                        label: format!(
                            "a lambda at byte {}",
                            closure.origin.span().start()
                        ),
                        origin: closure.origin,
                        body: Some(closure.body),
                        ceiling: closure.ceiling,
                        owner: None,
                        sites: Vec::new(),
                        unknown: false,
                    });
                }
            }
            cursor += 1;
        }
        let mut graph = Self {
            performed: vec![EffectRow::empty(); nodes.len()],
            partial: vec![false; nodes.len()],
            nodes,
        };
        graph.close();
        graph.taint();
        graph
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

    fn diagnostics(&self, types: &TypeNames) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut seen = BTreeSet::new();
        let mut distances = BTreeMap::<EffectRoot, Distances>::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let Some(performed) = self.performed.get(index) else {
                continue;
            };
            if node.body.is_none() {
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
            Expr::External { arguments, .. } => stack.extend(arguments),
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
