//! Generic lowering for stdlib-qualified calls with `$wasm.import` metadata.
//!
//! Top-level symbol mappings are envelope-shaped: exactly one `$`-form key (a
//! type constructor, `$function`, or `$import`) plus optional `=`-prefixed
//! annotations (`=where`, `=doc`, and forthcoming `=defs` / `=impl`).
//! `$forall` is no longer recognized; generics are declared at the symbol
//! level via `=where` and instantiated explicitly at type-position use sites.
//! Bare `where:` / `doc:` (the pre-1.0 spelling) is rejected with
//! `E-ANNO-002`.

use crate::load::{map_get_str, LoadedProgram};
use crate::project;
use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// While lowering a user-defined function body, validates `$return` against this type.
#[derive(Debug, Clone)]
pub struct UserFnContext {
    pub return_type: TypeRef,
    /// Full import path of the defining module (`""` = entry). Resolves private `-` calls,
    /// nested import aliases (e.g. `$util.id` → `a.util.id`), and iface/type qualification.
    pub home_module: String,
}

fn stmt_home_module(fn_ctx: Option<&UserFnContext>) -> &str {
    fn_ctx.map(|c| c.home_module.as_str()).unwrap_or("")
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<RuntimeValue>),
    Record(BTreeMap<String, RuntimeValue>),
    Tuple(Vec<RuntimeValue>),
    Map(Vec<(RuntimeValue, RuntimeValue)>),
    Typed {
        type_ref: TypeRef,
        value: Box<RuntimeValue>,
    },
    Capability(CapabilityGrant),
    GrantToken(GrantToken),
    Policy(PolicyValue),
    Enum {
        enum_key: String,
        tag: String,
        payload: Option<Box<RuntimeValue>>,
    },
    Void,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrantToken {
    pub name: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityGrant {
    pub type_key: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyValue {
    pub policy: PolicyType,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Value(RuntimeValue),
    VarRef(String),
    Call {
        call: Box<Call>,
        return_type: TypeRef,
    },
    Cast {
        from: Box<Expr>,
        target: TypeRef,
    },
    PolicyNarrow {
        from: Box<Expr>,
        target: TypeRef,
    },
    EnumConstructor {
        enum_key: String,
        tag: String,
        payload: Option<Box<Expr>>,
    },
    Record(BTreeMap<String, Expr>),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    If {
        cond: Box<Expr>,
        then_e: Box<Expr>,
        else_e: Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub struct ImportTarget {
    pub module: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum WasmArgSpec {
    Arg(String),
    ConstInt(i64),
    ConstStr(String),
}

/// A literal type pins a single value (e.g. the string `"ok"`).
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralType {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    Bool,
    Str,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Void,
    /// Reference to a registered type alias by qualified name (e.g. `m.pair`).
    Named(String),
    /// A nominal wrapper around `inner`. The `name` is populated when the
    /// top-level alias is registered and is the type identity for equality.
    Newtype {
        name: String,
        inner: Box<TypeRef>,
    },
    Capability {
        name: String,
        kind: String,
    },
    Policy(PolicyType),
    GrantToken,
    /// A type-parameter name in scope (declared in a `where:` annotation).
    Generic(String),
    /// A use of a generic type alias with explicit type arguments. `type_args`
    /// is positional (matches the alias's `type_params` order); for enum
    /// constructors at value sites the args are computed by payload inference.
    Instantiated {
        base: String,
        type_args: Vec<TypeRef>,
    },
    Literal(LiteralType),
    Record(BTreeMap<String, TypeRef>),
    Tuple(Vec<TypeRef>),
    Array(Box<TypeRef>),
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
    Union(Vec<TypeRef>),
    Enum(BTreeMap<String, TypeRef>),
    Interface(BTreeMap<String, TypeRef>),
    Intersect(Vec<TypeRef>),
    FnType {
        args: Box<TypeRef>,
        return_type: Box<TypeRef>,
    },
    /// The reserved `$self` type. Inside an `$interface` body it is an
    /// existential placeholder bound at impl time. Inside `=defs` / `=impl`
    /// (Phases 3/4) it is substituted by the enclosing type. Outside of
    /// those positions it is a parse-time error (`E-SELF-001`).
    SelfType,
}

/// A registered top-level type-form definition. Generic aliases carry
/// `type_params`; non-generic aliases have `type_params.is_empty()`.
#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub alias: String,
    pub name: String,
    pub type_params: Vec<String>,
    /// Parallel to `type_params`. Each inner `Vec<TypeRef>` is the list of
    /// interface bounds for that parameter (empty = unbounded). Multiple
    /// entries mean AND -- the substituted type must satisfy every iface in
    /// the list. `$intersect` is flattened into the same set.
    pub type_param_bounds: Vec<Vec<TypeRef>>,
    pub body: TypeRef,
    /// Compile-time documentation string from the symbol's `=doc` annotation.
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    Wasm {
        import: ImportTarget,
        wasm_args: Vec<WasmArgSpec>,
    },
    User {
        statements: Vec<Statement>,
    },
}

#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub alias: String,
    pub symbol: String,
    /// Names from the symbol's `=where` annotation when generic; empty if non-generic.
    pub type_params: Vec<String>,
    /// Parallel to `type_params`. See `TypeAlias::type_param_bounds`.
    pub type_param_bounds: Vec<Vec<TypeRef>>,
    pub arg_names: Vec<String>,
    pub arg_types: Vec<TypeRef>,
    pub grant_decls: Vec<GrantDecl>,
    pub return_type: TypeRef,
    pub body: FunctionBody,
    /// Compile-time documentation string from the symbol's `=doc` annotation.
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub callee_key: String,
    pub type_args: Vec<TypeRef>,
    pub args: Vec<Expr>,
    pub grant_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantRequirement {
    Mandatory,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantDecl {
    pub name: String,
    pub requirement: GrantRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyType {
    pub domains: BTreeMap<String, Vec<PolicyGroup>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyGroup {
    pub requirement: GrantRequirement,
    pub scopes: Vec<PolicyScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyScope {
    Any,
    File(String),
    Dir(String),
    Exact(String),
    Prefix(String),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Bind(String),
    Literal(RuntimeValue),
    Enum {
        enum_key: String,
        tag: String,
        payload: Option<Box<Pattern>>,
    },
    Record(BTreeMap<String, Pattern>),
    Tuple(Vec<Pattern>),
    Array(Vec<Pattern>),
    Map(Vec<(Pattern, Pattern)>),
    Newtype {
        type_ref: TypeRef,
        inner: Box<Pattern>,
    },
    Interface(TypeRef),
}

#[derive(Debug, Clone)]
pub enum LetValue {
    Call(Call),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub enum Statement {
    Call(Call),
    Let {
        var: String,
        value: LetValue,
    },
    Return(Expr),
    Match {
        target: Expr,
        arms: Vec<MatchArm>,
    },
    /// Evaluate an expression for side effects; result is discarded.
    Eval(Expr),
    If {
        cond: Expr,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
    While {
        cond: Expr,
        body: Vec<Statement>,
    },
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub alias: String,
    pub name: String,
    /// Type parameter names declared in the symbol's `=where` annotation, in order.
    pub type_params: Vec<String>,
    /// Parallel to `type_params`. See `TypeAlias::type_param_bounds`.
    pub type_param_bounds: Vec<Vec<TypeRef>>,
    pub tags: BTreeMap<String, TypeRef>,
}

/// Identifies a single nominal interface implementation.
///
/// Because `=impl` blocks live on the implementing type definition, only one
/// impl per `(implementing_type, interface)` pair is possible by
/// construction (the orphan rule is enforced syntactically). Concrete
/// bindings for the interface's `=where` params live in `ImplBody` as data,
/// not as part of the key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImplKey {
    pub implementing_type: String,
    pub interface: String,
}

#[derive(Debug, Clone)]
pub struct ImplBody {
    pub methods: HashMap<String, ImplMethodBinding>,
    /// Bindings for the interface's `=where` type parameters, in the order
    /// the interface declared them. May contain concrete types or
    /// `Generic(name)` references to impl-local type params.
    pub interface_args: Vec<TypeRef>,
    /// Type parameters introduced by the impl's own `=where` annotation,
    /// in declaration order. Empty for non-generic impls.
    pub impl_type_params: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum ImplMethodBinding {
    /// A fresh `$function` envelope was supplied; its sig is registered in
    /// `functions` under the qualified key recorded here.
    Fresh(String),
    /// The impl re-uses an already-registered function. The string is the
    /// sig key in `functions`.
    Ref(String),
}

#[derive(Debug, Clone)]
pub struct LoweredProgram {
    pub statements: Vec<Statement>,
    pub main_arg_bindings: Vec<(String, TypeRef)>,
    pub main_grant_decls: Vec<GrantDecl>,
    pub constants: HashMap<String, RuntimeValue>,
    pub functions: HashMap<String, FunctionSig>,
    pub impls: HashMap<ImplKey, ImplBody>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LoweredTestCase {
    pub name: String,
    pub program: LoweredProgram,
}

#[derive(Debug, Clone)]
pub struct LoweredExec {
    pub expr: Expr,
    pub program: LoweredProgram,
}

// ===== Annotation envelope =====

/// Envelope around a top-level symbol's value: one `$`-form key plus optional
/// `=`-prefixed annotation siblings (`=where`, `=doc`, `=defs`, `=impl`).
struct DefEnvelope<'a> {
    form_key: String,
    form_value: &'a Value,
    type_params: Vec<String>,
    /// Raw, unresolved bound `Value`s parallel to `type_params`. Bounds are
    /// resolved to `Vec<TypeRef>` in a later pass once all type aliases are
    /// known (so a bound can reference an iface declared later in the file).
    type_param_bound_values: Vec<Vec<&'a Value>>,
    doc: Option<String>,
    /// `=defs` annotation: a mapping of name -> function-definition envelope
    /// for inherent operations on the enclosing type.
    defs: Option<&'a serde_yaml::Mapping>,
    /// `=impl` annotation: a mapping of `$iface-alias` -> impl payload that
    /// pins iface type-args, declares any impl-local generics via `=where`,
    /// and binds each iface method to either a fresh `$function` envelope
    /// or a `$existing.qualified.name` reference.
    impls: Option<&'a serde_yaml::Mapping>,
    /// `$function`-only siblings: optional extra `args` mapping, required `return` / `do`.
    function_args: Option<&'a Value>,
    function_grants: Option<&'a Value>,
    function_return: Option<&'a Value>,
    function_do: Option<&'a Value>,
}

/// Synthetic field name for the `$function` payload in module-level functions.
const MODULE_FN_PRIMARY_ARG: &str = "subject";
/// Field name for the primary/`$self` argument in inherent ops and iface methods.
const INHERENT_FN_PRIMARY_ARG: &str = "self";

/// Annotation keys we currently understand. Anything else with a `=` prefix
/// is rejected with `E-ANNO-001`. Any sibling key that does not start with
/// `$` or `=` is rejected with `E-ANNO-002` (legacy un-prefixed annotation).
const KNOWN_ANNOTATIONS: &[&str] = &["=where", "=doc", "=defs", "=impl"];

fn parse_def_envelope<'a>(v: &'a Value, warnings: &mut Vec<String>) -> Result<DefEnvelope<'a>> {
    let m = v.as_mapping().context("definition must be a mapping")?;

    let mut form_key: Option<String> = None;
    let mut form_value: Option<&'a Value> = None;
    let mut type_params: Vec<String> = Vec::new();
    let mut type_param_bound_values: Vec<Vec<&'a Value>> = Vec::new();
    let mut doc: Option<String> = None;
    let mut defs: Option<&'a serde_yaml::Mapping> = None;
    let mut impls: Option<&'a serde_yaml::Mapping> = None;
    let mut function_args: Option<&'a Value> = None;
    let mut function_grants: Option<&'a Value> = None;
    let mut function_return: Option<&'a Value> = None;
    let mut function_do: Option<&'a Value> = None;

    for (k, val) in m {
        let ks = k.as_str().context("definition key must be a string")?;
        if ks.starts_with('$') {
            if form_key.is_some() {
                bail!(
                    "definition has multiple `$`-form keys (`{}` and `{ks}`); expected exactly one",
                    form_key.as_deref().unwrap_or("")
                );
            }
            form_key = Some(ks.to_string());
            form_value = Some(val);
        } else if ks == "=defs" {
            let dm = val
                .as_mapping()
                .context("E-DEFS-001: `=defs` must be a mapping of `name: $function` entries")?;
            if defs.is_some() {
                bail!("definition declares `=defs` more than once");
            }
            defs = Some(dm);
        } else if ks == "=impl" {
            let im = val.as_mapping().context(
                "E-IMPL-001: `=impl` must be a mapping of `$iface: <impl-payload>` entries",
            )?;
            if impls.is_some() {
                bail!("definition declares `=impl` more than once");
            }
            impls = Some(im);
        } else if ks == "=where" {
            let wm = val
                .as_mapping()
                .context("`=where` must be a mapping of type-parameter name to bound list")?;
            for (wk, wv) in wm {
                let name = wk.as_str().context("`=where` keys must be strings")?;
                maybe_warn_kebab(name, "type parameter", warnings);
                let bounds_seq = wv
                    .as_sequence()
                    .with_context(|| {
                        format!("`=where` value for `{name}` must be an array of bounds (use `[]` for unbounded)")
                    })?;
                if type_params.iter().any(|n| n == name) {
                    bail!("`=where` declares duplicate type parameter `{name}`");
                }
                let mut entry_values: Vec<&'a Value> = Vec::with_capacity(bounds_seq.len());
                for b in bounds_seq {
                    entry_values.push(b);
                }
                type_params.push(name.to_string());
                type_param_bound_values.push(entry_values);
            }
        } else if ks == "=doc" {
            let s = val.as_str().with_context(|| {
                format!("E-DOC-001: `=doc` annotation must be a string scalar (got non-string for `{ks}`)")
            })?;
            doc = Some(s.to_string());
        } else if ks == "args" {
            if function_args.is_some() {
                bail!("duplicate `args` key on definition");
            }
            function_args = Some(val);
        } else if ks == "grants" {
            if function_grants.is_some() {
                bail!("duplicate `grants` key on definition");
            }
            function_grants = Some(val);
        } else if ks == "return" {
            if function_return.is_some() {
                bail!("duplicate `return` key on definition");
            }
            function_return = Some(val);
        } else if ks == "do" {
            if function_do.is_some() {
                bail!("duplicate `do` key on definition");
            }
            function_do = Some(val);
        } else if ks == "where" || ks == "doc" {
            bail!(
                "E-ANNO-002: annotation `{ks}` must use the `=` prefix; rename it to `={ks}` (annotations are now `=`-prefixed in v1)"
            );
        } else if let Some(rest) = ks.strip_prefix('=') {
            bail!(
                "E-ANNO-001: unknown annotation `={rest}`; recognised annotations are: {}",
                KNOWN_ANNOTATIONS.join(", ")
            );
        } else {
            bail!(
                "E-ANNO-001: unknown sibling key `{ks}`; expected one `$`-form key plus optional `=`-annotations ({})",
                KNOWN_ANNOTATIONS.join(", ")
            );
        }
    }

    let form_key = form_key.context("definition must have one `$`-form key")?;
    let form_value = form_value.expect("set together with form_key");

    if form_key != "$function" && form_key != "$test" {
        if function_args.is_some()
            || function_grants.is_some()
            || function_return.is_some()
            || function_do.is_some()
        {
            bail!(
                "keys `args`, `grants`, `return`, and `do` are only valid on `$function` or `$test` definitions (got `{}`)",
                form_key
            );
        }
    } else if form_key == "$function" {
        let nested_function = form_value.as_mapping().is_some_and(|inner| {
            map_get_str(inner, "args").is_some()
                && map_get_str(inner, "return").is_some()
                && map_get_str(inner, "do").is_some()
        });
        if nested_function {
            bail!(
                "E-ONE-001: `$function` must be canonical (`$function: $void`, `$function: $self`, or `$function: {{ <name>: <type> }}` with sibling `return`/`do`); nested `{{ args, return, do }}` envelopes are not canonical"
            );
        } else {
            let _ = function_return.context("missing `return` on `$function`")?;
            let _ = function_do.context("missing `do` on `$function`")?;
        }
    } else {
        if function_args.is_some()
            || function_grants.is_some()
            || function_return.is_some()
            || function_do.is_some()
        {
            bail!("`$test` must use nested syntax `$test: {{ do: [...] }}`");
        }
        let tm = form_value
            .as_mapping()
            .context("`$test` value must be a mapping with `do`")?;
        if tm.len() != 1 || map_get_str(tm, "do").is_none() {
            bail!("`$test` value must contain only `do`");
        }
        let _ = map_get_str(tm, "do").context("missing `do` on `$test`")?;
    }

    if defs.is_some() && (form_key == "$function" || form_key == "$test") {
        bail!(
            "E-DEFS-001: `=defs` is only valid alongside a type definition, not on an executable definition"
        );
    }
    if defs.is_some() && form_key == "$import" {
        bail!("E-DEFS-001: `=defs` is only valid alongside a type definition, not on a `$import`");
    }
    if impls.is_some() && (form_key == "$function" || form_key == "$test") {
        bail!(
            "E-IMPL-001: `=impl` is only valid alongside a type definition, not on an executable definition"
        );
    }
    if impls.is_some() && form_key == "$import" {
        bail!("E-IMPL-001: `=impl` is only valid alongside a type definition, not on a `$import`");
    }

    Ok(DefEnvelope {
        form_key,
        form_value,
        type_params,
        type_param_bound_values,
        doc,
        defs,
        impls,
        function_args,
        function_grants,
        function_return,
        function_do,
    })
}

/// Resolve `DefEnvelope::type_param_bound_values` to a parallel `Vec<Vec<TypeRef>>`
/// using the symbol's own type-params as the parsing scope. The caller is
/// expected to qualify any `Named` references afterwards via
/// `qualify_named_type`. Bounds may not reference `$self` and may not
/// themselves be `$self`; we pass `self_allowed = false` to enforce this.
fn resolve_def_envelope_bounds(
    env: &DefEnvelope,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<Vec<Vec<TypeRef>>> {
    let mut out: Vec<Vec<TypeRef>> = Vec::with_capacity(env.type_params.len());
    for raws in &env.type_param_bound_values {
        let mut bounds = Vec::with_capacity(raws.len());
        for raw in raws {
            let ty = parse_type_ref(raw, &env.type_params, skeletons, warnings, false)
                .with_context(|| "invalid type expression in `=where` bound list")?;
            bounds.push(ty);
        }
        out.push(bounds);
    }
    Ok(out)
}

fn qualify_bounds(
    module_alias: &str,
    bounds: Vec<Vec<TypeRef>>,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Vec<Vec<TypeRef>> {
    bounds
        .into_iter()
        .map(|inner| {
            inner
                .into_iter()
                .map(|t| qualify_named_type(module_alias, t, type_aliases))
                .collect()
        })
        .collect()
}

// ===== Skeleton pre-pass =====

#[derive(Debug, Clone)]
struct AliasSkeleton {
    type_params: Vec<String>,
}

const BUILTIN_TYPE_FORMS: &[&str] = &[
    "$newtype",
    "$record",
    "$tuple",
    "$array",
    "$map",
    "$intersect",
    "$interface",
    "$fn-type",
    "$literal",
    "$union",
    "$enum",
    "$policy",
    "$grant-token",
];

fn collect_alias_skeletons(
    program: &LoadedProgram,
    entry_project: Option<&project::LoadedProject>,
) -> Result<HashMap<String, AliasSkeleton>> {
    let entry_map = program
        .modules
        .get(&program.entry)
        .context("internal: entry module not loaded")?
        .as_mapping()
        .context("entry root must be mapping")?;
    let mut skeletons: HashMap<String, AliasSkeleton> = HashMap::new();
    let mut sink: Vec<String> = Vec::new();
    let mut visited_imports: HashSet<(String, PathBuf)> = HashSet::new();

    for (k, v) in entry_map {
        let alias = k.as_str().context("module keys must be strings")?;
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else {
            continue;
        };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let imported_path = resolve_import_path(&program.entry, imp_s, entry_project)
            .with_context(|| format!("resolve import alias `{alias}`"))?;
        collect_module_skeleton_tree(
            alias,
            &imported_path,
            program,
            entry_project,
            &mut skeletons,
            &mut sink,
            &mut visited_imports,
        )?;
    }

    collect_module_skeletons(
        "",
        program.modules.get(&program.entry).unwrap(),
        &mut skeletons,
        &mut sink,
    )?;

    Ok(skeletons)
}

fn collect_module_skeleton_tree(
    alias: &str,
    module_path: &std::path::Path,
    program: &LoadedProgram,
    entry_project: Option<&project::LoadedProject>,
    skeletons: &mut HashMap<String, AliasSkeleton>,
    sink: &mut Vec<String>,
    visited_imports: &mut HashSet<(String, PathBuf)>,
) -> Result<()> {
    if !visited_imports.insert((alias.to_string(), module_path.to_path_buf())) {
        return Ok(());
    }

    let module_root = program
        .modules
        .get(module_path)
        .with_context(|| format!("imported module missing from graph `{alias}`"))?;
    let map = module_root
        .as_mapping()
        .context("module root must be mapping")?;
    for (k, v) in map {
        let nested_alias = k.as_str().context("module key must be string")?;
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else {
            continue;
        };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let nested_path = resolve_import_path(module_path, imp_s, entry_project)
            .with_context(|| format!("resolve nested import alias `{nested_alias}`"))?;
        let full_nested = join_module_prefix(alias, nested_alias);
        collect_module_skeleton_tree(
            &full_nested,
            &nested_path,
            program,
            entry_project,
            skeletons,
            sink,
            visited_imports,
        )?;
    }
    collect_module_skeletons(alias, module_root, skeletons, sink)
}

fn collect_module_skeletons(
    alias: &str,
    module_root: &Value,
    skeletons: &mut HashMap<String, AliasSkeleton>,
    sink: &mut Vec<String>,
) -> Result<()> {
    let map = module_root
        .as_mapping()
        .context("module root must be mapping")?;
    for (k, v) in map {
        let name = k.as_str().context("module key must be string")?;
        let Some(_) = v.as_mapping() else { continue };
        let env = match parse_def_envelope(v, sink) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if env.form_key == "$function" || env.form_key == "$test" || env.form_key == "$import" {
            continue;
        }
        if env.form_key == "$option" {
            bail!(
                "E-OPTION-001: `$option: T` was removed; import `stdlib/option.vibra` and instantiate its tagged enum alias"
            );
        }
        if !BUILTIN_TYPE_FORMS.contains(&env.form_key.as_str()) {
            continue;
        }
        let key = if alias.is_empty() {
            name.to_string()
        } else {
            format!("{alias}.{name}")
        };
        if skeletons.contains_key(&key) {
            bail!("duplicate type symbol `{key}` (skeleton pass)");
        }
        skeletons.insert(
            key,
            AliasSkeleton {
                type_params: env.type_params,
            },
        );
    }
    Ok(())
}

// ===== Type expression parser =====

/// Whether the reserved `$self` type may appear in the current type-position.
/// `true` inside `$interface` bodies (existential) and inside `=defs` / `=impl`
/// blocks (concrete, substituted later); `false` everywhere else, where `$self`
/// is rejected with `E-SELF-001`.
fn parse_type_ref(
    v: &Value,
    scope: &[String],
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
    self_allowed: bool,
) -> Result<TypeRef> {
    if let Some(m) = v.as_mapping() {
        if map_get_str(m, "$option").is_some()
            && m.len() == 1
            && !map_get_str(m, "$option").is_some_and(is_type_argument_map)
        {
            bail!(
                "E-OPTION-001: `$option: T` was removed; import `stdlib/option.vibra` and instantiate its tagged enum alias"
            );
        }
        for &form in BUILTIN_TYPE_FORMS {
            if let Some(form_v) = map_get_str(m, form) {
                if m.len() != 1 {
                    bail!(
                        "type expression `{form}` must be a single-key mapping; got {} keys",
                        m.len()
                    );
                }
                return parse_type_constructor(
                    form,
                    form_v,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                );
            }
        }
        if m.len() == 1 {
            let (k, type_args_v) = m.iter().next().expect("len 1");
            let key = k.as_str().context("type expression key must be a string")?;
            if let Some(name) = key.strip_prefix('$') {
                if name.is_empty() {
                    bail!("type alias reference must have a name after `$`");
                }
                maybe_warn_kebab_qualified(name, "type reference", warnings);
                let type_args = parse_instantiation_args(
                    name,
                    type_args_v,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                )?;
                return Ok(TypeRef::Instantiated {
                    base: name.to_string(),
                    type_args,
                });
            }
            bail!("type expression key `{key}` must start with `$`");
        }
        bail!(
            "unsupported type expression mapping; expected one of {} or `{{ $alias: {{ tparam: T, ... }} }}`",
            BUILTIN_TYPE_FORMS.join(", ")
        );
    }

    let raw = v
        .as_str()
        .context("type annotation must be string or type expression object")?;
    let name = raw
        .strip_prefix('$')
        .with_context(|| format!("type annotation `{raw}` must start with `$`"))?;
    let ty = match name {
        "bool" => TypeRef::Bool,
        "str" => TypeRef::Str,
        "int8" => TypeRef::Int8,
        "int16" => TypeRef::Int16,
        "int32" => TypeRef::Int32,
        "int64" => TypeRef::Int64,
        "uint8" => TypeRef::UInt8,
        "uint16" => TypeRef::UInt16,
        "uint32" => TypeRef::UInt32,
        "uint64" => TypeRef::UInt64,
        "float32" => TypeRef::Float32,
        "float64" => TypeRef::Float64,
        "void" => TypeRef::Void,
        "grant-token" => TypeRef::GrantToken,
        "self" => {
            if !self_allowed {
                bail!(
                    "E-SELF-001: `$self` is only valid inside an `$interface` body or inside a type's `=defs` / `=impl` annotation"
                );
            }
            TypeRef::SelfType
        }
        "int" | "float" => {
            bail!("type alias `${name}` was removed; use explicit numeric primitives")
        }
        _ => {
            maybe_warn_kebab_qualified(name, "type reference", warnings);
            if scope.iter().any(|s| s == name) {
                TypeRef::Generic(name.to_string())
            } else {
                if let Some(skel) = skeletons.get(name) {
                    if !skel.type_params.is_empty() {
                        let template = skel
                            .type_params
                            .iter()
                            .map(|p| format!("{p}: $T"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        bail!(
                            "E-GEN-001: generic type alias `${name}` requires explicit type arguments; use `{{ ${name}: {{ {template} }} }}`"
                        );
                    }
                }
                TypeRef::Named(name.to_string())
            }
        }
    };
    Ok(ty)
}

fn is_type_argument_map(value: &Value) -> bool {
    value.as_mapping().is_some_and(|map| {
        map.keys()
            .all(|key| key.as_str().is_some_and(|name| !name.starts_with('$')))
    })
}

fn parse_type_constructor(
    form: &str,
    v: &Value,
    scope: &[String],
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
    self_allowed: bool,
) -> Result<TypeRef> {
    match form {
        "$newtype" => {
            if let Some(m) = v.as_mapping() {
                if m.len() != 1 || map_get_str(m, "$newtype").is_some() {
                    bail!("E-NEWTYPE-002: `$newtype` body must be exactly one type expression");
                }
            }
            let inner = parse_type_ref(v, scope, skeletons, warnings, self_allowed)
                .context("E-NEWTYPE-002: invalid `$newtype` inner type")?;
            Ok(TypeRef::Newtype {
                name: String::new(),
                inner: Box::new(inner),
            })
        }
        "$policy" => Ok(TypeRef::Policy(parse_policy_type(v)?)),
        "$grant-token" => Ok(TypeRef::GrantToken),
        "$record" => {
            let m = v
                .as_mapping()
                .context("`$record` must be a mapping of name -> type")?;
            let mut fields = BTreeMap::new();
            for (k, fv) in m {
                let name = k.as_str().context("$record key must be string")?;
                maybe_warn_kebab(name, "record field", warnings);
                let ty = parse_type_ref(fv, scope, skeletons, warnings, self_allowed)
                    .with_context(|| format!("invalid type for record field `{name}`"))?;
                if fields.insert(name.to_string(), ty).is_some() {
                    bail!("duplicate $record field `{name}`");
                }
            }
            Ok(TypeRef::Record(fields))
        }
        "$tuple" => {
            let s = v
                .as_sequence()
                .context("`$tuple` must be an array of type expressions")?;
            let mut items = Vec::with_capacity(s.len());
            for it in s {
                items.push(parse_type_ref(
                    it,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                )?);
            }
            Ok(TypeRef::Tuple(items))
        }
        "$array" => Ok(TypeRef::Array(Box::new(parse_type_ref(
            v,
            scope,
            skeletons,
            warnings,
            self_allowed,
        )?))),
        "$map" => {
            let m = v
                .as_mapping()
                .context("`$map` must be a mapping with `key` and `value`")?;
            for (k, _) in m {
                let ks = k.as_str().context("$map key must be string")?;
                if ks != "key" && ks != "value" {
                    bail!("$map only accepts `key` and `value`, got `{ks}`");
                }
            }
            let key = map_get_str(m, "key").context("$map missing `key`")?;
            let value = map_get_str(m, "value").context("$map missing `value`")?;
            Ok(TypeRef::Map {
                key: Box::new(parse_type_ref(
                    key,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                )?),
                value: Box::new(parse_type_ref(
                    value,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                )?),
            })
        }
        "$intersect" => {
            let s = v
                .as_sequence()
                .context("`$intersect` must be an array of type expressions")?;
            if s.len() < 2 {
                bail!("`$intersect` must contain at least two members");
            }
            let mut items = Vec::with_capacity(s.len());
            for it in s {
                items.push(parse_type_ref(
                    it,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                )?);
            }
            Ok(TypeRef::Intersect(items))
        }
        "$interface" => {
            let m = v
                .as_mapping()
                .context("`$interface` must be a mapping of name -> type")?;
            let mut members = BTreeMap::new();
            for (k, fv) in m {
                let name = k.as_str().context("$interface key must be string")?;
                maybe_warn_kebab(name, "interface member", warnings);
                // Inside an `$interface` body `$self` is always allowed: it is
                // an existential placeholder bound at impl time. Even when the
                // interface appears nested in a position that otherwise forbids
                // `$self`, the body itself opens the binding scope.
                let ty = parse_type_ref(fv, scope, skeletons, warnings, true)
                    .with_context(|| format!("invalid type for interface member `{name}`"))?;
                if members.insert(name.to_string(), ty).is_some() {
                    bail!("duplicate $interface member `{name}`");
                }
            }
            Ok(TypeRef::Interface(members))
        }
        "$fn-type" => {
            let m = v
                .as_mapping()
                .context("`$fn-type` must be a mapping with `args` and `return`")?;
            for (k, _) in m {
                let ks = k.as_str().context("$fn-type key must be string")?;
                if ks != "args" && ks != "return" {
                    bail!("$fn-type only accepts `args` and `return`, got `{ks}`");
                }
            }
            let args = map_get_str(m, "args").context("$fn-type missing `args`")?;
            let ret = map_get_str(m, "return").context("$fn-type missing `return`")?;
            Ok(TypeRef::FnType {
                args: Box::new(parse_type_ref(
                    args,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                )?),
                return_type: Box::new(parse_type_ref(
                    ret,
                    scope,
                    skeletons,
                    warnings,
                    self_allowed,
                )?),
            })
        }
        "$literal" => {
            let lit = match v {
                Value::Bool(b) => LiteralType::Bool(*b),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        LiteralType::Int(i)
                    } else if let Some(f) = n.as_f64() {
                        LiteralType::Float(f)
                    } else {
                        bail!("$literal number is not representable")
                    }
                }
                Value::String(s) => LiteralType::Str(s.clone()),
                _ => bail!("`$literal` must be a string, number, or boolean"),
            };
            Ok(TypeRef::Literal(lit))
        }
        "$union" => parse_union_type(v, scope, skeletons, warnings, self_allowed),
        "$enum" => Ok(TypeRef::Enum(parse_enum_tags(
            v,
            scope,
            skeletons,
            warnings,
            self_allowed,
        )?)),
        _ => unreachable!("unknown builtin type form `{form}`"),
    }
}

fn parse_policy_type(v: &Value) -> Result<PolicyType> {
    let domains = v
        .as_mapping()
        .context("`$policy` must be a mapping of domain -> groups")?;
    let mut out = BTreeMap::new();
    for (domain, groups_v) in domains {
        let domain = domain
            .as_str()
            .context("`$policy` domain names must be strings")?
            .to_string();
        let groups = groups_v
            .as_sequence()
            .with_context(|| format!("`$policy.{domain}` must be a sequence"))?;
        let mut parsed_groups = Vec::with_capacity(groups.len());
        for group_v in groups {
            let group = group_v
                .as_mapping()
                .with_context(|| format!("`$policy.{domain}` entries must be mappings"))?;
            let requirement = match map_get_str(group, "requirement").and_then(Value::as_str) {
                Some("mandatory") => GrantRequirement::Mandatory,
                Some("optional") => GrantRequirement::Optional,
                _ => bail!("`$policy.{domain}` requirement must be `mandatory` or `optional`"),
            };
            let scopes_v = map_get_str(group, "scopes")
                .with_context(|| format!("`$policy.{domain}` entry missing `scopes`"))?;
            let scopes = parse_policy_scopes(&domain, scopes_v)?;
            parsed_groups.push(PolicyGroup {
                requirement,
                scopes,
            });
        }
        out.insert(domain, parsed_groups);
    }
    Ok(PolicyType { domains: out })
}

fn parse_policy_scopes(domain: &str, v: &Value) -> Result<Vec<PolicyScope>> {
    if v.as_str() == Some("any") {
        return Ok(vec![PolicyScope::Any]);
    }
    let scopes = v
        .as_sequence()
        .with_context(|| format!("`$policy.{domain}.scopes` must be `any` or a sequence"))?;
    let mut out = Vec::with_capacity(scopes.len());
    for scope_v in scopes {
        let scope = scope_v
            .as_mapping()
            .with_context(|| format!("`$policy.{domain}` scope must be a mapping"))?;
        if scope.len() != 1 {
            bail!("`$policy.{domain}` scope must contain exactly one selector");
        }
        let (k, v) = scope.iter().next().expect("checked one entry");
        let key = k
            .as_str()
            .with_context(|| format!("`$policy.{domain}` scope key must be a string"))?;
        let value = v
            .as_str()
            .with_context(|| format!("`$policy.{domain}.{key}` must be a string"))?
            .to_string();
        let parsed = match key {
            "file" => PolicyScope::File(value),
            "dir" => PolicyScope::Dir(value),
            "exact" => PolicyScope::Exact(value),
            "prefix" => PolicyScope::Prefix(value),
            _ => bail!("unsupported `$policy.{domain}` scope selector `{key}`"),
        };
        out.push(parsed);
    }
    Ok(out)
}

/// Parse a `{ tparam: $T, ... }` mapping at a type-position alias use site,
/// resolving the named arguments into the alias's declared positional order.
fn unique_skeleton_match<'a>(
    skeletons: &'a HashMap<String, AliasSkeleton>,
    base: &str,
    matches: impl Fn(&str) -> bool,
) -> Result<Option<&'a AliasSkeleton>> {
    let mut keys: Vec<&str> = skeletons
        .keys()
        .map(String::as_str)
        .filter(|key| matches(key))
        .collect();
    keys.sort_unstable();
    match keys.as_slice() {
        [] => Ok(None),
        [key] => Ok(skeletons.get(*key)),
        candidates => bail!(
            "E-GEN-002: ambiguous type alias `${base}`; qualify one of [{}]",
            candidates.join(", ")
        ),
    }
}

fn parse_instantiation_args(
    base: &str,
    v: &Value,
    scope: &[String],
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
    self_allowed: bool,
) -> Result<Vec<TypeRef>> {
    let skel = if let Some(skel) = skeletons.get(base) {
        skel
    } else if let Some(skel) =
        unique_skeleton_match(skeletons, base, |key| key.ends_with(&format!(".{base}")))?
    {
        skel
    } else {
        unique_skeleton_match(skeletons, base, |key| {
            strip_module_prefix(key) == strip_module_prefix(base)
        })?
        .with_context(|| format!("E-GEN-002: unknown type alias `${base}` in instantiation"))?
    };
    if skel.type_params.is_empty() {
        bail!("E-GEN-002: type alias `${base}` is non-generic; do not pass type arguments");
    }
    let m = v.as_mapping().with_context(|| {
        format!("instantiation of `${base}` must be a mapping of `tparam: type`")
    })?;
    let allowed: HashSet<&str> = skel.type_params.iter().map(String::as_str).collect();
    for (k, _) in m {
        let ks = k.as_str().context("instantiation key must be string")?;
        if !allowed.contains(ks) {
            bail!(
                "E-GEN-002: unknown type parameter `{ks}` in instantiation of `${base}`; expected one of [{}]",
                skel.type_params.join(", ")
            );
        }
    }
    let mut out = Vec::with_capacity(skel.type_params.len());
    for tp in &skel.type_params {
        let tv = m.get(Value::String(tp.clone())).with_context(|| {
            format!("E-GEN-002: missing type argument `{tp}` in instantiation of `${base}`")
        })?;
        out.push(parse_type_ref(
            tv,
            scope,
            skeletons,
            warnings,
            self_allowed,
        )?);
    }
    Ok(out)
}

fn parse_union_type(
    v: &Value,
    scope: &[String],
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
    self_allowed: bool,
) -> Result<TypeRef> {
    if let Some(m) = v.as_mapping() {
        if map_get_str(m, "variants").is_some() {
            bail!(
                "legacy `variants` union syntax was removed; use `$union: [...]` or `$enum: {{...}}`"
            );
        }
    }
    let items = v
        .as_sequence()
        .context("$union must be an array of type expressions")?;
    if items.len() < 2 {
        bail!("$union must contain at least two members");
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(parse_type_ref(
            item,
            scope,
            skeletons,
            warnings,
            self_allowed,
        )?);
    }
    if out.iter().any(|item| *item == TypeRef::Void) {
        bail!(
            "E-OPTION-001: `$union` with a direct `$void` member was removed; use the tagged stdlib option enum"
        );
    }
    Ok(TypeRef::Union(out))
}

fn parse_enum_tags(
    v: &Value,
    scope: &[String],
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
    self_allowed: bool,
) -> Result<BTreeMap<String, TypeRef>> {
    let m = v.as_mapping().context("$enum must be a mapping")?;
    if m.is_empty() {
        bail!("$enum must not be empty");
    }
    let mut tags = BTreeMap::new();
    for (k, tv) in m {
        let tag = k.as_str().context("$enum tag must be string")?;
        maybe_warn_kebab(tag, "enum tag", warnings);
        let ty = parse_type_ref(tv, scope, skeletons, warnings, self_allowed)
            .with_context(|| format!("invalid type for enum tag `{tag}`"))?;
        tags.insert(tag.to_string(), ty);
    }
    Ok(tags)
}

// ===== Type machinery =====

/// Replace every occurrence of `TypeRef::SelfType` inside `ty` with
/// `self_ty`, recursing through composite forms. Used by `=defs` / `=impl`
/// (Phases 3/4) to bind `$self` to the enclosing type. Inside an
/// `$interface` body, `SelfType` is left in place by callers that want
/// the existential meaning.
#[allow(dead_code)]
fn substitute_self(ty: &TypeRef, self_ty: &TypeRef) -> TypeRef {
    match ty {
        TypeRef::SelfType => self_ty.clone(),
        TypeRef::Union(items) => {
            TypeRef::Union(items.iter().map(|t| substitute_self(t, self_ty)).collect())
        }
        TypeRef::Enum(tags) => TypeRef::Enum(
            tags.iter()
                .map(|(k, v)| (k.clone(), substitute_self(v, self_ty)))
                .collect(),
        ),
        TypeRef::Record(fields) => TypeRef::Record(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), substitute_self(v, self_ty)))
                .collect(),
        ),
        TypeRef::Tuple(items) => {
            TypeRef::Tuple(items.iter().map(|t| substitute_self(t, self_ty)).collect())
        }
        TypeRef::Array(inner) => TypeRef::Array(Box::new(substitute_self(inner, self_ty))),
        TypeRef::Map { key, value } => TypeRef::Map {
            key: Box::new(substitute_self(key, self_ty)),
            value: Box::new(substitute_self(value, self_ty)),
        },
        TypeRef::Interface(members) => TypeRef::Interface(
            members
                .iter()
                .map(|(k, v)| (k.clone(), substitute_self(v, self_ty)))
                .collect(),
        ),
        TypeRef::Intersect(items) => {
            TypeRef::Intersect(items.iter().map(|t| substitute_self(t, self_ty)).collect())
        }
        TypeRef::FnType { args, return_type } => TypeRef::FnType {
            args: Box::new(substitute_self(args, self_ty)),
            return_type: Box::new(substitute_self(return_type, self_ty)),
        },
        TypeRef::Instantiated { base, type_args } => TypeRef::Instantiated {
            base: base.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_self(t, self_ty))
                .collect(),
        },
        TypeRef::Newtype { name, inner } => TypeRef::Newtype {
            name: name.clone(),
            inner: Box::new(substitute_self(inner, self_ty)),
        },
        TypeRef::Capability { .. } => ty.clone(),
        _ => ty.clone(),
    }
}

fn substitute_type(ty: &TypeRef, subst: &HashMap<String, TypeRef>) -> TypeRef {
    match ty {
        TypeRef::Generic(n) => subst.get(n.as_str()).cloned().unwrap_or_else(|| ty.clone()),
        TypeRef::Union(items) => {
            TypeRef::Union(items.iter().map(|t| substitute_type(t, subst)).collect())
        }
        TypeRef::Enum(tags) => TypeRef::Enum(
            tags.iter()
                .map(|(k, v)| (k.clone(), substitute_type(v, subst)))
                .collect(),
        ),
        TypeRef::Record(fields) => TypeRef::Record(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), substitute_type(v, subst)))
                .collect(),
        ),
        TypeRef::Tuple(items) => {
            TypeRef::Tuple(items.iter().map(|t| substitute_type(t, subst)).collect())
        }
        TypeRef::Array(inner) => TypeRef::Array(Box::new(substitute_type(inner, subst))),
        TypeRef::Map { key, value } => TypeRef::Map {
            key: Box::new(substitute_type(key, subst)),
            value: Box::new(substitute_type(value, subst)),
        },
        TypeRef::Interface(members) => TypeRef::Interface(
            members
                .iter()
                .map(|(k, v)| (k.clone(), substitute_type(v, subst)))
                .collect(),
        ),
        TypeRef::Intersect(items) => {
            TypeRef::Intersect(items.iter().map(|t| substitute_type(t, subst)).collect())
        }
        TypeRef::FnType { args, return_type } => TypeRef::FnType {
            args: Box::new(substitute_type(args, subst)),
            return_type: Box::new(substitute_type(return_type, subst)),
        },
        TypeRef::Instantiated { base, type_args } => TypeRef::Instantiated {
            base: base.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type(t, subst))
                .collect(),
        },
        TypeRef::Newtype { name, inner } => TypeRef::Newtype {
            name: name.clone(),
            inner: Box::new(substitute_type(inner, subst)),
        },
        TypeRef::Capability { .. } => ty.clone(),
        _ => ty.clone(),
    }
}

fn normalize_type_ref(ty: &TypeRef, aliases: &HashMap<String, TypeAlias>) -> TypeRef {
    match ty {
        TypeRef::Named(name) => {
            if let Some(al) = aliases.get(name) {
                if matches!(
                    al.body,
                    TypeRef::Newtype { .. } | TypeRef::Capability { .. }
                ) {
                    return ty.clone();
                }
                if al.type_params.is_empty() {
                    return normalize_type_ref(&al.body, aliases);
                }
            }
            ty.clone()
        }
        TypeRef::Instantiated { base, type_args } => {
            let normalized_args: Vec<TypeRef> = type_args
                .iter()
                .map(|t| normalize_type_ref(t, aliases))
                .collect();
            if let Some(al) = aliases.get(base) {
                if matches!(
                    al.body,
                    TypeRef::Newtype { .. } | TypeRef::Capability { .. }
                ) {
                    return TypeRef::Instantiated {
                        base: base.clone(),
                        type_args: normalized_args,
                    };
                }
                if al.type_params.len() == normalized_args.len() {
                    let subst: HashMap<String, TypeRef> = al
                        .type_params
                        .iter()
                        .cloned()
                        .zip(normalized_args.iter().cloned())
                        .collect();
                    return normalize_type_ref(&substitute_type(&al.body, &subst), aliases);
                }
            }
            TypeRef::Instantiated {
                base: base.clone(),
                type_args: normalized_args,
            }
        }
        TypeRef::Union(items) => TypeRef::Union(
            items
                .iter()
                .map(|t| normalize_type_ref(t, aliases))
                .collect(),
        ),
        TypeRef::Enum(tags) => TypeRef::Enum(
            tags.iter()
                .map(|(k, v)| (k.clone(), normalize_type_ref(v, aliases)))
                .collect(),
        ),
        TypeRef::Record(fields) => TypeRef::Record(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), normalize_type_ref(v, aliases)))
                .collect(),
        ),
        TypeRef::Tuple(items) => TypeRef::Tuple(
            items
                .iter()
                .map(|t| normalize_type_ref(t, aliases))
                .collect(),
        ),
        TypeRef::Array(inner) => TypeRef::Array(Box::new(normalize_type_ref(inner, aliases))),
        TypeRef::Map { key, value } => TypeRef::Map {
            key: Box::new(normalize_type_ref(key, aliases)),
            value: Box::new(normalize_type_ref(value, aliases)),
        },
        TypeRef::Interface(members) => TypeRef::Interface(
            members
                .iter()
                .map(|(k, v)| (k.clone(), normalize_type_ref(v, aliases)))
                .collect(),
        ),
        TypeRef::Intersect(items) => TypeRef::Intersect(
            items
                .iter()
                .map(|t| normalize_type_ref(t, aliases))
                .collect(),
        ),
        TypeRef::FnType { args, return_type } => TypeRef::FnType {
            args: Box::new(normalize_type_ref(args, aliases)),
            return_type: Box::new(normalize_type_ref(return_type, aliases)),
        },
        TypeRef::Newtype { name, inner } => TypeRef::Newtype {
            name: name.clone(),
            inner: Box::new(normalize_type_ref(inner, aliases)),
        },
        TypeRef::Capability { .. } => ty.clone(),
        _ => ty.clone(),
    }
}

fn unify_types(
    expected: &TypeRef,
    actual: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
    bindings: &mut HashMap<String, TypeRef>,
) -> bool {
    let expected_n = normalize_type_ref(expected, aliases);
    let actual_n = normalize_type_ref(actual, aliases);

    if expected_n == actual_n {
        return true;
    }

    if let TypeRef::Generic(name) = &expected_n {
        if let Some(bound) = bindings.get(name).cloned() {
            return unify_types(&bound, &actual_n, aliases, bindings);
        }
        bindings.insert(name.clone(), actual_n.clone());
        return true;
    }
    if let TypeRef::Generic(name) = &actual_n {
        if let Some(bound) = bindings.get(name).cloned() {
            return unify_types(&expected_n, &bound, aliases, bindings);
        }
        bindings.insert(name.clone(), expected_n.clone());
        return true;
    }

    match (&expected_n, &actual_n) {
        (TypeRef::Policy(_), TypeRef::Policy(_)) => {
            policy_type_is_subset(&actual_n, &expected_n, aliases)
        }
        (TypeRef::Literal(le), TypeRef::Literal(la)) => le == la,
        (e, TypeRef::Literal(la)) => literal_fits_primitive(la, e),
        (TypeRef::Literal(_), _) => false,
        (
            TypeRef::Instantiated {
                base: b1,
                type_args: a1,
            },
            TypeRef::Instantiated {
                base: b2,
                type_args: a2,
            },
        ) => {
            if strip_module_prefix(b1) != strip_module_prefix(b2) || a1.len() != a2.len() {
                return false;
            }
            a1.iter()
                .zip(a2.iter())
                .all(|(e, a)| unify_types(e, a, aliases, bindings))
        }
        (TypeRef::Union(opts), a) => opts.iter().any(|c| unify_types(c, a, aliases, bindings)),
        (TypeRef::Record(ef), TypeRef::Record(af)) => ef.iter().all(|(name, t)| {
            af.get(name)
                .is_some_and(|a| unify_types(t, a, aliases, bindings))
        }),
        (TypeRef::Interface(em), TypeRef::Record(af)) => em.iter().all(|(name, t)| {
            af.get(name)
                .is_some_and(|a| unify_types(t, a, aliases, bindings))
        }),
        (TypeRef::Interface(em), TypeRef::Interface(am)) => em.iter().all(|(name, t)| {
            am.get(name)
                .is_some_and(|a| unify_types(t, a, aliases, bindings))
        }),
        (TypeRef::Tuple(e), TypeRef::Tuple(a)) => {
            e.len() == a.len()
                && e.iter()
                    .zip(a.iter())
                    .all(|(et, at)| unify_types(et, at, aliases, bindings))
        }
        (TypeRef::Array(et), TypeRef::Array(at)) => unify_types(et, at, aliases, bindings),
        (TypeRef::Map { key: ek, value: ev }, TypeRef::Map { key: ak, value: av }) => {
            unify_types(ek, ak, aliases, bindings) && unify_types(ev, av, aliases, bindings)
        }
        (TypeRef::Intersect(parts), a) => {
            parts.iter().all(|p| unify_types(p, a, aliases, bindings))
        }
        (
            TypeRef::FnType {
                args: ea,
                return_type: er,
            },
            TypeRef::FnType {
                args: aa,
                return_type: ar,
            },
        ) => unify_types(ea, aa, aliases, bindings) && unify_types(er, ar, aliases, bindings),
        (e, a) if is_numeric_type(e) && is_numeric_type(a) => true,
        (TypeRef::Named(e), TypeRef::Named(a))
            if strip_module_prefix(e) == strip_module_prefix(a) =>
        {
            true
        }
        (TypeRef::Enum(e), TypeRef::Enum(a)) => e == a,
        _ => false,
    }
}

fn literal_fits_primitive(lit: &LiteralType, prim: &TypeRef) -> bool {
    match (lit, prim) {
        (LiteralType::Bool(_), TypeRef::Bool) => true,
        (LiteralType::Str(_), TypeRef::Str) => true,
        (LiteralType::Int(_), p) if is_numeric_type(p) => true,
        (LiteralType::Float(_), TypeRef::Float32) | (LiteralType::Float(_), TypeRef::Float64) => {
            true
        }
        _ => false,
    }
}

fn is_numeric_type(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::Int8
            | TypeRef::Int16
            | TypeRef::Int32
            | TypeRef::Int64
            | TypeRef::UInt8
            | TypeRef::UInt16
            | TypeRef::UInt32
            | TypeRef::UInt64
            | TypeRef::Float32
            | TypeRef::Float64
    )
}

fn type_compatible(
    expected: &TypeRef,
    actual: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    if policy_body(expected, aliases).is_some() && policy_body(actual, aliases).is_some() {
        return policy_type_is_subset(expected, actual, aliases);
    }
    let mut bindings = HashMap::new();
    unify_types(expected, actual, aliases, &mut bindings)
}

fn newtype_inner<'a>(
    ty: &'a TypeRef,
    aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a TypeRef> {
    match ty {
        TypeRef::Named(name) => aliases.get(name).and_then(|alias| match &alias.body {
            TypeRef::Newtype { inner, .. } => Some(inner.as_ref()),
            _ => None,
        }),
        TypeRef::Instantiated { base, .. } => {
            aliases.get(base).and_then(|alias| match &alias.body {
                TypeRef::Newtype { inner, .. } => Some(inner.as_ref()),
                _ => None,
            })
        }
        TypeRef::Newtype { inner, .. } => Some(inner.as_ref()),
        _ => None,
    }
}

fn capability_alias<'a>(
    ty: &'a TypeRef,
    aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a str> {
    match ty {
        TypeRef::Named(name) => aliases.get(name).and_then(|alias| match &alias.body {
            TypeRef::Capability { .. } => Some(name.as_str()),
            _ => None,
        }),
        TypeRef::Capability { name, .. } if !name.is_empty() => Some(name.as_str()),
        _ => None,
    }
}

fn crosses_newtype_boundary(
    expected: &TypeRef,
    actual: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let expected_inner = newtype_inner(expected, aliases);
    let actual_inner = newtype_inner(actual, aliases);
    match (expected_inner, actual_inner) {
        (Some(inner), None) => type_compatible(inner, actual, aliases),
        (None, Some(inner)) => type_compatible(expected, inner, aliases),
        _ => false,
    }
}

fn valid_cast_path(
    source: &TypeRef,
    target: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    if capability_alias(target, aliases).is_some()
        || matches!(target, TypeRef::Capability { .. } | TypeRef::Policy(_))
    {
        return false;
    }
    if let Some(inner) = newtype_inner(target, aliases) {
        if type_compatible(inner, source, aliases) {
            return true;
        }
    }
    if let Some(inner) = newtype_inner(source, aliases) {
        if type_compatible(target, inner, aliases) {
            return true;
        }
    }
    false
}

fn policy_body<'a>(
    ty: &'a TypeRef,
    aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a PolicyType> {
    match ty {
        TypeRef::Policy(policy) => Some(policy),
        TypeRef::Named(name) => aliases.get(name).and_then(|alias| match &alias.body {
            TypeRef::Policy(policy) => Some(policy),
            _ => None,
        }),
        _ => None,
    }
}

fn policy_type_is_subset(
    source: &TypeRef,
    target: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let Some(source) = policy_body(source, aliases) else {
        return false;
    };
    let Some(target) = policy_body(target, aliases) else {
        return false;
    };
    target.domains.iter().all(|(domain, target_groups)| {
        let Some(source_groups) = source.domains.get(domain) else {
            return false;
        };
        target_groups.iter().all(|target_group| {
            source_groups
                .iter()
                .any(|source_group| policy_group_covers(source_group, target_group))
        })
    })
}

fn policy_group_covers(source: &PolicyGroup, target: &PolicyGroup) -> bool {
    source.scopes.iter().any(|s| matches!(s, PolicyScope::Any))
        || target.scopes.iter().all(|target_scope| {
            source
                .scopes
                .iter()
                .any(|source_scope| policy_scope_covers(source_scope, target_scope))
        })
}

fn policy_scope_covers(source: &PolicyScope, target: &PolicyScope) -> bool {
    match (source, target) {
        (PolicyScope::Any, _) => true,
        (PolicyScope::Dir(a), PolicyScope::Dir(b)) => path_scope_covers(a, b),
        (PolicyScope::Dir(a), PolicyScope::File(b)) => path_scope_covers(a, b),
        (PolicyScope::File(a), PolicyScope::File(b)) => a == b,
        (PolicyScope::Exact(a), PolicyScope::Exact(b)) => a == b,
        (PolicyScope::Prefix(a), PolicyScope::Exact(b)) => b.starts_with(a),
        (PolicyScope::Prefix(a), PolicyScope::Prefix(b)) => b.starts_with(a),
        _ => false,
    }
}

fn path_scope_covers(source: &str, target: &str) -> bool {
    normalize_policy_path(target).starts_with(normalize_policy_path(source))
}

fn normalize_policy_path(path: &str) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn strip_module_prefix(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn resolve_import_path(
    module_path: &std::path::Path,
    import: &str,
    entry_project: Option<&project::LoadedProject>,
) -> Result<PathBuf> {
    let raw = if import.starts_with('@') {
        let project = entry_project.with_context(|| {
            format!(
                "{}: @ import `{import}` requires a project.vibra",
                module_path.display()
            )
        })?;
        project::resolve_project_import(project, import)?
    } else {
        let parent = module_path
            .parent()
            .with_context(|| format!("{}: path has no parent directory", module_path.display()))?;
        parent.join(import)
    };
    fs::canonicalize(&raw).with_context(|| format!("cannot resolve import `{}`", raw.display()))
}

/// Build the logical module prefix for nested `$import` keys (`a` + `util` → `a.util`).
fn join_module_prefix(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}

/// Prefer `{alias}.{name}` when that key exists in `aliases`, so nested imports like
/// `$fs.read-file` inside module `io` resolve to `io.fs.read-file` (and `a.io.fs.read-file`
/// when `io` is mounted under `a`). If the prefix misses, accept `name` when it is already
/// a registered key (fully-qualified reference). Otherwise keep `name` unqualified.
fn qualify_type_name_key(
    alias: &str,
    name: String,
    aliases: &HashMap<String, TypeAlias>,
) -> String {
    if !alias.is_empty() {
        let qual = format!("{alias}.{name}");
        if aliases.contains_key(&qual) {
            return qual;
        }
    }
    if aliases.contains_key(&name) {
        return name;
    }
    name
}

fn qualify_named_type(alias: &str, ty: TypeRef, aliases: &HashMap<String, TypeAlias>) -> TypeRef {
    match ty {
        TypeRef::Named(name) => TypeRef::Named(qualify_type_name_key(alias, name, aliases)),
        TypeRef::Instantiated { base, type_args } => {
            let new_base = qualify_type_name_key(alias, base, aliases);
            TypeRef::Instantiated {
                base: new_base,
                type_args: type_args
                    .into_iter()
                    .map(|t| qualify_named_type(alias, t, aliases))
                    .collect(),
            }
        }
        TypeRef::Newtype { name, inner } => TypeRef::Newtype {
            name,
            inner: Box::new(qualify_named_type(alias, *inner, aliases)),
        },
        TypeRef::Capability { name, kind } => TypeRef::Capability { name, kind },
        TypeRef::Union(items) => TypeRef::Union(
            items
                .into_iter()
                .map(|t| qualify_named_type(alias, t, aliases))
                .collect(),
        ),
        TypeRef::Enum(tags) => TypeRef::Enum(
            tags.into_iter()
                .map(|(k, v)| (k, qualify_named_type(alias, v, aliases)))
                .collect(),
        ),
        TypeRef::Record(fields) => TypeRef::Record(
            fields
                .into_iter()
                .map(|(k, v)| (k, qualify_named_type(alias, v, aliases)))
                .collect(),
        ),
        TypeRef::Tuple(items) => TypeRef::Tuple(
            items
                .into_iter()
                .map(|t| qualify_named_type(alias, t, aliases))
                .collect(),
        ),
        TypeRef::Array(inner) => {
            TypeRef::Array(Box::new(qualify_named_type(alias, *inner, aliases)))
        }
        TypeRef::Map { key, value } => TypeRef::Map {
            key: Box::new(qualify_named_type(alias, *key, aliases)),
            value: Box::new(qualify_named_type(alias, *value, aliases)),
        },
        TypeRef::Interface(members) => TypeRef::Interface(
            members
                .into_iter()
                .map(|(k, v)| (k, qualify_named_type(alias, v, aliases)))
                .collect(),
        ),
        TypeRef::Intersect(items) => TypeRef::Intersect(
            items
                .into_iter()
                .map(|t| qualify_named_type(alias, t, aliases))
                .collect(),
        ),
        TypeRef::FnType { args, return_type } => TypeRef::FnType {
            args: Box::new(qualify_named_type(alias, *args, aliases)),
            return_type: Box::new(qualify_named_type(alias, *return_type, aliases)),
        },
        _ => ty,
    }
}

// ===== Public lowering entry point =====

pub fn lower_program(program: &LoadedProgram) -> Result<LoweredProgram> {
    let entry_map = program
        .modules
        .get(&program.entry)
        .context("internal: entry module not loaded")?
        .as_mapping()
        .context("entry root must be mapping")?;
    let entry_project = project::find_project_for_file(&program.entry)?;
    let skeletons = collect_alias_skeletons(program, entry_project.as_ref())?;

    let mut sigs: HashMap<String, FunctionSig> = HashMap::new();
    let mut constants: HashMap<String, RuntimeValue> = HashMap::new();
    let mut type_aliases: HashMap<String, TypeAlias> = HashMap::new();
    let mut enums: HashMap<String, EnumDef> = HashMap::new();
    let mut impls: HashMap<ImplKey, ImplBody> = HashMap::new();
    let mut warnings = Vec::new();
    let mut pending_user_bodies: Vec<(String, Vec<Value>)> = Vec::new();
    let mut visited_import_defs: HashSet<(String, PathBuf)> = HashSet::new();

    for (k, v) in entry_map {
        let alias = k.as_str().context("module keys must be strings")?;
        if !alias.starts_with('-') {
            maybe_warn_kebab(alias, "import alias", &mut warnings);
        }
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else {
            continue;
        };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let imported_path = resolve_import_path(&program.entry, imp_s, entry_project.as_ref())
            .with_context(|| format!("resolve import alias `{alias}`"))?;
        collect_module_defs_tree(
            alias,
            &imported_path,
            program,
            entry_project.as_ref(),
            &mut sigs,
            &mut constants,
            &mut type_aliases,
            &mut enums,
            &mut impls,
            &mut pending_user_bodies,
            &skeletons,
            &mut warnings,
            &mut visited_import_defs,
        )?;
    }

    collect_module_defs(
        "",
        program
            .modules
            .get(&program.entry)
            .context("entry not loaded")?,
        &mut sigs,
        &mut constants,
        &mut type_aliases,
        &mut enums,
        &mut impls,
        &mut pending_user_bodies,
        &skeletons,
        &mut warnings,
    )?;

    lower_pending_user_functions(
        &mut pending_user_bodies,
        &mut sigs,
        &constants,
        &type_aliases,
        &enums,
        &impls,
        &mut warnings,
    )?;

    let main = map_get_str(entry_map, "main").context("missing top-level `main`")?;
    let main_env = parse_def_envelope(main, &mut warnings)
        .context("`main` must be a `$function` definition")?;
    if main_env.form_key != "$function" {
        bail!("`main` must be a `$function`");
    }
    if !main_env.type_params.is_empty() {
        bail!("`main` must not be generic (no `where:`)");
    }
    let (args, ret, do_seq, main_grant_decls) =
        resolve_function_envelope_fields(&main_env, MODULE_FN_PRIMARY_ARG)
            .context("invalid `main`")?;
    if !main_grant_decls.is_empty() {
        bail!("function `grants` were removed; use `$policy` arguments");
    }
    let mut main_arg_bindings = Vec::new();
    if !is_void_args(&args) {
        let (arg_names, arg_types) =
            parse_signature_args(&args, &[], &skeletons, &mut warnings, false)
                .context("invalid `main` args")?;
        for (name, ty) in arg_names.into_iter().zip(arg_types.into_iter()) {
            let ty = qualify_named_type("", ty, &type_aliases);
            let ty = policy_body(&ty, &type_aliases)
                .cloned()
                .map(TypeRef::Policy)
                .unwrap_or(ty);
            seed_arg_type_bindings(
                &format!("args.{name}"),
                &ty,
                &type_aliases,
                &mut main_arg_bindings,
            );
        }
    }
    if ret.as_str() != Some("$void") {
        bail!("main must have return: $void");
    }
    let steps = do_seq.as_sequence().context("`do` must be sequence")?;
    let mut statements = Vec::new();
    let mut locals = HashMap::new();
    for (name, ty) in &main_arg_bindings {
        locals.insert(name.clone(), ty.clone());
    }
    for grant in &main_grant_decls {
        locals.insert(format!("grants.{}", grant.name), TypeRef::GrantToken);
    }
    for step in steps {
        statements.push(lower_statement(
            step,
            &sigs,
            &constants,
            &type_aliases,
            &enums,
            &impls,
            &mut locals,
            &mut warnings,
            None,
        )?);
    }

    // Phase 5c: every `=where` bound element must resolve to an interface
    // (anonymous inline `$interface` or an alias whose body is one). Phase
    // 5d: every `Instantiated` reference (in type positions and at call
    // sites) must satisfy the base alias's bounds.
    validate_all_where_bounds(&type_aliases, &sigs, &enums)?;
    validate_all_instantiation_bounds(&type_aliases, &sigs, &enums, &impls, &statements)?;

    Ok(LoweredProgram {
        statements,
        main_arg_bindings,
        main_grant_decls,
        constants,
        functions: sigs,
        impls,
        warnings,
    })
}

pub fn lower_tests(program: &LoadedProgram) -> Result<Vec<LoweredTestCase>> {
    let entry_map = program
        .modules
        .get(&program.entry)
        .context("internal: entry module not loaded")?
        .as_mapping()
        .context("entry root must be mapping")?;
    let entry_project = project::find_project_for_file(&program.entry)?;
    let skeletons = collect_alias_skeletons(program, entry_project.as_ref())?;

    let mut sigs: HashMap<String, FunctionSig> = HashMap::new();
    let mut constants: HashMap<String, RuntimeValue> = HashMap::new();
    let mut type_aliases: HashMap<String, TypeAlias> = HashMap::new();
    let mut enums: HashMap<String, EnumDef> = HashMap::new();
    let mut impls: HashMap<ImplKey, ImplBody> = HashMap::new();
    let mut warnings = Vec::new();
    let mut pending_user_bodies: Vec<(String, Vec<Value>)> = Vec::new();
    let mut visited_import_defs: HashSet<(String, PathBuf)> = HashSet::new();

    for (k, v) in entry_map {
        let alias = k.as_str().context("module keys must be strings")?;
        if !alias.starts_with('-') {
            maybe_warn_kebab(alias, "import alias", &mut warnings);
        }
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else {
            continue;
        };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let imported_path = resolve_import_path(&program.entry, imp_s, entry_project.as_ref())
            .with_context(|| format!("resolve import alias `{alias}`"))?;
        collect_module_defs_tree(
            alias,
            &imported_path,
            program,
            entry_project.as_ref(),
            &mut sigs,
            &mut constants,
            &mut type_aliases,
            &mut enums,
            &mut impls,
            &mut pending_user_bodies,
            &skeletons,
            &mut warnings,
            &mut visited_import_defs,
        )?;
    }

    collect_module_defs(
        "",
        program
            .modules
            .get(&program.entry)
            .context("entry not loaded")?,
        &mut sigs,
        &mut constants,
        &mut type_aliases,
        &mut enums,
        &mut impls,
        &mut pending_user_bodies,
        &skeletons,
        &mut warnings,
    )?;

    lower_pending_user_functions(
        &mut pending_user_bodies,
        &mut sigs,
        &constants,
        &type_aliases,
        &enums,
        &impls,
        &mut warnings,
    )?;

    let mut tests = Vec::new();
    for (k, v) in entry_map {
        let name = k.as_str().context("module keys must be strings")?;
        if name.starts_with('-') {
            continue;
        }
        let Some(_) = v.as_mapping() else { continue };
        let env = parse_def_envelope(v, &mut warnings)
            .with_context(|| format!("invalid definition `{name}`"))?;
        if env.form_key != "$test" {
            continue;
        }
        maybe_warn_kebab(name, "test name", &mut warnings);
        if !env.type_params.is_empty() {
            bail!("`$test` `{name}` must not declare `=where`");
        }
        let tm = env
            .form_value
            .as_mapping()
            .context("`$test` value must be a mapping with `do`")?;
        let steps = map_get_str(tm, "do")
            .context("missing `do` on `$test`")?
            .as_sequence()
            .context("`$test.do` must be sequence")?;
        let mut locals = HashMap::new();
        let mut statements = Vec::new();
        for step in steps {
            statements.push(lower_statement(
                step,
                &sigs,
                &constants,
                &type_aliases,
                &enums,
                &impls,
                &mut locals,
                &mut warnings,
                None,
            )?);
        }
        validate_all_where_bounds(&type_aliases, &sigs, &enums)?;
        validate_all_instantiation_bounds(&type_aliases, &sigs, &enums, &impls, &statements)?;
        tests.push(LoweredTestCase {
            name: name.to_string(),
            program: LoweredProgram {
                statements,
                main_arg_bindings: Vec::new(),
                main_grant_decls: Vec::new(),
                constants: constants.clone(),
                functions: sigs.clone(),
                impls: impls.clone(),
                warnings: warnings.clone(),
            },
        });
    }
    if tests.is_empty() {
        bail!(
            "no `$test` declarations found in {}",
            program.entry.display()
        );
    }
    Ok(tests)
}

pub fn lower_exec_expr(
    program: &LoadedProgram,
    expr_value: &Value,
    local_types: &HashMap<String, TypeRef>,
) -> Result<LoweredExec> {
    let entry_map = program
        .modules
        .get(&program.entry)
        .context("internal: entry module not loaded")?
        .as_mapping()
        .context("entry root must be mapping")?;
    let entry_project = project::find_project_for_file(&program.entry)?;
    let skeletons = collect_alias_skeletons(program, entry_project.as_ref())?;

    let mut sigs: HashMap<String, FunctionSig> = HashMap::new();
    let mut constants: HashMap<String, RuntimeValue> = HashMap::new();
    let mut type_aliases: HashMap<String, TypeAlias> = HashMap::new();
    let mut enums: HashMap<String, EnumDef> = HashMap::new();
    let mut impls: HashMap<ImplKey, ImplBody> = HashMap::new();
    let mut warnings = Vec::new();
    let mut pending_user_bodies: Vec<(String, Vec<Value>)> = Vec::new();
    let mut visited_import_defs: HashSet<(String, PathBuf)> = HashSet::new();

    for (k, v) in entry_map {
        let alias = k.as_str().context("module keys must be strings")?;
        if !alias.starts_with('-') {
            maybe_warn_kebab(alias, "import alias", &mut warnings);
        }
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else {
            continue;
        };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let imported_path = resolve_import_path(&program.entry, imp_s, entry_project.as_ref())
            .with_context(|| format!("resolve import alias `{alias}`"))?;
        collect_module_defs_tree(
            alias,
            &imported_path,
            program,
            entry_project.as_ref(),
            &mut sigs,
            &mut constants,
            &mut type_aliases,
            &mut enums,
            &mut impls,
            &mut pending_user_bodies,
            &skeletons,
            &mut warnings,
            &mut visited_import_defs,
        )?;
    }

    collect_module_defs(
        "",
        program
            .modules
            .get(&program.entry)
            .context("entry not loaded")?,
        &mut sigs,
        &mut constants,
        &mut type_aliases,
        &mut enums,
        &mut impls,
        &mut pending_user_bodies,
        &skeletons,
        &mut warnings,
    )?;

    lower_pending_user_functions(
        &mut pending_user_bodies,
        &mut sigs,
        &constants,
        &type_aliases,
        &enums,
        &impls,
        &mut warnings,
    )?;

    let locals = local_types.clone();
    let expr = parse_expr(
        expr_value,
        &sigs,
        &constants,
        &type_aliases,
        &enums,
        &impls,
        &locals,
        "",
        &mut warnings,
    )?;
    let expr_ty = infer_expr_type(&expr, &constants, &locals, &type_aliases, &enums)
        .context("could not infer type for exec expression")?;
    if expr_ty == TypeRef::Void {
        bail!("void function call cannot be used as an expression");
    }
    let statement = Statement::Eval(expr.clone());
    validate_all_where_bounds(&type_aliases, &sigs, &enums)?;
    validate_all_instantiation_bounds(&type_aliases, &sigs, &enums, &impls, &[statement])?;

    Ok(LoweredExec {
        expr,
        program: LoweredProgram {
            statements: Vec::new(),
            main_arg_bindings: Vec::new(),
            main_grant_decls: Vec::new(),
            constants,
            functions: sigs,
            impls,
            warnings,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_module_defs_tree(
    alias: &str,
    module_path: &std::path::Path,
    program: &LoadedProgram,
    entry_project: Option<&project::LoadedProject>,
    sigs: &mut HashMap<String, FunctionSig>,
    constants: &mut HashMap<String, RuntimeValue>,
    type_aliases: &mut HashMap<String, TypeAlias>,
    enums: &mut HashMap<String, EnumDef>,
    impls: &mut HashMap<ImplKey, ImplBody>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
    visited_import_defs: &mut HashSet<(String, PathBuf)>,
) -> Result<()> {
    if !visited_import_defs.insert((alias.to_string(), module_path.to_path_buf())) {
        return Ok(());
    }

    let module_root = program
        .modules
        .get(module_path)
        .with_context(|| format!("imported module missing from graph `{alias}`"))?;
    let map = module_root
        .as_mapping()
        .context("module root must be mapping")?;
    for (k, v) in map {
        let nested_alias = k.as_str().context("module key must be string")?;
        let Some(sub) = v.as_mapping() else { continue };
        let Some(imp) = map_get_str(sub, "$import") else {
            continue;
        };
        let imp_s = imp.as_str().context("$import value must be string")?;
        let nested_path = resolve_import_path(module_path, imp_s, entry_project)
            .with_context(|| format!("resolve nested import alias `{nested_alias}`"))?;
        let full_nested = join_module_prefix(alias, nested_alias);
        collect_module_defs_tree(
            &full_nested,
            &nested_path,
            program,
            entry_project,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            pending_user_bodies,
            skeletons,
            warnings,
            visited_import_defs,
        )?;
    }
    collect_module_defs(
        alias,
        module_root,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        pending_user_bodies,
        skeletons,
        warnings,
    )
}

fn substituted_return_type(sig: &FunctionSig, type_args: &[TypeRef]) -> TypeRef {
    let mut subst = HashMap::new();
    for (p, a) in sig.type_params.iter().zip(type_args.iter()) {
        subst.insert(p.clone(), a.clone());
    }
    substitute_type(&sig.return_type, &subst)
}

#[allow(clippy::too_many_arguments)]
fn lower_pending_user_functions(
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    sigs: &mut HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    for (key, steps) in pending_user_bodies.drain(..) {
        let sig = sigs
            .get(&key)
            .with_context(|| format!("internal: missing sig for pending body `{key}`"))?
            .clone();
        let FunctionBody::User { .. } = &sig.body else {
            bail!("internal: pending body for non-user function `{key}`");
        };
        let mut locals: HashMap<String, TypeRef> = HashMap::new();
        for (n, t) in sig.arg_names.iter().zip(sig.arg_types.iter()) {
            locals.insert(format!("args.{n}"), t.clone());
        }
        for grant in &sig.grant_decls {
            locals.insert(format!("grants.{}", grant.name), TypeRef::GrantToken);
        }
        let fn_ctx = UserFnContext {
            return_type: sig.return_type.clone(),
            home_module: sig.alias.clone(),
        };
        let mut statements = Vec::with_capacity(steps.len());
        for step in &steps {
            statements.push(lower_statement(
                step,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                &mut locals,
                warnings,
                Some(&fn_ctx),
            )?);
        }
        validate_user_function_body(&statements, &sig.return_type)
            .with_context(|| format!("user function `{key}`"))?;
        let fs = sigs
            .get_mut(&key)
            .with_context(|| format!("internal: sig disappeared for `{key}`"))?;
        fs.body = FunctionBody::User { statements };
    }
    Ok(())
}

fn validate_user_function_body(stmts: &[Statement], return_type: &TypeRef) -> Result<()> {
    if *return_type == TypeRef::Void {
        return Ok(());
    }
    if !user_body_terminates(stmts) {
        bail!(
            "non-void function must end with `$return`, or with `$match` whose every arm ends with `$return`"
        );
    }
    Ok(())
}

fn user_body_terminates(stmts: &[Statement]) -> bool {
    if stmts.is_empty() {
        return false;
    }
    match stmts.last().expect("non-empty") {
        Statement::Return(_) => true,
        Statement::Match { arms, .. } => arms.iter().all(|a| user_body_terminates(&a.body)),
        Statement::If {
            then_body,
            else_body,
            ..
        } => user_body_terminates(then_body) && user_body_terminates(else_body),
        Statement::Eval(_)
        | Statement::Call(_)
        | Statement::Let { .. }
        | Statement::While { .. } => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn instantiated_type_for_constructor(
    enum_key: &str,
    enum_def: &EnumDef,
    tag: &str,
    payload_expr: Option<&Expr>,
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    let payload_ty = enum_def.tags.get(tag)?;
    let mut bindings: HashMap<String, TypeRef> = HashMap::new();
    match (payload_expr, payload_ty) {
        (None, t) if *t == TypeRef::Void => {}
        (Some(pl), t) if *t != TypeRef::Void => {
            let actual = infer_expr_type(pl, constants, locals, aliases, enums)?;
            if !unify_types(t, &actual, aliases, &mut bindings) {
                return None;
            }
        }
        _ => return None,
    }
    let type_args: Vec<TypeRef> = enum_def
        .type_params
        .iter()
        .map(|p| {
            bindings
                .get(p)
                .cloned()
                .unwrap_or_else(|| TypeRef::Generic(p.clone()))
        })
        .collect();
    Some(TypeRef::Instantiated {
        base: enum_key.to_string(),
        type_args,
    })
}

fn seed_arg_type_bindings(
    name: &str,
    ty: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
    out: &mut Vec<(String, TypeRef)>,
) {
    out.push((name.to_string(), ty.clone()));
    if let Some(fields) = record_fields_for_type(ty, aliases) {
        for (field, field_ty) in fields {
            seed_arg_type_bindings(&format!("{name}.{field}"), &field_ty, aliases, out);
        }
    }
}

fn record_fields_for_type(
    ty: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> Option<BTreeMap<String, TypeRef>> {
    match ty {
        TypeRef::Record(fields) => Some(fields.clone()),
        TypeRef::Named(name) => aliases.get(name).and_then(|alias| match &alias.body {
            TypeRef::Record(fields) => Some(fields.clone()),
            _ => None,
        }),
        TypeRef::Instantiated { base, type_args } => aliases.get(base).and_then(|alias| {
            let TypeRef::Record(fields) = &alias.body else {
                return None;
            };
            let subst: HashMap<String, TypeRef> = alias
                .type_params
                .iter()
                .cloned()
                .zip(type_args.iter().cloned())
                .collect();
            Some(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), substitute_type(v, &subst)))
                    .collect(),
            )
        }),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_module_defs(
    alias: &str,
    module_root: &Value,
    sigs: &mut HashMap<String, FunctionSig>,
    constants: &mut HashMap<String, RuntimeValue>,
    type_aliases: &mut HashMap<String, TypeAlias>,
    enums: &mut HashMap<String, EnumDef>,
    impls: &mut HashMap<ImplKey, ImplBody>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let map = module_root
        .as_mapping()
        .context("module root must be mapping")?;

    for (k, v) in map {
        let name = k.as_str().context("module key must be string")?;
        if !name.starts_with('-') {
            maybe_warn_kebab(name, "top-level symbol", warnings);
        }
        let Some(_) = v.as_mapping() else { continue };
        if let Some(def_map) = v.as_mapping() {
            if map_get_str(def_map, "$import").is_some() {
                continue;
            }
        }
        let env = parse_def_envelope(v, warnings).with_context(|| {
            if alias.is_empty() {
                format!("invalid definition `{name}`")
            } else {
                format!("invalid definition `{alias}.{name}`")
            }
        })?;
        if env.form_key == "$function" || env.form_key == "$test" || env.form_key == "$import" {
            continue;
        }
        if !BUILTIN_TYPE_FORMS.contains(&env.form_key.as_str()) {
            let display_key = if alias.is_empty() {
                name.to_string()
            } else {
                format!("{alias}.{name}")
            };
            bail!("unknown form `{}` on `{display_key}`", env.form_key);
        }
        let scope: Vec<String> = env.type_params.clone();
        // Top-level type definitions don't put `$self` in scope by themselves;
        // the only place `$self` can appear here is inside an `$interface`
        // body, which the constructor handler enables explicitly.
        let body = parse_type_constructor(
            &env.form_key,
            env.form_value,
            &scope,
            skeletons,
            warnings,
            false,
        )
        .with_context(|| {
            if alias.is_empty() {
                format!("invalid type definition `{name}`")
            } else {
                format!("invalid type definition `{alias}.{name}`")
            }
        })?;
        let qualified_key = if alias.is_empty() {
            name.to_string()
        } else {
            format!("{alias}.{name}")
        };
        let body = match qualify_named_type(alias, body, type_aliases) {
            TypeRef::Newtype { inner, .. } => TypeRef::Newtype {
                name: qualified_key.clone(),
                inner,
            },
            TypeRef::Capability { kind, .. } => TypeRef::Capability {
                name: qualified_key.clone(),
                kind,
            },
            other => other,
        };
        let raw_bounds = resolve_def_envelope_bounds(&env, skeletons, warnings)?;
        let resolved_bounds = qualify_bounds(alias, raw_bounds, type_aliases);
        let alias_def = TypeAlias {
            alias: alias.to_string(),
            name: name.to_string(),
            type_params: env.type_params.clone(),
            type_param_bounds: resolved_bounds.clone(),
            body: body.clone(),
            doc: env.doc.clone(),
        };
        if type_aliases.contains_key(&qualified_key) {
            bail!("duplicate type definition `{qualified_key}`");
        }
        type_aliases.insert(qualified_key.clone(), alias_def);
        if env.form_key == "$enum" {
            let TypeRef::Enum(tags) = body else {
                bail!("internal: $enum body did not produce TypeRef::Enum");
            };
            if enums.contains_key(&qualified_key) {
                bail!("duplicate enum `{qualified_key}`");
            }
            enums.insert(
                qualified_key,
                EnumDef {
                    alias: alias.to_string(),
                    name: name.to_string(),
                    type_params: env.type_params,
                    type_param_bounds: resolved_bounds,
                    tags,
                },
            );
        }
    }

    // Pass 1.5: register inherent functions from any `=defs` annotations now
    // that every type alias in the module is in `type_aliases`. We re-run the
    // envelope parser here to fish out the `=defs` map; this is cheap and
    // keeps the first pass purely structural.
    for (k, v) in map {
        let name = k.as_str().context("module key must be string")?;
        let Some(def_map) = v.as_mapping() else {
            continue;
        };
        if map_get_str(def_map, "$import").is_some() {
            continue;
        }
        let env = parse_def_envelope(v, warnings).with_context(|| {
            if alias.is_empty() {
                format!("invalid definition `{name}`")
            } else {
                format!("invalid definition `{alias}.{name}`")
            }
        })?;
        if env.form_key == "$function" || env.form_key == "$test" || env.form_key == "$import" {
            continue;
        }
        let Some(defs_map) = env.defs else { continue };
        let qualified_key = if alias.is_empty() {
            name.to_string()
        } else {
            format!("{alias}.{name}")
        };
        register_inherent_functions(
            alias,
            &qualified_key,
            &env.type_params,
            defs_map,
            sigs,
            pending_user_bodies,
            type_aliases,
            skeletons,
            warnings,
        )
        .with_context(|| format!("invalid `=defs` block on `{qualified_key}`"))?;
    }

    // Pass 1.6: register interface implementations from any `=impl`
    // annotations. Runs after Pass 1.5 so impl method bindings can refer to
    // already-registered inherent ops via `$ref` strings.
    for (k, v) in map {
        let name = k.as_str().context("module key must be string")?;
        let Some(def_map) = v.as_mapping() else {
            continue;
        };
        if map_get_str(def_map, "$import").is_some() {
            continue;
        }
        let env = parse_def_envelope(v, warnings).with_context(|| {
            if alias.is_empty() {
                format!("invalid definition `{name}`")
            } else {
                format!("invalid definition `{alias}.{name}`")
            }
        })?;
        if env.form_key == "$function" || env.form_key == "$test" || env.form_key == "$import" {
            continue;
        }
        let Some(impls_map) = env.impls else { continue };
        let qualified_key = if alias.is_empty() {
            name.to_string()
        } else {
            format!("{alias}.{name}")
        };
        register_impls_block(
            alias,
            &qualified_key,
            &env.type_params,
            impls_map,
            sigs,
            impls,
            pending_user_bodies,
            type_aliases,
            skeletons,
            warnings,
        )
        .with_context(|| format!("invalid `=impl` block on `{qualified_key}`"))?;
    }

    for (k, v) in map {
        let name = k.as_str().context("module key must be string")?;
        let qualified_key = if alias.is_empty() {
            name.to_string()
        } else {
            format!("{alias}.{name}")
        };
        if let Some(b) = v.as_bool() {
            if constants.contains_key(&qualified_key) {
                bail!("duplicate constant `{qualified_key}`");
            }
            constants.insert(qualified_key, RuntimeValue::Bool(b));
            continue;
        }
        if let Some(i) = v.as_i64() {
            if constants.contains_key(&qualified_key) {
                bail!("duplicate constant `{qualified_key}`");
            }
            constants.insert(qualified_key, RuntimeValue::Int(i));
            continue;
        }
        if let Some(f) = v.as_f64() {
            if constants.contains_key(&qualified_key) {
                bail!("duplicate constant `{qualified_key}`");
            }
            constants.insert(qualified_key, RuntimeValue::Float(f));
            continue;
        }
        if let Some(s) = v.as_str() {
            if constants.contains_key(&qualified_key) {
                bail!("duplicate constant `{qualified_key}`");
            }
            constants.insert(qualified_key, RuntimeValue::Str(s.to_string()));
            continue;
        }
        let Some(sub) = v.as_mapping() else { continue };
        if map_get_str(sub, "$import").is_some() {
            continue;
        }
        if alias.is_empty() && name == "main" {
            continue;
        }
        try_register_function(
            alias,
            name,
            v,
            sigs,
            pending_user_bodies,
            type_aliases,
            skeletons,
            warnings,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn try_register_function(
    alias: &str,
    name: &str,
    v: &Value,
    sigs: &mut HashMap<String, FunctionSig>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    type_aliases: &HashMap<String, TypeAlias>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let env = parse_def_envelope(v, warnings).with_context(|| {
        if alias.is_empty() {
            format!("invalid definition `{name}`")
        } else {
            format!("invalid definition `{alias}.{name}`")
        }
    })?;
    if env.form_key != "$function" {
        return Ok(());
    }
    if !name.starts_with('-') {
        maybe_warn_kebab(name, "function name", warnings);
    }
    let scope = env.type_params.clone();
    let (args, ret, do_seq, grant_decls) =
        resolve_function_envelope_fields(&env, MODULE_FN_PRIMARY_ARG).with_context(|| {
            if alias.is_empty() {
                format!("{name}: invalid `$function` envelope")
            } else {
                format!("{alias}.{name}: invalid `$function` envelope")
            }
        })?;
    // Free-standing module-level functions cannot reference `$self`; that
    // privilege belongs to functions declared inside `=defs` / `=impl` (see
    // Phases 3/4). Pass `false` here.
    let (arg_names, arg_types) = parse_signature_args(&args, &scope, skeletons, warnings, false)
        .with_context(|| {
            if alias.is_empty() {
                format!("{name}: invalid function args")
            } else {
                format!("{alias}.{name}: invalid function args")
            }
        })?;
    let arg_types = arg_types
        .into_iter()
        .map(|t| qualify_named_type(alias, t, type_aliases))
        .collect::<Vec<_>>();
    let return_type = qualify_named_type(
        alias,
        parse_type_ref(&ret, &scope, skeletons, warnings, false).with_context(|| {
            if alias.is_empty() {
                format!("{name}: invalid function return type")
            } else {
                format!("{alias}.{name}: invalid function return type")
            }
        })?,
        type_aliases,
    );
    let steps = do_seq
        .as_sequence()
        .context("function do must be sequence")?
        .to_vec();
    if steps.is_empty() {
        if alias.is_empty() {
            bail!("{name}: function do must be a non-empty sequence of statements");
        }
        bail!("{alias}.{name}: function do must be a non-empty sequence of statements");
    }
    let is_wasm_only = is_wasm_only_body(&steps);
    let sig_key = if alias.is_empty() {
        name.to_string()
    } else {
        format!("{alias}.{name}")
    };
    let body_kind = if is_wasm_only {
        let (import, wasm_args) = extract_wasm_body(&steps[0])?;
        FunctionBody::Wasm { import, wasm_args }
    } else {
        FunctionBody::User {
            statements: Vec::new(),
        }
    };
    let raw_bounds = resolve_def_envelope_bounds(&env, skeletons, warnings)?;
    let resolved_bounds = qualify_bounds(alias, raw_bounds, type_aliases);
    if sigs.contains_key(&sig_key) {
        bail!("duplicate function `{sig_key}`");
    }
    sigs.insert(
        sig_key.clone(),
        FunctionSig {
            alias: alias.to_string(),
            symbol: name.to_string(),
            type_params: env.type_params.clone(),
            type_param_bounds: resolved_bounds,
            arg_names,
            arg_types,
            grant_decls,
            return_type,
            body: body_kind,
            doc: env.doc.clone(),
        },
    );
    if !is_wasm_only {
        pending_user_bodies.push((sig_key, steps));
    }
    Ok(())
}

fn is_wasm_only_body(steps: &[Value]) -> bool {
    steps.len() == 1
        && steps[0]
            .as_mapping()
            .and_then(|m| map_get_str(m, "$wasm"))
            .is_some()
}

/// Extract `(ImportTarget, Vec<WasmArgSpec>)` from a single `$wasm`-only
/// `do:` step. Caller already ensured `is_wasm_only_body`.
fn extract_wasm_body(step: &Value) -> Result<(ImportTarget, Vec<WasmArgSpec>)> {
    let stmt = step
        .as_mapping()
        .context("function statement must be mapping")?;
    let wasm = map_get_str(stmt, "$wasm").context("function do must contain $wasm")?;
    let wm = wasm.as_mapping().context("$wasm body must be mapping")?;
    let import = map_get_str(wm, "import").context("$wasm missing import")?;
    let im = import
        .as_mapping()
        .context("$wasm.import must be mapping")?;
    let module = map_get_str(im, "module")
        .context("$wasm.import missing module")?
        .as_str()
        .context("$wasm.import.module must be string")?
        .to_string();
    let import_name = map_get_str(im, "name")
        .context("$wasm.import missing name")?
        .as_str()
        .context("$wasm.import.name must be string")?
        .to_string();
    let wasm_args_v = map_get_str(wm, "args").context("$wasm missing args")?;
    let wasm_args_seq = wasm_args_v
        .as_sequence()
        .context("$wasm.args must be sequence")?;
    let mut wasm_args = Vec::new();
    for a in wasm_args_seq {
        wasm_args.push(parse_wasm_arg_spec(a)?);
    }
    Ok((
        ImportTarget {
            module,
            name: import_name,
        },
        wasm_args,
    ))
}

#[allow(clippy::too_many_arguments)]
fn register_inherent_functions(
    module_alias: &str,
    qualified_type_key: &str,
    enclosing_type_params: &[String],
    defs_map: &serde_yaml::Mapping,
    sigs: &mut HashMap<String, FunctionSig>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    type_aliases: &HashMap<String, TypeAlias>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    // Build the `$self` substitution target. For non-generic types it is the
    // bare named type; for generic types, it is the alias instantiated by its
    // own type parameters (so `$self` inside an inherent op carries them).
    let self_ty = if enclosing_type_params.is_empty() {
        TypeRef::Named(qualified_type_key.to_string())
    } else {
        TypeRef::Instantiated {
            base: qualified_type_key.to_string(),
            type_args: enclosing_type_params
                .iter()
                .map(|p| TypeRef::Generic(p.clone()))
                .collect(),
        }
    };

    for (k, v) in defs_map {
        let entry_name = k.as_str().context("`=defs` key must be a string")?;
        maybe_warn_kebab(entry_name, "inherent op name", warnings);
        register_one_inherent_function(
            module_alias,
            qualified_type_key,
            entry_name,
            v,
            &self_ty,
            enclosing_type_params,
            sigs,
            pending_user_bodies,
            type_aliases,
            skeletons,
            warnings,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn register_one_inherent_function(
    module_alias: &str,
    qualified_type_key: &str,
    entry_name: &str,
    v: &Value,
    self_ty: &TypeRef,
    enclosing_type_params: &[String],
    sigs: &mut HashMap<String, FunctionSig>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    type_aliases: &HashMap<String, TypeAlias>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let env = parse_def_envelope(v, warnings)
        .with_context(|| format!("invalid inherent op `{qualified_type_key}.{entry_name}`"))?;
    if env.form_key != "$function" {
        bail!(
            "E-DEFS-001: inherent op `{qualified_type_key}.{entry_name}` must be a `$function`, got `{}`",
            env.form_key
        );
    }
    if env.defs.is_some() {
        bail!(
            "E-DEFS-001: inherent op `{qualified_type_key}.{entry_name}` cannot itself carry `=defs`"
        );
    }

    // Combined scope: enclosing type's params + this function's own `=where`.
    let mut all_type_params: Vec<String> = enclosing_type_params.to_vec();
    for tp in &env.type_params {
        if all_type_params.contains(tp) {
            bail!(
                "inherent op `{qualified_type_key}.{entry_name}` redeclares type parameter `{tp}` already in scope from the enclosing type"
            );
        }
        all_type_params.push(tp.clone());
    }

    let (args, ret, do_seq, grant_decls) =
        resolve_function_envelope_fields(&env, INHERENT_FN_PRIMARY_ARG).with_context(|| {
            format!("`{qualified_type_key}.{entry_name}`: invalid `$function` envelope")
        })?;
    let (arg_names, arg_types) =
        parse_signature_args(&args, &all_type_params, skeletons, warnings, true)
            .with_context(|| format!("{qualified_type_key}.{entry_name}: invalid function args"))?;
    let arg_types: Vec<TypeRef> = arg_types
        .into_iter()
        .map(|t| qualify_named_type(module_alias, t, type_aliases))
        .map(|t| substitute_self(&t, self_ty))
        .collect();
    let return_type = parse_type_ref(&ret, &all_type_params, skeletons, warnings, true)
        .with_context(|| {
            format!("{qualified_type_key}.{entry_name}: invalid function return type")
        })?;
    let return_type = qualify_named_type(module_alias, return_type, type_aliases);
    let return_type = substitute_self(&return_type, self_ty);
    let steps = do_seq
        .as_sequence()
        .context("function do must be sequence")?
        .to_vec();
    if steps.is_empty() {
        bail!(
            "{qualified_type_key}.{entry_name}: function do must be a non-empty sequence of statements"
        );
    }
    let is_wasm_only = is_wasm_only_body(&steps);

    // Sig key uses `qualified_type_key.entry_name` so call sites like
    // `$m.result.foo` resolve via `parse_qualified_call`'s first-dot split
    // (`("m", "result.foo")` -> sig key `"m.result.foo"`). We keep
    // `sig.alias` set to the original module alias so type-arg qualification
    // at call sites still walks the module's type-alias namespace.
    let sig_alias = module_alias.to_string();
    let bare_type_name = strip_module_prefix(qualified_type_key);
    let sig_symbol = format!("{}.{}", bare_type_name, entry_name);
    let sig_key = if sig_alias.is_empty() {
        sig_symbol.clone()
    } else {
        format!("{sig_alias}.{sig_symbol}")
    };

    if sigs.contains_key(&sig_key) {
        bail!("E-DEFS-001: inherent op `{sig_key}` collides with an existing function name");
    }

    let body_kind = if is_wasm_only {
        let (import, wasm_args) = extract_wasm_body(&steps[0])?;
        FunctionBody::Wasm { import, wasm_args }
    } else {
        FunctionBody::User {
            statements: Vec::new(),
        }
    };
    let raw_local_bounds = resolve_def_envelope_bounds(&env, skeletons, warnings)?;
    let local_bounds = qualify_bounds(module_alias, raw_local_bounds, type_aliases);
    // For inherent ops, the *enclosing* type's bounds also apply to its
    // type-params. Look those up from the enclosing alias (already
    // registered in `type_aliases`) and prepend.
    let enclosing_bounds: Vec<Vec<TypeRef>> = type_aliases
        .get(qualified_type_key)
        .map(|ta| ta.type_param_bounds.clone())
        .unwrap_or_default();
    let mut full_bounds = enclosing_bounds;
    full_bounds.extend(local_bounds);
    sigs.insert(
        sig_key.clone(),
        FunctionSig {
            alias: sig_alias,
            symbol: sig_symbol,
            type_params: all_type_params,
            type_param_bounds: full_bounds,
            arg_names,
            arg_types,
            grant_decls,
            return_type,
            body: body_kind,
            doc: env.doc.clone(),
        },
    );
    if !is_wasm_only {
        pending_user_bodies.push((sig_key, steps));
    }
    Ok(())
}

// ===== Phase 4: `=impl` (interface implementations) =====

/// Resolve an interface alias key like `"$display"` or `"$io.display"` to a
/// fully qualified key (`"display"` or `"io.display"`) and look it up in
/// `type_aliases`. Returns `(qualified_key, &TypeAlias)`.
fn resolve_iface_alias<'a>(
    iface_alias_str: &str,
    module_alias: &str,
    type_aliases: &'a HashMap<String, TypeAlias>,
) -> Result<(String, &'a TypeAlias)> {
    let stripped = iface_alias_str.strip_prefix('$').with_context(|| {
        format!("E-IMPL-002: interface key `{iface_alias_str}` must start with `$`")
    })?;
    // Try `{module_alias}.{stripped}` first even when `stripped` contains `.` so
    // `$fs.writable` inside module `io` resolves to `io.fs.writable`, not a stray global `fs.writable`.
    let mut candidates: Vec<String> = Vec::new();
    if !module_alias.is_empty() {
        candidates.push(format!("{module_alias}.{stripped}"));
    }
    candidates.push(stripped.to_string());
    for cand in candidates.iter().filter(|s| !s.is_empty()) {
        if let Some(ta) = type_aliases.get(cand) {
            return Ok((cand.clone(), ta));
        }
    }
    bail!(
        "E-IMPL-002: unknown interface alias `{iface_alias_str}`; expected a registered `$interface` type alias"
    );
}

#[allow(clippy::too_many_arguments)]
fn register_impls_block(
    module_alias: &str,
    qualified_type_key: &str,
    enclosing_type_params: &[String],
    impls_map: &serde_yaml::Mapping,
    sigs: &mut HashMap<String, FunctionSig>,
    impls: &mut HashMap<ImplKey, ImplBody>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    type_aliases: &HashMap<String, TypeAlias>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let self_ty = if enclosing_type_params.is_empty() {
        TypeRef::Named(qualified_type_key.to_string())
    } else {
        TypeRef::Instantiated {
            base: qualified_type_key.to_string(),
            type_args: enclosing_type_params
                .iter()
                .map(|p| TypeRef::Generic(p.clone()))
                .collect(),
        }
    };

    for (k, v) in impls_map {
        let iface_alias_str = k
            .as_str()
            .context("E-IMPL-002: `=impl` keys must be string interface aliases like `$display`")?;
        let (iface_qualified, iface_def) =
            resolve_iface_alias(iface_alias_str, module_alias, type_aliases)?;
        let TypeRef::Interface(iface_methods) = &iface_def.body else {
            bail!(
                "E-IMPL-002: `{iface_alias_str}` resolves to type `{iface_qualified}` but it is not an `$interface`"
            );
        };

        let payload = v.as_mapping().with_context(|| {
            format!("E-IMPL-001: `=impl: {iface_alias_str}` value must be a mapping")
        })?;

        register_one_impl(
            module_alias,
            qualified_type_key,
            enclosing_type_params,
            &self_ty,
            &iface_qualified,
            iface_def,
            iface_methods,
            iface_alias_str,
            payload,
            sigs,
            impls,
            pending_user_bodies,
            type_aliases,
            skeletons,
            warnings,
        )
        .with_context(|| {
            format!("invalid impl of `{iface_qualified}` for `{qualified_type_key}`")
        })?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn register_one_impl(
    module_alias: &str,
    qualified_type_key: &str,
    enclosing_type_params: &[String],
    self_ty: &TypeRef,
    iface_qualified: &str,
    iface_def: &TypeAlias,
    iface_methods: &BTreeMap<String, TypeRef>,
    iface_alias_str: &str,
    payload: &serde_yaml::Mapping,
    sigs: &mut HashMap<String, FunctionSig>,
    impls: &mut HashMap<ImplKey, ImplBody>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    type_aliases: &HashMap<String, TypeAlias>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    // 1. First pass over payload: extract impl-local `=where`. Methods that
    // reference impl-local generics need them in scope, so we lift the
    // annotation out before processing the rest of the payload. Bound values
    // (if any) are stashed on the side and resolved later during the
    // bound-validation sweep.
    let mut impl_local_params: Vec<String> = Vec::new();
    let mut impl_local_bound_values: Vec<Vec<&Value>> = Vec::new();
    if let Some(where_v) = payload.get(Value::String("=where".to_string())) {
        let wm = where_v
            .as_mapping()
            .context("`=where` must be a mapping of type-parameter name to bound list")?;
        for (wk, wv) in wm {
            let name = wk.as_str().context("`=where` keys must be strings")?;
            maybe_warn_kebab(name, "type parameter", warnings);
            let bounds = wv.as_sequence().with_context(|| {
                format!("`=where` value for `{name}` must be an array of bounds (use `[]` for unbounded)")
            })?;
            if impl_local_params.iter().any(|n| n == name)
                || enclosing_type_params.iter().any(|n| n == name)
            {
                bail!("`=where` declares duplicate type parameter `{name}` in impl scope");
            }
            impl_local_params.push(name.to_string());
            impl_local_bound_values.push(bounds.iter().collect());
        }
    }
    // Currently impl-local bounds are accepted by the parser but not yet
    // resolved or enforced. Phase 5d's bound-checks for generic call sites
    // and instantiations will pick them up; until then they are inert.
    let _ = &impl_local_bound_values;

    // 2. Combined scope: enclosing type's params + impl-local params. Method
    // bodies later add their own `$function`-level `=where` on top of this.
    let mut all_type_params: Vec<String> = enclosing_type_params.to_vec();
    all_type_params.extend(impl_local_params.iter().cloned());

    // 3. Parse iface type-arg bindings (one entry per iface `=where` param).
    //    Build the substitution map used for both signature comparison and
    //    method-body type rewriting.
    let mut iface_subst: HashMap<String, TypeRef> = HashMap::new();
    let mut iface_args_in_order: Vec<TypeRef> = Vec::with_capacity(iface_def.type_params.len());
    for iface_param in &iface_def.type_params {
        let v = payload
            .get(Value::String(iface_param.clone()))
            .with_context(|| {
                format!(
                    "E-IMPL-003: missing binding for interface type parameter `{iface_param}` in `=impl: {iface_alias_str}`"
                )
            })?;
        let ty =
            parse_type_ref(v, &all_type_params, skeletons, warnings, false).with_context(|| {
                format!("invalid binding for `{iface_param}` in `=impl: {iface_alias_str}`")
            })?;
        let ty = qualify_named_type(module_alias, ty, type_aliases);
        iface_subst.insert(iface_param.clone(), ty.clone());
        iface_args_in_order.push(ty);
    }

    // 4. Validate that every payload key is recognised: either `=where`, an
    //    iface type-param, or an iface method name.
    let iface_param_set: std::collections::HashSet<&str> =
        iface_def.type_params.iter().map(|s| s.as_str()).collect();
    let iface_method_set: std::collections::HashSet<&str> =
        iface_methods.keys().map(|s| s.as_str()).collect();
    for (k, _) in payload {
        let ks = k.as_str().context("payload key must be string")?;
        if ks == "=where" {
            continue;
        }
        if iface_param_set.contains(ks) {
            continue;
        }
        if iface_method_set.contains(ks) {
            continue;
        }
        bail!(
            "E-IMPL-004: unexpected key `{ks}` in `=impl: {iface_alias_str}`; expected one of: iface type-args ({}) or iface methods ({})",
            iface_def
                .type_params
                .join(", "),
            iface_methods
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // 5. Bind each interface method.
    let mut methods: HashMap<String, ImplMethodBinding> = HashMap::new();
    for (method_name, expected_fn_type) in iface_methods {
        let v = payload
            .get(Value::String(method_name.clone()))
            .with_context(|| {
                format!(
                    "E-IMPL-003: `=impl: {iface_alias_str}` for `{qualified_type_key}` is missing method `{method_name}`"
                )
            })?;

        // Substitute iface type-args and `$self` into the expected fn-type.
        let expected = substitute_self(&substitute_type(expected_fn_type, &iface_subst), self_ty);

        let binding = bind_impl_method(
            module_alias,
            qualified_type_key,
            iface_qualified,
            method_name,
            v,
            &expected,
            self_ty,
            &iface_subst,
            &iface_def.type_params,
            &all_type_params,
            sigs,
            pending_user_bodies,
            type_aliases,
            skeletons,
            warnings,
        )
        .with_context(|| {
            format!(
                "binding method `{method_name}` of `{iface_qualified}` for `{qualified_type_key}`"
            )
        })?;
        methods.insert(method_name.clone(), binding);
    }

    // 6. Insert into the impls table; reject duplicates.
    let key = ImplKey {
        implementing_type: qualified_type_key.to_string(),
        interface: iface_qualified.to_string(),
    };
    if impls.contains_key(&key) {
        bail!("duplicate `=impl` of `{iface_qualified}` for `{qualified_type_key}`");
    }
    impls.insert(
        key,
        ImplBody {
            methods,
            interface_args: iface_args_in_order,
            impl_type_params: impl_local_params,
        },
    );

    Ok(())
}

/// Bind a single interface method to either a fresh `$function` envelope or
/// a `$qualified.name` reference to an existing function.
#[allow(clippy::too_many_arguments)]
fn bind_impl_method(
    module_alias: &str,
    qualified_type_key: &str,
    iface_qualified: &str,
    method_name: &str,
    v: &Value,
    expected_fn_type: &TypeRef,
    self_ty: &TypeRef,
    iface_subst: &HashMap<String, TypeRef>,
    iface_type_params: &[String],
    impl_scope: &[String],
    sigs: &mut HashMap<String, FunctionSig>,
    pending_user_bodies: &mut Vec<(String, Vec<Value>)>,
    type_aliases: &HashMap<String, TypeAlias>,
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
) -> Result<ImplMethodBinding> {
    if let Some(s) = v.as_str() {
        // String reference: `$alias.symbol[.subsym]`.
        let stripped = s
            .strip_prefix('$')
            .context("impl method ref must start with `$`")?;
        // Walk the registry candidates: module-qualified path first, then bare path.
        let mut candidates: Vec<String> = Vec::new();
        if !module_alias.is_empty() {
            candidates.push(format!("{module_alias}.{stripped}"));
        }
        candidates.push(stripped.to_string());
        let mut found: Option<&FunctionSig> = None;
        let mut found_key: Option<String> = None;
        for cand in candidates.iter().filter(|s| !s.is_empty()) {
            if let Some(sig) = sigs.get(cand) {
                found = Some(sig);
                found_key = Some(cand.clone());
                break;
            }
        }
        let sig = found.with_context(|| {
            format!("E-IMPL-006: impl method `{method_name}` references unknown function `{s}`")
        })?;
        let sig_key = found_key.expect("set together with found");

        // Build the actual fn-type from the referenced sig.
        let actual = sig_function_type(sig);
        if !signatures_match(expected_fn_type, &actual, type_aliases) {
            bail!(
                "E-IMPL-005: signature of `{s}` does not match interface method `{iface_qualified}.{method_name}`; expected {:?}, got {:?}",
                expected_fn_type, actual
            );
        }
        return Ok(ImplMethodBinding::Ref(sig_key));
    }

    // Fresh `$function` envelope path.
    let env = parse_def_envelope(v, warnings)
        .with_context(|| format!("invalid `$function` envelope for impl method `{method_name}`"))?;
    if env.form_key != "$function" {
        bail!(
            "E-IMPL-001: impl method `{method_name}` must be a `$function` envelope or a `$ref` string, got `{}`",
            env.form_key
        );
    }
    if env.defs.is_some() || env.impls.is_some() {
        bail!("E-IMPL-001: impl method `{method_name}` cannot itself carry `=defs` or `=impl`");
    }

    // The *registered* sig type_params are only those actually free in the
    // method after `iface_subst` has been applied: enclosing + impl-local +
    // method's own `=where`. Iface type-params are synthetic during parsing
    // (they get substituted away before the sig is stored).
    let mut sig_type_params: Vec<String> = impl_scope.to_vec();
    for tp in &env.type_params {
        if sig_type_params.contains(tp) {
            bail!("impl method `{method_name}` redeclares type parameter `{tp}` already in scope");
        }
        sig_type_params.push(tp.clone());
    }
    // The *parsing* scope adds the iface's type-param names so `$t` inside
    // the method body parses as `Generic("t")` and the `iface_subst` rewrite
    // below replaces it with the bound type. Collisions across layers are
    // rejected.
    let mut method_scope: Vec<String> = sig_type_params.clone();
    for tp in iface_type_params {
        if method_scope.contains(tp) {
            bail!(
                "impl method `{method_name}` interface type-parameter `{tp}` collides with an enclosing or impl-local type parameter; rename one of them"
            );
        }
        method_scope.push(tp.clone());
    }

    let iface_primary = iface_impl_primary_field_name(expected_fn_type)?;
    let (args, ret, do_seq, grant_decls) = resolve_function_envelope_fields(&env, &iface_primary)
        .context("impl method: invalid `$function` envelope")?;
    let (arg_names, arg_types) =
        parse_signature_args(&args, &method_scope, skeletons, warnings, true)?;
    let arg_types: Vec<TypeRef> = arg_types
        .into_iter()
        .map(|t| qualify_named_type(module_alias, t, type_aliases))
        .map(|t| substitute_type(&t, iface_subst))
        .map(|t| substitute_self(&t, self_ty))
        .collect();
    let return_type = parse_type_ref(&ret, &method_scope, skeletons, warnings, true)?;
    let return_type = qualify_named_type(module_alias, return_type, type_aliases);
    let return_type = substitute_type(&return_type, iface_subst);
    let return_type = substitute_self(&return_type, self_ty);

    // Compare against the expected (already-substituted) iface fn-type.
    let actual = TypeRef::FnType {
        args: Box::new(record_from_named_args(&arg_names, &arg_types)),
        return_type: Box::new(return_type.clone()),
    };
    if !signatures_match(expected_fn_type, &actual, type_aliases) {
        bail!(
            "E-IMPL-005: impl method `{method_name}` signature does not match interface method `{iface_qualified}.{method_name}`; expected {:?}, got {:?}",
            expected_fn_type, actual
        );
    }

    let steps = do_seq
        .as_sequence()
        .context("function do must be sequence")?
        .to_vec();
    if steps.is_empty() {
        bail!("impl method `{method_name}`: function `do` must be a non-empty sequence");
    }
    let is_wasm_only = is_wasm_only_body(&steps);

    // Sig key shape: `mod.type.iface_local_name.method`. Using only the
    // iface's *bare* name (after stripping the module prefix) keeps the key
    // human-readable while still being unique per (type, iface, method).
    let bare_type = strip_module_prefix(qualified_type_key);
    let bare_iface = strip_module_prefix(iface_qualified);
    let sig_symbol = format!("{bare_type}.{bare_iface}.{method_name}");
    let sig_key = if module_alias.is_empty() {
        sig_symbol.clone()
    } else {
        format!("{module_alias}.{sig_symbol}")
    };
    if sigs.contains_key(&sig_key) {
        bail!("impl method `{sig_key}` collides with an existing function");
    }

    let body_kind = if is_wasm_only {
        let (import, wasm_args) = extract_wasm_body(&steps[0])?;
        FunctionBody::Wasm { import, wasm_args }
    } else {
        FunctionBody::User {
            statements: Vec::new(),
        }
    };

    // Bounds for the impl method: synthesised by stitching together
    // (a) bounds from the enclosing type's `=where` (one inner Vec per
    // enclosing param), (b) empty inner Vecs for impl-local params (their
    // bound resolution happens later, see Phase 5d), and (c) bounds from
    // the function's own `=where` (parsed via `resolve_def_envelope_bounds`
    // which uses only the function's own scope).
    let enclosing_bounds: Vec<Vec<TypeRef>> = type_aliases
        .get(qualified_type_key)
        .map(|ta| ta.type_param_bounds.clone())
        .unwrap_or_default();
    let mut sig_bounds: Vec<Vec<TypeRef>> = Vec::with_capacity(sig_type_params.len());
    sig_bounds.extend(enclosing_bounds);
    let impl_local_count = impl_scope
        .len()
        .saturating_sub(enclosing_type_params_len(self_ty));
    for _ in 0..impl_local_count {
        sig_bounds.push(Vec::new());
    }
    let raw_method_bounds = resolve_def_envelope_bounds(&env, skeletons, warnings)?;
    let method_bounds = qualify_bounds(module_alias, raw_method_bounds, type_aliases);
    sig_bounds.extend(method_bounds);

    sigs.insert(
        sig_key.clone(),
        FunctionSig {
            alias: module_alias.to_string(),
            symbol: sig_symbol,
            type_params: sig_type_params,
            type_param_bounds: sig_bounds,
            arg_names,
            arg_types,
            grant_decls,
            return_type,
            body: body_kind,
            doc: env.doc.clone(),
        },
    );
    if !is_wasm_only {
        pending_user_bodies.push((sig_key.clone(), steps));
    }
    Ok(ImplMethodBinding::Fresh(sig_key))
}

/// Helper for `bind_impl_method`: count of enclosing-type params reachable
/// from the `self_ty` placeholder. Non-generic enclosing -> 0; generic ->
/// the number of `type_args`.
fn enclosing_type_params_len(self_ty: &TypeRef) -> usize {
    match self_ty {
        TypeRef::Instantiated { type_args, .. } => type_args.len(),
        _ => 0,
    }
}

/// Build a `TypeRef::FnType` from a `FunctionSig`. The `args` side becomes a
/// `$record` keyed by the original argument names so it matches the shape
/// produced by `parse_type_constructor` for `$fn-type`.
fn sig_function_type(sig: &FunctionSig) -> TypeRef {
    TypeRef::FnType {
        args: Box::new(record_from_named_args(&sig.arg_names, &sig.arg_types)),
        return_type: Box::new(sig.return_type.clone()),
    }
}

fn record_from_named_args(arg_names: &[String], arg_types: &[TypeRef]) -> TypeRef {
    if arg_names.is_empty() {
        return TypeRef::Void;
    }
    let mut fields = BTreeMap::new();
    for (n, t) in arg_names.iter().zip(arg_types.iter()) {
        fields.insert(n.clone(), t.clone());
    }
    TypeRef::Record(fields)
}

/// Two function signatures match for the purposes of `=impl` checking iff
/// their arguments match invariantly while the concrete impl return can flow
/// into the interface-declared return type.
fn signatures_match(
    expected: &TypeRef,
    actual: &TypeRef,
    type_aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let (
        TypeRef::FnType {
            args: expected_args,
            return_type: expected_return,
        },
        TypeRef::FnType {
            args: actual_args,
            return_type: actual_return,
        },
    ) = (expected, actual)
    else {
        return false;
    };

    let mut bindings: HashMap<String, TypeRef> = HashMap::new();
    if !unify_types(expected_args, actual_args, type_aliases, &mut bindings) {
        return false;
    }

    let mut reverse_bindings = bindings.clone();
    unify_types(
        actual_args,
        expected_args,
        type_aliases,
        &mut reverse_bindings,
    ) && unify_types(expected_return, actual_return, type_aliases, &mut bindings)
}

// ===== Phase 5: bound resolution and validation =====

/// True iff `ty` is, or transitively resolves to, an `$interface` (or an
/// `$intersect` whose every part is an interface bound). Used by
/// `validate_all_where_bounds` to flag malformed bounds with `E-WHERE-002`.
fn is_interface_bound(ty: &TypeRef, type_aliases: &HashMap<String, TypeAlias>) -> bool {
    match ty {
        TypeRef::Interface(_) => true,
        TypeRef::Named(n) => type_aliases
            .get(n)
            .is_some_and(|ta| is_interface_bound(&ta.body, type_aliases)),
        TypeRef::Instantiated { base, .. } => type_aliases
            .get(base)
            .is_some_and(|ta| is_interface_bound(&ta.body, type_aliases)),
        TypeRef::Intersect(parts) => parts.iter().all(|p| is_interface_bound(p, type_aliases)),
        _ => false,
    }
}

/// Walk every registered symbol and check that every `=where` bound element
/// resolves to an interface (or intersect of interfaces). Bounds that point
/// to non-interface types are rejected with `E-WHERE-002`.
fn validate_all_where_bounds(
    type_aliases: &HashMap<String, TypeAlias>,
    sigs: &HashMap<String, FunctionSig>,
    enums: &HashMap<String, EnumDef>,
) -> Result<()> {
    for (key, ta) in type_aliases {
        check_bound_list_shape(key, &ta.type_params, &ta.type_param_bounds, type_aliases)?;
    }
    for (key, sig) in sigs {
        check_bound_list_shape(key, &sig.type_params, &sig.type_param_bounds, type_aliases)?;
    }
    for (key, ed) in enums {
        check_bound_list_shape(key, &ed.type_params, &ed.type_param_bounds, type_aliases)?;
    }
    Ok(())
}

fn check_bound_list_shape(
    sym_key: &str,
    params: &[String],
    bounds: &[Vec<TypeRef>],
    type_aliases: &HashMap<String, TypeAlias>,
) -> Result<()> {
    for (i, name) in params.iter().enumerate() {
        let Some(list) = bounds.get(i) else { continue };
        for b in list {
            if !is_interface_bound(b, type_aliases) {
                bail!(
                    "E-WHERE-002: bound for type-parameter `{name}` of `{sym_key}` is not an interface (or intersect of interfaces); got {b:?}"
                );
            }
        }
    }
    Ok(())
}

/// Flatten a bound expression to the set of qualified iface keys it
/// requires. `$intersect` parts are unioned; `Named`/`Instantiated` keep
/// their qualified key (the iface alias's own qualified name); transparent
/// pass-through aliases (an alias whose body is itself an intersect of
/// interface aliases) are normalised to the underlying iface keys.
fn collect_required_iface_keys(
    ty: &TypeRef,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Vec<String> {
    match ty {
        TypeRef::Named(n) | TypeRef::Instantiated { base: n, .. } => {
            if let Some(ta) = type_aliases.get(n) {
                match &ta.body {
                    TypeRef::Interface(_) => vec![n.clone()],
                    TypeRef::Intersect(_) => collect_required_iface_keys(&ta.body, type_aliases),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        TypeRef::Intersect(parts) => parts
            .iter()
            .flat_map(|p| collect_required_iface_keys(p, type_aliases))
            .collect(),
        _ => Vec::new(),
    }
}

/// Returns `true` iff `arg` satisfies the iface(s) required by `bound`. For
/// `Named`/`Instantiated` args, lookup is in the impl table. For `Generic`
/// args, satisfaction reduces to: the generic's declared bounds (looked up
/// in `enclosing_params`/`enclosing_bounds`) cover every required iface
/// key. Primitives, records, tuples, etc. cannot satisfy nominal bounds.
fn type_satisfies_bound(
    arg: &TypeRef,
    bound: &TypeRef,
    type_aliases: &HashMap<String, TypeAlias>,
    impls: &HashMap<ImplKey, ImplBody>,
    enclosing_params: &[String],
    enclosing_bounds: &[Vec<TypeRef>],
) -> bool {
    let required = collect_required_iface_keys(bound, type_aliases);
    if required.is_empty() {
        return true;
    }
    match arg {
        TypeRef::Named(n) | TypeRef::Instantiated { base: n, .. } => required.iter().all(|iface| {
            impls.contains_key(&ImplKey {
                implementing_type: n.clone(),
                interface: iface.clone(),
            })
        }),
        TypeRef::Generic(name) => {
            let Some(idx) = enclosing_params.iter().position(|p| p == name) else {
                return false;
            };
            let Some(arg_bounds) = enclosing_bounds.get(idx) else {
                return false;
            };
            let provided: std::collections::HashSet<String> = arg_bounds
                .iter()
                .flat_map(|b| collect_required_iface_keys(b, type_aliases))
                .collect();
            required.iter().all(|r| provided.contains(r))
        }
        _ => false,
    }
}

/// Walk `ty` looking for `Instantiated` references. For each one, look up
/// the base alias's `type_param_bounds` and ensure every type-arg satisfies
/// its bound. Emits `E-BOUND-001` with `context` on the first violation.
fn check_typeref_bounds(
    ty: &TypeRef,
    type_aliases: &HashMap<String, TypeAlias>,
    impls: &HashMap<ImplKey, ImplBody>,
    enclosing_params: &[String],
    enclosing_bounds: &[Vec<TypeRef>],
    context: &str,
) -> Result<()> {
    match ty {
        TypeRef::Instantiated { base, type_args } => {
            if let Some(ta) = type_aliases.get(base) {
                for (i, tp) in ta.type_params.iter().enumerate() {
                    let Some(bound_list) = ta.type_param_bounds.get(i) else {
                        continue;
                    };
                    if bound_list.is_empty() {
                        continue;
                    }
                    let Some(arg) = type_args.get(i) else {
                        continue;
                    };
                    for required in bound_list {
                        if !type_satisfies_bound(
                            arg,
                            required,
                            type_aliases,
                            impls,
                            enclosing_params,
                            enclosing_bounds,
                        ) {
                            bail!(
                                "E-BOUND-001: in `{context}`, type argument `{arg:?}` for `{base}.{tp}` does not satisfy bound `{required:?}`"
                            );
                        }
                    }
                }
            }
            for arg in type_args {
                check_typeref_bounds(
                    arg,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    context,
                )?;
            }
        }
        TypeRef::Record(fields) | TypeRef::Enum(fields) | TypeRef::Interface(fields) => {
            for t in fields.values() {
                check_typeref_bounds(
                    t,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    context,
                )?;
            }
        }
        TypeRef::Tuple(items) | TypeRef::Union(items) | TypeRef::Intersect(items) => {
            for t in items {
                check_typeref_bounds(
                    t,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    context,
                )?;
            }
        }
        TypeRef::Array(inner) => check_typeref_bounds(
            inner,
            type_aliases,
            impls,
            enclosing_params,
            enclosing_bounds,
            context,
        )?,
        TypeRef::Map { key, value } => {
            check_typeref_bounds(
                key,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                context,
            )?;
            check_typeref_bounds(
                value,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                context,
            )?;
        }
        TypeRef::FnType { args, return_type } => {
            check_typeref_bounds(
                args,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                context,
            )?;
            check_typeref_bounds(
                return_type,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                context,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// First path segment of a qualified symbol key, or "" for entry-level symbols
/// (`main`, `-priv`, etc.).
fn symbol_owner_module_prefix(sym_key: &str) -> &str {
    sym_key.split_once('.').map(|(a, _)| a).unwrap_or("")
}

/// True for names like `m.-priv` / `m.-t`: a *public* import alias followed by
/// a module-private (`-`‑prefixed) symbol. Such paths must not be referenced
/// from outside that imported module.
fn qualified_type_leaks_imported_module_private(name: &str) -> bool {
    let Some((prefix, rest)) = name.split_once('.') else {
        return false;
    };
    !prefix.is_empty() && !prefix.starts_with('-') && rest.starts_with('-')
}

fn check_typeref_module_private_access(
    ty: &TypeRef,
    owner_prefix: &str,
    context: &str,
) -> Result<()> {
    let check_name = |n: &str| -> Result<()> {
        if qualified_type_leaks_imported_module_private(n) {
            let victim = n.split_once('.').map(|(a, _)| a).unwrap();
            if owner_prefix != victim {
                bail!("unknown type `{n}` in `{context}`");
            }
        }
        Ok(())
    };
    match ty {
        TypeRef::Named(n) => check_name(n),
        TypeRef::Instantiated { base, type_args } => {
            check_name(base)?;
            for arg in type_args {
                check_typeref_module_private_access(arg, owner_prefix, context)?;
            }
            Ok(())
        }
        TypeRef::Record(fields) | TypeRef::Enum(fields) | TypeRef::Interface(fields) => {
            for t in fields.values() {
                check_typeref_module_private_access(t, owner_prefix, context)?;
            }
            Ok(())
        }
        TypeRef::Tuple(items) | TypeRef::Union(items) | TypeRef::Intersect(items) => {
            for t in items {
                check_typeref_module_private_access(t, owner_prefix, context)?;
            }
            Ok(())
        }
        TypeRef::Array(inner) => check_typeref_module_private_access(inner, owner_prefix, context),
        TypeRef::Map { key, value } => {
            check_typeref_module_private_access(key, owner_prefix, context)?;
            check_typeref_module_private_access(value, owner_prefix, context)
        }
        TypeRef::FnType { args, return_type } => {
            check_typeref_module_private_access(args, owner_prefix, context)?;
            check_typeref_module_private_access(return_type, owner_prefix, context)
        }
        TypeRef::Newtype { inner, .. } => {
            check_typeref_module_private_access(inner, owner_prefix, context)
        }
        _ => Ok(()),
    }
}

/// Final post-hoc validation sweep over every registered symbol's type
/// expressions and every lowered call's `type_args`. Catches both
/// type-position bound violations and call-site violations in one pass.
fn validate_all_instantiation_bounds(
    type_aliases: &HashMap<String, TypeAlias>,
    sigs: &HashMap<String, FunctionSig>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    main_statements: &[Statement],
) -> Result<()> {
    for (key, ta) in type_aliases {
        let owner = symbol_owner_module_prefix(key);
        check_typeref_module_private_access(&ta.body, owner, key)?;
        for bounds_list in &ta.type_param_bounds {
            for b in bounds_list {
                check_typeref_module_private_access(b, owner, key)?;
            }
        }
        check_typeref_bounds(
            &ta.body,
            type_aliases,
            impls,
            &ta.type_params,
            &ta.type_param_bounds,
            key,
        )?;
    }
    for (key, sig) in sigs {
        let owner = symbol_owner_module_prefix(key);
        for bounds_list in &sig.type_param_bounds {
            for b in bounds_list {
                check_typeref_module_private_access(b, owner, key)?;
            }
        }
        for (i, at) in sig.arg_types.iter().enumerate() {
            let arg_name = &sig.arg_names[i];
            let ctx = format!("{key} (arg `{arg_name}`)");
            check_typeref_module_private_access(at, owner, &ctx)?;
            check_typeref_bounds(
                at,
                type_aliases,
                impls,
                &sig.type_params,
                &sig.type_param_bounds,
                &ctx,
            )?;
        }
        let ret_ctx = format!("{key} (return)");
        check_typeref_module_private_access(&sig.return_type, owner, &ret_ctx)?;
        check_typeref_bounds(
            &sig.return_type,
            type_aliases,
            impls,
            &sig.type_params,
            &sig.type_param_bounds,
            &ret_ctx,
        )?;
        if let FunctionBody::User { statements } = &sig.body {
            check_statements_call_bounds(
                statements,
                sigs,
                type_aliases,
                impls,
                &sig.type_params,
                &sig.type_param_bounds,
                owner,
                key,
            )?;
        }
    }
    for (key, ed) in enums {
        let owner = symbol_owner_module_prefix(key);
        for bounds_list in &ed.type_param_bounds {
            for b in bounds_list {
                check_typeref_module_private_access(b, owner, key)?;
            }
        }
        for (tag, t) in &ed.tags {
            let ctx = format!("{key}.{tag}");
            check_typeref_module_private_access(t, owner, &ctx)?;
            check_typeref_bounds(
                t,
                type_aliases,
                impls,
                &ed.type_params,
                &ed.type_param_bounds,
                &ctx,
            )?;
        }
    }
    check_statements_call_bounds(
        main_statements,
        sigs,
        type_aliases,
        impls,
        &[],
        &[],
        "",
        "main",
    )?;
    Ok(())
}

fn check_expr_call_bounds(
    expr: &Expr,
    sigs: &HashMap<String, FunctionSig>,
    type_aliases: &HashMap<String, TypeAlias>,
    impls: &HashMap<ImplKey, ImplBody>,
    enclosing_params: &[String],
    enclosing_bounds: &[Vec<TypeRef>],
    referrer_owner: &str,
    context: &str,
) -> Result<()> {
    match expr {
        Expr::Call { call, .. } => check_call_and_arg_bounds(
            call,
            sigs,
            type_aliases,
            impls,
            enclosing_params,
            enclosing_bounds,
            referrer_owner,
            context,
        )?,
        Expr::Cast { from, .. } => check_expr_call_bounds(
            from,
            sigs,
            type_aliases,
            impls,
            enclosing_params,
            enclosing_bounds,
            referrer_owner,
            context,
        )?,
        Expr::PolicyNarrow { from, .. } => check_expr_call_bounds(
            from,
            sigs,
            type_aliases,
            impls,
            enclosing_params,
            enclosing_bounds,
            referrer_owner,
            context,
        )?,
        Expr::Record(fields) => {
            for value in fields.values() {
                check_expr_call_bounds(
                    value,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
            }
        }
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                check_expr_call_bounds(
                    item,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                check_expr_call_bounds(
                    key,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
                check_expr_call_bounds(
                    value,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
            }
        }
        Expr::EnumConstructor { payload, .. } => {
            if let Some(payload) = payload {
                check_expr_call_bounds(
                    payload,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
            }
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => {
            check_expr_call_bounds(
                cond,
                sigs,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                referrer_owner,
                context,
            )?;
            check_expr_call_bounds(
                then_e,
                sigs,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                referrer_owner,
                context,
            )?;
            check_expr_call_bounds(
                else_e,
                sigs,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                referrer_owner,
                context,
            )?;
        }
        Expr::Value(_) | Expr::VarRef(_) => {}
    }
    Ok(())
}

fn check_call_and_arg_bounds(
    call: &Call,
    sigs: &HashMap<String, FunctionSig>,
    type_aliases: &HashMap<String, TypeAlias>,
    impls: &HashMap<ImplKey, ImplBody>,
    enclosing_params: &[String],
    enclosing_bounds: &[Vec<TypeRef>],
    referrer_owner: &str,
    context: &str,
) -> Result<()> {
    check_call_bounds(
        call,
        sigs,
        type_aliases,
        impls,
        enclosing_params,
        enclosing_bounds,
        referrer_owner,
        context,
    )?;
    for arg in &call.args {
        check_expr_call_bounds(
            arg,
            sigs,
            type_aliases,
            impls,
            enclosing_params,
            enclosing_bounds,
            referrer_owner,
            context,
        )?;
    }
    Ok(())
}

fn check_statements_call_bounds(
    statements: &[Statement],
    sigs: &HashMap<String, FunctionSig>,
    type_aliases: &HashMap<String, TypeAlias>,
    impls: &HashMap<ImplKey, ImplBody>,
    enclosing_params: &[String],
    enclosing_bounds: &[Vec<TypeRef>],
    referrer_owner: &str,
    context: &str,
) -> Result<()> {
    for stmt in statements {
        match stmt {
            Statement::Call(call) => check_call_and_arg_bounds(
                call,
                sigs,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
                referrer_owner,
                context,
            )?,
            Statement::Let { value, .. } => match value {
                LetValue::Call(call) => check_call_and_arg_bounds(
                    call,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?,
                LetValue::Expr(expr) => check_expr_call_bounds(
                    expr,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?,
            },
            Statement::Match { arms, .. } => {
                for arm in arms {
                    check_statements_call_bounds(
                        &arm.body,
                        sigs,
                        type_aliases,
                        impls,
                        enclosing_params,
                        enclosing_bounds,
                        referrer_owner,
                        context,
                    )?;
                }
            }
            Statement::Return(expr) | Statement::Eval(expr) => {
                check_expr_call_bounds(
                    expr,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                check_statements_call_bounds(
                    then_body,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
                check_statements_call_bounds(
                    else_body,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
            }
            Statement::While { body, .. } => {
                check_statements_call_bounds(
                    body,
                    sigs,
                    type_aliases,
                    impls,
                    enclosing_params,
                    enclosing_bounds,
                    referrer_owner,
                    context,
                )?;
            }
        }
    }
    Ok(())
}

fn check_call_bounds(
    call: &Call,
    sigs: &HashMap<String, FunctionSig>,
    type_aliases: &HashMap<String, TypeAlias>,
    impls: &HashMap<ImplKey, ImplBody>,
    enclosing_params: &[String],
    enclosing_bounds: &[Vec<TypeRef>],
    referrer_owner: &str,
    context: &str,
) -> Result<()> {
    for ta in &call.type_args {
        check_typeref_module_private_access(ta, referrer_owner, context)?;
    }
    let Some(sig) = sigs.get(&call.callee_key) else {
        return Ok(());
    };
    if sig.type_params.is_empty() || call.type_args.is_empty() {
        return Ok(());
    }
    for (i, tp) in sig.type_params.iter().enumerate() {
        let Some(bound_list) = sig.type_param_bounds.get(i) else {
            continue;
        };
        if bound_list.is_empty() {
            continue;
        }
        let Some(arg) = call.type_args.get(i) else {
            continue;
        };
        for required in bound_list {
            if !type_satisfies_bound(
                arg,
                required,
                type_aliases,
                impls,
                enclosing_params,
                enclosing_bounds,
            ) {
                bail!(
                    "E-BOUND-001: in `{context}`, call to `{}` passes type argument `{arg:?}` for `{tp}` that does not satisfy bound `{required:?}`",
                    call.callee_key
                );
            }
        }
    }
    Ok(())
}

fn parse_wasm_arg_spec(v: &Value) -> Result<WasmArgSpec> {
    if let Some(i) = v.as_i64() {
        return Ok(WasmArgSpec::ConstInt(i));
    }
    let s = v
        .as_str()
        .context("wasm arg spec must be string or int literal")?;
    if let Some(name) = s.strip_prefix("$args.") {
        return Ok(WasmArgSpec::Arg(name.to_string()));
    }
    if let Some(n) = s.strip_prefix("$const.") {
        let i = n
            .parse::<i64>()
            .with_context(|| format!("invalid $const int `{n}`"))?;
        return Ok(WasmArgSpec::ConstInt(i));
    }
    Ok(WasmArgSpec::ConstStr(s.to_string()))
}

fn parse_signature_args(
    v: &Value,
    scope: &[String],
    skeletons: &HashMap<String, AliasSkeleton>,
    warnings: &mut Vec<String>,
    self_allowed: bool,
) -> Result<(Vec<String>, Vec<TypeRef>)> {
    if is_void_args(v) {
        return Ok((Vec::new(), Vec::new()));
    }
    let args_map = v
        .as_mapping()
        .context("function args must be `$void` or a mapping of arg->type")?;
    if args_map.is_empty() {
        bail!("zero-arg functions must use `args: $void`");
    }
    let mut arg_names = Vec::with_capacity(args_map.len());
    let mut arg_types = Vec::with_capacity(args_map.len());
    for (k, t) in args_map {
        let arg_name = k.as_str().context("arg name must be string")?.to_string();
        maybe_warn_kebab(&arg_name, "function argument", warnings);
        let arg_type = parse_type_ref(t, scope, skeletons, warnings, self_allowed)
            .with_context(|| format!("invalid type for arg `{arg_name}`"))?;
        arg_names.push(arg_name);
        arg_types.push(arg_type);
    }
    Ok((arg_names, arg_types))
}

#[allow(clippy::too_many_arguments)]
fn verify_stmt_keys(m: &serde_yaml::Mapping, allowed: &[&str]) -> Result<()> {
    for k in m.keys() {
        let ks = k.as_str().context("statement key must be string")?;
        if !allowed.contains(&ks) {
            bail!(
                "unexpected key `{ks}` in statement (expected only: {})",
                allowed.join(", ")
            );
        }
    }
    Ok(())
}

/// After lowering both branches of `$if`, locals must match on every key that
/// remains in scope: only bindings present in **both** branch environments with
/// identical inferred types are kept (same rule as `$match` arms, which use a
/// fresh `locals` clone per arm and never merge into the parent map).
fn merge_if_branch_env(
    then_env: &HashMap<String, TypeRef>,
    else_env: &HashMap<String, TypeRef>,
) -> Result<HashMap<String, TypeRef>> {
    let mut out = HashMap::with_capacity(then_env.len().min(else_env.len()));
    for k in then_env.keys() {
        match (then_env.get(k), else_env.get(k)) {
            (Some(t), Some(e)) if t == e => {
                out.insert(k.clone(), t.clone());
            }
            (Some(t), Some(e)) => {
                bail!("`$if` branches disagree on inferred type of local `{k}` ({t:?} vs {e:?})");
            }
            _ => {}
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn lower_branch_to_body(
    v: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &mut HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
    fn_ctx: Option<&UserFnContext>,
) -> Result<Vec<Statement>> {
    if let Some(seq) = v.as_sequence() {
        let mut out = Vec::with_capacity(seq.len());
        for step in seq {
            out.push(lower_statement(
                step,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                warnings,
                fn_ctx,
            )?);
        }
        return Ok(out);
    }
    Ok(vec![Statement::Eval(parse_expr(
        v,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        locals,
        stmt_home_module(fn_ctx),
        warnings,
    )?)])
}

#[allow(clippy::too_many_arguments)]
fn parse_if_statement(
    stmt: &serde_yaml::Mapping,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &mut HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
    fn_ctx: Option<&UserFnContext>,
) -> Result<Statement> {
    verify_stmt_keys(stmt, &["$if", "then", "else"])?;
    let cond = parse_expr(
        map_get_str(stmt, "$if").context("$if missing condition")?,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        locals,
        stmt_home_module(fn_ctx),
        warnings,
    )?;
    let cond_ty = infer_expr_type(&cond, constants, locals, type_aliases, enums)
        .context("could not infer type of `$if` condition")?;
    if cond_ty != TypeRef::Bool {
        bail!("`$if` condition must be `$bool`, got {cond_ty:?}");
    }
    let then_v = map_get_str(stmt, "then").context("$if missing `then`")?;
    let else_v = map_get_str(stmt, "else").context("$if missing `else`")?;
    let baseline = locals.clone();
    let mut then_locals = baseline.clone();
    let then_body = lower_branch_to_body(
        then_v,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        &mut then_locals,
        warnings,
        fn_ctx,
    )?;
    let mut else_locals = baseline.clone();
    let else_body = lower_branch_to_body(
        else_v,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        &mut else_locals,
        warnings,
        fn_ctx,
    )?;
    *locals = merge_if_branch_env(&then_locals, &else_locals)?;
    Ok(Statement::If {
        cond,
        then_body,
        else_body,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_while_statement(
    stmt: &serde_yaml::Mapping,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &mut HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
    fn_ctx: Option<&UserFnContext>,
) -> Result<Statement> {
    verify_stmt_keys(stmt, &["$while", "do"])?;
    let cond = parse_expr(
        map_get_str(stmt, "$while").context("$while missing condition")?,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        locals,
        stmt_home_module(fn_ctx),
        warnings,
    )?;
    let cond_ty = infer_expr_type(&cond, constants, locals, type_aliases, enums)
        .context("could not infer type of `$while` condition")?;
    if cond_ty != TypeRef::Bool {
        bail!("`$while` condition must be `$bool`, got {cond_ty:?}");
    }
    let do_v = map_get_str(stmt, "do").context("$while missing `do`")?;
    let steps = do_v
        .as_sequence()
        .context("`$while.do` must be a block sequence")?;
    let baseline = locals.clone();
    let mut body_locals = baseline.clone();
    let mut body = Vec::with_capacity(steps.len());
    for step in steps {
        body.push(lower_statement(
            step,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            &mut body_locals,
            warnings,
            fn_ctx,
        )?);
    }
    *locals = baseline;
    Ok(Statement::While { cond, body })
}

#[allow(clippy::too_many_arguments)]
fn lower_statement(
    step: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &mut HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
    fn_ctx: Option<&UserFnContext>,
) -> Result<Statement> {
    let stmt = step.as_mapping().context("statement must be a mapping")?;
    let home = stmt_home_module(fn_ctx);

    if map_get_str(stmt, "$let").is_some() {
        if stmt.len() != 1 {
            bail!("`$let` statement must contain only the `$let` key");
        }
        let let_v = map_get_str(stmt, "$let").expect("checked");
        let lm = let_v.as_mapping().context("$let must be mapping")?;
        if lm.len() != 1 {
            bail!("$let must define one variable");
        }
        let (vk, vv) = lm.iter().next().expect("let one");
        let var = vk
            .as_str()
            .context("$let variable must be string")?
            .to_string();
        maybe_warn_kebab(&var, "local variable", warnings);
        if looks_like_call(vv, sigs, home) || looks_like_iface_call(vv, home, type_aliases) {
            let call = parse_call(
                vv,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home,
                warnings,
            )?;
            let sig = sigs
                .get(&call.callee_key)
                .context("internal: missing callee after parse_call")?;
            let ret_ty = substituted_return_type(sig, &call.type_args);
            if ret_ty == TypeRef::Void {
                bail!("cannot bind void return in $let");
            }
            locals.insert(var.clone(), ret_ty);
            Ok(Statement::Let {
                var,
                value: LetValue::Call(call),
            })
        } else {
            let expr = parse_expr(
                vv,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home,
                warnings,
            )?;
            let expr_ty = infer_expr_type(&expr, constants, locals, type_aliases, enums)
                .context("could not infer type for $let expression")?;
            if expr_ty == TypeRef::Void {
                bail!("cannot bind void expression in $let");
            }
            locals.insert(var.clone(), expr_ty);
            Ok(Statement::Let {
                var,
                value: LetValue::Expr(expr),
            })
        }
    } else if map_get_str(stmt, "$return").is_some() {
        if stmt.len() != 1 {
            bail!("`$return` statement must contain only the `$return` key");
        }
        let ctx = fn_ctx.context("`$return` is only valid inside user-defined functions")?;
        let ret_v = map_get_str(stmt, "$return").expect("checked");
        let expr = parse_expr(
            ret_v,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            locals,
            home,
            warnings,
        )?;
        let actual = infer_expr_type(&expr, constants, locals, type_aliases, enums)
            .context("could not infer type for `$return` expression")?;
        if !type_compatible(&ctx.return_type, &actual, type_aliases) {
            if crosses_newtype_boundary(&ctx.return_type, &actual, type_aliases) {
                bail!(
                    "E-NEWTYPE-001: implicit coercion between `$newtype` and its inner type is forbidden in `$return`; use `$cast` (expected {:?}, got {:?})",
                    ctx.return_type,
                    actual
                );
            }
            bail!(
                "`$return` type mismatch: expected {:?}, got {:?}",
                ctx.return_type,
                actual
            );
        }
        Ok(Statement::Return(expr))
    } else if map_get_str(stmt, "$match").is_some() {
        parse_match_statement(
            stmt,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            locals,
            warnings,
            fn_ctx,
        )
    } else if map_get_str(stmt, "$if").is_some() {
        parse_if_statement(
            stmt,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            locals,
            warnings,
            fn_ctx,
        )
    } else if map_get_str(stmt, "$while").is_some() {
        parse_while_statement(
            stmt,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            locals,
            warnings,
            fn_ctx,
        )
    } else {
        let call = parse_call(
            step,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            locals,
            home,
            warnings,
        )?;
        Ok(Statement::Call(call))
    }
}

/// Resolve a dotted interface path from source (`fs.writable`) using the
/// current module scope (`io` → `io.fs.writable`), then fall back to the bare path.
fn resolve_iface_key_for_scope(
    iface_path: &str,
    module_scope: &str,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Result<String> {
    if !module_scope.is_empty() {
        let scoped = format!("{module_scope}.{iface_path}");
        if type_aliases.contains_key(&scoped) {
            return Ok(scoped);
        }
    }
    if type_aliases.contains_key(iface_path) {
        return Ok(iface_path.to_string());
    }
    bail!(
        "interface path `${iface_path}` is not registered for this module scope \
         (try `{}` or `{}`)",
        if module_scope.is_empty() {
            iface_path.to_string()
        } else {
            format!("{module_scope}.{iface_path}")
        },
        iface_path
    );
}

/// When `module_scope` is `a.io` but a value is still typed as a peer key like `io.fs.*`,
/// rewrite to `a.io.fs.*` if that nominal type exists.
fn nominal_type_key_for_module_scope(
    key: String,
    module_scope: &str,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
) -> String {
    if module_scope.is_empty() || key.starts_with(module_scope) {
        return key;
    }
    let prefixed = format!("{module_scope}.{key}");
    if type_aliases.contains_key(&prefixed) || enums.contains_key(&prefixed) {
        return prefixed;
    }
    // Inference sometimes keeps types keyed as `io.fs.*` (peer of `io`) while the body is
    // compiled under a mount like `a.io`; bridge to `a.io.fs.*`.
    if let Some(rest) = key.strip_prefix("io.fs.") {
        let bridged = format!("{module_scope}.fs.{rest}");
        if type_aliases.contains_key(&bridged) || enums.contains_key(&bridged) {
            return bridged;
        }
    }
    key
}

/// Heuristic: does this value look like an interface-qualified call, i.e.
/// a single-key mapping `$alias.symbol: <payload>` whose first segment
/// resolves to a registered interface alias? Used by `lower_statement` to
/// route `$let` values through `parse_call` rather than `parse_expr`.
fn looks_like_iface_call(
    v: &Value,
    home_module: &str,
    type_aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let Some(m) = v.as_mapping() else {
        return false;
    };
    let Ok((call_key, _, _, _)) = split_call_envelope(m) else {
        return false;
    };
    iface_dispatch_arg_name(&call_key, home_module, type_aliases).is_ok()
}

/// Phase 6: resolve an interface-qualified call (`$iface.method`) to the
/// concrete impl method's sig key by inspecting the static type of the
/// `$self`-typed argument value.
///
/// Errors:
/// - `E-CALL-IFACE-NOSELF` — iface method has no `$self` arg.
/// - `E-DISPATCH-001` — `$self` arg has a *generic* static type (deferred
///   until monomorphisation lands).
/// - `E-BOUND-001` — implementing type has no `=impl` for the iface.
#[allow(clippy::too_many_arguments)]
fn try_resolve_iface_call(
    call_key: &str,
    payload: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &HashMap<String, TypeRef>,
    home_module: &str,
    warnings: &mut Vec<String>,
) -> Result<String> {
    let _ = sigs;
    let stripped = call_key
        .strip_prefix('$')
        .with_context(|| format!("call key `{call_key}` must start with `$`"))?;
    let (iface_path, method) = stripped.rsplit_once('.').with_context(|| {
        format!("`{call_key}` is not an interface-qualified call (no `.method` suffix)")
    })?;

    let iface_qualified = resolve_iface_key_for_scope(iface_path, home_module, type_aliases)
        .with_context(|| format!("`{call_key}`: interface dispatch"))?;
    let iface_def = type_aliases.get(&iface_qualified).with_context(|| {
        format!("`{call_key}`: interface alias `{iface_qualified}` is not registered")
    })?;
    let TypeRef::Interface(iface_methods) = &iface_def.body else {
        bail!(
            "`{call_key}`: `{iface_qualified}` is not an interface (its body is `{:?}`)",
            iface_def.body
        );
    };
    let expected = iface_methods.get(method).with_context(|| {
        format!("interface `{iface_qualified}` has no method `{method}` (called via `{call_key}`)")
    })?;

    let TypeRef::FnType { args, .. } = expected else {
        bail!(
            "interface `{iface_qualified}` method `{method}` is not a `$fn-type`; got `{:?}`",
            expected
        );
    };
    let TypeRef::Record(args_record) = args.as_ref() else {
        bail!(
            "interface `{iface_qualified}` method `{method}` has non-record `args`; got `{:?}`",
            args
        );
    };
    let self_arg_name = args_record
        .iter()
        .find(|(_, t)| matches!(t, TypeRef::SelfType))
        .map(|(n, _)| n.clone());
    let Some(self_arg_name) = self_arg_name else {
        bail!(
            "E-CALL-IFACE-NOSELF: interface method `{iface_qualified}.{method}` has no `$self` argument; \
             call it via the type-qualified form `$<implementing-type>.{iface_short}.{method}` instead",
            iface_short = iface_qualified
                .rsplit('.')
                .next()
                .unwrap_or(&iface_qualified)
        );
    };

    let payload_map = payload.as_mapping().with_context(|| {
        format!("interface-qualified call `{call_key}` requires a mapping payload")
    })?;
    let dispatch_v = payload_map
        .get(Value::String(self_arg_name.clone()))
        .with_context(|| {
            format!(
                "interface-qualified call `{call_key}` is missing dispatch argument `{self_arg_name}`"
            )
        })?;

    let dispatch_expr = parse_expr(
        dispatch_v,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        locals,
        home_module,
        warnings,
    )
    .with_context(|| format!("could not parse dispatch arg `{self_arg_name}` of `{call_key}`"))?;
    let dispatch_ty = infer_expr_type(&dispatch_expr, constants, locals, type_aliases, enums)
        .with_context(|| {
            format!("could not infer type of dispatch arg `{self_arg_name}` of `{call_key}`")
        })?;

    let implementing = match &dispatch_ty {
        TypeRef::Named(n) | TypeRef::Instantiated { base: n, .. } => n.clone(),
        TypeRef::Generic(_) => bail!(
            "E-DISPATCH-001: interface-qualified dispatch on a generic-typed value is not yet \
             implemented (monomorphisation pending). Call site: `{call_key}` with dispatch arg \
             `{self_arg_name}` of type `{dispatch_ty:?}`."
        ),
        _ => bail!(
            "interface-qualified call `{call_key}` cannot dispatch on dispatch-arg type `{dispatch_ty:?}` \
             (no nominal `=impl` block can exist for primitives, tuples, records, or unions)"
        ),
    };
    let implementing =
        nominal_type_key_for_module_scope(implementing, home_module, type_aliases, enums);

    let impl_key = ImplKey {
        implementing_type: implementing.clone(),
        interface: iface_qualified.clone(),
    };
    let impl_body =
        impls.get(&impl_key).with_context(|| {
            format!(
            "E-BOUND-001: type `{implementing}` does not implement interface `{iface_qualified}` \
             (no `=impl: {{ ${} }}` block found); cannot dispatch `{call_key}`",
            iface_qualified.rsplit('.').next().unwrap_or(&iface_qualified)
        )
        })?;
    let binding = impl_body.methods.get(method).with_context(|| {
        format!("internal: impl `{implementing} : {iface_qualified}` is missing method `{method}`")
    })?;
    let sig_key = match binding {
        ImplMethodBinding::Fresh(sk) | ImplMethodBinding::Ref(sk) => sk.clone(),
    };
    Ok(sig_key)
}

fn resolve_call_target(
    call_key: &str,
    sigs: &HashMap<String, FunctionSig>,
    home_module: &str,
) -> Result<String> {
    if let Ok((alias, symbol)) = parse_qualified_call(call_key) {
        // Importers must not reach into another module's `-` private namespace.
        if !alias.is_empty() && symbol.starts_with('-') {
            bail!("unknown function `{call_key}`");
        }
        // Prefer module-scoped keys before unscoped `{alias}.{symbol}` so a nested `util`
        // does not accidentally bind to a same-named entry import.
        if !home_module.is_empty() {
            let scoped = format!("{home_module}.{alias}.{symbol}");
            if sigs.contains_key(&scoped) {
                return Ok(scoped);
            }
            // Stdlib modules refer to their own exports as `$io.foo` while mounted at `a.io`;
            // resolve that to `a.io.foo`, not `a.io.io.foo`.
            let leaf = home_module.rsplit('.').next().unwrap_or(home_module);
            if alias == leaf {
                let self_export = format!("{home_module}.{symbol}");
                if sigs.contains_key(&self_export) {
                    return Ok(self_export);
                }
            }
        }
        let sig_key = format!("{alias}.{symbol}");
        if sigs.contains_key(&sig_key) {
            return Ok(sig_key);
        }
    }
    let rest = call_key
        .strip_prefix('$')
        .context("call key must start with `$`")?;
    if !rest.contains('.') {
        if !home_module.is_empty() && rest.starts_with('-') {
            let q = format!("{home_module}.{rest}");
            if sigs.contains_key(&q) {
                return Ok(q);
            }
        }
        if sigs.contains_key(rest) && sigs[rest].alias.is_empty() {
            return Ok(rest.to_string());
        }
    }
    bail!("unknown function `{call_key}`")
}

/// Name of the `$self`-typed dispatch argument for an interface-qualified call.
fn iface_dispatch_arg_name(
    call_key: &str,
    home_module: &str,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Result<String> {
    let stripped = call_key
        .strip_prefix('$')
        .with_context(|| format!("call key `{call_key}` must start with `$`"))?;
    let (iface_path, method) = stripped.rsplit_once('.').with_context(|| {
        format!("`{call_key}` is not an interface-qualified call (no `.method` suffix)")
    })?;

    let iface_qualified = resolve_iface_key_for_scope(iface_path, home_module, type_aliases)
        .with_context(|| format!("`{call_key}`: interface dispatch"))?;
    let iface_def = type_aliases.get(&iface_qualified).with_context(|| {
        format!("`{call_key}`: interface alias `{iface_qualified}` is not registered")
    })?;
    let TypeRef::Interface(iface_methods) = &iface_def.body else {
        bail!(
            "`{call_key}`: `{iface_qualified}` is not an interface (its body is `{:?}`)",
            iface_def.body
        );
    };
    let expected = iface_methods.get(method).with_context(|| {
        format!("interface `{iface_qualified}` has no method `{method}` (called via `{call_key}`)")
    })?;

    let TypeRef::FnType { args, .. } = expected else {
        bail!(
            "interface `{iface_qualified}` method `{method}` is not a `$fn-type`; got `{:?}`",
            expected
        );
    };
    let TypeRef::Record(args_record) = args.as_ref() else {
        bail!(
            "interface `{iface_qualified}` method `{method}` has non-record `args`; got `{:?}`",
            args
        );
    };
    let self_arg_name = args_record
        .iter()
        .find(|(_, t)| matches!(t, TypeRef::SelfType))
        .map(|(n, _)| n.clone());
    let Some(self_arg_name) = self_arg_name else {
        bail!(
            "E-CALL-IFACE-NOSELF: interface method `{iface_qualified}.{method}` has no `$self` argument; \
             call it via the type-qualified form `$<implementing-type>.{iface_short}.{method}` instead",
            iface_short = iface_qualified
                .rsplit('.')
                .next()
                .unwrap_or(&iface_qualified)
        );
    };
    Ok(self_arg_name)
}

fn build_iface_dispatch_payload(
    call_key: &str,
    subject: &Value,
    siblings: &[(String, Value)],
    home_module: &str,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Result<Value> {
    let self_name = iface_dispatch_arg_name(call_key, home_module, type_aliases)?;
    let mut map = serde_yaml::Mapping::new();
    map.insert(Value::String(self_name), subject.clone());
    for (k, v) in siblings {
        map.insert(Value::String(k.clone()), v.clone());
    }
    Ok(Value::Mapping(map))
}

/// Field names (declaration order) for an interface method's args record, if `call_key` refers to
/// a registered interface method.
fn iface_method_record_field_names(
    call_key: &str,
    home_module: &str,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Result<Option<Vec<String>>> {
    let stripped = match call_key.strip_prefix('$') {
        Some(s) => s,
        None => return Ok(None),
    };
    let (iface_path, method) = match stripped.rsplit_once('.') {
        Some(p) => p,
        None => return Ok(None),
    };
    let iface_qualified = match resolve_iface_key_for_scope(iface_path, home_module, type_aliases) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let Some(iface_def) = type_aliases.get(&iface_qualified) else {
        return Ok(None);
    };
    let TypeRef::Interface(iface_methods) = &iface_def.body else {
        return Ok(None);
    };
    let Some(expected) = iface_methods.get(method) else {
        return Ok(None);
    };
    let TypeRef::FnType { args, .. } = expected else {
        return Ok(None);
    };
    let TypeRef::Record(args_record) = args.as_ref() else {
        return Ok(None);
    };
    Ok(Some(args_record.keys().cloned().collect::<Vec<_>>()))
}

/// Rejects nesting the full parameter map under the callee (`$f: {{ t: ..., x: ... }}`).
fn reject_iface_nested_call_bundle(
    call_key: &str,
    subject: &Value,
    siblings: &[(String, Value)],
    home_module: &str,
    type_aliases: &HashMap<String, TypeAlias>,
) -> Result<()> {
    if !siblings.is_empty() {
        return Ok(());
    }
    let Some(fields) = iface_method_record_field_names(call_key, home_module, type_aliases)? else {
        return Ok(());
    };
    let Some(subm) = subject.as_mapping() else {
        return Ok(());
    };
    if subm.is_empty() {
        return Ok(());
    }
    let expected: HashSet<String> = fields.iter().cloned().collect();
    let keys: HashSet<String> = subm
        .keys()
        .filter_map(|k| k.as_str().map(std::string::ToString::to_string))
        .collect();
    if keys == expected {
        bail!(
            "call `{call_key}` must not nest all parameters under the callee; pass the dispatch value as the callee payload and use sibling keys for other parameters"
        );
    }
    Ok(())
}

type CallEnvelope = (String, Value, Vec<(String, Value)>, Vec<String>);

fn split_call_envelope(m: &serde_yaml::Mapping) -> Result<CallEnvelope> {
    let mut callee: Option<(String, Value)> = None;
    let mut siblings: Vec<(String, Value)> = Vec::new();
    let mut grants: Vec<String> = Vec::new();
    for (k, v) in m {
        let ks = k.as_str().context("call mapping key must be string")?;
        if ks.starts_with('$') {
            if callee.is_some() {
                bail!("call must have exactly one `$callee` key");
            }
            callee = Some((ks.to_string(), v.clone()));
        } else if ks.starts_with('=') {
            if ks != "=grants" {
                bail!("unexpected `=` key `{ks}` in call site (only `=grants` is supported)");
            }
            let seq = v
                .as_sequence()
                .context("`=grants` must be a sequence of `$grants.<kebab-name>` references")?;
            for item in seq {
                let s = item
                    .as_str()
                    .context("`=grants` entries must be `$grants.<kebab-name>` references")?;
                grants.push(parse_grant_ref(s)?);
            }
        } else {
            siblings.push((ks.to_string(), v.clone()));
        }
    }
    let (call_key, subject) = callee.context("call missing `$callee` key")?;
    Ok((call_key, subject, siblings, grants))
}

fn parse_grant_ref(s: &str) -> Result<String> {
    let Some(name) = s.strip_prefix("$grants.") else {
        bail!("grant references must use `$grants.<kebab-name>`");
    };
    if !is_kebab_case(name) {
        bail!("grant references must use `$grants.<kebab-name>`");
    }
    Ok(name.to_string())
}

fn reject_unknown_call_keys(
    function: &FunctionSig,
    call_key: &str,
    subject: &Value,
    siblings: &[(String, Value)],
) -> Result<()> {
    let allowed: HashSet<String> = function
        .type_params
        .iter()
        .chain(function.arg_names.iter())
        .cloned()
        .collect();
    if let Some(m) = subject.as_mapping() {
        if split_call_envelope(m).is_ok() {
            return Ok(());
        }
        if m.keys()
            .all(|k| k.as_str().is_some_and(|ks| ks.starts_with('$')))
        {
            return Ok(());
        }
        if siblings.is_empty() && !function.arg_names.is_empty() {
            let primary = &function.arg_names[0];
            let unknown_count = m
                .keys()
                .filter_map(|k| k.as_str())
                .filter(|ks| !ks.starts_with('$') && !allowed.contains(*ks))
                .count();
            let primary_absent = !m.contains_key(Value::String(primary.clone()));
            if unknown_count == 1 && primary_absent {
                return Ok(());
            }
        }
        if siblings.is_empty()
            && function.arg_names.len() == 1
            && m.len() == 1
            && m.keys()
                .all(|k| k.as_str().is_some_and(|ks| ks.starts_with('$')))
        {
            return Ok(());
        }
        if siblings.is_empty()
            && function.arg_names.len() == 1
            && m.len() == 1
            && m.keys()
                .all(|k| k.as_str().is_some_and(|ks| !ks.starts_with('$')))
        {
            return Ok(());
        }
        for k in m.keys() {
            let ks = k.as_str().context("call mapping key must be string")?;
            if !allowed.contains(ks) {
                bail!("unexpected key `{ks}` in call `{call_key}`");
            }
        }
    }
    for (k, _) in siblings {
        if !allowed.contains(k) {
            bail!("unexpected argument or type parameter `{k}` in call `{call_key}`");
        }
    }
    Ok(())
}

/// When the callee payload is a mapping of only parameter names (no `$...` keys), detect a
/// *partial* arg map (e.g. missing `grant`) before [`merge_call_payload`] nests it incorrectly.
fn report_missing_inline_call_args(
    function: &FunctionSig,
    call_key: &str,
    subject: &Value,
    siblings: &[(String, Value)],
) -> Result<()> {
    if !siblings.is_empty() {
        return Ok(());
    }
    let Some(m) = subject.as_mapping() else {
        return Ok(());
    };
    let allowed: HashSet<String> = function
        .type_params
        .iter()
        .chain(function.arg_names.iter())
        .cloned()
        .collect();
    if m.is_empty()
        || !m
            .keys()
            .all(|k| k.as_str().is_some_and(|s| allowed.contains(s)))
    {
        return Ok(());
    }
    for tp in &function.type_params {
        if !m.contains_key(Value::String(tp.clone())) {
            bail!("missing type argument `{tp}` in call `{call_key}`");
        }
    }
    for n in &function.arg_names {
        if !m.contains_key(Value::String(n.clone())) {
            bail!("missing value argument `{n}` in call `{call_key}`");
        }
    }
    Ok(())
}

fn merge_call_payload(
    function: &FunctionSig,
    subject: &Value,
    siblings: &[(String, Value)],
) -> Result<Value> {
    if function.arg_names.is_empty() {
        if !siblings.is_empty() {
            bail!("unexpected labeled arguments on zero-argument call");
        }
        if !subject.is_null() {
            bail!("zero-arg call payload must be `null`");
        }
        return Ok(Value::Null);
    }
    let primary = &function.arg_names[0];
    if siblings.is_empty() {
        if let Some(subject_map) = subject.as_mapping() {
            let keys: Vec<&str> = subject_map.keys().filter_map(|k| k.as_str()).collect();
            if !keys.is_empty() && keys.iter().all(|k| !k.starts_with('$')) {
                let all_known = keys.iter().all(|k| {
                    function.type_params.iter().any(|p| p == k)
                        || function.arg_names.iter().any(|a| a == k)
                });
                if all_known {
                    return Ok(subject.clone());
                }
                let unknown_keys: Vec<&str> = keys
                    .iter()
                    .copied()
                    .filter(|k| {
                        !function.type_params.iter().any(|p| p == k)
                            && !function.arg_names.iter().any(|a| a == k)
                    })
                    .collect();
                if unknown_keys.len() == 1
                    && !subject_map.contains_key(Value::String(primary.clone()))
                    && keys.iter().all(|k| {
                        unknown_keys.contains(k)
                            || function.type_params.iter().any(|p| p == k)
                            || function.arg_names.iter().skip(1).any(|a| a == k)
                    })
                {
                    let mut map = serde_yaml::Mapping::new();
                    for (k, v) in subject_map {
                        let ks = k.as_str().context("call mapping key must be string")?;
                        if ks == unknown_keys[0] {
                            map.insert(Value::String(primary.clone()), v.clone());
                        } else {
                            map.insert(k.clone(), v.clone());
                        }
                    }
                    return Ok(Value::Mapping(map));
                }
                if function.arg_names.len() == 1 && keys.len() == 1 {
                    let only_value = subject_map
                        .values()
                        .next()
                        .expect("map length checked")
                        .clone();
                    let mut map = serde_yaml::Mapping::new();
                    map.insert(Value::String(primary.clone()), only_value);
                    return Ok(Value::Mapping(map));
                }
            }
        }
    }
    let allowed: HashSet<String> = function
        .type_params
        .iter()
        .chain(function.arg_names.iter().skip(1))
        .cloned()
        .collect();
    let mut map = serde_yaml::Mapping::new();
    map.insert(Value::String(primary.clone()), subject.clone());
    for (k, v) in siblings {
        if k == primary {
            bail!(
                "duplicate primary argument `{k}` (pass it as the callee payload, not a sibling key)"
            );
        }
        if !allowed.contains(k) {
            bail!(
                "unexpected argument or type parameter `{k}` in call to `{}`",
                function.symbol
            );
        }
        let vk = Value::String(k.clone());
        if map.contains_key(&vk) {
            bail!("duplicate key `{k}` in call");
        }
        map.insert(vk, v.clone());
    }
    Ok(Value::Mapping(map))
}

#[allow(clippy::too_many_arguments)]
fn parse_call(
    call_mapping_value: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &HashMap<String, TypeRef>,
    home_module: &str,
    warnings: &mut Vec<String>,
) -> Result<Call> {
    let m = call_mapping_value
        .as_mapping()
        .context("call must be mapping")?;
    let (call_key, subject, siblings, grant_args) = split_call_envelope(m)?;
    let callee_key = match resolve_call_target(&call_key, sigs, home_module) {
        Ok(k) => k,
        Err(direct_err) => {
            reject_iface_nested_call_bundle(
                &call_key,
                &subject,
                &siblings,
                home_module,
                type_aliases,
            )?;
            let merged = build_iface_dispatch_payload(
                &call_key,
                &subject,
                &siblings,
                home_module,
                type_aliases,
            )
            .with_context(|| format!("{direct_err}"))?;
            try_resolve_iface_call(
                &call_key,
                &merged,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home_module,
                warnings,
            )
            .with_context(|| format!("{direct_err}"))?
        }
    };
    let function = sigs
        .get(&callee_key)
        .with_context(|| format!("unknown function `{callee_key}`"))?;

    reject_unknown_call_keys(function, &call_key, &subject, &siblings)?;
    report_missing_inline_call_args(function, &call_key, &subject, &siblings)?;

    let av = merge_call_payload(function, &subject, &siblings)
        .with_context(|| format!("invalid arguments for call `{call_key}`"))?;

    let mut type_args: Vec<TypeRef> = Vec::new();
    let mut subst: HashMap<String, TypeRef> = HashMap::new();
    if !function.type_params.is_empty() {
        let map = av
            .as_mapping()
            .context("generic function call requires a mapping payload with type arguments")?;
        let empty_skeletons: HashMap<String, AliasSkeleton> = HashMap::new();
        for tp in &function.type_params {
            let tv = map
                .get(Value::String(tp.clone()))
                .with_context(|| format!("missing type argument `{tp}` in call `{call_key}`"))?;
            // Type arguments at a call site are concrete types; `$self` makes
            // no sense here, so disallow it.
            let ty = parse_type_ref(tv, &[], &empty_skeletons, warnings, false)?;
            let q = qualify_named_type(&function.alias, ty, type_aliases);
            type_args.push(q.clone());
            subst.insert(tp.clone(), q);
        }
        let allowed: HashSet<String> = function
            .type_params
            .iter()
            .chain(function.arg_names.iter())
            .cloned()
            .collect();
        for k in map.keys() {
            let ks = k.as_str().context("call mapping key must be string")?;
            if !allowed.contains(ks) {
                bail!("unexpected key `{ks}` in call `{call_key}`");
            }
        }
    }

    let args = parse_call_args(
        &av,
        function,
        !function.type_params.is_empty(),
        &call_key,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        locals,
        home_module,
        warnings,
    )?;
    for (idx, expr) in args.iter().enumerate() {
        let expected = substitute_type(&function.arg_types[idx], &subst);
        let Some(actual) = infer_expr_type(expr, constants, locals, type_aliases, enums) else {
            bail!(
                "could not infer type for call `{call_key}` argument `{}` (unbound variable or incompatible sub-expressions)",
                function.arg_names[idx]
            );
        };
        if !type_compatible(&expected, &actual, type_aliases) {
            if crosses_newtype_boundary(&expected, &actual, type_aliases) {
                bail!(
                    "E-NEWTYPE-001: implicit coercion between `$newtype` and its inner type is forbidden in call `{call_key}` arg `{}`; use `$cast` (expected {:?}, got {:?})",
                    function.arg_names[idx],
                    expected,
                    actual
                );
            }
            bail!(
                "type mismatch in call `{call_key}` arg `{}`: expected {:?}, got {:?}",
                function.arg_names[idx],
                expected,
                actual
            );
        }
    }
    validate_call_grants(function, &grant_args, &call_key)?;
    Ok(Call {
        callee_key,
        type_args,
        args,
        grant_args,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_call_args(
    av: &Value,
    function: &FunctionSig,
    generic_call: bool,
    call_key: &str,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &HashMap<String, TypeRef>,
    home_module: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Expr>> {
    let arg_names = &function.arg_names;
    if arg_names.is_empty() {
        if av.is_null() {
            return Ok(Vec::new());
        }
        if let Some(m) = av.as_mapping() {
            if !m.is_empty() {
                bail!("expected no args");
            }
        }
        bail!("zero-arg call payload must be `null`");
    }
    if !generic_call && arg_names.len() == 1 && !av.is_mapping() {
        return Ok(vec![parse_expr(
            av,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            locals,
            home_module,
            warnings,
        )?]);
    }
    if !generic_call && arg_names.len() == 1 {
        if let Some(map) = av.as_mapping() {
            let only_key_constructor = map.len() == 1
                && map
                    .iter()
                    .next()
                    .and_then(|(k, _)| k.as_str())
                    .map(|s| s.starts_with('$'))
                    .unwrap_or(false);
            if only_key_constructor {
                return Ok(vec![parse_expr(
                    av,
                    sigs,
                    constants,
                    type_aliases,
                    enums,
                    impls,
                    locals,
                    home_module,
                    warnings,
                )?]);
            }
        }
    }
    let map = av
        .as_mapping()
        .context("expected mapping arguments for this call")?;
    if !generic_call {
        let allowed: HashSet<&str> = arg_names.iter().map(String::as_str).collect();
        for k in map.keys() {
            let ks = k.as_str().context("call argument key must be string")?;
            if !allowed.contains(ks) {
                bail!("unexpected key `{ks}` in call `{call_key}`");
            }
        }
    }
    let mut out = Vec::with_capacity(arg_names.len());
    for n in arg_names {
        let v = map
            .get(Value::String(n.clone()))
            .with_context(|| format!("missing value argument `{n}`"))?;
        maybe_warn_kebab(n, "argument key", warnings);
        out.push(parse_expr(
            v,
            sigs,
            constants,
            type_aliases,
            enums,
            impls,
            locals,
            home_module,
            warnings,
        )?);
    }
    Ok(out)
}

fn validate_call_grants(
    function: &FunctionSig,
    grant_args: &[String],
    call_key: &str,
) -> Result<()> {
    let mut seen = HashSet::new();
    for grant in grant_args {
        if !seen.insert(grant.as_str()) {
            bail!("duplicate grant `{grant}` in `=grants` for call `{call_key}`");
        }
        if !function.grant_decls.iter().any(|d| d.name == *grant) {
            bail!("unknown grant `{grant}` in `=grants` for call `{call_key}`");
        }
    }
    for decl in &function.grant_decls {
        if decl.requirement == GrantRequirement::Mandatory
            && !grant_args.iter().any(|g| g == &decl.name)
        {
            bail!(
                "missing mandatory grant `{}` in `=grants` for call `{call_key}`",
                decl.name
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_expr(
    v: &Value,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &HashMap<String, TypeRef>,
    home_module: &str,
    warnings: &mut Vec<String>,
) -> Result<Expr> {
    if let Some(m) = v.as_mapping() {
        if looks_like_call(v, sigs, home_module)
            || looks_like_iface_call(v, home_module, type_aliases)
        {
            let call = parse_call(
                v,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home_module,
                warnings,
            )?;
            let sig = sigs
                .get(&call.callee_key)
                .context("internal: missing callee after expression parse_call")?;
            let return_type = substituted_return_type(sig, &call.type_args);
            if return_type == TypeRef::Void {
                bail!("void function call cannot be used as an expression");
            }
            return Ok(Expr::Call {
                call: Box::new(call),
                return_type,
            });
        }

        fn mapping_keys_exactly(m: &serde_yaml::Mapping, keys: &[&str]) -> bool {
            m.len() == keys.len()
                && keys
                    .iter()
                    .all(|k| m.contains_key(Value::String((*k).into())))
        }

        if mapping_keys_exactly(m, &["$cast", "into"]) {
            let from_v = map_get_str(m, "$cast")
                .context("E-CAST-002: `$cast` missing subject expression")?;
            let into_v =
                map_get_str(m, "into").context("E-CAST-002: `$cast` missing `into` type")?;
            let from = parse_expr(
                from_v,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home_module,
                warnings,
            )?;
            let source = infer_expr_type(&from, constants, locals, type_aliases, enums)
                .context("E-CAST-001: could not infer `$cast` subject type")?;
            let empty_skeletons: HashMap<String, AliasSkeleton> = HashMap::new();
            let target = parse_type_ref(into_v, &[], &empty_skeletons, warnings, false)
                .context("E-CAST-002: invalid `$cast.into` type")?;
            let target = qualify_named_type(home_module, target, type_aliases);
            if capability_alias(&target, type_aliases).is_some()
                || matches!(target, TypeRef::Capability { .. })
            {
                bail!("E-CAP-001: capability values are runtime-minted and cannot be created with `$cast`");
            }
            if !valid_cast_path(&source, &target, type_aliases) {
                bail!(
                    "E-CAST-001: no valid cast path from {:?} to {:?}",
                    source,
                    target
                );
            }
            return Ok(Expr::Cast {
                from: Box::new(from),
                target,
            });
        }
        if mapping_keys_exactly(m, &["$policy.narrow", "into"]) {
            let from = parse_expr(
                map_get_str(m, "$policy.narrow").context("`$policy.narrow` missing subject")?,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home_module,
                warnings,
            )?;
            let source = infer_expr_type(&from, constants, locals, type_aliases, enums)
                .context("could not infer `$policy.narrow` subject type")?;
            let empty_skeletons: HashMap<String, AliasSkeleton> = HashMap::new();
            let target = parse_type_ref(
                map_get_str(m, "into").context("`$policy.narrow` missing `into` type")?,
                &[],
                &empty_skeletons,
                warnings,
                false,
            )?;
            let target = qualify_named_type(home_module, target, type_aliases);
            if !policy_type_is_subset(&source, &target, type_aliases) {
                bail!("policy narrowing cannot widen authority");
            }
            let target = policy_body(&target, type_aliases)
                .cloned()
                .map(TypeRef::Policy)
                .unwrap_or(target);
            return Ok(Expr::PolicyNarrow {
                from: Box::new(from),
                target,
            });
        }
        if mapping_keys_exactly(m, &["$if", "then", "else"]) {
            let cond = parse_expr(
                map_get_str(m, "$if").context("$if missing condition")?,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home_module,
                warnings,
            )?;
            let then_e = parse_expr(
                map_get_str(m, "then").context("$if missing `then`")?,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home_module,
                warnings,
            )?;
            let else_e = parse_expr(
                map_get_str(m, "else").context("$if missing `else`")?,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                locals,
                home_module,
                warnings,
            )?;
            return Ok(Expr::If {
                cond: Box::new(cond),
                then_e: Box::new(then_e),
                else_e: Box::new(else_e),
            });
        }

        if m.len() == 1 {
            let (k, payload_v) = m.iter().next().expect("one key");
            if let Some(constructor) = k.as_str() {
                if constructor == "$record" {
                    let payload = payload_v
                        .as_mapping()
                        .context("`$record` value must be a mapping")?;
                    let mut fields = BTreeMap::new();
                    for (fk, fv) in payload {
                        let name = fk.as_str().context("$record value key must be string")?;
                        maybe_warn_kebab(name, "record field", warnings);
                        let expr = parse_expr(
                            fv,
                            sigs,
                            constants,
                            type_aliases,
                            enums,
                            impls,
                            locals,
                            home_module,
                            warnings,
                        )?;
                        if fields.insert(name.to_string(), expr).is_some() {
                            bail!("duplicate $record value field `{name}`");
                        }
                    }
                    return Ok(Expr::Record(fields));
                }
                if constructor == "$tuple" {
                    let payload = payload_v
                        .as_sequence()
                        .context("`$tuple` value must be a sequence")?;
                    let mut items = Vec::with_capacity(payload.len());
                    for item in payload {
                        items.push(parse_expr(
                            item,
                            sigs,
                            constants,
                            type_aliases,
                            enums,
                            impls,
                            locals,
                            home_module,
                            warnings,
                        )?);
                    }
                    return Ok(Expr::Tuple(items));
                }
                if constructor == "$array" {
                    let payload = payload_v
                        .as_sequence()
                        .context("`$array` value must be a sequence")?;
                    let mut items = Vec::with_capacity(payload.len());
                    for item in payload {
                        items.push(parse_expr(
                            item,
                            sigs,
                            constants,
                            type_aliases,
                            enums,
                            impls,
                            locals,
                            home_module,
                            warnings,
                        )?);
                    }
                    return Ok(Expr::Array(items));
                }
                if constructor == "$map" {
                    let payload = payload_v
                        .as_sequence()
                        .context("`$map` value must be a sequence of `{key, value}` entries")?;
                    let mut items = Vec::with_capacity(payload.len());
                    for entry in payload {
                        let entry_m = entry
                            .as_mapping()
                            .context("$map value entries must be mappings")?;
                        let key_v =
                            map_get_str(entry_m, "key").context("$map entry missing key")?;
                        let value_v =
                            map_get_str(entry_m, "value").context("$map entry missing value")?;
                        items.push((
                            parse_expr(
                                key_v,
                                sigs,
                                constants,
                                type_aliases,
                                enums,
                                impls,
                                locals,
                                home_module,
                                warnings,
                            )?,
                            parse_expr(
                                value_v,
                                sigs,
                                constants,
                                type_aliases,
                                enums,
                                impls,
                                locals,
                                home_module,
                                warnings,
                            )?,
                        ));
                    }
                    return Ok(Expr::Map(items));
                }
                if constructor == "$cast" {
                    bail!(
                        "E-CAST-002: `$cast` must use `$cast: <expr>` with sibling `into: <type>`"
                    );
                }
                if constructor.starts_with('$') {
                    let (enum_key, tag) = resolve_enum_tag_ref(constructor, home_module, enums)?;
                    maybe_warn_kebab(&tag, "enum tag", warnings);
                    let enum_def = enums.get(&enum_key).with_context(|| {
                        format!("unknown enum `{enum_key}` in constructor `{constructor}`")
                    })?;
                    let payload_ty = enum_def.tags.get(&tag).with_context(|| {
                        format!("unknown enum tag `{tag}` for enum `{enum_key}`")
                    })?;
                    return if *payload_ty == TypeRef::Void {
                        if !payload_v.is_null() {
                            bail!(
                                "constructor `{constructor}` tag `{tag}` does not take payload; use `null`"
                            );
                        }
                        Ok(Expr::EnumConstructor {
                            enum_key,
                            tag: tag.to_string(),
                            payload: None,
                        })
                    } else {
                        let payload_expr = parse_expr(
                            payload_v,
                            sigs,
                            constants,
                            type_aliases,
                            enums,
                            impls,
                            locals,
                            home_module,
                            warnings,
                        )?;
                        if let Some(actual_ty) =
                            infer_expr_type(&payload_expr, constants, locals, type_aliases, enums)
                        {
                            if !type_compatible(payload_ty, &actual_ty, type_aliases) {
                                bail!(
                                    "constructor `{constructor}` payload type mismatch: expected {:?}, got {:?}",
                                    payload_ty,
                                    actual_ty
                                );
                            }
                        }
                        Ok(Expr::EnumConstructor {
                            enum_key,
                            tag: tag.to_string(),
                            payload: Some(Box::new(payload_expr)),
                        })
                    };
                }
            }
        }
        bail!("unsupported expression mapping");
    }

    if v.is_null() {
        return Ok(Expr::Value(RuntimeValue::Void));
    }
    if let Some(i) = v.as_i64() {
        return Ok(Expr::Value(RuntimeValue::Int(i)));
    }
    if let Some(f) = v.as_f64() {
        return Ok(Expr::Value(RuntimeValue::Float(f)));
    }
    if let Some(b) = v.as_bool() {
        return Ok(Expr::Value(RuntimeValue::Bool(b)));
    }
    if let Some(s) = v.as_str() {
        if let Some(var) = s.strip_prefix('$') {
            maybe_warn_kebab_qualified(var, "symbol reference", warnings);
            if let Some(grant_name) = var.strip_prefix("grants.") {
                if grant_name.is_empty() || grant_name.contains('.') {
                    bail!("grant references must use `$grants.<kebab-name>`");
                }
            }
            if let Ok((enum_key, tag)) = resolve_enum_tag_ref(s, home_module, enums) {
                if let Some(enum_def) = enums.get(&enum_key) {
                    let payload_ty = enum_def.tags.get(tag.as_str()).with_context(|| {
                        format!("unknown enum tag `{tag}` for enum `{enum_key}`")
                    })?;
                    if *payload_ty == TypeRef::Void {
                        return Ok(Expr::EnumConstructor {
                            enum_key,
                            tag,
                            payload: None,
                        });
                    }
                    bail!("constructor `{s}` requires payload; use mapping form `{{{s}: ...}}`");
                }
            }
            if let Some(c) = constants.get(var) {
                return Ok(Expr::Value(c.clone()));
            }
            return Ok(Expr::VarRef(var.to_string()));
        }
        return Ok(Expr::Value(RuntimeValue::Str(s.to_string())));
    }
    bail!("unsupported expression: expected null/number/string/$var or constructor")
}

fn is_void_args(v: &Value) -> bool {
    matches!(v.as_str(), Some("$void"))
}

/// Build the internal `args:` mapping for [`FunctionSig`] from labeled `$function` syntax.
fn build_function_args_mapping(
    primary_name: &str,
    first_arg_type: &Value,
    rest_args: Option<&Value>,
) -> Result<Value> {
    if is_void_args(first_arg_type) {
        if rest_args.is_some() {
            bail!("E-ONE-001: `$function: $void` is only valid for a true zero-argument function");
        }
        return Ok(Value::String("$void".into()));
    }
    let mut combined = serde_yaml::Mapping::new();
    combined.insert(Value::String(primary_name.into()), first_arg_type.clone());
    if let Some(rest) = rest_args {
        let rm = rest.as_mapping().context("`args` must be a mapping")?;
        for (k, v) in rm {
            let name = k.as_str().context("`args` key must be string")?;
            if name == primary_name {
                bail!(
                    "`args` cannot repeat the primary parameter `{primary_name}` (it is already `$function: <type>`)"
                );
            }
            if combined.contains_key(k) {
                bail!("duplicate argument `{name}` in `args`");
            }
            combined.insert(k.clone(), v.clone());
        }
    }
    Ok(Value::Mapping(combined))
}

/// Resolves `$function` fields from labeled syntax: `$function: <first-arg-type>` plus optional
/// sibling `args`, and required `return` / `do`.
fn resolve_function_envelope_fields(
    env: &DefEnvelope<'_>,
    primary_name: &str,
) -> Result<(Value, Value, Value, Vec<GrantDecl>)> {
    let (primary, first_arg_ty) = canonical_function_first_arg(env.form_value, primary_name)?;
    let merged = build_function_args_mapping(&primary, first_arg_ty, env.function_args)?;
    let ret = env
        .function_return
        .context("missing `return` on `$function`")?
        .clone();
    let d = env
        .function_do
        .context("missing `do` on `$function`")?
        .clone();
    if record_has_grants_arg(&merged) {
        bail!(
            "grant arguments moved out of `args`; declare function `grants:` and access them as `$grants.<name>`"
        );
    }
    let grant_decls = parse_grant_decls(env.function_grants)?;
    Ok((merged, ret, d, grant_decls))
}

fn record_has_grants_arg(v: &Value) -> bool {
    v.as_mapping()
        .is_some_and(|m| m.contains_key(Value::String("grants".to_string())))
}

fn parse_grant_decls(v: Option<&Value>) -> Result<Vec<GrantDecl>> {
    let Some(v) = v else {
        return Ok(Vec::new());
    };
    let m = v.as_mapping().context(
        "`grants` must be a mapping of grant name -> $security.grant.mandatory|optional",
    )?;
    let mut out = Vec::with_capacity(m.len());
    let mut seen = HashSet::new();
    for (k, v) in m {
        let name = k.as_str().context("grant name must be string")?.to_string();
        if !is_kebab_case(&name) {
            bail!("grant names must be kebab-case, got `{name}`");
        }
        if !seen.insert(name.clone()) {
            bail!("duplicate grant `{name}`");
        }
        let s = v
            .as_str()
            .with_context(|| format!("grant `{name}` requirement must be a scalar enum tag"))?;
        let requirement = if s.ends_with(".mandatory") || s == "$security.grant.mandatory" {
            GrantRequirement::Mandatory
        } else if s.ends_with(".optional") || s == "$security.grant.optional" {
            GrantRequirement::Optional
        } else {
            bail!(
                "grant `{name}` must be `$security.grant.mandatory` or `$security.grant.optional`"
            );
        };
        out.push(GrantDecl { name, requirement });
    }
    Ok(out)
}

/// Peel the first parameter from labeled `$function: …` surface syntax.
///
/// When the payload is the scalar `$self`, the parameter name is `scalar_self_primary`
/// (`self` for `=defs`, `subject` for module-level functions).
///
/// For non-`$void`/`$self`, `$function` must be a mapping with exactly one key that does not start with `$`,
/// treated as `{ <param-name>: <type> }`.
fn canonical_function_first_arg<'a>(
    form_value: &'a Value,
    scalar_self_primary: &str,
) -> Result<(String, &'a Value)> {
    if is_void_args(form_value) {
        return Ok((String::new(), form_value));
    }
    if form_value.as_str() == Some("$self") {
        return Ok((scalar_self_primary.to_string(), form_value));
    }
    let m = form_value
        .as_mapping()
        .context("E-ONE-001: non-zero `$function` must declare one labeled primary argument")?;
    if m.len() != 1 {
        bail!("E-ONE-001: `$function` must declare exactly one labeled primary argument");
    }
    let (k, v) = m.iter().next().expect("len checked");
    let key = k
        .as_str()
        .context("E-ONE-001: `$function` primary argument name must be a string")?;
    if key.starts_with('$') {
        bail!("E-ONE-001: `$function` primary argument must use a label, not `{key}`");
    }
    Ok((key.to_string(), v))
}

/// Primary parameter name for an `=impl` `$function` envelope: the `$self` field if present,
/// otherwise the first record field (declaration order in the stored record).
fn iface_impl_primary_field_name(expected_fn_type: &TypeRef) -> Result<String> {
    let TypeRef::FnType { args, .. } = expected_fn_type else {
        bail!("internal: expected interface method `$fn-type`");
    };
    let TypeRef::Record(rec) = args.as_ref() else {
        bail!("internal: interface method args must be a record");
    };
    if let Some((n, _)) = rec.iter().find(|(_, t)| matches!(t, TypeRef::SelfType)) {
        return Ok(n.clone());
    }
    // After `$self` substitution the receiver is no longer `SelfType`, but the field is
    // still conventionally named `self` — prefer that over lexicographic-first keys like `b`.
    if rec.contains_key("self") {
        return Ok("self".to_string());
    }
    rec.keys()
        .next()
        .cloned()
        .context("interface method must declare at least one argument")
}

fn infer_expr_type(
    expr: &Expr,
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    match expr {
        Expr::Value(RuntimeValue::Bool(_)) => Some(TypeRef::Bool),
        Expr::Value(RuntimeValue::Int(_)) => Some(TypeRef::Int64),
        Expr::Value(RuntimeValue::Float(_)) => Some(TypeRef::Float64),
        Expr::Value(RuntimeValue::Str(_)) => Some(TypeRef::Str),
        Expr::Value(RuntimeValue::Array(items)) => {
            infer_array_type(items, constants, locals, aliases, enums)
        }
        Expr::Value(RuntimeValue::Record(fields)) => fields
            .iter()
            .map(|(k, v)| {
                infer_expr_type(&Expr::Value(v.clone()), constants, locals, aliases, enums)
                    .map(|t| (k.clone(), t))
            })
            .collect::<Option<BTreeMap<_, _>>>()
            .map(TypeRef::Record),
        Expr::Value(RuntimeValue::Tuple(items)) => items
            .iter()
            .map(|v| infer_expr_type(&Expr::Value(v.clone()), constants, locals, aliases, enums))
            .collect::<Option<Vec<_>>>()
            .map(TypeRef::Tuple),
        Expr::Value(RuntimeValue::Map(items)) => {
            infer_map_type(items, constants, locals, aliases, enums)
        }
        Expr::Value(RuntimeValue::Typed { type_ref, .. }) => Some(type_ref.clone()),
        Expr::Value(RuntimeValue::Capability(grant)) => {
            Some(TypeRef::Named(grant.type_key.clone()))
        }
        Expr::Value(RuntimeValue::Policy(value)) => Some(TypeRef::Policy(value.policy.clone())),
        Expr::Value(RuntimeValue::GrantToken(_)) => Some(TypeRef::GrantToken),
        Expr::Value(RuntimeValue::Void) => Some(TypeRef::Void),
        Expr::Value(RuntimeValue::Enum { enum_key, .. }) => Some(TypeRef::Named(enum_key.clone())),
        Expr::VarRef(v) => locals.get(v).cloned().or_else(|| {
            constants.get(v).and_then(|rv| {
                infer_expr_type(&Expr::Value(rv.clone()), constants, locals, aliases, enums)
            })
        }),
        Expr::Call { return_type, .. } => Some(return_type.clone()),
        Expr::Cast { target, .. } => Some(target.clone()),
        Expr::PolicyNarrow { target, .. } => Some(target.clone()),
        Expr::Record(fields) => fields
            .iter()
            .map(|(k, v)| {
                infer_expr_type(v, constants, locals, aliases, enums).map(|t| (k.clone(), t))
            })
            .collect::<Option<BTreeMap<_, _>>>()
            .map(TypeRef::Record),
        Expr::Tuple(items) => items
            .iter()
            .map(|v| infer_expr_type(v, constants, locals, aliases, enums))
            .collect::<Option<Vec<_>>>()
            .map(TypeRef::Tuple),
        Expr::Array(items) => infer_expr_array_type(items, constants, locals, aliases, enums),
        Expr::Map(items) => infer_expr_map_type(items, constants, locals, aliases, enums),
        Expr::EnumConstructor {
            enum_key,
            tag,
            payload,
        } => {
            let def = enums.get(enum_key)?;
            instantiated_type_for_constructor(
                enum_key,
                def,
                tag,
                payload.as_deref(),
                constants,
                locals,
                aliases,
                enums,
            )
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => {
            let cond_ty = infer_expr_type(cond, constants, locals, aliases, enums)?;
            if cond_ty != TypeRef::Bool {
                return None;
            }
            let t_then = infer_expr_type(then_e, constants, locals, aliases, enums)?;
            let t_else = infer_expr_type(else_e, constants, locals, aliases, enums)?;
            if type_compatible(&t_then, &t_else, aliases) {
                Some(t_then)
            } else if type_compatible(&t_else, &t_then, aliases) {
                Some(t_else)
            } else {
                None
            }
        }
    }
}

fn infer_array_type(
    items: &[RuntimeValue],
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    let first = items.first()?;
    let first_ty = infer_expr_type(
        &Expr::Value(first.clone()),
        constants,
        locals,
        aliases,
        enums,
    )?;
    for item in &items[1..] {
        let ty = infer_expr_type(
            &Expr::Value(item.clone()),
            constants,
            locals,
            aliases,
            enums,
        )?;
        if !type_compatible(&first_ty, &ty, aliases) {
            return None;
        }
    }
    Some(TypeRef::Array(Box::new(first_ty)))
}

fn infer_expr_array_type(
    items: &[Expr],
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    let first = items.first()?;
    let first_ty = infer_expr_type(first, constants, locals, aliases, enums)?;
    for item in &items[1..] {
        let ty = infer_expr_type(item, constants, locals, aliases, enums)?;
        if !type_compatible(&first_ty, &ty, aliases) {
            return None;
        }
    }
    Some(TypeRef::Array(Box::new(first_ty)))
}

fn infer_map_type(
    items: &[(RuntimeValue, RuntimeValue)],
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    let (first_k, first_v) = items.first()?;
    let key_ty = infer_expr_type(
        &Expr::Value(first_k.clone()),
        constants,
        locals,
        aliases,
        enums,
    )?;
    let value_ty = infer_expr_type(
        &Expr::Value(first_v.clone()),
        constants,
        locals,
        aliases,
        enums,
    )?;
    for (k, v) in &items[1..] {
        let kt = infer_expr_type(&Expr::Value(k.clone()), constants, locals, aliases, enums)?;
        let vt = infer_expr_type(&Expr::Value(v.clone()), constants, locals, aliases, enums)?;
        if !type_compatible(&key_ty, &kt, aliases) || !type_compatible(&value_ty, &vt, aliases) {
            return None;
        }
    }
    Some(TypeRef::Map {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    })
}

fn infer_expr_map_type(
    items: &[(Expr, Expr)],
    constants: &HashMap<String, RuntimeValue>,
    locals: &HashMap<String, TypeRef>,
    aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
) -> Option<TypeRef> {
    let (first_k, first_v) = items.first()?;
    let key_ty = infer_expr_type(first_k, constants, locals, aliases, enums)?;
    let value_ty = infer_expr_type(first_v, constants, locals, aliases, enums)?;
    for (k, v) in &items[1..] {
        let kt = infer_expr_type(k, constants, locals, aliases, enums)?;
        let vt = infer_expr_type(v, constants, locals, aliases, enums)?;
        if !type_compatible(&key_ty, &kt, aliases) || !type_compatible(&value_ty, &vt, aliases) {
            return None;
        }
    }
    Some(TypeRef::Map {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    })
}

fn parse_pattern(
    v: &Value,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    module_scope: &str,
    warnings: &mut Vec<String>,
) -> Result<Pattern> {
    if let Some(m) = v.as_mapping() {
        if m.len() != 1 {
            bail!("pattern mapping must contain exactly one key");
        }
        let (k, payload_v) = m.iter().next().expect("one key");
        let key = k.as_str().context("pattern key must be string")?;
        match key {
            "$wildcard" => return Ok(Pattern::Wildcard),
            "$bind" => {
                let name = payload_v.as_str().context("$bind pattern expects a name")?;
                maybe_warn_kebab(name, "pattern bind", warnings);
                return Ok(Pattern::Bind(name.to_string()));
            }
            "$record" => {
                let fields_v = payload_v
                    .as_mapping()
                    .context("$record pattern must be mapping")?;
                let mut fields = BTreeMap::new();
                for (fk, fv) in fields_v {
                    let name = fk.as_str().context("$record pattern key must be string")?;
                    maybe_warn_kebab(name, "record pattern field", warnings);
                    let pat = parse_pattern(fv, type_aliases, enums, module_scope, warnings)?;
                    if fields.insert(name.to_string(), pat).is_some() {
                        bail!("duplicate $record pattern field `{name}`");
                    }
                }
                return Ok(Pattern::Record(fields));
            }
            "$tuple" => {
                let items_v = payload_v
                    .as_sequence()
                    .context("$tuple pattern must be sequence")?;
                return Ok(Pattern::Tuple(
                    items_v
                        .iter()
                        .map(|item| {
                            parse_pattern(item, type_aliases, enums, module_scope, warnings)
                        })
                        .collect::<Result<Vec<_>>>()?,
                ));
            }
            "$array" => {
                let items_v = payload_v
                    .as_sequence()
                    .context("$array pattern must be sequence")?;
                return Ok(Pattern::Array(
                    items_v
                        .iter()
                        .map(|item| {
                            parse_pattern(item, type_aliases, enums, module_scope, warnings)
                        })
                        .collect::<Result<Vec<_>>>()?,
                ));
            }
            "$map" => {
                let entries_v = payload_v
                    .as_sequence()
                    .context("$map pattern must be a sequence of `{key, value}` entries")?;
                let mut entries = Vec::with_capacity(entries_v.len());
                for entry_v in entries_v {
                    let entry = entry_v
                        .as_mapping()
                        .context("$map pattern entry must be mapping")?;
                    let key_v =
                        map_get_str(entry, "key").context("$map pattern entry missing key")?;
                    let value_v =
                        map_get_str(entry, "value").context("$map pattern entry missing value")?;
                    entries.push((
                        parse_pattern(key_v, type_aliases, enums, module_scope, warnings)?,
                        parse_pattern(value_v, type_aliases, enums, module_scope, warnings)?,
                    ));
                }
                return Ok(Pattern::Map(entries));
            }
            "$newtype" => {
                let m = payload_v
                    .as_mapping()
                    .context("$newtype pattern must be mapping")?;
                let type_v = map_get_str(m, "type").context("$newtype pattern missing type")?;
                let inner_v = map_get_str(m, "inner").context("$newtype pattern missing inner")?;
                let empty_skeletons = HashMap::new();
                let type_ref = parse_type_ref(type_v, &[], &empty_skeletons, warnings, false)?;
                return Ok(Pattern::Newtype {
                    type_ref: qualify_named_type(module_scope, type_ref, type_aliases),
                    inner: Box::new(parse_pattern(
                        inner_v,
                        type_aliases,
                        enums,
                        module_scope,
                        warnings,
                    )?),
                });
            }
            "$interface" => {
                let empty_skeletons = HashMap::new();
                let type_ref = parse_type_ref(payload_v, &[], &empty_skeletons, warnings, false)?;
                return Ok(Pattern::Interface(qualify_named_type(
                    module_scope,
                    type_ref,
                    type_aliases,
                )));
            }
            _ if key.starts_with('$') => {
                let (enum_key, tag) = resolve_enum_tag_ref(key, module_scope, enums)?;
                maybe_warn_kebab(&tag, "enum pattern tag", warnings);
                let payload = if payload_v.is_null() {
                    None
                } else {
                    Some(Box::new(parse_pattern(
                        payload_v,
                        type_aliases,
                        enums,
                        module_scope,
                        warnings,
                    )?))
                };
                return Ok(Pattern::Enum {
                    enum_key,
                    tag,
                    payload,
                });
            }
            _ => bail!("unknown pattern form `{key}`"),
        }
    }
    if v.is_null() {
        return Ok(Pattern::Literal(RuntimeValue::Void));
    }
    if let Some(b) = v.as_bool() {
        return Ok(Pattern::Literal(RuntimeValue::Bool(b)));
    }
    if let Some(i) = v.as_i64() {
        return Ok(Pattern::Literal(RuntimeValue::Int(i)));
    }
    if let Some(f) = v.as_f64() {
        return Ok(Pattern::Literal(RuntimeValue::Float(f)));
    }
    if let Some(s) = v.as_str() {
        return Ok(Pattern::Literal(RuntimeValue::Str(s.to_string())));
    }
    bail!("unsupported pattern")
}

fn enum_target_def<'a>(
    target_ty: &TypeRef,
    enums: &'a HashMap<String, EnumDef>,
) -> Option<(String, &'a EnumDef)> {
    let enum_key = match target_ty {
        TypeRef::Instantiated { base, .. } | TypeRef::Named(base) => base,
        _ => return None,
    };
    enums
        .get(enum_key)
        .or_else(|| {
            enums
                .iter()
                .find(|(k, _)| strip_module_prefix(k) == strip_module_prefix(enum_key))
                .map(|(_, v)| v)
        })
        .map(|def| (enum_key.clone(), def))
}

#[allow(clippy::too_many_arguments)]
fn validate_pattern(
    pattern: &Pattern,
    target_ty: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    locals: &mut HashMap<String, TypeRef>,
    covered_enum_tags: &mut HashSet<String>,
    has_wildcard: &mut bool,
) -> Result<()> {
    match pattern {
        Pattern::Wildcard => {
            *has_wildcard = true;
            Ok(())
        }
        Pattern::Bind(name) => {
            locals.insert(name.clone(), target_ty.clone());
            Ok(())
        }
        Pattern::Literal(value) => {
            let lit_ty = infer_expr_type(
                &Expr::Value(value.clone()),
                &HashMap::new(),
                &HashMap::new(),
                aliases,
                enums,
            )
            .context("could not infer literal pattern type")?;
            if type_compatible(target_ty, &lit_ty, aliases) {
                Ok(())
            } else {
                bail!("literal pattern type mismatch: target {target_ty:?}, pattern {lit_ty:?}")
            }
        }
        Pattern::Enum {
            enum_key,
            tag,
            payload,
        } => {
            let (base, type_args): (&String, Vec<TypeRef>) = match target_ty {
                TypeRef::Instantiated { base, type_args } => (base, type_args.clone()),
                TypeRef::Named(base) => (base, Vec::new()),
                _ => bail!("enum pattern requires enum target, got {target_ty:?}"),
            };
            if strip_module_prefix(base) != strip_module_prefix(enum_key) {
                bail!("enum pattern `{enum_key}.{tag}` does not match target `{base}`");
            }
            let enum_def = enums
                .get(enum_key)
                .or_else(|| enums.get(base))
                .with_context(|| format!("unknown enum `{enum_key}` in pattern"))?;
            let payload_ty = enum_def
                .tags
                .get(tag)
                .with_context(|| format!("unknown enum tag `{tag}`"))?;
            covered_enum_tags.insert(tag.clone());
            let mut subst = HashMap::new();
            for (p, a) in enum_def.type_params.iter().zip(type_args) {
                subst.insert(p.clone(), a);
            }
            let payload_ty = substitute_type(payload_ty, &subst);
            match (payload_ty == TypeRef::Void, payload) {
                (true, None) => Ok(()),
                (true, Some(_)) => bail!("enum pattern `{tag}` has payload for `$void` tag"),
                (false, None) => bail!("enum pattern `{tag}` must include payload pattern"),
                (false, Some(p)) => validate_pattern(
                    p,
                    &payload_ty,
                    aliases,
                    enums,
                    locals,
                    covered_enum_tags,
                    has_wildcard,
                ),
            }
        }
        Pattern::Record(fields) => {
            let target_n = normalize_type_ref(target_ty, aliases);
            let TypeRef::Record(target_fields) = target_n else {
                bail!("$record pattern requires record target, got {target_ty:?}");
            };
            for (name, pat) in fields {
                let field_ty = target_fields
                    .get(name)
                    .with_context(|| format!("record target has no field `{name}`"))?;
                validate_pattern(
                    pat,
                    field_ty,
                    aliases,
                    enums,
                    locals,
                    covered_enum_tags,
                    has_wildcard,
                )?;
            }
            Ok(())
        }
        Pattern::Tuple(items) => {
            let target_n = normalize_type_ref(target_ty, aliases);
            let TypeRef::Tuple(target_items) = target_n else {
                bail!("$tuple pattern requires tuple target, got {target_ty:?}");
            };
            if items.len() != target_items.len() {
                bail!("$tuple pattern length mismatch");
            }
            for (pat, item_ty) in items.iter().zip(target_items.iter()) {
                validate_pattern(
                    pat,
                    item_ty,
                    aliases,
                    enums,
                    locals,
                    covered_enum_tags,
                    has_wildcard,
                )?;
            }
            Ok(())
        }
        Pattern::Array(items) => {
            let target_n = normalize_type_ref(target_ty, aliases);
            let TypeRef::Array(item_ty) = target_n else {
                bail!("$array pattern requires array target, got {target_ty:?}");
            };
            for pat in items {
                validate_pattern(
                    pat,
                    &item_ty,
                    aliases,
                    enums,
                    locals,
                    covered_enum_tags,
                    has_wildcard,
                )?;
            }
            Ok(())
        }
        Pattern::Map(entries) => {
            let target_n = normalize_type_ref(target_ty, aliases);
            let TypeRef::Map { key, value } = target_n else {
                bail!("$map pattern requires map target, got {target_ty:?}");
            };
            for (kp, vp) in entries {
                validate_pattern(
                    kp,
                    &key,
                    aliases,
                    enums,
                    locals,
                    covered_enum_tags,
                    has_wildcard,
                )?;
                validate_pattern(
                    vp,
                    &value,
                    aliases,
                    enums,
                    locals,
                    covered_enum_tags,
                    has_wildcard,
                )?;
            }
            Ok(())
        }
        Pattern::Newtype { type_ref, inner } => {
            if !type_compatible(type_ref, target_ty, aliases) {
                bail!("$newtype pattern type mismatch: target {target_ty:?}, pattern {type_ref:?}");
            }
            let inner_ty = newtype_inner(type_ref, aliases)
                .cloned()
                .context("$newtype pattern type is not a newtype")?;
            validate_pattern(
                inner,
                &inner_ty,
                aliases,
                enums,
                locals,
                covered_enum_tags,
                has_wildcard,
            )
        }
        Pattern::Interface(iface) => {
            if !is_interface_bound(iface, aliases) {
                bail!("$interface pattern requires an interface type, got {iface:?}");
            }
            Ok(())
        }
    }
}

fn is_literal_singleton_pattern(pattern: &Pattern, _target_ty: &TypeRef) -> bool {
    matches!(pattern, Pattern::Literal(_))
}

#[allow(clippy::too_many_arguments)]
fn parse_match_statement(
    stmt: &serde_yaml::Mapping,
    sigs: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, RuntimeValue>,
    type_aliases: &HashMap<String, TypeAlias>,
    enums: &HashMap<String, EnumDef>,
    impls: &HashMap<ImplKey, ImplBody>,
    locals: &HashMap<String, TypeRef>,
    warnings: &mut Vec<String>,
    fn_ctx: Option<&UserFnContext>,
) -> Result<Statement> {
    let home = stmt_home_module(fn_ctx);
    let match_v = map_get_str(stmt, "$match").context("$match missing subject value")?;
    if let Some(match_m) = match_v.as_mapping() {
        if map_get_str(match_m, "target").is_some() || map_get_str(match_m, "arms").is_some() {
            bail!(
                "E-ONE-007: structured `$match` is not canonical; use `$match: <expr>` with sibling `when:` arms"
            );
        }
    }
    verify_stmt_keys(stmt, &["$match", "when"])?;
    let target_v = match_v;
    let arms_v = map_get_str(stmt, "when").context("$match missing `when`")?;
    let target = parse_expr(
        target_v,
        sigs,
        constants,
        type_aliases,
        enums,
        impls,
        locals,
        home,
        warnings,
    )?;
    let target_ty = infer_expr_type(&target, constants, locals, type_aliases, enums)
        .context("$match subject type could not be inferred")?;

    let arms_seq = arms_v
        .as_sequence()
        .context("$match `when` must be a sequence")?;
    let mut arms = Vec::new();
    let mut covered_enum_tags = HashSet::new();
    let mut has_wildcard = false;
    for arm_v in arms_seq {
        let arm_map = arm_v.as_mapping().context("$match arm must be mapping")?;
        let pattern_v = map_get_str(arm_map, "pattern").context("$match arm missing pattern")?;
        let do_v = map_get_str(arm_map, "do").context("$match arm missing do")?;
        let pattern = parse_pattern(pattern_v, type_aliases, enums, home, warnings)?;
        let mut scoped_locals = locals.clone();
        validate_pattern(
            &pattern,
            &target_ty,
            type_aliases,
            enums,
            &mut scoped_locals,
            &mut covered_enum_tags,
            &mut has_wildcard,
        )?;
        let do_seq = do_v
            .as_sequence()
            .context("$match arm do must be sequence")?;
        let mut body = Vec::new();
        for step in do_seq {
            body.push(lower_statement(
                step,
                sigs,
                constants,
                type_aliases,
                enums,
                impls,
                &mut scoped_locals,
                warnings,
                fn_ctx,
            )?);
        }
        arms.push(MatchArm { pattern, body });
    }

    if !has_wildcard {
        if let Some((enum_key, enum_def)) = enum_target_def(&target_ty, enums) {
            for tag in enum_def.tags.keys() {
                if !covered_enum_tags.contains(tag) {
                    bail!("$match for enum `{enum_key}` missing arm for tag `{tag}`");
                }
            }
        } else if arms.len() != 1 || !is_literal_singleton_pattern(&arms[0].pattern, &target_ty) {
            bail!("$match over open-ended type `{target_ty:?}` requires a wildcard arm");
        }
    }

    Ok(Statement::Match { target, arms })
}

fn parse_qualified_call(call: &str) -> Result<(&str, &str)> {
    let rest = call
        .strip_prefix('$')
        .context("call key must start with `$`")?;
    let (a, b) = rest
        .split_once('.')
        .context("expected qualified call `$alias.symbol`")?;
    if a.is_empty() || b.is_empty() {
        bail!("invalid qualified call `{call}`");
    }
    Ok((a, b))
}

fn resolve_enum_tag_ref(
    raw: &str,
    module_scope: &str,
    enums: &HashMap<String, EnumDef>,
) -> Result<(String, String)> {
    let rest = raw
        .strip_prefix('$')
        .context("constructor key must start with `$`")?;
    let parts: Vec<&str> = rest.split('.').collect();
    if parts.len() == 3 {
        let alias = parts[0];
        let enum_name = parts[1];
        let tag = parts[2];
        if alias.is_empty() || enum_name.is_empty() || tag.is_empty() {
            bail!("invalid constructor `{raw}`");
        }
        let enum_key = format!("{alias}.{enum_name}");
        if qualified_type_leaks_imported_module_private(&enum_key) {
            bail!("unknown enum reference `{enum_key}` in `{raw}`");
        }
        return Ok((enum_key, tag.to_string()));
    }
    if parts.len() == 2 {
        let enum_name = parts[0];
        let tag = parts[1];
        if enum_name.is_empty() || tag.is_empty() {
            bail!("invalid constructor `{raw}`");
        }
        if enums.contains_key(enum_name) {
            return Ok((enum_name.to_string(), tag.to_string()));
        }
        if !module_scope.is_empty() {
            let scoped = format!("{module_scope}.{enum_name}");
            if enums.contains_key(&scoped) {
                return Ok((scoped, tag.to_string()));
            }
        }
        let matches: Vec<String> = enums
            .keys()
            .filter(|k| strip_module_prefix(k) == enum_name)
            .cloned()
            .collect();
        match matches.as_slice() {
            [single] => Ok((single.clone(), tag.to_string())),
            [] => bail!("unknown enum reference `{enum_name}` in `{raw}`"),
            _ => bail!(
                "ambiguous enum reference `{enum_name}` in `{raw}`; use `$alias.{enum_name}.{tag}`"
            ),
        }
    } else {
        bail!("constructor `{raw}` must be `$alias.enum.tag` or `$enum.tag`")
    }
}

fn looks_like_call(v: &Value, sigs: &HashMap<String, FunctionSig>, home_module: &str) -> bool {
    let Some(m) = v.as_mapping() else {
        return false;
    };
    let Ok((call_key, _, _, _)) = split_call_envelope(m) else {
        return false;
    };
    resolve_call_target(&call_key, sigs, home_module).is_ok()
}

fn maybe_warn_kebab(name: &str, context: &str, warnings: &mut Vec<String>) {
    if !is_kebab_case(name) {
        warnings.push(format!(
            "non-kebab-case {context}: `{name}` (recommended: kebab-case)"
        ));
    }
}

fn maybe_warn_kebab_qualified(name: &str, context: &str, warnings: &mut Vec<String>) {
    for segment in name.split('.') {
        if segment.is_empty() {
            continue;
        }
        if segment.starts_with('-') {
            continue;
        }
        maybe_warn_kebab(segment, context, warnings);
    }
}

fn is_kebab_case(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
