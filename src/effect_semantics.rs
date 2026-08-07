//! Effect inference over lowered body IR.
//!
//! This pass runs after lowering, when every `Call::callee_key` is resolved
//! and interface dispatch has collapsed to a concrete key. It builds the
//! module call graph and computes the least fixed point of the effect join
//! semilattice, so private callees can omit declarations without losing
//! transitive effects or terminating on recursive components.
//!
//! This module reports performed effects and their witnesses. Boundary policy
//! (which rows are declarations) is checked here as well so the legacy and
//! typed lowering paths share exactly one inference and validation algorithm.

use crate::host_abi;
use crate::lower::{
    effect_label_display, Call, EffectRow, Expr, FunctionBody, FunctionSig, FunctionVisibility,
    LetValue, Statement,
};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// An effect a body performs, together with the call that introduced it.
///
/// The witness is the whole point: naming the nominal root and the call that
/// introduced it makes an effect failure actionable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectWitness {
    pub label: (String, String),
    /// The callee or host import responsible, for the diagnostic.
    pub source: String,
}

/// Every effect a body performs, each with one representative witness.
#[derive(Debug, Default, Clone)]
pub struct InferredEffects {
    witnesses: Vec<EffectWitness>,
}

impl InferredEffects {
    fn record(&mut self, label: (String, String), source: &str) {
        if self.witnesses.iter().any(|found| found.label == label) {
            return;
        }
        self.witnesses.push(EffectWitness {
            label,
            source: source.to_string(),
        });
    }

    fn merge(&mut self, other: InferredEffects) {
        for witness in other.witnesses {
            self.record(witness.label, &witness.source);
        }
    }

    fn merge_from_call(&mut self, other: &InferredEffects, source: &str) {
        for witness in &other.witnesses {
            self.record(witness.label.clone(), source);
        }
    }

    pub fn labels(&self) -> BTreeSet<(String, String)> {
        self.witnesses.iter().map(|w| w.label.clone()).collect()
    }

    /// Effects this body performs that `declared` does not permit.
    pub fn undeclared(&self, declared: &EffectRow) -> Vec<&EffectWitness> {
        self.witnesses
            .iter()
            .filter(|witness| !declared.covers(&witness.label))
            .collect()
    }

    /// Declared labels that are not supported by the inferred body.
    ///
    /// A declared root is intentional declaration-side sugar: if any leaf in
    /// that domain is inferred, the root is considered used for warning
    /// purposes. The inferred row itself remains leaf-only.
    pub fn overdeclared(&self, declared: &EffectRow) -> Vec<(String, String)> {
        let inferred = self.labels();
        declared
            .labels
            .iter()
            .filter(|label| {
                if label.1.is_empty() {
                    !inferred.iter().any(|candidate| candidate.0 == label.0)
                } else {
                    !inferred.contains(*label)
                }
            })
            .cloned()
            .collect()
    }
}

/// The fixed-point result for a module's registered functions.
#[derive(Debug, Clone, Default)]
pub struct InferenceResult {
    pub functions: BTreeMap<String, InferredEffects>,
    pub call_edges: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Default)]
struct FunctionFacts {
    direct: InferredEffects,
    callees: BTreeSet<String>,
}

/// Infer every registered function with a deterministic worklist over the
/// real call graph. The graph includes edges from statements, call-valued
/// lets, expressions, and inline host-call arguments.
pub fn infer_program(sigs: &HashMap<String, FunctionSig>) -> InferenceResult {
    let mut facts = BTreeMap::new();
    for (key, sig) in sigs {
        facts.insert(key.clone(), collect_function_facts(sig));
    }

    let call_edges: BTreeMap<_, _> = facts
        .iter()
        .map(|(key, node)| (key.clone(), node.callees.clone()))
        .collect();
    let mut reverse_edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (caller, node) in &facts {
        for callee in &node.callees {
            if facts.contains_key(callee) {
                reverse_edges
                    .entry(callee.clone())
                    .or_default()
                    .insert(caller.clone());
            }
        }
    }

    let mut inferred: BTreeMap<_, _> = facts
        .iter()
        .map(|(key, node)| (key.clone(), node.direct.clone()))
        .collect();
    let mut worklist: VecDeque<String> = facts.keys().cloned().collect();

    while let Some(caller) = worklist.pop_front() {
        let node = &facts[&caller];
        let mut candidate = node.direct.clone();
        for callee in &node.callees {
            if let Some(callee_effects) = inferred.get(callee) {
                candidate.merge_from_call(callee_effects, callee);
            }
        }

        let before = inferred[&caller].labels();
        inferred
            .get_mut(&caller)
            .expect("inference node exists for every graph key")
            .merge(candidate);
        let after = inferred[&caller].labels();
        if before != after {
            if let Some(parents) = reverse_edges.get(&caller) {
                worklist.extend(parents.iter().cloned());
            }
        }
    }

    InferenceResult {
        functions: inferred,
        call_edges,
    }
}

/// Render an undeclared-effect failure, naming the offending call.
pub fn undeclared_effect_message(
    owner: &str,
    declared: &EffectRow,
    witness: &EffectWitness,
) -> String {
    let declared_list = if declared.is_empty() {
        "no effects".to_string()
    } else {
        declared
            .labels
            .iter()
            .map(effect_label_display)
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        "E-EFFECT-001: `{owner}` declares {declared_list} but its body performs {} via `{}`; \
         add it to `effects:` or stop calling it",
        effect_label_display(&witness.label),
        witness.source,
    )
}

/// Check all declaration-boundary functions against the shared fixed-point
/// result. Private functions infer without a ceiling; public functions,
/// interface methods, and deffect operations retain declaration checking.
/// Under-declaration is an error. Over-declaration is deterministic warning
/// text appended to the caller's existing warning sink.
pub fn validate_declared_effects(
    sigs: &HashMap<String, FunctionSig>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let inferred = infer_program(sigs);
    for owner in inferred.functions.keys() {
        // The typed path keeps the entry `main` in its function map so it can
        // reuse the ordinary body materializer. It is still the unannotated
        // program entry boundary, not a declaration boundary; legacy lowering
        // keeps it outside `sigs` altogether.
        if owner == "main" {
            continue;
        }
        let sig = &sigs[owner];
        if !is_effect_boundary(sig) {
            continue;
        }
        let performed = &inferred.functions[owner];
        if let Some(witness) = performed.undeclared(&sig.effects).first() {
            bail!(undeclared_effect_message(owner, &sig.effects, witness));
        }
        let overdeclared = performed.overdeclared(&sig.effects);
        if !overdeclared.is_empty() {
            let declared = overdeclared
                .iter()
                .map(effect_label_display)
                .collect::<Vec<_>>()
                .join(" ");
            let inferred = performed
                .labels()
                .iter()
                .map(effect_label_display)
                .collect::<Vec<_>>()
                .join(" ");
            warnings.push(format!(
                "W-EFFECT-001: `{owner}` over-declares {declared}; inferred effects: {}",
                if inferred.is_empty() {
                    "no effects"
                } else {
                    &inferred
                }
            ));
        }
    }
    Ok(())
}

/// Validate an explicitly declared legacy entry row against the entry body.
/// An omitted `main` annotation keeps the program-entry inference behavior;
/// when a row is written, it is a ceiling and uses the same root subsumption
/// relation as every other declaration boundary.
pub fn validate_entry_effects(
    statements: &[Statement],
    declared: Option<&EffectRow>,
    sigs: &HashMap<String, FunctionSig>,
) -> Result<()> {
    let Some(declared) = declared else {
        return Ok(());
    };
    let performed = infer_statements(statements, sigs);
    if let Some(witness) = performed.undeclared(declared).first() {
        bail!(undeclared_effect_message("main", declared, witness));
    }
    Ok(())
}

fn is_effect_boundary(sig: &FunctionSig) -> bool {
    sig.visibility == FunctionVisibility::Public
        || sig.interface_method
        || sig.owner_effect.is_some()
}

/// The effects a function's own body performs, plus the fixed-point effects of
/// all registered callees.
pub fn infer_function(sig: &FunctionSig, sigs: &HashMap<String, FunctionSig>) -> InferredEffects {
    let result = infer_program(sigs);
    let key = sigs
        .iter()
        .find(|(_, candidate)| std::ptr::eq(*candidate, sig))
        .map(|(key, _)| key.clone())
        .or_else(|| {
            let key = if sig.alias.is_empty() {
                sig.symbol.clone()
            } else {
                format!("{}.{}", sig.alias, sig.symbol)
            };
            result.functions.contains_key(&key).then_some(key)
        });
    if let Some(key) = key {
        return result.functions.get(&key).cloned().unwrap_or_default();
    }
    infer_unregistered_function(sig, &result)
}

/// Infer an unregistered body (notably `main`), using the already-fixed
/// registered graph for its call edges.
pub fn infer_statements(
    statements: &[Statement],
    sigs: &HashMap<String, FunctionSig>,
) -> InferredEffects {
    let result = infer_program(sigs);
    let facts = collect_statements_facts(statements);
    infer_facts_with_result(facts, &result)
}

/// Preserve the expression-level inference API for tooling callers that do
/// not own a complete function body. Registered callees still contribute their
/// fixed-point rows.
pub fn infer_expr(expr: &Expr, sigs: &HashMap<String, FunctionSig>) -> InferredEffects {
    let result = infer_program(sigs);
    infer_facts_with_result(
        {
            let mut facts = FunctionFacts::default();
            collect_expr_facts(expr, &mut facts);
            facts
        },
        &result,
    )
}

/// Return direct call edges for an unregistered body such as `main`. The
/// report uses this alongside `InferenceResult::call_edges`, keeping graph
/// construction in the semantic pass rather than in CLI presentation code.
pub fn call_edges_for_statements(statements: &[Statement]) -> BTreeSet<String> {
    collect_statements_facts(statements).callees
}

fn infer_unregistered_function(sig: &FunctionSig, result: &InferenceResult) -> InferredEffects {
    infer_facts_with_result(collect_function_facts(sig), result)
}

fn infer_facts_with_result(facts: FunctionFacts, result: &InferenceResult) -> InferredEffects {
    let mut found = facts.direct;
    for callee in facts.callees {
        if let Some(callee_effects) = result.functions.get(&callee) {
            found.merge_from_call(callee_effects, &callee);
        }
    }
    found
}

fn collect_function_facts(sig: &FunctionSig) -> FunctionFacts {
    let mut facts = FunctionFacts::default();
    if let Some(owner) = &sig.owner_effect {
        // A deffect operation performs its owner even when its implementation
        // is composed entirely from pure values or other calls. The owner is
        // the operation's nominal boundary.
        facts.direct.record(owner.clone(), "deffect owner");
    }
    match &sig.body {
        FunctionBody::Wasm { import, .. } => {
            let source = format!("{}.{}", import.module, import.name);
            for (domain, action) in host_abi::effects_for(&import.module, &import.name) {
                facts
                    .direct
                    .record(((*domain).to_string(), (*action).to_string()), &source);
            }
        }
        FunctionBody::User { statements } => collect_statements_facts_into(statements, &mut facts),
    }
    facts
}

fn collect_statements_facts(statements: &[Statement]) -> FunctionFacts {
    let mut facts = FunctionFacts::default();
    collect_statements_facts_into(statements, &mut facts);
    facts
}

fn collect_statements_facts_into(statements: &[Statement], facts: &mut FunctionFacts) {
    for statement in statements {
        collect_statement_facts(statement, facts);
    }
}

fn collect_statement_facts(statement: &Statement, facts: &mut FunctionFacts) {
    match statement {
        Statement::Call(call) => collect_call_facts(call, facts),
        Statement::Let { value, .. } => match value {
            LetValue::Call(call) => collect_call_facts(call, facts),
            LetValue::Expr(expr) => collect_expr_facts(expr, facts),
        },
        Statement::Set { value, .. } | Statement::Return(value) | Statement::Eval(value) => {
            collect_expr_facts(value, facts)
        }
        Statement::Match { target, arms } => {
            collect_expr_facts(target, facts);
            for arm in arms {
                collect_statements_facts_into(&arm.body, facts);
            }
        }
        Statement::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_expr_facts(cond, facts);
            collect_statements_facts_into(then_body, facts);
            collect_statements_facts_into(else_body, facts);
        }
        Statement::While { cond, body } => {
            collect_expr_facts(cond, facts);
            collect_statements_facts_into(body, facts);
        }
        Statement::For { source, body, .. } => {
            collect_expr_facts(source, facts);
            collect_statements_facts_into(body, facts);
        }
        // A task's effects belong to its parent: there is no runtime isolation,
        // so spawning work that touches the network still means this function does.
        Statement::Task { body, .. } => collect_statements_facts_into(body, facts),
        Statement::Spawn { value, .. } => collect_expr_facts(value, facts),
        Statement::Join { .. } | Statement::Break | Statement::Continue => {}
    }
}

fn collect_call_facts(call: &Call, facts: &mut FunctionFacts) {
    for arg in &call.args {
        collect_expr_facts(arg, facts);
    }
    facts.callees.insert(call.callee_key.clone());
}

fn collect_expr_facts(expr: &Expr, facts: &mut FunctionFacts) {
    match expr {
        // Inline host imports inside a larger body, distinct from a wasm-only
        // function body.
        Expr::HostCall {
            import,
            args,
            intrinsic,
            ..
        } => {
            let source = format!("{}.{}", import.module, import.name);
            let effects = if *intrinsic {
                crate::intrinsics::effects_for(&import.name)
            } else {
                host_abi::effects_for(&import.module, &import.name)
            };
            for (domain, action) in effects {
                facts
                    .direct
                    .record(((*domain).to_string(), (*action).to_string()), &source);
            }
            for arg in args {
                collect_expr_facts(arg, facts);
            }
        }
        Expr::Call { call, .. } => collect_call_facts(call, facts),
        Expr::Mutable(inner) | Expr::Cast { from: inner, .. } => collect_expr_facts(inner, facts),
        Expr::Reference { target, .. } => collect_expr_facts(target, facts),
        Expr::Primitive { args, .. } | Expr::Tuple(args) | Expr::Array(args) => {
            for arg in args {
                collect_expr_facts(arg, facts);
            }
        }
        Expr::EnumConstructor { payload, .. } => {
            if let Some(payload) = payload {
                collect_expr_facts(payload, facts);
            }
        }
        Expr::Record(fields) => {
            for value in fields.values() {
                collect_expr_facts(value, facts);
            }
        }
        Expr::Map(entries) => {
            for (key, value) in entries {
                collect_expr_facts(key, facts);
                collect_expr_facts(value, facts);
            }
        }
        Expr::Range { start, end, step } => {
            collect_expr_facts(start, facts);
            collect_expr_facts(end, facts);
            collect_expr_facts(step, facts);
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => {
            collect_expr_facts(cond, facts);
            collect_expr_facts(then_e, facts);
            collect_expr_facts(else_e, facts);
        }
        // Literals and variable reads compute over values already in hand.
        Expr::Value(_) | Expr::VarRef(_) => {}
    }
}
