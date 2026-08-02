//! Effect inference over lowered body IR.
//!
//! This runs *after* lowering rather than during it. By then every
//! `Call::callee_key` is resolved and interface dispatch has collapsed to a concrete
//! key, so inference is exact rather than approximate. It is also a single pass with
//! no fixpoint: a callee contributes its *declared* row, never its inferred one, so
//! nothing has to be re-analysed as the set grows. That is a direct consequence of
//! declarations being mandatory.
//!
//! What this module does not do is decide policy. It reports what a body performs and
//! which call introduced it; `src/lower.rs` owns the comparison against the
//! declaration.

use crate::host_abi;
use crate::lower::{
    effect_label_display, Call, EffectRow, Expr, FunctionBody, FunctionSig, LetValue, Statement,
};
use std::collections::{BTreeSet, HashMap};

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

    pub fn labels(&self) -> BTreeSet<(String, String)> {
        self.witnesses.iter().map(|w| w.label.clone()).collect()
    }

    /// Effects this body performs that `declared` does not permit.
    pub fn undeclared(&self, declared: &EffectRow) -> Vec<&EffectWitness> {
        self.witnesses
            .iter()
            .filter(|witness| !declared.labels.contains(&witness.label))
            .collect()
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

/// The effects a function's own body performs, ignoring its declaration.
pub fn infer_function(sig: &FunctionSig, sigs: &HashMap<String, FunctionSig>) -> InferredEffects {
    let mut found = InferredEffects::default();
    if let Some(owner) = &sig.owner_effect {
        // A deffect operation performs its owner even when its implementation
        // is composed entirely from pure values or other calls. The owner is
        // not inferred transitively; it is the operation's nominal boundary.
        found.record(owner.clone(), "deffect owner");
    }
    match &sig.body {
        // A host-import body has no statements at all; the registry is its effect set.
        FunctionBody::Wasm { import, .. } => {
            let source = format!("{}.{}", import.module, import.name);
            for (domain, action) in host_abi::effects_for(&import.module, &import.name) {
                found.record(((*domain).to_string(), (*action).to_string()), &source);
            }
        }
        FunctionBody::User { statements } => found.merge(infer_statements(statements, sigs)),
    }
    found
}

pub fn infer_statements(
    statements: &[Statement],
    sigs: &HashMap<String, FunctionSig>,
) -> InferredEffects {
    let mut found = InferredEffects::default();
    for statement in statements {
        found.merge(infer_statement(statement, sigs));
    }
    found
}

fn infer_statement(statement: &Statement, sigs: &HashMap<String, FunctionSig>) -> InferredEffects {
    let mut found = InferredEffects::default();
    match statement {
        Statement::Call(call) => found.merge(infer_call(call, sigs)),
        Statement::Let { value, .. } => match value {
            LetValue::Call(call) => found.merge(infer_call(call, sigs)),
            LetValue::Expr(expr) => found.merge(infer_expr(expr, sigs)),
        },
        Statement::Set { value, .. } | Statement::Return(value) | Statement::Eval(value) => {
            found.merge(infer_expr(value, sigs));
        }
        Statement::Match { target, arms } => {
            found.merge(infer_expr(target, sigs));
            for arm in arms {
                found.merge(infer_statements(&arm.body, sigs));
            }
        }
        Statement::If {
            cond,
            then_body,
            else_body,
        } => {
            found.merge(infer_expr(cond, sigs));
            found.merge(infer_statements(then_body, sigs));
            found.merge(infer_statements(else_body, sigs));
        }
        Statement::While { cond, body } => {
            found.merge(infer_expr(cond, sigs));
            found.merge(infer_statements(body, sigs));
        }
        Statement::For { source, body, .. } => {
            found.merge(infer_expr(source, sigs));
            found.merge(infer_statements(body, sigs));
        }
        // A task's effects belong to its parent: there is no runtime isolation, so
        // spawning work that touches the network still means this function does.
        Statement::Task { body, .. } => found.merge(infer_statements(body, sigs)),
        Statement::Spawn { value, .. } => found.merge(infer_expr(value, sigs)),
        Statement::Join { .. } | Statement::Break | Statement::Continue => {}
    }
    found
}

fn infer_call(call: &Call, sigs: &HashMap<String, FunctionSig>) -> InferredEffects {
    let mut found = InferredEffects::default();
    for arg in &call.args {
        found.merge(infer_expr(arg, sigs));
    }
    // The callee contributes its *declared* row, not its inferred one. That is what
    // makes this a single pass, and it is sound because every function's declaration
    // is itself checked against its own body.
    if let Some(callee) = sigs.get(&call.callee_key) {
        for label in &callee.effects.labels {
            found.record(label.clone(), &call.callee_key);
        }
    }
    found
}

pub fn infer_expr(expr: &Expr, sigs: &HashMap<String, FunctionSig>) -> InferredEffects {
    let mut found = InferredEffects::default();
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
                found.record(((*domain).to_string(), (*action).to_string()), &source);
            }
            for arg in args {
                found.merge(infer_expr(arg, sigs));
            }
        }
        Expr::Call { call, .. } => found.merge(infer_call(call, sigs)),
        Expr::Mutable(inner) | Expr::Cast { from: inner, .. } => {
            found.merge(infer_expr(inner, sigs));
        }
        Expr::Reference { target, .. } => found.merge(infer_expr(target, sigs)),
        Expr::Primitive { args, .. } | Expr::Tuple(args) | Expr::Array(args) => {
            for arg in args {
                found.merge(infer_expr(arg, sigs));
            }
        }
        Expr::EnumConstructor { payload, .. } => {
            if let Some(payload) = payload {
                found.merge(infer_expr(payload, sigs));
            }
        }
        Expr::Record(fields) => {
            for value in fields.values() {
                found.merge(infer_expr(value, sigs));
            }
        }
        Expr::Map(entries) => {
            for (key, value) in entries {
                found.merge(infer_expr(key, sigs));
                found.merge(infer_expr(value, sigs));
            }
        }
        Expr::Range { start, end, step } => {
            found.merge(infer_expr(start, sigs));
            found.merge(infer_expr(end, sigs));
            found.merge(infer_expr(step, sigs));
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => {
            found.merge(infer_expr(cond, sigs));
            found.merge(infer_expr(then_e, sigs));
            found.merge(infer_expr(else_e, sigs));
        }
        // Literals and variable reads compute over values already in hand.
        Expr::Value(_) | Expr::VarRef(_) => {}
    }
    found
}
