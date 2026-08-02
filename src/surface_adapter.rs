//! Internal, single-direction bridge from the typed S-expression surface AST
//! (`crate::ast::surface`) into the internal compatibility value shape that
//! `src/load.rs` and `src/lower.rs` already consume.
//!
//! This is a temporary migration mechanism for issue #150. It is private,
//! single-direction (AST -> `Value`, never `Value` -> AST), and is not a
//! supported interface. It is slated for deletion once the typed frontend
//! (`typed_lower`/`typed_body`/`typed_program`) replaces `src/lower.rs`'s
//! `Value`-based reading layer entirely. See the amendment in
//! `docs/decisions/s-expression-language.md` for
//! the contract exception this file relies on.
//!
//! `src/lower.rs`'s call sites key every non-primary argument by the
//! callee's *declared parameter name* (`merge_call_payload`,
//! `parse_call_args`), and every generic call requires *explicit* type
//! arguments in that same named mapping (no type inference). Converting a
//! positional S-expression call therefore requires knowing the callee's
//! parameter names ahead of time. [`collect_local_signatures`] gathers those
//! names for one module; [`resolve_signatures`] combines a module's own
//! names with its imports' names (mirroring how `tools/corpus-migrator`
//! resolves qualified calls); [`module_to_value`] then performs the actual
//! conversion.
//!
//! Fails closed: any construct this file cannot map to a precise legacy
//! shape produces a specific, path-qualified `E-ADAPT-*` error rather than a
//! best-effort guess. A silently wrong `Value` would be a miscompilation.
//!
//! Not yet wired into `load.rs` or any CLI path (that is the next, atomic
//! PR), so nothing outside this module's own tests calls it yet; allow the
//! resulting dead-code warnings rather than adding call sites this PR was
//! explicitly scoped to avoid.
#![allow(dead_code)]

use crate::ast::{
    Annotation, AnnotationKind, Constant, Definition, EffectRow, EmbedFormat as AstEmbedFormat,
    ExpectedError, Expr as AstExpr, ExprField, ExprKind, Function, FunctionTypeParameter,
    HandleAccess as AstHandleAccess, ImplItem, Import, Literal, MatchCase, MethodBinding, Module,
    Pattern as AstPattern, PatternKind, Spanned, Test, TestMeta, TopLevel, TypeExpr, TypeExprKind,
    TypeMember, Visibility, WasmArgument, WasmImport,
};
use crate::legacy_value::{Mapping, Value};
use anyhow::{bail, Result};
use std::collections::BTreeMap;

// ===================== Signature index =====================

/// A callable's positional parameter picture: generic type-parameter names
/// (in declaration order) and value-argument names (in declaration order).
/// Legacy call sites key non-primary arguments by these exact names.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallSignature {
    pub(crate) type_params: Vec<String>,
    pub(crate) arg_names: Vec<String>,
}

/// One module's own (unqualified) call, generic-type, and interface
/// signatures, as declared in its own source -- no import resolution yet.
#[derive(Clone, Debug, Default)]
pub(crate) struct LocalSignatures {
    pub(crate) calls: BTreeMap<String, CallSignature>,
    pub(crate) types: BTreeMap<String, Vec<String>>,
    /// Interface name -> its member list, for interfaces declared directly
    /// in this module. `impl_method_value` needs each member's real
    /// parameter/result types (not just arity) to type the forwarding shim
    /// it synthesizes, so unlike `calls`/`types` this stores full type
    /// information, not just a name/arity picture.
    pub(crate) interfaces: BTreeMap<String, Vec<TypeMember>>,
    /// Bare names of every top-level `fn`/`def` declared `private` in this
    /// module (`Visibility::Private`), independent of `calls`/`types` (which
    /// track callability/genericity, not visibility, and `types` only ever
    /// holds *generic* type names). Legacy encodes privacy as a literal `-`
    /// prefix baked into the declaration's own key (`visible_key`); every
    /// reference to a private symbol -- bare, self-qualified, or
    /// alias-qualified -- must reproduce that literal dash to reach (or be
    /// correctly rejected by) the same legacy entry. See
    /// `Converter::dashify_private_reference`.
    pub(crate) private: std::collections::BTreeSet<String>,
}

/// A module's fully resolved signature view: its own bare names, its own
/// names again qualified by its file stem (for self-qualified references,
/// e.g. `option.vibra` calling `option.empty`), and each import's local
/// names qualified by the alias that module declared for it. Mirrors the
/// role `SurfaceProgram.imports` (`src/frontend.rs`) plays for the on-disk
/// typed frontend, built here instead from in-memory ASTs since the
/// migrated corpus this adapter drives is never written to disk.
#[derive(Clone, Debug, Default)]
pub(crate) struct ResolvedSignatures {
    calls: BTreeMap<String, CallSignature>,
    types: BTreeMap<String, Vec<String>>,
    interfaces: BTreeMap<String, Vec<TypeMember>>,
    /// Every qualified spelling (bare, self-qualified, alias-qualified) under
    /// which a private symbol from `LocalSignatures::private` might be
    /// referenced, *without* its legacy dash. See
    /// `Converter::dashify_private_reference`.
    private_names: std::collections::BTreeSet<String>,
}

pub(crate) fn collect_local_signatures(module: &Module) -> Result<LocalSignatures> {
    let mut sigs = LocalSignatures::default();
    let mut top_level_names = std::collections::BTreeSet::new();
    for form in &module.forms {
        match form {
            TopLevel::Import(import) => {
                if !top_level_names.insert(import.alias.value.clone()) {
                    bail!(
                        "E-DEFFECT-004: duplicate top-level symbol `{}`",
                        import.alias.value
                    );
                }
            }
            TopLevel::Function(function) => {
                if !top_level_names.insert(function.name.value.clone()) {
                    bail!(
                        "E-DEFFECT-004: duplicate top-level symbol `{}`",
                        function.name.value
                    );
                }
                insert_call_signature(&mut sigs.calls, function.name.value.clone(), function);
                if function.visibility == Visibility::Private {
                    sigs.private.insert(function.name.value.clone());
                }
            }
            TopLevel::Definition(definition) => {
                if !top_level_names.insert(definition.name.value.clone()) {
                    bail!(
                        "E-DEFFECT-004: duplicate top-level symbol `{}`",
                        definition.name.value
                    );
                }
                if definition.visibility == Visibility::Private {
                    sigs.private.insert(definition.name.value.clone());
                }
                let params = where_param_names(&definition.annotations);
                if !params.is_empty() {
                    sigs.types.insert(definition.name.value.clone(), params);
                }
                if let TypeExprKind::Interface(members) = &definition.body.value {
                    for member in members {
                        if let TypeExprKind::Function { parameters, .. } = &member.ty.value {
                            sigs.calls.insert(
                                format!("{}.{}", definition.name.value, member.name.value),
                                CallSignature {
                                    type_params: Vec::new(),
                                    arg_names: parameters
                                        .iter()
                                        .map(|parameter| parameter.name.value.clone())
                                        .collect(),
                                },
                            );
                        }
                    }
                    sigs.interfaces
                        .insert(definition.name.value.clone(), members.clone());
                }
                for annotation in &definition.annotations {
                    if let AnnotationKind::Definitions(functions) = &annotation.value {
                        for f in functions {
                            insert_call_signature(
                                &mut sigs.calls,
                                format!("{}.{}", definition.name.value, f.name.value),
                                f,
                            );
                        }
                    }
                }
            }
            TopLevel::Constant(constant) => {
                if !top_level_names.insert(constant.name.value.clone()) {
                    bail!(
                        "E-DEFFECT-004: duplicate top-level symbol `{}`",
                        constant.name.value
                    );
                }
            }
            TopLevel::Deffect(deffect) => {
                if !top_level_names.insert(deffect.name.value.clone()) {
                    bail!(
                        "E-DEFFECT-004: deffect root `{}` collides with a top-level symbol",
                        deffect.name.value
                    );
                }
                for operation in &deffect.operations {
                    insert_call_signature(
                        &mut sigs.calls,
                        format!("{}.{}", deffect.name.value, operation.function.name.value),
                        &operation.function,
                    );
                }
            }
            _ => {}
        }
    }
    Ok(sigs)
}

fn insert_call_signature(
    calls: &mut BTreeMap<String, CallSignature>,
    name: String,
    function: &Function,
) {
    let type_params = where_param_names(&function.annotations);
    let arg_names = function
        .parameters
        .iter()
        .map(|p| p.name.value.clone())
        .collect();
    calls.insert(
        name,
        CallSignature {
            type_params,
            arg_names,
        },
    );
}

/// The `effects:` row on an inline impl method, if it declares one.
///
/// An inline `(method m (fn ... effects: (...)))` is split by the adapter into a
/// hoisted helper carrying the real body and a forwarding shim bound into the impl.
/// The helper inherits the annotation through `function_envelope`, but the shim's
/// envelope is built by hand, so without copying the row across it would declare
/// nothing while calling straight into an effectful helper.
fn effects_annotation<'a>(annotations: &'a [Annotation]) -> Option<&'a EffectRow> {
    annotations
        .iter()
        .find_map(|annotation| match &annotation.value {
            AnnotationKind::Effects(row) => Some(row),
            _ => None,
        })
}

fn where_param_names(annotations: &[Annotation]) -> Vec<String> {
    for annotation in annotations {
        if let AnnotationKind::Where(params) = &annotation.value {
            return params.iter().map(|p| p.name.value.clone()).collect();
        }
    }
    Vec::new()
}

/// Combine `own_local` (this module's own declarations) with its imports'
/// local signatures, qualified the same way real call sites spell them:
/// bare (own), `{own_stem}.{name}` (self-qualified), and `{alias}.{name}`
/// per declared import.
pub(crate) fn resolve_signatures(
    own_stem: &str,
    own_local: &LocalSignatures,
    imports: &[(String, LocalSignatures)],
) -> ResolvedSignatures {
    let mut calls = own_local.calls.clone();
    let mut types = own_local.types.clone();
    let mut interfaces = own_local.interfaces.clone();
    let mut private_names: std::collections::BTreeSet<String> = own_local.private.clone();
    for name in &own_local.private {
        private_names.insert(format!("{own_stem}.{name}"));
    }
    for (alias, local) in imports {
        for name in &local.private {
            private_names.insert(format!("{alias}.{name}"));
        }
    }
    for (name, sig) in &own_local.calls {
        calls
            .entry(format!("{own_stem}.{name}"))
            .or_insert_with(|| sig.clone());
    }
    for (name, params) in &own_local.types {
        types
            .entry(format!("{own_stem}.{name}"))
            .or_insert_with(|| params.clone());
    }
    for (name, members) in &own_local.interfaces {
        interfaces
            .entry(format!("{own_stem}.{name}"))
            .or_insert_with(|| members.clone());
    }
    for (alias, local) in imports {
        for (name, sig) in &local.calls {
            calls.insert(format!("{alias}.{name}"), sig.clone());
        }
        for (name, params) in &local.types {
            types.insert(format!("{alias}.{name}"), params.clone());
        }
        for (name, members) in &local.interfaces {
            interfaces.insert(format!("{alias}.{name}"), members.clone());
        }
    }
    ResolvedSignatures {
        calls,
        types,
        interfaces,
        private_names,
    }
}

fn canonical_arg_names(arity: usize) -> Vec<String> {
    (0..arity)
        .map(|i| {
            if i == 0 {
                "self".to_string()
            } else {
                format!("arg{i}")
            }
        })
        .collect()
}

// ===================== Small Value builders =====================

fn single_dollar(head: &str, payload: Value) -> Value {
    let mut map = Mapping::new();
    map.insert(Value::String(format!("${head}")), payload);
    Value::Mapping(map)
}

fn dollar_name(name: &str) -> Value {
    Value::String(format!("${name}"))
}

fn insert_unique(map: &mut Mapping, key: String, value: Value) -> Result<()> {
    let key_value = Value::String(key.clone());
    if map.insert(key_value, value).is_some() {
        bail!("E-ADAPT-044: duplicate top-level key `{key}`");
    }
    Ok(())
}

fn visible_key(name: &str, visibility: Visibility) -> String {
    match visibility {
        Visibility::Public => name.to_string(),
        Visibility::Private => format!("-{name}"),
    }
}

fn literal_value(literal: &Literal) -> Value {
    match literal {
        Literal::Atom(name) => Value::Atom(name.clone()),
        Literal::String(s) => string_literal_value(s),
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Int(i) => Value::Number((*i).into()),
        Literal::Float(f) => Value::Number((*f).into()),
        Literal::Unit => Value::Null,
    }
}

/// A string literal that happens to start with `$` is indistinguishable from
/// a legacy variable reference (`src/lower.rs` `parse_expr`'s bare-string
/// branch); wrap it in the explicit `$literal` envelope instead.
fn string_literal_value(s: &str) -> Value {
    if s.starts_with('$') {
        single_dollar("literal", Value::String(s.to_string()))
    } else {
        Value::String(s.to_string())
    }
}

fn handle_access_str(access: AstHandleAccess) -> &'static str {
    match access {
        AstHandleAccess::Read => "read",
        AstHandleAccess::Write => "write",
        AstHandleAccess::ReadWrite => "read-write",
        AstHandleAccess::Any => "any",
        AstHandleAccess::Process => "process",
    }
}

fn intrinsic_import_name(name: &str) -> String {
    crate::intrinsics::import_name(name)
}

fn is_control_form(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Let { .. }
            | ExprKind::Set { .. }
            | ExprKind::Return(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::While { .. }
            | ExprKind::For { .. }
            | ExprKind::Match { .. }
            | ExprKind::Task { .. }
            | ExprKind::Spawn { .. }
            | ExprKind::Join { .. }
    )
}

fn expr_kind_name(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Let { .. } => "let",
        ExprKind::Set { .. } => "set",
        ExprKind::Return(_) => "return",
        ExprKind::Break => "break",
        ExprKind::Continue => "continue",
        ExprKind::While { .. } => "while",
        ExprKind::For { .. } => "for",
        ExprKind::Match { .. } => "match",
        ExprKind::Task { .. } => "task",
        ExprKind::Spawn { .. } => "spawn",
        ExprKind::Join { .. } => "join",
        _ => "expression",
    }
}

fn single_body_expr(body: &[AstExpr]) -> Result<&AstExpr> {
    match body {
        [single] => Ok(single),
        other => bail!(
            "E-ADAPT-022: an `if` used as a nested (non-statement) expression must have exactly \
             one expression per branch; found {} (legacy has no block sub-expression)",
            other.len()
        ),
    }
}

// ===================== Converter =====================

/// Per-module conversion state: the resolved signature index (which carries
/// every reachable interface's member types, needed to type the synthesized
/// impl forwarding shims -- see `impl_method_value`), any freshly synthesized
/// private top-level helpers, and a counter for their names.
struct Converter<'a> {
    sig: &'a ResolvedSignatures,
    extra: Vec<(String, Value)>,
    next_id: u32,
    /// Parameter names of the function currently being converted. Legacy
    /// stores a function's own parameters under `locals["args.{name}"]`
    /// (`src/lower.rs::lower_pending_user_functions`) while every other
    /// binding -- `let`, `for`, match binds, spawn/join handles -- lives
    /// under its own bare name. A reference, `$set` target, or capture that
    /// names a current parameter must therefore be spelled `args.{name}`;
    /// everything else is spelled bare. See `local_name_ref`.
    current_params: Vec<String>,
    module_alias: String,
    intrinsic_owner: bool,
}

/// Convert one typed module into the legacy top-level mapping `Value` shape
/// `src/lower.rs::collect_module_defs` and peers consume.
pub(crate) fn module_to_value(module: &Module, sig: &ResolvedSignatures) -> Result<Value> {
    module_to_value_with_alias(module, sig, "")
}

pub(crate) fn module_to_value_with_alias(
    module: &Module,
    sig: &ResolvedSignatures,
    module_alias: &str,
) -> Result<Value> {
    let mut converter = Converter {
        sig,
        extra: Vec::new(),
        next_id: 0,
        current_params: Vec::new(),
        module_alias: module_alias.to_string(),
        intrinsic_owner: false,
    };
    let mut map = Mapping::new();
    for form in &module.forms {
        match form {
            TopLevel::Import(import) => {
                insert_unique(&mut map, import.alias.value.clone(), import_value(import))?;
            }
            TopLevel::Definition(definition) => {
                let (key, value) = converter.definition_value(definition)?;
                insert_unique(&mut map, key, value)?;
            }
            TopLevel::Constant(constant) => {
                let (key, value) = constant_value(constant)?;
                insert_unique(&mut map, key, value)?;
            }
            TopLevel::Function(function) => {
                let key = visible_key(&function.name.value, function.visibility);
                let value = converter.function_envelope(function)?;
                insert_unique(&mut map, key, value)?;
            }
            TopLevel::Deffect(deffect) => {
                for operation in &deffect.operations {
                    let mut function = operation.function.clone();
                    function.name.value = format!("{}.{}", deffect.name.value, function.name.value);
                    let key = visible_key(&function.name.value, function.visibility);
                    let value = converter
                        .function_envelope_with_owner(&function, Some(&deffect.name.value))?;
                    insert_unique(&mut map, key, value)?;
                }
            }
            TopLevel::Test(test) => {
                let (key, value) = converter.test_value(test)?;
                insert_unique(&mut map, key, value)?;
            }
            TopLevel::TestScenario(scenario) => {
                for test in &scenario.cases {
                    let mut test = test.clone();
                    test.name.value = format!("{}::{}", scenario.name.value, test.name.value);
                    let (key, value) = converter.test_value(&test)?;
                    insert_unique(&mut map, key, value)?;
                }
            }
            TopLevel::Macro(macro_def) => {
                bail!(
                    "E-ADAPT-001: top-level `macro {}` has no legacy YAML representation; the \
                     S-expression macro system (quote/unquote/splice/capture) is not bridged by \
                     src/surface_adapter.rs",
                    macro_def.name.value
                );
            }
        }
    }
    for (key, value) in std::mem::take(&mut converter.extra) {
        insert_unique(&mut map, key, value)?;
    }
    Ok(Value::Mapping(map))
}

/// Adapt one standalone typed S-expression for inline tooling.
pub(crate) fn inline_expression_to_value(expression: &AstExpr) -> Result<Value> {
    let signatures = ResolvedSignatures::default();
    let mut converter = Converter {
        sig: &signatures,
        extra: Vec::new(),
        next_id: 0,
        current_params: Vec::new(),
        module_alias: String::new(),
        intrinsic_owner: false,
    };
    converter.expr_value(expression)
}

fn import_value(import: &Import) -> Value {
    single_dollar("import", Value::String(import.path.value.clone()))
}

fn constant_value(constant: &Constant) -> Result<(String, Value)> {
    let key = visible_key(&constant.name.value, constant.visibility);
    let ExprKind::Literal(literal) = &constant.value.value else {
        bail!(
            "E-ADAPT-030: constant `{}` is not a literal; the legacy shape only represents bare \
             scalar constants (`src/lower.rs::collect_module_defs`), never a general expression",
            constant.name.value
        );
    };
    let TypeExprKind::Named(type_name) = &constant.ty.value else {
        bail!(
            "E-ADAPT-031: constant `{}` type must be a bare primitive name (`bool`, `int64`, \
             `float64`, or `str`); legacy constants have no explicit type annotation and infer \
             one of only these four shapes",
            constant.name.value
        );
    };
    let value = match (type_name.as_str(), literal) {
        ("bool", Literal::Bool(b)) => Value::Bool(*b),
        ("int64", Literal::Int(i)) => Value::Number((*i).into()),
        ("float64", Literal::Float(f)) => Value::Number((*f).into()),
        ("str", Literal::String(s)) => Value::String(s.clone()),
        _ => bail!(
            "E-ADAPT-032: constant `{}` has type `{type_name}` with a mismatched or unsupported \
             literal; the legacy shape only represents `bool`, `int64`, `float64`, or `str` \
             constants (no `int8`/`uint32`/etc. width, no non-literal expression)",
            constant.name.value
        ),
    };
    Ok((key, value))
}

impl<'a> Converter<'a> {
    fn fresh_name(&mut self, hint: &str) -> String {
        self.next_id += 1;
        format!("-adapt-{}-{}", hint.replace('.', "-"), self.next_id)
    }

    // ---------------- Definitions and types ----------------

    fn definition_value(&mut self, definition: &Definition) -> Result<(String, Value)> {
        let key = visible_key(&definition.name.value, definition.visibility);
        let (form_key, payload) = self.type_head_payload(&definition.body)?;
        let mut map = Mapping::new();
        map.insert(Value::String(form_key), payload);
        self.apply_annotations(&mut map, &definition.annotations, &definition.name.value)?;
        Ok((key, Value::Mapping(map)))
    }

    /// Split a type's general `Value` shape into the `($key, payload)` pair
    /// a top-level `def` envelope needs. Only single-entry-mapping shapes
    /// (every type constructor) split cleanly; a bare `Named` alias (`(def
    /// byte uint8)`) has no legacy envelope at all -- `collect_module_defs`
    /// silently skips any top-level value that is not itself a mapping,
    /// which would make such a definition vanish rather than fail loudly.
    fn type_head_payload(&mut self, ty: &TypeExpr) -> Result<(String, Value)> {
        match self.type_value(ty)? {
            Value::Mapping(m) if m.len() == 1 => {
                let (key, value) = m.into_iter().next().expect("checked len == 1");
                let key = key
                    .as_str()
                    .expect("adapter-built type keys are always strings")
                    .to_string();
                Ok((key, value))
            }
            Value::String(s) => bail!(
                "E-ADAPT-002: top-level `def` body `{s}` is a bare type alias; \
                 `src/lower.rs::collect_module_defs` silently ignores non-mapping definition \
                 bodies, so this cannot be represented without becoming invisible to the legacy \
                 loader"
            ),
            other => bail!("E-ADAPT-003: internal: unexpected type value shape {other:?}"),
        }
    }

    fn type_value(&mut self, ty: &TypeExpr) -> Result<Value> {
        match &ty.value {
            TypeExprKind::Literal(literal) => Ok(single_dollar("literal", literal_value(literal))),
            TypeExprKind::Named(name) => Ok(dollar_name(&self.dashify_private_reference(name))),
            TypeExprKind::Application {
                constructor,
                arguments,
            } => self.type_application_value(&constructor.value, arguments),
            TypeExprKind::Record(members) => {
                let mut m = Mapping::new();
                for member in members {
                    let value = self.type_value(&member.ty)?;
                    if m.insert(Value::String(member.name.value.clone()), value)
                        .is_some()
                    {
                        bail!(
                            "E-ADAPT-004: duplicate record field `{}`",
                            member.name.value
                        );
                    }
                }
                Ok(single_dollar("record", Value::Mapping(m)))
            }
            TypeExprKind::Tuple(items) => {
                let seq = items
                    .iter()
                    .map(|t| self.type_value(t))
                    .collect::<Result<Vec<_>>>()?;
                Ok(single_dollar("tuple", Value::Sequence(seq)))
            }
            TypeExprKind::Array(inner) => {
                let v = self.type_value(inner)?;
                Ok(single_dollar("array", v))
            }
            TypeExprKind::Map(key, value) => {
                let key_v = self.type_value(key)?;
                let value_v = self.type_value(value)?;
                let mut m = Mapping::new();
                m.insert(Value::String("key".into()), key_v);
                m.insert(Value::String("value".into()), value_v);
                Ok(single_dollar("map", Value::Mapping(m)))
            }
            TypeExprKind::Union(items) => {
                let seq = items
                    .iter()
                    .map(|t| self.type_value(t))
                    .collect::<Result<Vec<_>>>()?;
                Ok(single_dollar("union", Value::Sequence(seq)))
            }
            TypeExprKind::Enum(members) => {
                let mut m = Mapping::new();
                for member in members {
                    let value = self.type_value(&member.ty)?;
                    if m.insert(Value::String(member.name.value.clone()), value)
                        .is_some()
                    {
                        bail!("E-ADAPT-005: duplicate enum tag `{}`", member.name.value);
                    }
                }
                Ok(single_dollar("enum", Value::Mapping(m)))
            }
            TypeExprKind::Interface(members) => {
                // Legacy's own `$interface` parser (`src/lower.rs`) accepts
                // any type for a member, not only `fn-type` -- the fn-type
                // requirement below is specific to the *impl-dispatch*
                // machinery (`Converter::interface_member`, used only when
                // an `impl` actually implements that member as a method),
                // not to declaring the interface type itself. A data-shaped
                // member converts through the ordinary `type_value` path,
                // exactly as legacy would parse it.
                let mut m = Mapping::new();
                for member in members {
                    let value = match &member.ty.value {
                        TypeExprKind::Function {
                            parameters,
                            result,
                            effects,
                        } => self.interface_fn_type_value(parameters, result, effects)?,
                        _ => self.type_value(&member.ty)?,
                    };
                    if m.insert(Value::String(member.name.value.clone()), value)
                        .is_some()
                    {
                        bail!(
                            "E-ADAPT-007: duplicate interface member `{}`",
                            member.name.value
                        );
                    }
                }
                Ok(single_dollar("interface", Value::Mapping(m)))
            }
            TypeExprKind::Function {
                parameters,
                result,
                effects,
            } => self.fn_type_value(parameters, result, effects),
            TypeExprKind::Newtype(inner) => Ok(single_dollar("newtype", self.type_value(inner)?)),
            TypeExprKind::Mutable(inner) => Ok(single_dollar("mut", self.type_value(inner)?)),
            TypeExprKind::Reference(inner) => Ok(single_dollar("ref", self.type_value(inner)?)),
            TypeExprKind::MutableReference(inner) => {
                let inner_v = self.type_value(inner)?;
                Ok(single_dollar("ref", single_dollar("mut", inner_v)))
            }
            TypeExprKind::Intersect(items) => {
                let seq = items
                    .iter()
                    .map(|t| self.type_value(t))
                    .collect::<Result<Vec<_>>>()?;
                Ok(single_dollar("intersect", Value::Sequence(seq)))
            }
            TypeExprKind::Handle(access) => {
                let head = format!("handle.{}", handle_access_str(access.value));
                Ok(single_dollar(&head, Value::Null))
            }
            TypeExprKind::WasmValue(name) => bail!(
                "E-ADAPT-008: `(wasm {})` host ABI value type has no legacy `Value` shape",
                name.value
            ),
        }
    }

    fn fn_type_value(
        &mut self,
        parameters: &[FunctionTypeParameter],
        result: &TypeExpr,
        effects: &EffectRow,
    ) -> Result<Value> {
        let args = match parameters.len() {
            0 => Value::String("$void".into()),
            1 => self.type_value(&parameters[0].ty)?,
            _ => {
                let mut m = Mapping::new();
                for parameter in parameters {
                    m.insert(
                        Value::String(parameter.name.value.clone()),
                        self.type_value(&parameter.ty)?,
                    );
                }
                single_dollar("record", Value::Mapping(m))
            }
        };
        let ret = self.type_value(result)?;
        let mut m = Mapping::new();
        m.insert(Value::String("args".into()), args);
        m.insert(Value::String("return".into()), ret);
        if !effects.labels.is_empty() {
            let labels = effects
                .labels
                .iter()
                .map(|label| self.type_value(label))
                .collect::<Result<Vec<_>>>()?;
            m.insert(Value::String("effects".into()), Value::Sequence(labels));
        }
        Ok(single_dollar("fn-type", Value::Mapping(m)))
    }

    fn interface_fn_type_value(
        &mut self,
        parameters: &[FunctionTypeParameter],
        result: &TypeExpr,
        effects: &EffectRow,
    ) -> Result<Value> {
        let args = if parameters.is_empty() {
            Value::String("$void".into())
        } else {
            let mut m = Mapping::new();
            for parameter in parameters {
                m.insert(
                    Value::String(parameter.name.value.clone()),
                    self.type_value(&parameter.ty)?,
                );
            }
            single_dollar("record", Value::Mapping(m))
        };
        let ret = self.type_value(result)?;
        let mut m = Mapping::new();
        m.insert(Value::String("args".into()), args);
        m.insert(Value::String("return".into()), ret);
        if !effects.labels.is_empty() {
            let labels = effects
                .labels
                .iter()
                .map(|label| self.type_value(label))
                .collect::<Result<Vec<_>>>()?;
            m.insert(Value::String("effects".into()), Value::Sequence(labels));
        }
        Ok(single_dollar("fn-type", Value::Mapping(m)))
    }

    fn type_application_value(
        &mut self,
        constructor: &str,
        arguments: &[TypeExpr],
    ) -> Result<Value> {
        let params = self.sig.types.get(constructor).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "E-ADAPT-009: generic type application `({constructor} ...)` has no known \
                 type-parameter names in the adapter's signature index (its `where:` clause \
                 must be visible to this module -- declared locally or reachable through a \
                 direct import)"
            )
        })?;
        if params.len() != arguments.len() {
            bail!(
                "E-ADAPT-010: generic type `{constructor}` expects {} type argument(s), found {}",
                params.len(),
                arguments.len()
            );
        }
        let mut m = Mapping::new();
        for (name, arg) in params.iter().zip(arguments) {
            m.insert(Value::String(name.clone()), self.type_value(arg)?);
        }
        Ok(single_dollar(constructor, Value::Mapping(m)))
    }

    // ---------------- Annotations (doc/where/defs/impls) ----------------

    fn apply_annotations(
        &mut self,
        map: &mut Mapping,
        annotations: &[Annotation],
        owner: &str,
    ) -> Result<()> {
        for annotation in annotations {
            match &annotation.value {
                AnnotationKind::Doc(text) => {
                    insert_unique(map, "=doc".into(), Value::String(text.clone()))?;
                }
                AnnotationKind::Effects(row) => {
                    let labels = row
                        .labels
                        .iter()
                        .map(|label| self.type_value(label))
                        .collect::<Result<Vec<_>>>()?;
                    insert_unique(map, "=effects".into(), Value::Sequence(labels))?;
                }
                AnnotationKind::Where(params) => {
                    let mut m = Mapping::new();
                    for param in params {
                        let bounds = param
                            .bounds
                            .iter()
                            .map(|b| self.type_value(b))
                            .collect::<Result<Vec<_>>>()?;
                        m.insert(
                            Value::String(param.name.value.clone()),
                            Value::Sequence(bounds),
                        );
                    }
                    insert_unique(map, "=where".into(), Value::Mapping(m))?;
                }
                AnnotationKind::Definitions(functions) => {
                    let mut m = Mapping::new();
                    for f in functions {
                        let value = self.function_envelope(f)?;
                        if m.insert(Value::String(f.name.value.clone()), value)
                            .is_some()
                        {
                            bail!(
                                "E-ADAPT-013: duplicate `defs:` function `{}` on `{owner}`",
                                f.name.value
                            );
                        }
                    }
                    insert_unique(map, "=defs".into(), Value::Mapping(m))?;
                }
                AnnotationKind::Implementation { interface, items } => {
                    let (key, value) = self.impl_entry(owner, interface, items)?;
                    merge_impl_entry(map, key, value)?;
                }
            }
        }
        Ok(())
    }

    fn impl_entry(
        &mut self,
        owner: &str,
        interface: &TypeExpr,
        items: &[ImplItem],
    ) -> Result<(Value, Value)> {
        let TypeExprKind::Named(iface_name) = &interface.value else {
            bail!(
                "E-ADAPT-033: `impls:` on `{owner}` targets a generic or non-named interface \
                 type; the adapter only supports a bare interface name"
            );
        };
        let mut methods_map = Mapping::new();
        for item in items {
            match item {
                ImplItem::Types(list) => {
                    if list.is_empty() {
                        continue;
                    }
                    // A parametric interface's own `where:`-declared
                    // type-param names double as legacy's flat `=impl:
                    // {$iface: {t: <type>, method: ...}}` keys (the same
                    // registration path any generic `def`'s `where:` uses,
                    // `LocalSignatures::types` -- interfaces are not special
                    // there). `types:` binds them positionally, in that
                    // declared order, exactly like a generic type
                    // application's own positional arguments
                    // (`type_application_value`).
                    let params = self.sig.types.get(iface_name).cloned().ok_or_else(|| {
                        anyhow::anyhow!(
                            "E-ADAPT-034: `impls:` on `{owner}` pins `types:` for interface \
                             `{iface_name}`, which has no known `where:` type-parameter names \
                             in the adapter's signature index (its `where:` clause must be \
                             visible to this module -- declared locally or reachable through a \
                             direct import)"
                        )
                    })?;
                    if params.len() != list.len() {
                        bail!(
                            "E-ADAPT-034: `impls:` on `{owner}` supplies {} `types:` \
                             argument(s) for interface `{iface_name}`, which declares {} \
                             `where:` type parameter(s)",
                            list.len(),
                            params.len()
                        );
                    }
                    for (name, ty) in params.iter().zip(list) {
                        let value = self.type_value(ty)?;
                        if methods_map
                            .insert(Value::String(name.clone()), value)
                            .is_some()
                        {
                            bail!(
                                "E-ADAPT-035: duplicate impl method `{name}` for interface \
                                 `{iface_name}` on `{owner}`"
                            );
                        }
                    }
                }
                ImplItem::Method { name, binding, .. } => {
                    let value = self.impl_method_value(owner, iface_name, &name.value, binding)?;
                    if methods_map
                        .insert(Value::String(name.value.clone()), value)
                        .is_some()
                    {
                        bail!(
                            "E-ADAPT-035: duplicate impl method `{}` for interface \
                             `{iface_name}` on `{owner}`",
                            name.value
                        );
                    }
                }
            }
        }
        Ok((
            Value::String(format!("${iface_name}")),
            Value::Mapping(methods_map),
        ))
    }

    fn impl_method_value(
        &mut self,
        owner: &str,
        iface_name: &str,
        method: &str,
        binding: &MethodBinding,
    ) -> Result<Value> {
        let sig_key = format!("{iface_name}.{method}");
        let canonical = self.sig.calls.get(&sig_key).cloned().ok_or_else(|| {
            anyhow::anyhow!(
                "E-ADAPT-036: cannot resolve interface `{iface_name}` method `{method}` \
                 (implemented on `{owner}`) in the adapter's signature index"
            )
        })?;
        let arg_names = canonical.arg_names;

        let (target_callee, target_arg_names): (String, Vec<String>) = match binding {
            MethodBinding::Reference(name) => {
                let target_sig = self.sig.calls.get(&name.value).cloned().ok_or_else(|| {
                    anyhow::anyhow!(
                        "E-ADAPT-037: impl method `{method}` on `{owner}` references unknown \
                         function `{}`",
                        name.value
                    )
                })?;
                if !target_sig.type_params.is_empty() {
                    bail!(
                        "E-ADAPT-038: impl method `{method}` on `{owner}` references generic \
                         function `{}`, which the adapter's forwarding shim does not support",
                        name.value
                    );
                }
                (name.value.clone(), target_sig.arg_names)
            }
            MethodBinding::Function(inline) => {
                if !where_param_names(&inline.annotations).is_empty() {
                    bail!(
                        "E-ADAPT-039: impl method `{method}` on `{owner}` is itself generic, \
                         which the adapter's forwarding shim does not support"
                    );
                }
                let synthetic = self.fresh_name(&format!("impl-{owner}-{iface_name}-{method}"));
                // The synthesized helper is spliced in as a bare top-level
                // `$function` entry (`self.extra`), not nested inside an
                // `=impl:`/`=defs:` annotation. Legacy's own semantic check
                // (`E-SELF-001`) only allows the bare `self` type placeholder
                // *inside* that structural nesting, so an inline method
                // written with `self` typed parameters/return (the ordinary,
                // near-universal way to spell "the receiver" or "returns the
                // receiver's own type") must have `self` closed over the
                // concrete `owner` type before conversion -- there is no
                // structural context left for legacy to infer it from once
                // flattened.
                let closed = close_over_self(inline, owner);
                let value = self.function_envelope(&closed)?;
                self.extra.push((synthetic.clone(), value));
                let names = inline
                    .parameters
                    .iter()
                    .map(|p| p.name.value.clone())
                    .collect();
                (synthetic, names)
            }
        };
        if target_arg_names.len() != arg_names.len() {
            bail!(
                "E-ADAPT-040: impl method `{method}` on `{owner}` has arity {}, but interface \
                 `{iface_name}` declares {}",
                target_arg_names.len(),
                arg_names.len()
            );
        }

        // A qualified-symbol reference (`(method fmt box.show)`) has no
        // legacy forwarding-shim shape at all: `src/lower.rs`'s impl-method
        // parser (`impl_method_binding`) branches on the raw legacy `Value`
        // itself -- a bare string `$alias.symbol` yields
        // `ImplMethodBinding::Alias` (checked directly against the
        // referenced function's own real signature), while *any* mapping
        // (including a shim we might synthesize here) yields `Fresh`. There
        // is no separate signal to carry "this was written as a reference"
        // once the shape is a mapping, so preserving `Alias` requires
        // emitting the bare string here rather than building a shim for it.
        if matches!(binding, MethodBinding::Reference(_)) {
            return Ok(dollar_name(&target_callee));
        }

        // The shim's exposed parameter/result types must come from wherever
        // they were actually *written*, not from the interface's own member
        // declaration. An imported interface's member types are captured
        // verbatim from its declaring module (`collect_local_signatures`
        // never requalifies them), so a bare name in that declaration (e.g.
        // `context`, `option`) means a name local to *that* module -- not to
        // `owner`'s module. Reusing them unmodified here would silently
        // reinterpret them relative to the wrong import graph: a real bug
        // this fixed (an `E-IMPL-005` mismatch on `fs.fs-error`'s `context`
        // method, whose declared return type `(option.option error-lib
        // .context)` uses `fs.vibra`'s own aliases and was being replaced by
        // `error.vibra`'s differently-qualified, differently-aliased
        // declaration of the same member).
        //
        // An inline method's own parameters/return type are already
        // correctly qualified relative to `owner`'s module (they are
        // literally written there), so use those directly. A referenced
        // function is rejected above if generic (E-ADAPT-038), so a
        // non-generic reference's own declared shape is exactly as
        // trustworthy -- but the adapter does not currently retain full type
        // information for arbitrary resolved calls (`CallSignature` tracks
        // only names/arity, not types), so that path still falls back to the
        // interface's declared shape pending that extension.
        let (parameters, result): (Vec<TypeExpr>, TypeExpr) = match binding {
            MethodBinding::Function(inline) => (
                inline.parameters.iter().map(|p| p.ty.clone()).collect(),
                inline.return_type.clone(),
            ),
            MethodBinding::Reference(_) => {
                let (p, r) = self.interface_member(iface_name, method)?;
                (
                    p.iter().map(|parameter| parameter.ty.clone()).collect(),
                    r.clone(),
                )
            }
        };
        // Same flattening problem as the synthesized helper above: this
        // shim is also a bare top-level `$function` entry, so any bare
        // `self` in the exposed parameter/result types (from either source)
        // must be closed over `owner` here too.
        let param_type_values = parameters
            .iter()
            .map(|t| self.type_value(&substitute_self_type(t, owner)))
            .collect::<Result<Vec<_>>>()?;
        let return_value = self.type_value(&substitute_self_type(&result, owner))?;

        let mut fn_map = Mapping::new();
        match arg_names.len() {
            0 => {
                fn_map.insert(
                    Value::String("$function".into()),
                    Value::String("$void".into()),
                );
            }
            _ => {
                let mut primary = Mapping::new();
                primary.insert(
                    Value::String(arg_names[0].clone()),
                    param_type_values[0].clone(),
                );
                fn_map.insert(Value::String("$function".into()), Value::Mapping(primary));
                if arg_names.len() > 1 {
                    let mut rest = Mapping::new();
                    for (name, ty) in arg_names[1..].iter().zip(&param_type_values[1..]) {
                        rest.insert(Value::String(name.clone()), ty.clone());
                    }
                    fn_map.insert(Value::String("args".into()), Value::Mapping(rest));
                }
            }
        }
        fn_map.insert(Value::String("return".into()), return_value);
        let call = forwarding_call_value(&target_callee, &target_arg_names, &arg_names);
        // A void-returning member forwards as a bare statement call: wrapping
        // it in `$return: <call>` would pass a void-typed call as `$return`'s
        // value, which legacy's expression parser rejects ("void function
        // call cannot be used as an expression") -- the same rule
        // `user_fn_non_void_return_requires_return_statement` exercises for
        // ordinary functions, just reached through the synthesized shim here.
        let is_void_return = matches!(&result.value, TypeExprKind::Named(name) if name == "void");
        let do_body = if is_void_return {
            Value::Sequence(vec![call])
        } else {
            Value::Sequence(vec![single_dollar("return", call)])
        };
        fn_map.insert(Value::String("do".into()), do_body);
        // The shim forwards into the target, so it performs whatever the target
        // declares. Copy the row across rather than leaving the shim looking pure.
        if let MethodBinding::Function(inline) = binding {
            if let Some(row) = effects_annotation(&inline.annotations) {
                let labels = row
                    .labels
                    .iter()
                    .map(|label| self.type_value(label))
                    .collect::<Result<Vec<_>>>()?;
                fn_map.insert(Value::String("=effects".into()), Value::Sequence(labels));
            }
        }
        Ok(Value::Mapping(fn_map))
    }

    /// Look up an interface member's parameter/result types.
    ///
    /// Resolvable across the whole program: `ResolvedSignatures::interfaces`
    /// carries full member types for this module's own interfaces, the same
    /// under its self-qualified stem, and each import's under the alias the
    /// module declared. An interface that is genuinely unreachable fails
    /// closed rather than being given a guessed member shape, since a wrong
    /// legacy `$record` args shape would be a miscompilation, not a test
    /// failure.
    fn interface_member(
        &self,
        iface_name: &str,
        method: &str,
    ) -> Result<(&[FunctionTypeParameter], &TypeExpr)> {
        let members = self.sig.interfaces.get(iface_name).ok_or_else(|| {
            anyhow::anyhow!(
                "E-ADAPT-041: interface `{iface_name}` is not declared in this module or any \
                 module it imports"
            )
        })?;
        let member = members
            .iter()
            .find(|m| m.name.value == method)
            .ok_or_else(|| {
                anyhow::anyhow!("E-ADAPT-042: interface `{iface_name}` has no method `{method}`")
            })?;
        let TypeExprKind::Function {
            parameters, result, ..
        } = &member.ty.value
        else {
            bail!("E-ADAPT-006: interface member `{iface_name}.{method}` must be a `fn-type`");
        };
        Ok((parameters.as_slice(), result.as_ref()))
    }

    // ---------------- Functions and bodies ----------------

    fn function_envelope(&mut self, function: &Function) -> Result<Value> {
        self.function_envelope_with_owner(function, None)
    }

    fn function_envelope_with_owner(
        &mut self,
        function: &Function,
        owner_root: Option<&str>,
    ) -> Result<Value> {
        let params = &function.parameters;
        let mut map = Mapping::new();
        match params.len() {
            0 => {
                map.insert(
                    Value::String("$function".into()),
                    Value::String("$void".into()),
                );
            }
            _ => {
                let mut primary = Mapping::new();
                primary.insert(
                    Value::String(params[0].name.value.clone()),
                    self.type_value(&params[0].ty)?,
                );
                map.insert(Value::String("$function".into()), Value::Mapping(primary));
                if params.len() > 1 {
                    let mut rest = Mapping::new();
                    for p in &params[1..] {
                        let value = self.type_value(&p.ty)?;
                        if rest
                            .insert(Value::String(p.name.value.clone()), value)
                            .is_some()
                        {
                            bail!(
                                "E-ADAPT-014: duplicate parameter `{}` in `fn {}`",
                                p.name.value,
                                function.name.value
                            );
                        }
                    }
                    map.insert(Value::String("args".into()), Value::Mapping(rest));
                }
            }
        }
        map.insert(
            Value::String("return".into()),
            self.type_value(&function.return_type)?,
        );

        let param_names: Vec<String> = params.iter().map(|p| p.name.value.clone()).collect();
        let mut bound = Vec::new();
        collect_binding_names(&function.body, &mut bound);
        for name in &bound {
            if param_names.contains(name) {
                bail!(
                    "E-ADAPT-048: a local binding named `{name}` in `fn {}` shadows a \
                     parameter of the same name; the adapter does not support this, because \
                     legacy stores parameters as `args.{name}` and locals as bare `{name}` in \
                     separate namespaces, so a same-named local would not actually shadow the \
                     parameter the way the typed source intends",
                    function.name.value
                );
            }
        }
        let previous_params = std::mem::replace(&mut self.current_params, param_names);
        let previous_intrinsic_owner = self.intrinsic_owner;
        self.intrinsic_owner = owner_root.is_some();
        let body_result = self.body_to_do(&function.body);
        self.current_params = previous_params;
        self.intrinsic_owner = previous_intrinsic_owner;
        map.insert(Value::String("do".into()), body_result?);

        let mut annotations = function.annotations.clone();
        if let Some(root) = owner_root {
            let span = function.name.span;
            let owner_module = if self.module_alias.is_empty() {
                "module"
            } else {
                self.module_alias.as_str()
            };
            let owner = TypeExpr {
                value: TypeExprKind::Named(format!("{owner_module}.{root}")),
                span,
                origin: crate::ast::Origin::Source(span),
            };
            if let Some(annotation) = annotations
                .iter_mut()
                .find(|annotation| matches!(annotation.value, AnnotationKind::Effects(_)))
            {
                if let AnnotationKind::Effects(row) = &mut annotation.value {
                    row.labels.insert(0, owner);
                }
            } else {
                annotations.push(Spanned {
                    value: AnnotationKind::Effects(EffectRow {
                        labels: vec![owner],
                        tail: None,
                        span,
                    }),
                    span,
                    origin: crate::ast::Origin::Source(span),
                });
            }
            map.insert(
                Value::String("=owner".into()),
                Value::Sequence(vec![
                    Value::String(owner_module.to_string()),
                    Value::String(root.to_string()),
                ]),
            );
        }
        self.apply_annotations(&mut map, &annotations, &function.name.value)?;
        Ok(Value::Mapping(map))
    }

    /// Spell a reference to a bare identifier the way legacy's `locals` map
    /// keys it: `args.{name}` if `name` is one of the current function's own
    /// parameters, otherwise the bare name. See `Converter::current_params`.
    fn local_name_ref(&self, name: &str) -> String {
        if self.current_params.iter().any(|p| p == name) {
            format!("args.{name}")
        } else {
            name.to_string()
        }
    }

    fn body_to_do(&mut self, body: &[AstExpr]) -> Result<Value> {
        if body.len() == 1 {
            if let ExprKind::Wasm { import, arguments } = &body[0].value {
                if arguments
                    .iter()
                    .all(|argument| !matches!(argument, WasmArgument::Expression(_)))
                {
                    if !self.intrinsic_owner {
                        bail!("E-WASM-008: raw `wasm` is only valid inside a `deffect` operation");
                    }
                    return Ok(Value::Sequence(vec![
                        self.wasm_statement_value(import, arguments)?
                    ]));
                }
            }
            if matches!(body[0].value, ExprKind::Intrinsic { .. }) {
                return Ok(Value::Sequence(vec![single_dollar(
                    "return",
                    self.expr_value(&body[0])?,
                )]));
            }
        }
        let mut seq = Vec::new();
        self.push_statements(body, &mut seq)?;
        Ok(Value::Sequence(seq))
    }

    fn push_statements(&mut self, body: &[AstExpr], out: &mut Vec<Value>) -> Result<()> {
        for expr in body {
            if let ExprKind::Do(inner) = &expr.value {
                self.push_statements(inner, out)?;
                continue;
            }
            out.push(self.stmt_value(expr)?);
        }
        Ok(())
    }

    fn wasm_statement_value(
        &mut self,
        import: &WasmImport,
        arguments: &[WasmArgument],
    ) -> Result<Value> {
        let mut import_m = Mapping::new();
        import_m.insert(
            Value::String("module".into()),
            Value::String(import.module.value.clone()),
        );
        import_m.insert(
            Value::String("name".into()),
            Value::String(import.name.value.clone()),
        );
        let mut args = Vec::with_capacity(arguments.len());
        for arg in arguments {
            args.push(match arg {
                WasmArgument::Parameter(name) => {
                    // `tools/corpus-migrator`'s `wasm_expression` strips only
                    // the leading `$` off a legacy `$args.name` wasm-arg
                    // spec, not the `args.` segment itself (unlike ordinary
                    // `$args.x` variable references, which it rewrites to
                    // bare `x`), so migrated source spells this `(arg
                    // args.name)`. Hand-written S-expression source is
                    // expected to spell it `(arg name)` per the contract's
                    // `$args.x` -> `x` migration row. Accept both: only add
                    // the `args.` segment back when it is not already
                    // present verbatim.
                    let spec = if name.value.starts_with("args.") {
                        name.value.clone()
                    } else {
                        format!("args.{}", name.value)
                    };
                    Value::String(format!("${spec}"))
                }
                WasmArgument::ConstInt(v) => Value::Number(v.value.into()),
                WasmArgument::ConstString(v) => {
                    if v.value.starts_with("$args.") || v.value.starts_with("$const.") {
                        bail!(
                            "E-ADAPT-015: wasm string constant `{}` collides with the legacy \
                             `$args.`/`$const.` argument-spec prefix (`src/lower.rs \
                             parse_wasm_arg_spec`)",
                            v.value
                        );
                    }
                    Value::String(v.value.clone())
                }
                WasmArgument::Expression(_) => {
                    unreachable!("expression arguments use general host-call lowering")
                }
            });
        }
        let mut wasm_m = Mapping::new();
        wasm_m.insert(Value::String("import".into()), Value::Mapping(import_m));
        wasm_m.insert(Value::String("args".into()), Value::Sequence(args));
        Ok(single_dollar("wasm", Value::Mapping(wasm_m)))
    }

    // ---------------- Statements ----------------

    fn stmt_value(&mut self, expr: &AstExpr) -> Result<Value> {
        match &expr.value {
            ExprKind::Let { name, value, .. } => {
                let mut inner = Mapping::new();
                inner.insert(Value::String(name.value.clone()), self.expr_value(value)?);
                Ok(single_dollar("let", Value::Mapping(inner)))
            }
            ExprKind::Set { name, value } => {
                let mut inner = Mapping::new();
                let key = self.local_name_ref(&name.value);
                inner.insert(Value::String(key), self.expr_value(value)?);
                Ok(single_dollar("set", Value::Mapping(inner)))
            }
            ExprKind::Return(value) => {
                let payload = match value {
                    Some(v) => self.expr_value(v)?,
                    None => Value::Null,
                };
                Ok(single_dollar("return", payload))
            }
            ExprKind::Break => Ok(single_dollar("break", Value::Null)),
            ExprKind::Continue => Ok(single_dollar("continue", Value::Null)),
            ExprKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut m = Mapping::new();
                m.insert(Value::String("$if".into()), self.expr_value(condition)?);
                m.insert(Value::String("then".into()), self.body_to_do(then_body)?);
                m.insert(Value::String("else".into()), self.body_to_do(else_body)?);
                Ok(Value::Mapping(m))
            }
            ExprKind::While { condition, body } => {
                let mut m = Mapping::new();
                m.insert(Value::String("$while".into()), self.expr_value(condition)?);
                m.insert(Value::String("do".into()), self.body_to_do(body)?);
                Ok(Value::Mapping(m))
            }
            ExprKind::For {
                binding,
                source,
                body,
            } => {
                let mut m = Mapping::new();
                m.insert(
                    Value::String("$for".into()),
                    Value::String(binding.value.clone()),
                );
                m.insert(Value::String("in".into()), self.expr_value(source)?);
                m.insert(Value::String("do".into()), self.body_to_do(body)?);
                Ok(Value::Mapping(m))
            }
            ExprKind::Match { target, cases } => {
                let mut m = Mapping::new();
                m.insert(Value::String("$match".into()), self.expr_value(target)?);
                let mut when = Vec::with_capacity(cases.len());
                for case in cases {
                    when.push(self.match_case_value(case)?);
                }
                m.insert(Value::String("when".into()), Value::Sequence(when));
                Ok(Value::Mapping(m))
            }
            ExprKind::Task { captures, body } => {
                let mut m = Mapping::new();
                m.insert(
                    Value::String("$task".into()),
                    Value::Sequence(
                        captures
                            .iter()
                            .map(|c| Value::String(self.local_name_ref(&c.value)))
                            .collect(),
                    ),
                );
                m.insert(Value::String("do".into()), self.body_to_do(body)?);
                Ok(Value::Mapping(m))
            }
            ExprKind::Spawn {
                handle,
                captures,
                value,
            } => {
                let mut m = Mapping::new();
                m.insert(
                    Value::String("$spawn".into()),
                    Value::String(handle.value.clone()),
                );
                m.insert(
                    Value::String("captures".into()),
                    Value::Sequence(
                        captures
                            .iter()
                            .map(|c| Value::String(self.local_name_ref(&c.value)))
                            .collect(),
                    ),
                );
                m.insert(Value::String("value".into()), self.expr_value(value)?);
                Ok(Value::Mapping(m))
            }
            ExprKind::Join { handle, binding } => {
                let mut m = Mapping::new();
                m.insert(
                    Value::String("$join".into()),
                    Value::String(handle.value.clone()),
                );
                m.insert(
                    Value::String("into".into()),
                    Value::String(binding.value.clone()),
                );
                Ok(Value::Mapping(m))
            }
            _ => self.expr_value(expr),
        }
    }

    fn match_case_value(&mut self, case: &MatchCase) -> Result<Value> {
        let mut m = Mapping::new();
        m.insert(Value::String("case".into()), pattern_value(&case.pattern)?);
        m.insert(Value::String("do".into()), self.body_to_do(&case.body)?);
        Ok(Value::Mapping(m))
    }

    // ---------------- Expressions ----------------

    fn expr_value(&mut self, expr: &AstExpr) -> Result<Value> {
        match &expr.value {
            ExprKind::Literal(literal) => Ok(literal_value(literal)),
            ExprKind::Reference(name) => {
                Ok(Value::String(format!("${}", self.local_name_ref(name))))
            }
            ExprKind::Call {
                callee, arguments, ..
            } => self.call_value(&callee.value, arguments),
            ExprKind::AnonymousFunction { .. } => bail!(
                "E-ADAPT-046: anonymous function values are staged and cannot be represented by the legacy adapter"
            ),
            ExprKind::Do(items) => match items.as_slice() {
                [single] if !is_control_form(&single.value) => self.expr_value(single),
                _ => bail!(
                    "E-ADAPT-018: nested `(do ...)` with more than one statement (or any \
                     control-flow statement) is not representable as a legacy sub-expression"
                ),
            },
            ExprKind::Record(fields) => {
                let mut m = Mapping::new();
                for field in fields {
                    let value = self.expr_value(&field.value)?;
                    if m.insert(Value::String(field.name.value.clone()), value)
                        .is_some()
                    {
                        bail!("E-ADAPT-019: duplicate record field `{}`", field.name.value);
                    }
                }
                Ok(single_dollar("record", Value::Mapping(m)))
            }
            ExprKind::Tuple(items) => {
                let seq = items
                    .iter()
                    .map(|e| self.expr_value(e))
                    .collect::<Result<Vec<_>>>()?;
                Ok(single_dollar("tuple", Value::Sequence(seq)))
            }
            ExprKind::Array(items) => {
                let seq = items
                    .iter()
                    .map(|e| self.expr_value(e))
                    .collect::<Result<Vec<_>>>()?;
                Ok(single_dollar("array", Value::Sequence(seq)))
            }
            ExprKind::Map(pairs) => {
                let mut seq = Vec::with_capacity(pairs.len());
                for (k, v) in pairs {
                    let mut m = Mapping::new();
                    m.insert(Value::String("key".into()), self.expr_value(k)?);
                    m.insert(Value::String("value".into()), self.expr_value(v)?);
                    seq.push(Value::Mapping(m));
                }
                Ok(single_dollar("map", Value::Sequence(seq)))
            }
            ExprKind::Mutable(inner) => Ok(single_dollar("mut", self.expr_value(inner)?)),
            ExprKind::ReferenceOf(inner) => Ok(single_dollar("ref", self.expr_value(inner)?)),
            ExprKind::Range(start, end, step) => {
                let mut m = Mapping::new();
                m.insert(Value::String("start".into()), self.expr_value(start)?);
                m.insert(Value::String("end".into()), self.expr_value(end)?);
                m.insert(Value::String("step".into()), self.expr_value(step)?);
                Ok(single_dollar("range", Value::Mapping(m)))
            }
            ExprKind::Convert {
                value,
                into,
                fallback,
            } => {
                let mut m = Mapping::new();
                m.insert(Value::String("$convert".into()), self.expr_value(value)?);
                m.insert(Value::String("into".into()), self.type_value(into)?);
                m.insert(Value::String("or".into()), literal_value(&fallback.value));
                Ok(Value::Mapping(m))
            }
            ExprKind::Cast { value, into } => {
                let mut m = Mapping::new();
                m.insert(Value::String("$cast".into()), self.expr_value(value)?);
                m.insert(Value::String("into".into()), self.type_value(into)?);
                Ok(Value::Mapping(m))
            }
            ExprKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let then_e = single_body_expr(then_body)?;
                let else_e = single_body_expr(else_body)?;
                let cond_v = self.expr_value(condition)?;
                let then_v = self.expr_value(then_e)?;
                let else_v = self.expr_value(else_e)?;
                let mut m = Mapping::new();
                m.insert(Value::String("$if".into()), cond_v);
                m.insert(Value::String("then".into()), then_v);
                m.insert(Value::String("else".into()), else_v);
                Ok(Value::Mapping(m))
            }
            ExprKind::Embed { path, format } => Ok(embed_value(path, format.value)),
            ExprKind::Template { path, bindings } => self.template_value(path, bindings),
            ExprKind::Intrinsic { name, arguments } => {
                if !self.intrinsic_owner && crate::intrinsics::requires_effect(&name.value) {
                    bail!(
                        "E-INTRINSIC-001: effectful intrinsic `@{}` is only valid inside a deffect operation",
                        name.value
                    );
                }
                if !crate::intrinsics::is_known(&name.value) {
                    bail!("E-INTRINSIC-003: unknown intrinsic `@{}`", name.value);
                }
                let mut host = Mapping::new();
                host.insert(
                    Value::String("module".into()),
                    Value::String(crate::intrinsics::module_name(&name.value).into()),
                );
                host.insert(
                    Value::String("name".into()),
                    Value::String(intrinsic_import_name(&name.value)),
                );
                host.insert(
                    Value::String("intrinsic".into()),
                    Value::Bool(self.intrinsic_owner),
                );
                host.insert(
                    Value::String("args".into()),
                    Value::Sequence(
                        arguments
                            .iter()
                            .map(|argument| self.expr_value(argument))
                            .collect::<Result<Vec<_>>>()?,
                    ),
                );
                Ok(single_dollar("host-call", Value::Mapping(host)))
            }
            ExprKind::Wasm { import, arguments } => {
                if !self.intrinsic_owner {
                    bail!(
                        "E-WASM-008: raw `wasm` is only valid inside a `deffect` operation"
                    );
                }
                let mut host = Mapping::new();
                host.insert(Value::String("module".into()), Value::String(import.module.value.clone()));
                host.insert(Value::String("name".into()), Value::String(import.name.value.clone()));
                let args = arguments.iter().map(|argument| match argument {
                    WasmArgument::Parameter(name) => Ok(Value::String(format!("${}", self.local_name_ref(&name.value)))),
                    WasmArgument::ConstInt(value) => Ok(Value::Number(value.value.into())),
                    WasmArgument::ConstString(value) => Ok(Value::String(value.value.clone())),
                    WasmArgument::Expression(value) => self.expr_value(value),
                }).collect::<Result<Vec<_>>>()?;
                host.insert(Value::String("args".into()), Value::Sequence(args));
                Ok(single_dollar("host-call", Value::Mapping(host)))
            }
            ExprKind::While { .. }
            | ExprKind::For { .. }
            | ExprKind::Match { .. }
            | ExprKind::Task { .. }
            | ExprKind::Spawn { .. }
            | ExprKind::Join { .. }
            | ExprKind::Let { .. }
            | ExprKind::Set { .. }
            | ExprKind::Return(_)
            | ExprKind::Break
            | ExprKind::Continue => bail!(
                "E-ADAPT-021: `{}` is a statement-only construct in the legacy shape and cannot \
                 appear as a nested expression",
                expr_kind_name(&expr.value)
            ),
        }
    }

    fn template_value(&mut self, path: &Spanned<String>, bindings: &[ExprField]) -> Result<Value> {
        let mut with_m = Mapping::new();
        for field in bindings {
            let value = self.expr_value(&field.value)?;
            with_m.insert(Value::String(field.name.value.clone()), value);
        }
        let mut m = Mapping::new();
        m.insert(
            Value::String("path".into()),
            Value::String(path.value.clone()),
        );
        m.insert(Value::String("with".into()), Value::Mapping(with_m));
        Ok(single_dollar("template", Value::Mapping(m)))
    }

    // ---------------- Calls ----------------

    /// Whether `callee` (e.g. `stream.writable.string`) names a method of
    /// a known `$interface` type reachable from this module's resolved
    /// signature index -- i.e. whether legacy's
    /// `reject_iface_nested_call_bundle` will apply its sibling-key shape
    /// requirement to a multi-argument call to it (see
    /// [`Self::named_call_value`]). Inherent (`=defs:`) methods are not
    /// interfaces and are unaffected: `self.sig.interfaces` only ever
    /// contains genuine `$interface` type declarations
    /// (`collect_local_signatures`).
    fn is_interface_dispatch_call(&self, callee: &str) -> bool {
        callee
            .rsplit_once('.')
            .is_some_and(|(interface, _method)| self.sig.interfaces.contains_key(interface))
    }

    /// Reproduce legacy's literal `-` privacy encoding for a *reference* to a
    /// declaration (`LocalSignatures::private`/`ResolvedSignatures::private_names`
    /// records every declaration's bare/self-qualified/alias-qualified
    /// spelling *without* the dash `visible_key` bakes into the declaration
    /// site itself). Two shapes need it:
    ///
    /// - a direct reference (function call, bare/qualified type name, or a
    ///   bare enum type) -- the private segment is the reference's own last
    ///   segment, e.g. `priv` -> `-priv`, `m.priv-t` -> `m.-priv-t`;
    /// - an enum-constructor call `Type.tag` (or `alias.Type.tag`) -- the
    ///   private segment is the *type*, one segment short of the full
    ///   reference, e.g. `m.priv-e.a` -> `m.-priv-e.a`, not `m.priv-e.-a`.
    ///
    /// Public references pass through unchanged. This only rewrites the
    /// *spelling*; whether the resulting dashed reference then resolves or
    /// is correctly rejected (e.g. an importer reaching into another
    /// module's private namespace) remains entirely `src/lower.rs`'s call.
    fn dashify_private_reference(&self, qualified: &str) -> String {
        fn dash_last_segment(s: &str) -> String {
            match s.rsplit_once('.') {
                Some((prefix, last)) => format!("{prefix}.-{last}"),
                None => format!("-{s}"),
            }
        }
        if self.sig.private_names.contains(qualified) {
            return dash_last_segment(qualified);
        }
        if let Some((prefix, tag)) = qualified.rsplit_once('.') {
            if self.sig.private_names.contains(prefix) {
                return format!("{}.{tag}", dash_last_segment(prefix));
            }
        }
        qualified.to_string()
    }

    fn call_value(&mut self, callee: &str, args: &[crate::ast::CallArgument]) -> Result<Value> {
        let args = args
            .iter()
            .map(|argument| argument.value().clone())
            .collect::<Vec<_>>();
        if !callee.contains('.') {
            if let Some((_, arity)) = crate::lower::typed_primitive_op(callee) {
                return self.primitive_value(callee, arity, &args);
            }
        }
        if let Some(sig) = self.sig.calls.get(callee).cloned() {
            return self.named_call_value(callee, &sig, &args);
        }
        let key = self.dashify_private_reference(callee);
        match args.len() {
            0 => Ok(single_dollar(&key, Value::Null)),
            1 => {
                let v = self.expr_value(&args[0])?;
                Ok(single_dollar(&key, v))
            }
            n => bail!(
                "E-ADAPT-023: call to `{callee}` with {n} arguments has no signature in the \
                 adapter's index; the legacy shape keys every non-primary argument by the \
                 callee's declared parameter name, which is unknown here (unresolved import, \
                 forward reference, or genuinely unknown callee)"
            ),
        }
    }

    fn primitive_value(&mut self, name: &str, arity: usize, args: &[AstExpr]) -> Result<Value> {
        if args.len() != arity {
            bail!(
                "E-ADAPT-024: primitive `{name}` expects {arity} operand(s), found {}",
                args.len()
            );
        }
        let payload = if arity == 1 {
            self.expr_value(&args[0])?
        } else {
            Value::Sequence(
                args.iter()
                    .map(|a| self.expr_value(a))
                    .collect::<Result<Vec<_>>>()?,
            )
        };
        Ok(single_dollar(name, payload))
    }

    fn named_call_value(
        &mut self,
        callee: &str,
        sig: &CallSignature,
        args: &[AstExpr],
    ) -> Result<Value> {
        let type_arity = sig.type_params.len();
        if args.len() < type_arity {
            bail!(
                "E-ADAPT-025: call to `{callee}` supplies {} argument(s), fewer than its {} \
                 type parameter(s); the adapter requires generic calls to spell type arguments \
                 explicitly as leading positional arguments (matching the migrated corpus's \
                 convention) -- it does not perform type inference",
                args.len(),
                type_arity
            );
        }
        let (type_args, value_args) = args.split_at(type_arity);
        if value_args.len() != sig.arg_names.len() {
            bail!(
                "E-ADAPT-026: call to `{callee}` supplies {} value argument(s), expected {}",
                value_args.len(),
                sig.arg_names.len()
            );
        }
        let legacy_head = self.dashify_private_reference(callee);
        if type_arity == 0 {
            return match sig.arg_names.len() {
                0 => {
                    if !args.is_empty() {
                        bail!("E-ADAPT-027: call to `{callee}` takes no arguments");
                    }
                    Ok(single_dollar(&legacy_head, Value::Null))
                }
                1 => {
                    let v = self.expr_value(&value_args[0])?;
                    Ok(single_dollar(&legacy_head, v))
                }
                _ if self.is_interface_dispatch_call(callee) => {
                    // Legacy's `reject_iface_nested_call_bundle`
                    // (`src/lower.rs`) requires a multi-argument interface
                    // dispatch call to spell its dispatch value as the
                    // callee's own payload, with every other argument as a
                    // *sibling* key at the same mapping level as the callee
                    // -- never all bundled into one mapping nested under the
                    // callee. `tools/corpus-migrator`'s `named_call`
                    // reconstructs exactly this sibling shape back into an
                    // ordinary positional call, confirming it as the
                    // canonical legacy spelling for this construct, not the
                    // generic per-callee-name-bundle shape ordinary
                    // (non-interface) multi-argument calls use.
                    let primary = self.expr_value(&value_args[0])?;
                    let mut m = Mapping::new();
                    m.insert(Value::String(format!("${legacy_head}")), primary);
                    for (name, expr) in sig.arg_names[1..].iter().zip(&value_args[1..]) {
                        let v = self.expr_value(expr)?;
                        m.insert(Value::String(name.clone()), v);
                    }
                    Ok(Value::Mapping(m))
                }
                _ => {
                    let mut m = Mapping::new();
                    for (name, expr) in sig.arg_names.iter().zip(value_args) {
                        let v = self.expr_value(expr)?;
                        m.insert(Value::String(name.clone()), v);
                    }
                    Ok(single_dollar(&legacy_head, Value::Mapping(m)))
                }
            };
        }
        let mut m = Mapping::new();
        for (name, expr) in sig.type_params.iter().zip(type_args) {
            let v = self.expr_as_type_value(expr)?;
            m.insert(Value::String(name.clone()), v);
        }
        for (name, expr) in sig.arg_names.iter().zip(value_args) {
            let key = Value::String(name.clone());
            if m.contains_key(&key) {
                bail!(
                    "E-ADAPT-028: call to `{callee}`: value argument `{name}` collides with a \
                     type-parameter name of the same name"
                );
            }
            let v = self.expr_value(expr)?;
            m.insert(key, v);
        }
        Ok(single_dollar(&legacy_head, Value::Mapping(m)))
    }

    /// Reinterpret an expression written in a generic call's leading
    /// (type-argument) position as a type. The migrated corpus always
    /// spells explicit type arguments as ordinary positional expressions
    /// (a bare type name parses identically to a variable reference; a
    /// generic type application parses identically to a call) -- see
    /// `tools/corpus-migrator`'s `named_call`, which never emits `apply`.
    fn expr_as_type_value(&mut self, expr: &AstExpr) -> Result<Value> {
        match &expr.value {
            ExprKind::Reference(name) => Ok(dollar_name(name)),
            ExprKind::Call {
                callee, arguments, ..
            } => {
                let params = self.sig.types.get(&callee.value).cloned().ok_or_else(|| {
                    anyhow::anyhow!(
                        "E-ADAPT-009: generic type application `({} ...)` used as a call's type \
                         argument has no known type-parameter names in the adapter's signature \
                         index",
                        callee.value
                    )
                })?;
                if params.len() != arguments.len() {
                    bail!(
                        "E-ADAPT-010: generic type `{}` expects {} type argument(s), found {}",
                        callee.value,
                        params.len(),
                        arguments.len()
                    );
                }
                let mut m = Mapping::new();
                for (name, arg) in params.iter().zip(arguments) {
                    let v = self.expr_as_type_value(arg.value())?;
                    m.insert(Value::String(name.clone()), v);
                }
                Ok(single_dollar(&callee.value, Value::Mapping(m)))
            }
            _ => bail!(
                "E-ADAPT-029: expected a type-argument expression (a bare name or generic type \
                 application), found a value expression"
            ),
        }
    }
}

/// Collect every name a function body introduces as a *new* local binding
/// (`let`, `for`, match binds, spawn/join handles). Used to reject -- fail
/// closed -- a local binding that reuses one of the enclosing function's own
/// parameter names, which the adapter's `args.{name}` / bare-`{name}`
/// namespace split (see `Converter::current_params`) cannot represent as
/// real shadowing.
fn collect_binding_names(body: &[AstExpr], names: &mut Vec<String>) {
    for expr in body {
        collect_binding_names_expr(expr, names);
    }
}

fn collect_binding_names_expr(expr: &AstExpr, names: &mut Vec<String>) {
    match &expr.value {
        ExprKind::Let { name, value, .. } => {
            names.push(name.value.clone());
            collect_binding_names_expr(value, names);
        }
        ExprKind::Set { value, .. } => collect_binding_names_expr(value, names),
        ExprKind::Return(value) => {
            if let Some(value) = value {
                collect_binding_names_expr(value, names);
            }
        }
        ExprKind::If {
            condition,
            then_body,
            else_body,
        } => {
            collect_binding_names_expr(condition, names);
            collect_binding_names(then_body, names);
            collect_binding_names(else_body, names);
        }
        ExprKind::While { condition, body } => {
            collect_binding_names_expr(condition, names);
            collect_binding_names(body, names);
        }
        ExprKind::For {
            binding,
            source,
            body,
        } => {
            names.push(binding.value.clone());
            collect_binding_names_expr(source, names);
            collect_binding_names(body, names);
        }
        ExprKind::Match { target, cases } => {
            collect_binding_names_expr(target, names);
            for case in cases {
                collect_pattern_binding_names(&case.pattern, names);
                collect_binding_names(&case.body, names);
            }
        }
        ExprKind::Do(items) => collect_binding_names(items, names),
        ExprKind::Record(fields) => {
            for field in fields {
                collect_binding_names_expr(&field.value, names);
            }
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                collect_binding_names_expr(item, names);
            }
        }
        ExprKind::Map(pairs) => {
            for (k, v) in pairs {
                collect_binding_names_expr(k, names);
                collect_binding_names_expr(v, names);
            }
        }
        ExprKind::Mutable(inner) | ExprKind::ReferenceOf(inner) => {
            collect_binding_names_expr(inner, names);
        }
        ExprKind::Range(start, end, step) => {
            collect_binding_names_expr(start, names);
            collect_binding_names_expr(end, names);
            collect_binding_names_expr(step, names);
        }
        ExprKind::Convert { value, .. } | ExprKind::Cast { value, .. } => {
            collect_binding_names_expr(value, names)
        }
        ExprKind::Call { arguments, .. } => {
            for arg in arguments {
                collect_binding_names_expr(arg.value(), names);
            }
        }
        ExprKind::Intrinsic { arguments, .. } => {
            for argument in arguments {
                collect_binding_names_expr(argument, names);
            }
        }
        ExprKind::AnonymousFunction { body, .. } => {
            for value in body {
                collect_binding_names_expr(value, names);
            }
        }
        ExprKind::Template { bindings, .. } => {
            for field in bindings {
                collect_binding_names_expr(&field.value, names);
            }
        }
        ExprKind::Task { body, .. } => collect_binding_names(body, names),
        ExprKind::Spawn { handle, value, .. } => {
            names.push(handle.value.clone());
            collect_binding_names_expr(value, names);
        }
        ExprKind::Join { binding, .. } => names.push(binding.value.clone()),
        ExprKind::Literal(_)
        | ExprKind::Reference(_)
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Embed { .. }
        | ExprKind::Wasm { .. } => {}
    }
}

fn collect_pattern_binding_names(pattern: &AstPattern, names: &mut Vec<String>) {
    match &pattern.value {
        PatternKind::Bind(name) => names.push(name.value.clone()),
        PatternKind::Constructor { arguments, .. } => {
            for p in arguments {
                collect_pattern_binding_names(p, names);
            }
        }
        PatternKind::Record(fields) => {
            for field in fields {
                collect_pattern_binding_names(&field.pattern, names);
            }
        }
        PatternKind::Tuple(items) | PatternKind::Array(items) => {
            for p in items {
                collect_pattern_binding_names(p, names);
            }
        }
        PatternKind::Map(pairs) => {
            for (k, v) in pairs {
                collect_pattern_binding_names(k, names);
                collect_pattern_binding_names(v, names);
            }
        }
        PatternKind::Newtype { pattern, .. } => collect_pattern_binding_names(pattern, names),
        PatternKind::Literal(_) | PatternKind::Wildcard | PatternKind::Interface { .. } => {}
    }
}

/// Build the shim's own forwarding call. Every reference here is to one of
/// the *shim's own* declared parameters (the canonical `self`/`arg1`/...
/// names), which legacy stores under `args.{name}` just like any other
/// function's parameters (`lower_pending_user_functions`), so every operand
/// is spelled `$args.{name}`, never bare `${name}`.
fn forwarding_call_value(
    target: &str,
    target_arg_names: &[String],
    canonical_arg_names: &[String],
) -> Value {
    let refs: Vec<Value> = canonical_arg_names
        .iter()
        .map(|n| Value::String(format!("$args.{n}")))
        .collect();
    match target_arg_names.len() {
        0 => single_dollar(target, Value::Null),
        1 => single_dollar(target, refs.into_iter().next().expect("checked len == 1")),
        _ => {
            let mut m = Mapping::new();
            for (name, value) in target_arg_names.iter().zip(refs) {
                m.insert(Value::String(name.clone()), value);
            }
            single_dollar(target, Value::Mapping(m))
        }
    }
}

fn merge_impl_entry(map: &mut Mapping, key: Value, value: Value) -> Result<()> {
    let impl_key = Value::String("=impl".into());
    if let Some(existing) = map.get_mut(&impl_key) {
        let Value::Mapping(inner) = existing else {
            bail!("E-ADAPT-045: internal: `=impl` entry is not a mapping");
        };
        if inner.insert(key.clone(), value).is_some() {
            bail!("E-ADAPT-043: duplicate `impls:` interface entry `{key:?}`");
        }
    } else {
        let mut inner = Mapping::new();
        inner.insert(key, value);
        map.insert(impl_key, Value::Mapping(inner));
    }
    Ok(())
}

fn embed_value(path: &Spanned<String>, format: AstEmbedFormat) -> Value {
    let name = match format {
        AstEmbedFormat::Auto => return single_dollar("embed", Value::String(path.value.clone())),
        AstEmbedFormat::Text => "text",
        AstEmbedFormat::Binary => "binary",
        AstEmbedFormat::Json => "json",
        AstEmbedFormat::Toml => "toml",
        AstEmbedFormat::Xml => "xml",
    };
    let mut m = Mapping::new();
    m.insert(
        Value::String("path".into()),
        Value::String(path.value.clone()),
    );
    m.insert(Value::String("format".into()), Value::String(name.into()));
    single_dollar("embed", Value::Mapping(m))
}

// ---------------- Patterns ----------------

fn pattern_value(pattern: &AstPattern) -> Result<Value> {
    match &pattern.value {
        PatternKind::Literal(literal) => Ok(literal_value(literal)),
        PatternKind::Wildcard => Ok(single_dollar("wildcard", Value::Null)),
        PatternKind::Bind(name) => Ok(single_dollar("bind", Value::String(name.value.clone()))),
        PatternKind::Constructor {
            constructor,
            arguments,
        } => {
            let payload = match arguments.len() {
                0 => Value::Null,
                1 => pattern_value(&arguments[0])?,
                _ => Value::Sequence(
                    arguments
                        .iter()
                        .map(pattern_value)
                        .collect::<Result<Vec<_>>>()?,
                ),
            };
            Ok(single_dollar(&constructor.value, payload))
        }
        PatternKind::Record(fields) => {
            let mut m = Mapping::new();
            for field in fields {
                let value = pattern_value(&field.pattern)?;
                if m.insert(Value::String(field.name.value.clone()), value)
                    .is_some()
                {
                    bail!(
                        "E-ADAPT-046: duplicate record pattern field `{}`",
                        field.name.value
                    );
                }
            }
            Ok(single_dollar("record", Value::Mapping(m)))
        }
        PatternKind::Tuple(items) => Ok(single_dollar(
            "tuple",
            Value::Sequence(
                items
                    .iter()
                    .map(pattern_value)
                    .collect::<Result<Vec<_>>>()?,
            ),
        )),
        PatternKind::Array(items) => Ok(single_dollar(
            "array",
            Value::Sequence(
                items
                    .iter()
                    .map(pattern_value)
                    .collect::<Result<Vec<_>>>()?,
            ),
        )),
        PatternKind::Map(pairs) => {
            let mut seq = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                let mut m = Mapping::new();
                m.insert(Value::String("key".into()), pattern_value(k)?);
                m.insert(Value::String("value".into()), pattern_value(v)?);
                seq.push(Value::Mapping(m));
            }
            Ok(single_dollar("map", Value::Sequence(seq)))
        }
        PatternKind::Newtype { ty, pattern } => {
            let mut m = Mapping::new();
            m.insert(Value::String("type".into()), bare_type_value(ty)?);
            m.insert(Value::String("inner".into()), pattern_value(pattern)?);
            Ok(single_dollar("newtype", Value::Mapping(m)))
        }
        // `src/lower.rs`'s `Pattern::Interface(TypeRef)` carries only a type,
        // with no slot for a nested sub-pattern at all -- not even to record
        // that one was written. Legacy's own YAML surface spelled this same
        // construct `$interface: <type-expr>` with no inner pattern (see
        // `parse_pattern`'s `"$interface"` arm in `src/lower.rs`), and
        // legacy's match evaluation for it (`Pattern::Interface` in
        // `validate_pattern`) only ever checks the interface bound and binds
        // nothing. A `_` wildcard sub-pattern is the one case that is
        // faithfully representable: it binds nothing either, so dropping it
        // loses no information legacy would have captured. Any other
        // sub-pattern (a bind, a nested destructure) would need a real slot
        // legacy does not have, so it still must fail loudly rather than be
        // silently dropped.
        PatternKind::Interface { ty, pattern } => {
            if !matches!(pattern.value, PatternKind::Wildcard) {
                bail!(
                    "E-ADAPT-017: `(interface Type pattern)` pattern is not supported by the \
                     adapter unless `pattern` is `_`; `src/lower.rs`'s `Pattern::Interface` \
                     carries only a type, with no slot for a nested sub-pattern, so a non- \
                     wildcard inner pattern cannot be represented"
                );
            }
            Ok(single_dollar("interface", bare_type_value(ty)?))
        }
    }
}

/// Patterns need type conversion but have no `&mut Converter` available
/// (pattern conversion is free-standing, not tied to the module's generic
/// signature index at this call depth); newtype/interface pattern payloads
/// are restricted to non-generic named types, which need no signature
/// lookups at all.
fn bare_type_value(ty: &TypeExpr) -> Result<Value> {
    match &ty.value {
        TypeExprKind::Named(name) => Ok(dollar_name(name)),
        _ => bail!(
            "E-ADAPT-047: pattern type payloads only support a bare named type, not a generic \
             application or compound type shape"
        ),
    }
}

// ---------------- Tests ----------------

impl<'a> Converter<'a> {
    fn test_value(&mut self, test: &Test) -> Result<(String, Value)> {
        let body = self.body_to_do(&test.body)?;
        self.test_metadata_value(test, body)
    }

    /// Shared by [`Self::test_value`] (real compilation, real body) and
    /// [`test_discovery_value`] (test-name/tag/metadata discovery only, which
    /// deliberately never resolves imports or converts bodies -- see that
    /// function's doc comment). `body` is the already-converted `do` value, or
    /// a placeholder the caller promises nothing will read.
    fn test_metadata_value(&mut self, test: &Test, body: Value) -> Result<(String, Value)> {
        let key = test.name.value.clone();
        let mut m = Mapping::new();
        m.insert(
            Value::String("$test".into()),
            Value::String(test.profile.value.clone()),
        );
        m.insert(Value::String("do".into()), body);
        for meta in &test.metadata {
            match meta {
                TestMeta::Tags(tags) => {
                    m.insert(
                        Value::String("tags".into()),
                        Value::Sequence(
                            tags.iter()
                                .map(|n| Value::String(n.value.clone()))
                                .collect(),
                        ),
                    );
                }
                TestMeta::TimeoutMillis(v) => {
                    m.insert(
                        Value::String("timeout-ms".into()),
                        Value::Number(v.value.into()),
                    );
                }
                TestMeta::RandomSeed(v) => {
                    m.insert(
                        Value::String("random-seed".into()),
                        Value::Number(v.value.into()),
                    );
                }
                TestMeta::Skip(reason) => {
                    m.insert(
                        Value::String("skip".into()),
                        Value::String(reason.value.clone()),
                    );
                }
                TestMeta::Workspace(name) => {
                    m.insert(
                        Value::String("workspace".into()),
                        Value::String(name.value.clone()),
                    );
                }
                TestMeta::ExpectError(err) => {
                    let mut em = Mapping::new();
                    match err {
                        ExpectedError::Load { code, message } => {
                            em.insert(Value::String("phase".into()), Value::String("load".into()));
                            em.insert(
                                Value::String("code".into()),
                                Value::String(code.value.clone()),
                            );
                            if let Some(msg) = message {
                                em.insert(
                                    Value::String("message-contains".into()),
                                    Value::String(msg.value.clone()),
                                );
                            }
                        }
                        ExpectedError::Compile { code, message } => {
                            em.insert(
                                Value::String("phase".into()),
                                Value::String("compile".into()),
                            );
                            em.insert(
                                Value::String("code".into()),
                                Value::String(code.value.clone()),
                            );
                            if let Some(msg) = message {
                                em.insert(
                                    Value::String("message-contains".into()),
                                    Value::String(msg.value.clone()),
                                );
                            }
                        }
                        ExpectedError::Runtime { message } => {
                            em.insert(
                                Value::String("phase".into()),
                                Value::String("runtime".into()),
                            );
                            em.insert(
                                Value::String("message-contains".into()),
                                Value::String(message.value.clone()),
                            );
                        }
                    }
                    m.insert(Value::String("expect-error".into()), Value::Mapping(em));
                }
                TestMeta::Clock {
                    unix_millis,
                    monotonic_millis,
                } => {
                    let mut cm = Mapping::new();
                    cm.insert(
                        Value::String("unix-millis".into()),
                        Value::Number(unix_millis.value.into()),
                    );
                    cm.insert(
                        Value::String("monotonic-millis".into()),
                        Value::Number(monotonic_millis.value.into()),
                    );
                    m.insert(Value::String("clock".into()), Value::Mapping(cm));
                }
            }
        }
        Ok((key, Value::Mapping(m)))
    }
}

/// Clone `function` with every bare `self` type reference (in its parameter
/// and return types) closed over the concrete `owner` type name. Used only
/// for [`Converter::impl_method_value`]'s synthesized helper function --
/// see that call site for why closing over `self` is necessary there.
fn close_over_self(function: &Function, owner: &str) -> Function {
    let mut function = function.clone();
    for parameter in &mut function.parameters {
        parameter.ty = substitute_self_type(&parameter.ty, owner);
    }
    function.return_type = substitute_self_type(&function.return_type, owner);
    function
}

/// Recursively replace the bare `self` type placeholder with the concrete
/// `owner` type name. `self` is reserved syntax meaning "the type
/// implementing this interface/impl", valid in the surface grammar only
/// inside an `$interface`/`=defs`/`=impl` YAML context (legacy's
/// `E-SELF-001`). [`Converter::impl_method_value`] flattens both its
/// synthesized shim and its synthesized helper function into bare
/// top-level `$function` entries -- structurally outside any
/// `=impl:`/`=defs:` nesting -- so any `self` they carry must already be a
/// closed, concrete type by the time they are converted; there is no
/// structural context left for legacy to resolve it from.
fn substitute_self_type(ty: &TypeExpr, owner: &str) -> TypeExpr {
    let value = match &ty.value {
        TypeExprKind::Named(name) if name == "self" => TypeExprKind::Named(owner.to_string()),
        TypeExprKind::Named(name) => TypeExprKind::Named(name.clone()),
        TypeExprKind::Application {
            constructor,
            arguments,
        } => TypeExprKind::Application {
            constructor: constructor.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_self_type(argument, owner))
                .collect(),
        },
        TypeExprKind::Record(members) => TypeExprKind::Record(
            members
                .iter()
                .map(|member| substitute_self_member(member, owner))
                .collect(),
        ),
        TypeExprKind::Tuple(items) => TypeExprKind::Tuple(
            items
                .iter()
                .map(|item| substitute_self_type(item, owner))
                .collect(),
        ),
        TypeExprKind::Array(inner) => {
            TypeExprKind::Array(Box::new(substitute_self_type(inner, owner)))
        }
        TypeExprKind::Map(key, value) => TypeExprKind::Map(
            Box::new(substitute_self_type(key, owner)),
            Box::new(substitute_self_type(value, owner)),
        ),
        TypeExprKind::Union(items) => TypeExprKind::Union(
            items
                .iter()
                .map(|item| substitute_self_type(item, owner))
                .collect(),
        ),
        TypeExprKind::Enum(members) => TypeExprKind::Enum(
            members
                .iter()
                .map(|member| substitute_self_member(member, owner))
                .collect(),
        ),
        TypeExprKind::Interface(members) => TypeExprKind::Interface(
            members
                .iter()
                .map(|member| substitute_self_member(member, owner))
                .collect(),
        ),
        TypeExprKind::Function {
            parameters,
            result,
            effects,
        } => TypeExprKind::Function {
            effects: effects.clone(),
            parameters: parameters
                .iter()
                .map(|parameter| FunctionTypeParameter {
                    name: parameter.name.clone(),
                    ty: substitute_self_type(&parameter.ty, owner),
                    variadic: parameter.variadic,
                    span: parameter.span,
                })
                .collect(),
            result: Box::new(substitute_self_type(result, owner)),
        },
        TypeExprKind::Newtype(inner) => {
            TypeExprKind::Newtype(Box::new(substitute_self_type(inner, owner)))
        }
        TypeExprKind::Mutable(inner) => {
            TypeExprKind::Mutable(Box::new(substitute_self_type(inner, owner)))
        }
        TypeExprKind::Reference(inner) => {
            TypeExprKind::Reference(Box::new(substitute_self_type(inner, owner)))
        }
        TypeExprKind::MutableReference(inner) => {
            TypeExprKind::MutableReference(Box::new(substitute_self_type(inner, owner)))
        }
        TypeExprKind::Intersect(items) => TypeExprKind::Intersect(
            items
                .iter()
                .map(|item| substitute_self_type(item, owner))
                .collect(),
        ),
        other => other.clone(),
    };
    TypeExpr {
        value,
        span: ty.span,
        origin: ty.origin.clone(),
    }
}

fn substitute_self_member(member: &TypeMember, owner: &str) -> TypeMember {
    TypeMember {
        name: member.name.clone(),
        ty: substitute_self_type(&member.ty, owner),
        span: member.span,
    }
}

/// Convert one typed `Test` into the legacy `(name, envelope)` shape for name
/// and metadata discovery only -- never for compilation.
///
/// [`crate::load::load_entry_module_for_test_discovery`] deliberately skips
/// import resolution (a test that expects a load failure must still be
/// selectable), and `src/lower.rs::discover_test_specs_in_entry` only reads a
/// test's `$test` profile and its metadata attributes (`tags`, `timeout-ms`,
/// `expect-error`, `clock`, ...); it never reads `do`. So this never resolves
/// call signatures (there is no cross-module signature index available yet at
/// discovery time) and never converts the test body -- it stubs `do` with an
/// empty sequence that the discovery reader is guaranteed not to inspect.
pub(crate) fn test_discovery_value(test: &Test) -> Result<(String, Value)> {
    let sig = ResolvedSignatures::default();
    let mut converter = Converter {
        sig: &sig,
        extra: Vec::new(),
        next_id: 0,
        current_params: Vec::new(),
        module_alias: String::new(),
        intrinsic_owner: false,
    };
    converter.test_metadata_value(test, Value::Sequence(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::LoadedProgram;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    fn module_from_source(source: &str) -> Module {
        let document = crate::syntax::parse(source).expect("parse S-expression source");
        crate::ast::lower_document(&document).expect("lower to typed AST")
    }

    /// Write a placeholder file at `path` and return its canonical form.
    /// `program.modules` is keyed by canonical path but its file *content*
    /// is never re-read by `lower::lower_program`/`lower_library`/
    /// `lower_tests`; only import-path resolution needs the file to exist.
    fn touch(path: &Path) -> std::path::PathBuf {
        fs::write(path, "placeholder").expect("write placeholder module file");
        fs::canonicalize(path).expect("canonicalize placeholder module file")
    }

    fn single_module_program(dir: &Path, value: Value) -> LoadedProgram {
        let entry = touch(&dir.join("entry.vibra"));
        let mut modules = HashMap::new();
        modules.insert(entry.clone(), value);
        LoadedProgram {
            entry,
            modules,
            module_parts: HashMap::new(),
            embedded_files: Default::default(),
        }
    }

    fn assert_lowers(source: &str) -> LoadedProgram {
        let module = module_from_source(source);
        let signatures = collect_local_signatures(&module).expect("collect signatures");
        let resolved = resolve_signatures("entry", &signatures, &[]);
        let value = module_to_value(&module, &resolved).expect("module_to_value");
        let dir = tempfile::tempdir().expect("tempdir");
        let program = single_module_program(dir.path(), value);
        if let Err(error) = crate::lower::lower_library(&program) {
            panic!("expected lower_library to succeed, got: {error:#}");
        }
        program
    }

    // ---- function with args/return/body, plus bare primitives ----

    #[test]
    fn function_with_args_return_body_and_primitives_lowers() {
        assert_lowers(
            r#"
(defn
  combine
  (left int64 right int64)
  int64
  (do
    (let sum (add left right))
    (if (greater-than sum 0) (do (return sum)) (do (return (negate sum))))
  )
)
"#,
        );
    }

    #[test]
    fn primitive_call_uses_dollar_keyed_shape() {
        let module =
            module_from_source("(defn f (a int64 b int64) bool (do (return (equal a b))))");
        let signatures = collect_local_signatures(&module).unwrap();
        let resolved = resolve_signatures("entry", &signatures, &[]);
        let value = module_to_value(&module, &resolved).unwrap();
        let rendered = format!("{value:?}");
        assert!(
            rendered.contains("$equal"),
            "expected bare `equal` to become `$equal`, got:\n{rendered}"
        );
    }

    #[test]
    fn deffect_operation_implicitly_declares_its_owner_root() {
        let module = module_from_source(
            r#"(defn main () void (do (let value 1)))
(deffect now
  (defn unix-millis () uint64
    (intrinsic @clock-now-unix-millis)
    effects: ()))"#,
        );
        let signatures = collect_local_signatures(&module).expect("collect signatures");
        let resolved = resolve_signatures("time", &signatures, &[]);
        let value =
            module_to_value_with_alias(&module, &resolved, "time").expect("deffect adapter");
        let debug = format!("{value:?}");
        assert!(
            debug.contains("time"),
            "owner root missing from effects: {debug}"
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let program = single_module_program(dir.path(), value);
        crate::lower::lower_library(&program).expect("nominal owner effect should lower");
    }

    #[test]
    fn deffect_operations_are_only_visible_through_their_root() {
        let module = module_from_source(
            r#"(defn file () int64 (return 1))
(deffect read
  (defn file () int64 (return 2)))"#,
        );
        let local = collect_local_signatures(&module).expect("collect signatures");
        assert!(local.calls.contains_key("file"));
        assert!(local.calls.contains_key("read.file"));
        assert!(!local.calls.contains_key("file.file"));
        let resolved = resolve_signatures("fs", &local, &[]);
        assert!(resolved.calls.contains_key("fs.read.file"));
        assert!(resolved.calls.contains_key("read.file"));
    }

    #[test]
    fn deffect_effects_are_owner_plus_explicit_additive_roots() {
        let module = module_from_source(
            r#"(defn main () void (do (let value 1)))
(deffect now
  (defn unix-millis () uint64
    (intrinsic @clock-now-unix-millis)
    effects: (stream.read)))"#,
        );
        let signatures = collect_local_signatures(&module).expect("collect signatures");
        let resolved = resolve_signatures("time", &signatures, &[]);
        let value =
            module_to_value_with_alias(&module, &resolved, "time").expect("deffect adapter");
        let dir = tempfile::tempdir().expect("tempdir");
        let program = single_module_program(dir.path(), value);
        let lowered = crate::lower::lower_program(&program).expect("lower nominal deffect");
        let signature = lowered
            .functions
            .iter()
            .find(|(key, _)| key.ends_with("now.unix-millis"))
            .map(|(_, signature)| signature)
            .expect("operation signature");
        let expected = [
            ("stream".to_string(), "read".to_string()),
            ("time".to_string(), "now".to_string()),
        ]
        .into_iter()
        .collect();
        assert_eq!(signature.effects.labels, expected);
    }

    #[test]
    fn validated_intrinsics_mint_host_backed_nominal_endpoints() {
        let module = module_from_source(
            r#"(def io.input (newtype (handle @read)))
(defn main () void (do (let value 1)))
(deffect stdin
  (defn open () io.input
    (intrinsic @stdin-open)
    effects: ()))"#,
        );
        let signatures = collect_local_signatures(&module).expect("collect signatures");
        let resolved = resolve_signatures("io", &signatures, &[]);
        let value = module_to_value_with_alias(&module, &resolved, "io").expect("deffect adapter");
        let dir = tempfile::tempdir().expect("tempdir");
        let program = single_module_program(dir.path(), value);
        let lowered = crate::lower::lower_program(&program).expect("lower endpoint acquisition");
        let operation = lowered
            .functions
            .iter()
            .find(|(key, _)| key.ends_with("stdin.open"))
            .map(|(_, signature)| signature)
            .expect("operation signature");
        assert!(matches!(
            operation.return_type,
            crate::lower::TypeRef::Named(_)
        ));
    }

    #[test]
    fn intrinsic_result_shape_cannot_be_laundered_by_a_return_annotation() {
        let module = module_from_source(
            r#"(deffect now
  (defn unix-millis () str
    (intrinsic @clock-now-unix-millis)
    effects: ()))"#,
        );
        let signatures = collect_local_signatures(&module).expect("collect signatures");
        let resolved = resolve_signatures("time", &signatures, &[]);
        let value = module_to_value_with_alias(&module, &resolved, "time").expect("adapt");
        let dir = tempfile::tempdir().expect("tempdir");
        let program = single_module_program(dir.path(), value);
        let error = crate::lower::lower_library(&program).expect_err("wrong result type");
        let message = format!("{error:#}");
        assert!(message.contains("E-INTRINSIC-005"), "{message}");
    }

    #[test]
    fn pure_intrinsics_are_available_to_ordinary_functions() {
        let module = module_from_source(
            r#"(defn scalar-len (value str) uint64
  (return (intrinsic @str-scalar-len value)))"#,
        );
        let signatures = collect_local_signatures(&module).expect("collect signatures");
        let resolved = resolve_signatures("text", &signatures, &[]);
        let value = module_to_value_with_alias(&module, &resolved, "text").expect("adapt");
        let dir = tempfile::tempdir().expect("tempdir");
        let program = single_module_program(dir.path(), value);
        crate::lower::lower_library(&program).expect("pure intrinsic");
    }

    #[test]
    fn modules_reject_raw_wasm_outside_deffect_operations() {
        let module = module_from_source(
            r#"(defn bad () uint64
  (do (wasm "vibra_v1" "clock_now_unix_millis")))
(deffect now
  (defn unix-millis () uint64
    (intrinsic @clock-now-unix-millis)
    effects: ()))"#,
        );
        let signatures = collect_local_signatures(&module).expect("collect signatures");
        let resolved = resolve_signatures("time", &signatures, &[]);
        let error = module_to_value_with_alias(&module, &resolved, "time")
            .expect_err("raw wasm outside native operation");
        assert!(error.to_string().contains("E-WASM-008"));
    }

    // ---- generic function with `where:`, called with explicit type args ----

    #[test]
    fn generic_function_with_where_lowers() {
        assert_lowers(
            r#"
(defn pair-up (a t b t) (tuple t t) (do (return (tuple a b))) where: (t any))

(defn use-pair-up () (tuple int64 int64) (do (return (pair-up int64 1 2))))
"#,
        );
    }

    // ---- enum and record definitions ----

    #[test]
    fn enum_and_record_definitions_lower() {
        assert_lowers(
            r#"
(def color (enum (red void) (green void) (blue void)))
(def point (record (x int64) (y int64)))
(defn origin () point (do (return (record (x 0) (y 0)))))
(defn favorite () color (do (return (color.red))))
"#,
        );
    }

    // ---- impls: with methods ----

    #[test]
    fn impls_with_methods_lowers() {
        assert_lowers(
            r#"
(def shape (interface (area (fn-type (self self) int64))))
(def square
  (record (side int64))
  impls: ((impl shape methods: ((method area (fn (self square) int64 (do (return 1))))))))
"#,
        );
    }

    // ---- imports with aliases (cross-module call) ----

    #[test]
    fn import_with_alias_lowers_across_modules() {
        let util_module =
            module_from_source("(defn double (x int64) int64 (do (return (multiply x 2))))");
        let util_local = collect_local_signatures(&util_module).unwrap();
        let util_resolved = resolve_signatures("util", &util_local, &[]);
        let util_value = module_to_value(&util_module, &util_resolved).unwrap();

        let entry_module = module_from_source(
            r#"
(import util "./util.vibra")
(defn compute (x int64) int64 (do (return (util.double x))))
"#,
        );
        let entry_local = collect_local_signatures(&entry_module).unwrap();
        let entry_resolved = resolve_signatures(
            "entry",
            &entry_local,
            &[("util".to_string(), util_local.clone())],
        );
        let entry_value = module_to_value(&entry_module, &entry_resolved).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let util_path = touch(&dir.path().join("util.vibra"));
        let entry_path = touch(&dir.path().join("entry.vibra"));
        let mut modules = HashMap::new();
        modules.insert(util_path, util_value);
        modules.insert(entry_path.clone(), entry_value);
        let program = LoadedProgram {
            entry: entry_path,
            modules,
            module_parts: HashMap::new(),
            embedded_files: Default::default(),
        };
        if let Err(error) = crate::lower::lower_library(&program) {
            panic!("expected cross-module lower_library to succeed, got: {error:#}");
        }
    }

    #[test]
    fn import_value_shape_matches_legacy_envelope() {
        let module = module_from_source(r#"(import io "./io.vibra")"#);
        let signatures = collect_local_signatures(&module).unwrap();
        let resolved = resolve_signatures("entry", &signatures, &[]);
        let value = module_to_value(&module, &resolved).unwrap();
        let map = value.as_mapping().unwrap();
        let io_def = map.get(&Value::String("io".into())).unwrap();
        let io_map = io_def.as_mapping().unwrap();
        assert_eq!(
            io_map.get(&Value::String("$import".into())),
            Some(&Value::String("./io.vibra".into()))
        );
    }

    // ---- test with tags: ----

    #[test]
    fn test_declaration_with_tags_lowers() {
        // `test.assert` and peers come from the `test` stdlib module, which
        // this single-module program does not import; use a self-contained
        // expression built only from primitives instead.
        let module = module_from_source(
            r#"
(test.scenario "arithmetic"
  (test.case "addition-is-checked"
    (let ok (equal 1 1))
    tags: (@fast @language)))
"#,
        );
        let signatures = collect_local_signatures(&module).unwrap();
        let resolved = resolve_signatures("entry", &signatures, &[]);
        let value = module_to_value(&module, &resolved).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let program = single_module_program(dir.path(), value);
        let tests = crate::lower::lower_tests(&program).expect("lower_tests");
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "arithmetic::addition-is-checked");
    }

    // ---- convert / cast ----

    #[test]
    fn convert_and_cast_lower() {
        assert_lowers(
            r#"
(def meters (newtype int64))
(defn narrow (x int64) int32 (do (return (convert x int32 5))))
(defn wrap (x int64) meters (do (return (cast x meters))))
"#,
        );
    }

    // ---- handle types ----

    #[test]
    fn handle_types_lower() {
        assert_lowers(
            r#"
(def out-handle (handle @write))
"#,
        );
    }

    // ---- unmappable construct: a specific, path-qualified error ----

    #[test]
    fn top_level_macro_fails_closed_with_specific_error() {
        let module = module_from_source(
            r#"
(macro
  unless
  (condition @expr-syntax body @expr-syntax)
  @expr-syntax
  (do
    (quote @expr-syntax (if (not (unquote condition)) (do (unquote body)) (do)))
  )
)
"#,
        );
        let signatures = collect_local_signatures(&module).unwrap();
        let resolved = resolve_signatures("entry", &signatures, &[]);
        let error = module_to_value(&module, &resolved).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("E-ADAPT-001") && message.contains("unless"),
            "expected a specific macro error naming the construct, got: {message}"
        );
    }

    #[test]
    fn nested_control_flow_expression_fails_closed() {
        // `while` used as a nested expression (not a statement) has no
        // legacy shape; this must fail closed rather than emit something
        // `lower.rs` would silently misinterpret.
        let module = module_from_source(
            r#"
(defn odd () void (do (let x (tuple (while true (do)) 1))))
"#,
        );
        let signatures = collect_local_signatures(&module).unwrap();
        let resolved = resolve_signatures("entry", &signatures, &[]);
        let error = module_to_value(&module, &resolved).unwrap_err();
        assert!(format!("{error:#}").contains("E-ADAPT-021"));
    }
}
