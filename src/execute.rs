//! Execute lowered Vibra programs with stdlib io/fs support.

use crate::lower::{
    Call, CapabilityDomain, CapabilityType, CapabilityValue, Expr, FunctionBody, HandleAccess,
    HostHandle, LetValue, LoweredExec, LoweredProgram, Pattern, PolicyGroup, PolicyRequirement,
    PolicyScope, PolicyType, PolicyValue, PrimitiveOp, RuntimeValue, Statement, TypeRef,
    WasmArgSpec,
};
use crate::runtime::RunConfig;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{cell::RefCell, rc::Rc};

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    crate::wasm_backend::run_lowered(program, config)
}

#[cfg(test)]
pub(crate) fn run_lowered_interpreted(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    let mut env: HashMap<String, RuntimeValue> = HashMap::new();
    seed_main_args(program, config, &mut env)?;
    let mut files = FileTable::new(config.max_open_files);
    for stmt in &program.statements {
        if exec_statement(stmt, program, &mut env, &mut files, config)?.is_some() {
            bail!("unexpected `$return` at top level");
        }
    }
    Ok(())
}

/// Run a lowered program with injected guest stdout/stderr sinks.
///
/// This mirrors [`run_lowered`] but routes all guest standard-output writes
/// through the provided writers instead of the process's standard streams. It
/// exists primarily so tests (and embedders) can observe or deliberately fail
/// guest output writes — e.g. simulating a broken pipe — without panicking.
pub fn run_lowered_with_io(
    program: &LoweredProgram,
    config: &RunConfig,
    stdout: Box<dyn Write>,
    stderr: Box<dyn Write>,
) -> Result<()> {
    crate::wasm_backend::run_lowered_with_io(program, config, stdout, stderr)
}

#[cfg(test)]
pub(crate) fn run_lowered_interpreted_with_io(
    program: &LoweredProgram,
    config: &RunConfig,
    stdout: Box<dyn Write>,
    stderr: Box<dyn Write>,
) -> Result<()> {
    let mut env: HashMap<String, RuntimeValue> = HashMap::new();
    seed_main_args(program, config, &mut env)?;
    let mut files = FileTable::new(config.max_open_files);
    files.stdout_sink = Some(stdout);
    files.stderr_sink = Some(stderr);
    for stmt in &program.statements {
        if exec_statement(stmt, program, &mut env, &mut files, config)?.is_some() {
            bail!("unexpected `$return` at top level");
        }
    }
    Ok(())
}

pub fn eval_lowered_exec(
    exec: &LoweredExec,
    bindings: &HashMap<String, RuntimeValue>,
    config: &RunConfig,
) -> Result<RuntimeValue> {
    let env = bindings.clone();
    let mut files = FileTable::new(config.max_open_files);
    eval_expr(&exec.expr, &env, &exec.program, &mut files, config)
}

/// Seed every `$policy`-typed root argument (of `main` or a `$test`) with an
/// **attenuated** capability value: the intersection of the requested policy
/// type and the run's approved policy. Mandatory domain groups must intersect
/// to a non-empty scope set; optional groups may end up empty and fail closed
/// at the point of use.
pub(crate) fn seed_main_args(
    program: &LoweredProgram,
    config: &RunConfig,
    env: &mut HashMap<String, RuntimeValue>,
) -> Result<()> {
    let mut approved: Option<PolicyType> = None;
    for (name, ty) in &program.main_arg_bindings {
        let TypeRef::Policy(requested) = ty else {
            continue;
        };
        let approved = approved.get_or_insert_with(|| config.effective_approved_policy());
        let effective = intersect_requested_policy(requested, approved).map_err(|domain| {
            anyhow::anyhow!(
                "mandatory policy coverage is missing: `{domain}` is not approved for this run (approve it with the matching `--allow-*` flag)"
            )
        })?;
        env.insert(
            name.clone(),
            RuntimeValue::Policy(PolicyValue { policy: effective }),
        );
    }
    Ok(())
}

/// Runtime half of `$policy.narrow`: the statically checked target type is
/// additionally intersected with the **live source value's** scopes, so a
/// narrowed capability can never exceed the value it came from.
pub(crate) fn narrow_policy_value(
    requested: &PolicyType,
    source: &PolicyType,
) -> Result<PolicyValue> {
    let policy = intersect_requested_policy(requested, source).map_err(|domain| {
        anyhow::anyhow!(
            "`$policy.narrow` requires mandatory `{domain}` scopes the source value does not carry"
        )
    })?;
    Ok(PolicyValue { policy })
}

pub(crate) fn narrow_capability_value(
    requested: &CapabilityType,
    source: &PolicyType,
) -> Result<CapabilityValue> {
    if requested.groups.is_empty() {
        let groups = source
            .domains
            .get(&requested.domain)
            .cloned()
            .unwrap_or_default();
        let policy = PolicyType {
            domains: std::collections::BTreeMap::from([(requested.domain, groups.clone())]),
        };
        return Ok(CapabilityValue {
            capability: CapabilityType {
                domain: requested.domain,
                groups,
            },
            policy,
        });
    }
    let policy = PolicyType {
        domains: std::collections::BTreeMap::from([(requested.domain, requested.groups.clone())]),
    };
    let narrowed = intersect_requested_policy(&policy, source).map_err(|domain| {
        anyhow::anyhow!(
            "`$policy.narrow` requires mandatory `{domain}` scopes the source value does not carry"
        )
    })?;
    Ok(CapabilityValue {
        capability: CapabilityType {
            domain: requested.domain,
            groups: narrowed
                .domains
                .get(&requested.domain)
                .cloned()
                .unwrap_or_default(),
        },
        policy: narrowed,
    })
}

/// Intersect a requested policy type with an approved policy. Every
/// `mandatory` group must retain at least one scope (otherwise the offending
/// domain is returned as the error); `optional` groups may intersect to empty
/// and fail closed at the point of use.
fn intersect_requested_policy(
    requested: &PolicyType,
    approved: &PolicyType,
) -> std::result::Result<PolicyType, CapabilityDomain> {
    let mut domains = std::collections::BTreeMap::new();
    for (domain, groups) in &requested.domains {
        let approved_scopes: Vec<PolicyScope> = approved
            .domains
            .get(domain)
            .map(|groups| {
                groups
                    .iter()
                    .flat_map(|group| group.scopes.iter().cloned())
                    .collect()
            })
            .unwrap_or_default();
        let mut effective_groups = Vec::with_capacity(groups.len());
        for group in groups {
            let effective_scopes = intersect_scope_sets(&group.scopes, &approved_scopes);
            if effective_scopes.is_empty() && group.requirement == PolicyRequirement::Mandatory {
                return Err(domain.clone());
            }
            effective_groups.push(PolicyGroup {
                requirement: group.requirement.clone(),
                scopes: effective_scopes,
            });
        }
        domains.insert(domain.clone(), effective_groups);
    }
    Ok(PolicyType { domains })
}

/// Scope-set intersection: the portion of `requested` that `approved` covers.
/// Requesting `any` yields exactly the approved scopes (never more).
fn intersect_scope_sets(requested: &[PolicyScope], approved: &[PolicyScope]) -> Vec<PolicyScope> {
    let mut out: Vec<PolicyScope> = Vec::new();
    for req in requested {
        for app in approved {
            if let Some(scope) = intersect_scopes(req, app) {
                if !out.contains(&scope) {
                    out.push(scope);
                }
            }
        }
    }
    out
}

/// The intersection of two scopes, or `None` when they are disjoint. The
/// result never exceeds either input.
fn intersect_scopes(a: &PolicyScope, b: &PolicyScope) -> Option<PolicyScope> {
    match (a, b) {
        (PolicyScope::Any, other) | (other, PolicyScope::Any) => Some(other.clone()),
        (PolicyScope::Dir(x), PolicyScope::Dir(y)) => {
            let nx = normalize_absolute_path(Path::new(x)).ok()?;
            let ny = normalize_absolute_path(Path::new(y)).ok()?;
            if nx.starts_with(&ny) {
                Some(PolicyScope::Dir(x.clone()))
            } else if ny.starts_with(&nx) {
                Some(PolicyScope::Dir(y.clone()))
            } else {
                None
            }
        }
        (PolicyScope::File(f), PolicyScope::Dir(d))
        | (PolicyScope::Dir(d), PolicyScope::File(f)) => {
            let nf = normalize_absolute_path(Path::new(f)).ok()?;
            let nd = normalize_absolute_path(Path::new(d)).ok()?;
            nf.starts_with(&nd).then(|| PolicyScope::File(f.clone()))
        }
        (PolicyScope::File(x), PolicyScope::File(y)) => {
            let nx = normalize_absolute_path(Path::new(x)).ok()?;
            let ny = normalize_absolute_path(Path::new(y)).ok()?;
            (nx == ny).then(|| PolicyScope::File(x.clone()))
        }
        (PolicyScope::Exact(x), PolicyScope::Exact(y)) => {
            (x == y).then(|| PolicyScope::Exact(x.clone()))
        }
        (PolicyScope::Exact(e), PolicyScope::Prefix(p))
        | (PolicyScope::Prefix(p), PolicyScope::Exact(e)) => {
            e.starts_with(p).then(|| PolicyScope::Exact(e.clone()))
        }
        (PolicyScope::Prefix(x), PolicyScope::Prefix(y)) => {
            if x.starts_with(y) {
                Some(PolicyScope::Prefix(x.clone()))
            } else if y.starts_with(x) {
                Some(PolicyScope::Prefix(y.clone()))
            } else {
                None
            }
        }
        (
            PolicyScope::Dir(_) | PolicyScope::File(_),
            PolicyScope::Exact(_) | PolicyScope::Prefix(_),
        )
        | (
            PolicyScope::Exact(_) | PolicyScope::Prefix(_),
            PolicyScope::Dir(_) | PolicyScope::File(_),
        ) => None,
    }
}

enum FileHandle {
    Stdin,
    Stdout,
    Stderr,
    File(File),
}

#[derive(Clone, Copy)]
enum StdStream {
    Out,
    Err,
}

#[derive(Clone, Copy)]
enum HandleKind {
    Stdin,
    Stdout,
    Stderr,
    File,
}

impl FileHandle {
    fn kind(&self) -> HandleKind {
        match self {
            FileHandle::Stdin => HandleKind::Stdin,
            FileHandle::Stdout => HandleKind::Stdout,
            FileHandle::Stderr => HandleKind::Stderr,
            FileHandle::File(_) => HandleKind::File,
        }
    }
}

pub(crate) struct FileTable {
    next: u64,
    handles: HashMap<u64, FileHandle>,
    /// Optional injected sinks for guest stdout/stderr. When `None`, writes go
    /// to the process's locked standard streams. Injected sinks are primarily
    /// used by tests to exercise write-failure paths deterministically.
    stdout_sink: Option<Box<dyn Write>>,
    stderr_sink: Option<Box<dyn Write>>,
    /// Maximum number of live, user-opened file handles. `0` means unlimited.
    /// The reserved stdio entries (ids 0/1/2) are never counted against it.
    limit: usize,
}

impl FileTable {
    pub(crate) fn new(limit: usize) -> Self {
        let mut handles = HashMap::new();
        // Stdout/stderr are baseline authority and always present. Stdin is
        // *not* preinserted: a stdin handle only exists after a
        // capability-checked `stdin_open`, so a forged integer handle cannot
        // read stdin in a program that never presented a `stdin-read` policy.
        handles.insert(1, FileHandle::Stdout);
        handles.insert(2, FileHandle::Stderr);
        Self {
            next: 3,
            handles,
            stdout_sink: None,
            stderr_sink: None,
            limit,
        }
    }

    pub(crate) fn with_io(limit: usize, stdout: Box<dyn Write>, stderr: Box<dyn Write>) -> Self {
        let mut table = Self::new(limit);
        table.stdout_sink = Some(stdout);
        table.stderr_sink = Some(stderr);
        table
    }

    /// Count of live user-opened file handles, excluding stdio entries.
    fn open_file_count(&self) -> usize {
        self.handles
            .values()
            .filter(|handle| matches!(handle, FileHandle::File(_)))
            .count()
    }

    /// Whether opening another file would exceed the configured limit.
    fn at_capacity(&self) -> bool {
        self.limit != 0 && self.open_file_count() >= self.limit
    }

    fn insert(&mut self, file: File) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::File(file));
        id
    }

    /// Mint a stdin handle. Only reachable through the capability-checked
    /// `stdin_open` host import.
    fn insert_stdin(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::Stdin);
        id
    }

    fn get_mut(&mut self, id: u64) -> Result<&mut FileHandle> {
        self.handles
            .get_mut(&id)
            .with_context(|| format!("invalid file handle `{id}`"))
    }

    fn close(&mut self, id: u64) {
        if id > 2 {
            self.handles.remove(&id);
        }
    }

    /// Write `bytes` to a guest standard stream, honoring any injected sink and
    /// otherwise the process's locked standard stream. Errors (e.g. a broken
    /// pipe) are returned rather than panicking the way `print!`/`eprint!` do.
    fn write_std(&mut self, stream: StdStream, bytes: &[u8]) -> std::io::Result<()> {
        let sink = match stream {
            StdStream::Out => &mut self.stdout_sink,
            StdStream::Err => &mut self.stderr_sink,
        };
        if let Some(writer) = sink {
            writer.write_all(bytes)?;
            return writer.flush();
        }
        match stream {
            StdStream::Out => {
                let mut out = std::io::stdout().lock();
                out.write_all(bytes)?;
                out.flush()
            }
            StdStream::Err => {
                let mut err = std::io::stderr().lock();
                err.write_all(bytes)?;
                err.flush()
            }
        }
    }

    fn flush_std(&mut self, stream: StdStream) -> std::io::Result<()> {
        let sink = match stream {
            StdStream::Out => &mut self.stdout_sink,
            StdStream::Err => &mut self.stderr_sink,
        };
        if let Some(writer) = sink {
            return writer.flush();
        }
        match stream {
            StdStream::Out => std::io::stdout().lock().flush(),
            StdStream::Err => std::io::stderr().lock().flush(),
        }
    }

    /// Write `bytes` through the handle `id`, returning an `io::Result` so the
    /// caller can map failures into the runtime `fs-error` path. Standard
    /// streams route through the injected-sink-aware helpers; regular files use
    /// the underlying `File`. Stdin is reported as not writable.
    fn write_to_handle(&mut self, id: u64, bytes: &[u8]) -> std::io::Result<()> {
        match self.handles.get(&id).map(FileHandle::kind) {
            Some(HandleKind::Stdout) => self.write_std(StdStream::Out, bytes),
            Some(HandleKind::Stderr) => self.write_std(StdStream::Err, bytes),
            Some(HandleKind::Stdin) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "handle is not writable",
            )),
            Some(HandleKind::File) => match self.handles.get_mut(&id) {
                Some(FileHandle::File(file)) => file.write_all(bytes),
                _ => unreachable!("handle kind changed between lookups"),
            },
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid file handle `{id}`"),
            )),
        }
    }

    /// Flush the handle `id`, returning an `io::Result` for fs-error mapping.
    fn flush_handle(&mut self, id: u64) -> std::io::Result<()> {
        match self.handles.get(&id).map(FileHandle::kind) {
            Some(HandleKind::Stdout) => self.flush_std(StdStream::Out),
            Some(HandleKind::Stderr) => self.flush_std(StdStream::Err),
            Some(HandleKind::Stdin) => Ok(()),
            Some(HandleKind::File) => match self.handles.get_mut(&id) {
                Some(FileHandle::File(file)) => file.flush(),
                _ => unreachable!("handle kind changed between lookups"),
            },
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid file handle `{id}`"),
            )),
        }
    }
}

fn eval_expr(
    expr: &Expr,
    env: &HashMap<String, RuntimeValue>,
    program: &LoweredProgram,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<RuntimeValue> {
    match expr {
        Expr::Value(v) => Ok(v.clone()),
        Expr::VarRef(v) => env
            .get(v)
            .map(read_place_value)
            .with_context(|| format!("unknown variable `${v}`")),
        Expr::Mutable(inner) => {
            if let Expr::VarRef(name) = inner.as_ref() {
                if let Some(existing) = env.get(name) {
                    match existing {
                        RuntimeValue::Mutable(cell)
                        | RuntimeValue::Reference {
                            cell,
                            mutable: true,
                        } => return Ok(RuntimeValue::Mutable(cell.clone())),
                        _ => {}
                    }
                }
            }
            let value = eval_expr(inner, env, program, files, config)?;
            Ok(RuntimeValue::Mutable(Rc::new(RefCell::new(value))))
        }
        Expr::Reference { target, mutable } => {
            let cell = if let Expr::VarRef(name) = target.as_ref() {
                match env
                    .get(name)
                    .with_context(|| format!("unknown reference target `${name}`"))?
                {
                    RuntimeValue::Mutable(cell) | RuntimeValue::Reference { cell, .. } => {
                        cell.clone()
                    }
                    value => Rc::new(RefCell::new(value.clone())),
                }
            } else {
                match eval_expr(target, env, program, files, config)? {
                    RuntimeValue::Mutable(cell) | RuntimeValue::Reference { cell, .. } => cell,
                    value => Rc::new(RefCell::new(value)),
                }
            };
            Ok(RuntimeValue::Reference {
                cell,
                mutable: *mutable,
            })
        }
        Expr::Call { call, .. } => exec_call(call, program, env, files, config),
        Expr::Primitive {
            op,
            args,
            operand_type,
            return_type,
        } => {
            let values = args
                .iter()
                .map(|arg| eval_expr(arg, env, program, files, config))
                .collect::<Result<Vec<_>>>()?;
            eval_primitive(*op, operand_type, return_type, &values)
        }
        Expr::Cast { from, target } => Ok(RuntimeValue::Typed {
            type_ref: target.clone(),
            value: Box::new(eval_expr(from, env, program, files, config)?),
        }),
        Expr::PolicyNarrow { from, target } => {
            let value = eval_expr(from, env, program, files, config)?;
            let RuntimeValue::Policy(source) = value else {
                bail!("`$policy.narrow` expects a policy value");
            };
            match target {
                TypeRef::Capability(requested) => Ok(RuntimeValue::Capability(
                    narrow_capability_value(requested, &source.policy)?,
                )),
                TypeRef::Policy(requested) => Ok(RuntimeValue::Policy(narrow_policy_value(
                    requested,
                    &source.policy,
                )?)),
                _ => bail!("`$policy.narrow.into` must be a capability type"),
            }
        }
        Expr::EnumConstructor {
            enum_key,
            tag,
            payload,
        } => {
            let payload_value = payload
                .as_ref()
                .map(|p| eval_expr(p, env, program, files, config))
                .transpose()?
                .map(Box::new);
            Ok(RuntimeValue::Enum {
                enum_key: enum_key.clone(),
                tag: tag.clone(),
                payload: payload_value,
            })
        }
        Expr::Record(fields) => fields
            .iter()
            .map(|(name, expr)| Ok((name.clone(), eval_expr(expr, env, program, files, config)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>>>()
            .map(RuntimeValue::Record),
        Expr::Tuple(items) => items
            .iter()
            .map(|expr| eval_expr(expr, env, program, files, config))
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Tuple),
        Expr::Array(items) => items
            .iter()
            .map(|expr| eval_expr(expr, env, program, files, config))
            .collect::<Result<Vec<_>>>()
            .map(RuntimeValue::Array),
        Expr::Map(items) => {
            let mut values: Vec<(RuntimeValue, RuntimeValue)> = Vec::with_capacity(items.len());
            for (key, value) in items {
                let key = eval_expr(key, env, program, files, config)?;
                let value = eval_expr(value, env, program, files, config)?;
                // Map literals have the same deterministic upsert rule as the
                // public API: the first key fixes order and the last value wins.
                if let Some((_, current)) = values
                    .iter_mut()
                    .find(|(current, _)| runtime_value_eq(current, &key))
                {
                    *current = value;
                } else {
                    values.push((key, value));
                }
            }
            Ok(RuntimeValue::Map(values))
        }
        Expr::If {
            cond,
            then_e,
            else_e,
        } => match eval_expr(cond, env, program, files, config)? {
            RuntimeValue::Bool(true) => eval_expr(then_e, env, program, files, config),
            RuntimeValue::Bool(false) => eval_expr(else_e, env, program, files, config),
            other => bail!("`$if` condition must be `$bool`, got {other:?}"),
        },
    }
}

fn primitive_inner(value: &RuntimeValue) -> &RuntimeValue {
    match value {
        RuntimeValue::Typed { value, .. } => primitive_inner(value),
        value => value,
    }
}

fn integer_bounds(ty: &TypeRef) -> Option<(i128, i128, u32)> {
    Some(match ty {
        TypeRef::Int8 => (i8::MIN as i128, i8::MAX as i128, 8),
        TypeRef::Int16 => (i16::MIN as i128, i16::MAX as i128, 16),
        TypeRef::Int32 => (i32::MIN as i128, i32::MAX as i128, 32),
        TypeRef::Int64 => (i64::MIN as i128, i64::MAX as i128, 64),
        TypeRef::UInt8 => (0, u8::MAX as i128, 8),
        TypeRef::UInt16 => (0, u16::MAX as i128, 16),
        TypeRef::UInt32 => (0, u32::MAX as i128, 32),
        // RuntimeValue::Int is currently i64-backed; values above i64::MAX are
        // deliberately not fabricated until the value representation grows.
        TypeRef::UInt64 => (0, i64::MAX as i128, 64),
        _ => return None,
    })
}

fn wrap_primitive(ty: &TypeRef, value: RuntimeValue) -> RuntimeValue {
    if matches!(
        ty,
        TypeRef::Int64 | TypeRef::Float64 | TypeRef::Bool | TypeRef::Str
    ) {
        value
    } else {
        RuntimeValue::Typed {
            type_ref: ty.clone(),
            value: Box::new(value),
        }
    }
}

fn checked_integer_result(ty: &TypeRef, value: i128) -> Result<RuntimeValue> {
    let (min, max, _) = integer_bounds(ty).context("E-OP-001: expected integer operand")?;
    if !(min..=max).contains(&value) {
        bail!("E-OP-002: integer overflow for {ty:?}");
    }
    Ok(wrap_primitive(ty, RuntimeValue::Int(value as i64)))
}

fn checked_numeric_conversion(
    value: &RuntimeValue,
    target: &TypeRef,
) -> Result<Option<RuntimeValue>> {
    if let Some((min, max, _)) = integer_bounds(target) {
        let candidate = match primitive_inner(value) {
            RuntimeValue::Int(value) => Some(*value as i128),
            RuntimeValue::Float(value) if value.is_finite() && value.fract() == 0.0 => {
                Some(*value as i128)
            }
            RuntimeValue::Float(_) => None,
            _ => bail!("E-OP-001: expected numeric conversion source"),
        };
        return Ok(candidate
            .filter(|value| (min..=max).contains(value))
            .map(|value| wrap_primitive(target, RuntimeValue::Int(value as i64))));
    }
    let (candidate, exact_integer) = match primitive_inner(value) {
        RuntimeValue::Int(value) => (*value as f64, Some(*value as i128)),
        RuntimeValue::Float(value) => (*value, None),
        _ => bail!("E-OP-001: expected numeric conversion source"),
    };
    let converted = if target == &TypeRef::Float32 {
        (candidate as f32) as f64
    } else {
        candidate
    };
    let exact = if let Some(integer) = exact_integer {
        converted.is_finite() && converted as i128 == integer
    } else {
        (candidate.is_nan() && converted.is_nan()) || converted == candidate
    };
    Ok(exact.then(|| wrap_primitive(target, RuntimeValue::Float(converted))))
}

pub(crate) fn eval_primitive(
    op: PrimitiveOp,
    operand_type: &TypeRef,
    return_type: &TypeRef,
    values: &[RuntimeValue],
) -> Result<RuntimeValue> {
    use PrimitiveOp::*;
    if op == Convert {
        if let Some(converted) = checked_numeric_conversion(&values[0], return_type)? {
            return Ok(converted);
        }
        return checked_numeric_conversion(&values[1], return_type)?
            .context("internal: statically checked conversion fallback was not representable");
    }
    if operand_type == &TypeRef::Bool {
        let bools = values
            .iter()
            .map(|v| match primitive_inner(v) {
                RuntimeValue::Bool(v) => Ok(*v),
                _ => bail!("E-OP-001: expected boolean operand"),
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(RuntimeValue::Bool(match op {
            Equal => bools[0] == bools[1],
            NotEqual => bools[0] != bools[1],
            And => bools[0] && bools[1],
            Or => bools[0] || bools[1],
            Not => !bools[0],
            _ => bail!("E-OP-001: invalid boolean operation"),
        }));
    }
    if operand_type == &TypeRef::Str {
        let strings = values
            .iter()
            .map(|v| match primitive_inner(v) {
                RuntimeValue::Str(v) => Ok(v.as_str()),
                _ => bail!("E-OP-001: expected string operand"),
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(RuntimeValue::Bool(match op {
            Equal => strings[0] == strings[1],
            NotEqual => strings[0] != strings[1],
            LessThan => strings[0] < strings[1],
            LessOrEqual => strings[0] <= strings[1],
            GreaterThan => strings[0] > strings[1],
            GreaterOrEqual => strings[0] >= strings[1],
            _ => bail!("E-OP-001: invalid string operation"),
        }));
    }
    if matches!(operand_type, TypeRef::Float32 | TypeRef::Float64) {
        let floats = values
            .iter()
            .map(|v| match primitive_inner(v) {
                RuntimeValue::Float(v) => Ok(*v),
                _ => bail!("E-OP-001: expected float operand"),
            })
            .collect::<Result<Vec<_>>>()?;
        let comparison = match op {
            Equal => Some(floats[0] == floats[1]),
            NotEqual => Some(floats[0] != floats[1]),
            LessThan => Some(floats[0] < floats[1]),
            LessOrEqual => Some(floats[0] <= floats[1]),
            GreaterThan => Some(floats[0] > floats[1]),
            GreaterOrEqual => Some(floats[0] >= floats[1]),
            _ => None,
        };
        if let Some(value) = comparison {
            return Ok(RuntimeValue::Bool(value));
        }
        let mut value = match op {
            Add => floats[0] + floats[1],
            Subtract => floats[0] - floats[1],
            Multiply => floats[0] * floats[1],
            Divide => floats[0] / floats[1],
            Remainder => floats[0] % floats[1],
            Negate => -floats[0],
            _ => bail!("E-OP-001: invalid float operation"),
        };
        if operand_type == &TypeRef::Float32 {
            value = (value as f32) as f64;
        }
        return Ok(wrap_primitive(operand_type, RuntimeValue::Float(value)));
    }

    let ints = values
        .iter()
        .map(|v| match primitive_inner(v) {
            RuntimeValue::Int(v) => Ok(*v as i128),
            _ => bail!("E-OP-001: expected integer operand"),
        })
        .collect::<Result<Vec<_>>>()?;
    let comparison = match op {
        Equal => Some(ints[0] == ints[1]),
        NotEqual => Some(ints[0] != ints[1]),
        LessThan => Some(ints[0] < ints[1]),
        LessOrEqual => Some(ints[0] <= ints[1]),
        GreaterThan => Some(ints[0] > ints[1]),
        GreaterOrEqual => Some(ints[0] >= ints[1]),
        _ => None,
    };
    if let Some(value) = comparison {
        return Ok(RuntimeValue::Bool(value));
    }
    let (_, _, width) = integer_bounds(operand_type).context("E-OP-001: expected integer type")?;
    let result = match op {
        Add => ints[0] + ints[1],
        Subtract => ints[0] - ints[1],
        Multiply => ints[0] * ints[1],
        Divide | Remainder => {
            if ints[1] == 0 {
                bail!("E-OP-003: integer division by zero");
            }
            if op == Divide {
                ints[0] / ints[1]
            } else {
                ints[0] % ints[1]
            }
        }
        Negate => -ints[0],
        BitAnd => ints[0] & ints[1],
        BitOr => ints[0] | ints[1],
        BitXor => ints[0] ^ ints[1],
        BitNot => {
            let mask = (1_i128 << width) - 1;
            let raw = (!ints[0]) & mask;
            let signed = matches!(
                operand_type,
                TypeRef::Int8 | TypeRef::Int16 | TypeRef::Int32 | TypeRef::Int64
            );
            if signed && raw >= (1_i128 << (width - 1)) {
                raw - (1_i128 << width)
            } else {
                raw
            }
        }
        ShiftLeft | ShiftRight => {
            if ints[1] < 0 || ints[1] >= width as i128 {
                bail!("E-OP-004: shift count must be in 0..{width}");
            }
            if op == ShiftLeft {
                ints[0] << ints[1]
            } else {
                ints[0] >> ints[1]
            }
        }
        _ => bail!("E-OP-001: invalid integer operation"),
    };
    checked_integer_result(operand_type, result)
}

/// Runs one statement; `Some` means a `$return` was executed.
fn exec_statement(
    stmt: &Statement,
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<Option<RuntimeValue>> {
    match stmt {
        Statement::Return(expr) => Ok(Some(eval_expr(expr, env, program, files, config)?)),
        Statement::Call(call) => {
            let _ = exec_call(call, program, env, files, config)?;
            Ok(None)
        }
        Statement::Let {
            var,
            value: binding,
        } => {
            let value = match binding {
                LetValue::Call(c) => exec_call(c, program, env, files, config)?,
                LetValue::Expr(e) => eval_expr(e, env, program, files, config)?,
            };
            env.insert(var.clone(), value);
            Ok(None)
        }
        Statement::Set { var, value } => {
            let next = eval_expr(value, env, program, files, config)?;
            let target = env
                .get(var)
                .with_context(|| format!("E-SET-002: unknown `$set` target `{var}`"))?;
            match target {
                RuntimeValue::Mutable(cell)
                | RuntimeValue::Reference {
                    cell,
                    mutable: true,
                } => {
                    *cell.borrow_mut() = next;
                    Ok(None)
                }
                _ => bail!("E-SET-002: symbol `{var}` is not writable"),
            }
        }
        Statement::Match { target, arms } => {
            let value = eval_expr(target, env, program, files, config)?;
            for arm in arms {
                let mut scoped = env.clone();
                if pattern_matches(&arm.pattern, &value, program, &mut scoped)? {
                    if let Some(v) = run_block(&arm.body, program, &mut scoped, files, config)? {
                        return Ok(Some(v));
                    }
                    return Ok(None);
                }
            }
            bail!("non-exhaustive $match reached runtime with value `{value:?}`")
        }
        Statement::Eval(expr) => {
            eval_expr(expr, env, program, files, config)?;
            Ok(None)
        }
        Statement::If {
            cond,
            then_body,
            else_body,
        } => match eval_expr(cond, env, program, files, config)? {
            RuntimeValue::Bool(true) => run_block(then_body, program, env, files, config),
            RuntimeValue::Bool(false) => run_block(else_body, program, env, files, config),
            other => bail!("`$if` condition must be `$bool`, got {other:?}"),
        },
        Statement::While { cond, body } => loop {
            match eval_expr(cond, env, program, files, config)? {
                RuntimeValue::Bool(true) => {
                    if let Some(v) = run_block(body, program, env, files, config)? {
                        return Ok(Some(v));
                    }
                }
                RuntimeValue::Bool(false) => return Ok(None),
                other => bail!("`$while` condition must be `$bool`, got {other:?}"),
            }
        },
    }
}

fn read_place_value(value: &RuntimeValue) -> RuntimeValue {
    match value {
        RuntimeValue::Mutable(cell) | RuntimeValue::Reference { cell, .. } => {
            read_place_value(&cell.borrow())
        }
        value => value.clone(),
    }
}

pub fn materialize_runtime_value(value: RuntimeValue) -> RuntimeValue {
    read_place_value(&value)
}

fn strip_type_enum_suffix(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

pub(crate) fn pattern_matches(
    pattern: &Pattern,
    value: &RuntimeValue,
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
) -> Result<bool> {
    match pattern {
        Pattern::Wildcard => Ok(true),
        Pattern::Bind(name) => {
            env.insert(name.clone(), strip_type_tag(value.clone()));
            Ok(true)
        }
        Pattern::Literal(expected) => Ok(runtime_value_eq(expected, value)),
        Pattern::Enum {
            enum_key,
            tag,
            payload,
        } => {
            let RuntimeValue::Enum {
                enum_key: actual_enum,
                tag: actual_tag,
                payload: actual_payload,
            } = untyped(value)
            else {
                return Ok(false);
            };
            // Patterns often use `$result.result.*` while runtime values carry the mount-qualified
            // key (e.g. `fs.result.result`); align with `validate_pattern` in the lowerer.
            if strip_type_enum_suffix(actual_enum) != strip_type_enum_suffix(enum_key)
                || actual_tag != tag
            {
                return Ok(false);
            }
            match (payload, actual_payload.as_deref()) {
                (None, None) => Ok(true),
                (None, Some(RuntimeValue::Void)) => Ok(true),
                (Some(p), Some(v)) => pattern_matches(p, v, program, env),
                _ => Ok(false),
            }
        }
        Pattern::Record(fields) => {
            let RuntimeValue::Record(actual) = untyped(value) else {
                return Ok(false);
            };
            for (name, pat) in fields {
                let Some(v) = actual.get(name) else {
                    return Ok(false);
                };
                if !pattern_matches(pat, v, program, env)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Tuple(items) => {
            let RuntimeValue::Tuple(actual) = untyped(value) else {
                return Ok(false);
            };
            if actual.len() != items.len() {
                return Ok(false);
            }
            for (pat, v) in items.iter().zip(actual.iter()) {
                if !pattern_matches(pat, v, program, env)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Array(items) => {
            let RuntimeValue::Array(actual) = untyped(value) else {
                return Ok(false);
            };
            if actual.len() != items.len() {
                return Ok(false);
            }
            for (pat, v) in items.iter().zip(actual.iter()) {
                if !pattern_matches(pat, v, program, env)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Map(entries) => {
            let RuntimeValue::Map(actual) = untyped(value) else {
                return Ok(false);
            };
            for (kp, vp) in entries {
                let mut found = false;
                for (ak, av) in actual {
                    let mut key_env = env.clone();
                    if pattern_matches(kp, ak, program, &mut key_env)?
                        && pattern_matches(vp, av, program, &mut key_env)?
                    {
                        *env = key_env;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Newtype { type_ref, inner } => {
            let RuntimeValue::Typed {
                type_ref: actual_ty,
                value,
            } = value
            else {
                return Ok(false);
            };
            if actual_ty != type_ref {
                return Ok(false);
            }
            pattern_matches(inner, value, program, env)
        }
        Pattern::Interface(iface) => {
            let Some(actual_ty) = runtime_type(value) else {
                return Ok(false);
            };
            let (TypeRef::Named(type_name)
            | TypeRef::Instantiated {
                base: type_name, ..
            }) = actual_ty
            else {
                return Ok(false);
            };
            let (TypeRef::Named(iface_name)
            | TypeRef::Instantiated {
                base: iface_name, ..
            }) = iface
            else {
                return Ok(false);
            };
            Ok(program
                .impls
                .keys()
                .any(|k| k.implementing_type == *type_name && k.interface == *iface_name))
        }
    }
}

fn runtime_type(value: &RuntimeValue) -> Option<&TypeRef> {
    match value {
        RuntimeValue::Typed { type_ref, .. } => Some(type_ref),
        _ => None,
    }
}

fn untyped(value: &RuntimeValue) -> &RuntimeValue {
    match value {
        RuntimeValue::Typed { value, .. } => value,
        _ => value,
    }
}

fn strip_type_tag(value: RuntimeValue) -> RuntimeValue {
    match value {
        RuntimeValue::Typed { value, .. } => *value,
        other => other,
    }
}

fn runtime_value_eq(expected: &RuntimeValue, actual: &RuntimeValue) -> bool {
    expected == untyped(actual)
}

fn run_block(
    stmts: &[Statement],
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<Option<RuntimeValue>> {
    for stmt in stmts {
        if let Some(v) = exec_statement(stmt, program, env, files, config)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// Unwrap a runtime value to an `i64`, looking through newtype wrappers.
fn value_i64(value: &RuntimeValue) -> Result<i64> {
    match untyped(value) {
        RuntimeValue::Int(i) => Ok(*i),
        RuntimeValue::Float(_) => bail!("expected integer, got float"),
        other => bail!("expected integer, got {other:?}"),
    }
}

/// Unwrap a runtime value to a non-negative file handle.
fn value_handle(value: &RuntimeValue) -> Result<u64> {
    match value {
        RuntimeValue::HostHandle(handle) => Ok(handle.id),
        RuntimeValue::Typed { value, .. } => value_handle(value),
        other => bail!("expected an opaque host handle, got {other:?}"),
    }
}

/// Unwrap a runtime value to a string, looking through newtype wrappers.
fn value_string(value: &RuntimeValue) -> Result<String> {
    match untyped(value) {
        RuntimeValue::Str(s) => Ok(s.clone()),
        other => bail!("expected string, got {other:?}"),
    }
}

fn value_bytes(value: &RuntimeValue) -> Result<Vec<u8>> {
    match untyped(value) {
        RuntimeValue::Array(items) => items
            .iter()
            .map(|item| match untyped(item) {
                RuntimeValue::Int(value) => {
                    u8::try_from(*value).context("byte outside uint8 range")
                }
                other => bail!("expected byte integer, got {other:?}"),
            })
            .collect(),
        other => bail!("expected bytes, got {other:?}"),
    }
}

fn runtime_bytes(bytes: Vec<u8>) -> RuntimeValue {
    RuntimeValue::Array(
        bytes
            .into_iter()
            .map(|byte| RuntimeValue::Int(i64::from(byte)))
            .collect(),
    )
}

/// Unwrap a runtime value to a bool, looking through newtype wrappers.
fn value_bool(value: &RuntimeValue) -> Result<bool> {
    match untyped(value) {
        RuntimeValue::Bool(b) => Ok(*b),
        other => bail!("expected bool, got {other:?}"),
    }
}

/// Require a genuine runtime-minted `$policy` capability value.
fn value_policy(value: &RuntimeValue) -> Result<&PolicyType> {
    match value {
        RuntimeValue::Capability(capability) => Ok(&capability.policy),
        RuntimeValue::Policy(_) => bail!(
            "expected an explicitly narrowed `$capability.<domain>` value, got root `$policy`"
        ),
        other => bail!("expected a domain capability value, got {other:?}"),
    }
}

fn result_enum_key(sig: &crate::lower::FunctionSig) -> String {
    match &sig.return_type {
        TypeRef::Instantiated { base, .. } => base.clone(),
        TypeRef::Named(name) => name.clone(),
        other => format!("{other:?}"),
    }
}

fn result_ok(sig: &crate::lower::FunctionSig, value: RuntimeValue) -> RuntimeValue {
    RuntimeValue::Enum {
        enum_key: result_enum_key(sig),
        tag: "ok".to_string(),
        payload: Some(Box::new(value)),
    }
}

fn collection_result_err(
    sig: &crate::lower::FunctionSig,
    tag: &str,
    payload: Option<RuntimeValue>,
) -> RuntimeValue {
    let error_key = match &sig.return_type {
        TypeRef::Instantiated { type_args, .. } if type_args.len() >= 2 => match &type_args[1] {
            TypeRef::Named(name) => name.clone(),
            other => format!("{other:?}"),
        },
        _ => "collection-error".to_string(),
    };
    RuntimeValue::Enum {
        enum_key: result_enum_key(sig),
        tag: "err".to_string(),
        payload: Some(Box::new(RuntimeValue::Enum {
            enum_key: error_key,
            tag: tag.to_string(),
            payload: payload.map(Box::new),
        })),
    }
}

fn collection_bounds_err(
    sig: &crate::lower::FunctionSig,
    index: usize,
    len: usize,
) -> RuntimeValue {
    collection_result_err(
        sig,
        "out-of-bounds",
        Some(RuntimeValue::Record(BTreeMap::from([
            ("index".to_string(), RuntimeValue::Int(index as i64)),
            ("len".to_string(), RuntimeValue::Int(len as i64)),
        ]))),
    )
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CodeNodeHandle {
    source: String,
    path: crate::code::Path,
    revision: String,
    fingerprint: String,
}

fn code_document(
    source: &str,
) -> std::result::Result<
    (crate::code::SourceDatabase, crate::code::DocumentSnapshot),
    crate::code::CodeError,
> {
    let database = crate::code::SourceDatabase::from_sources(
        PathBuf::from("."),
        [(PathBuf::from("document.vibra"), source.to_string())],
    )?;
    let document = database.document("document.vibra")?;
    Ok((database, document))
}

fn encode_code_node(
    source: &str,
    node: &crate::code::Node,
) -> std::result::Result<String, crate::code::CodeError> {
    serde_json::to_string(&CodeNodeHandle {
        source: source.to_string(),
        path: node.path().clone(),
        revision: node.revision().to_string(),
        fingerprint: node.fingerprint().to_string(),
    })
    .map_err(|error| {
        crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            format!("encode structural node handle: {error}"),
        )
    })
}

fn decode_code_node(
    handle: &str,
) -> std::result::Result<(String, crate::code::Node), crate::code::CodeError> {
    let handle: CodeNodeHandle = serde_json::from_str(handle).map_err(|error| {
        crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            format!("decode structural node handle: {error}"),
        )
    })?;
    let (_, document) = code_document(&handle.source)?;
    if document.revision() != handle.revision {
        return Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::StaleRevision,
            "structural node document revision changed",
        ));
    }
    let node = document.at(&handle.path)?;
    if node.fingerprint() != handle.fingerprint {
        return Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::StaleNode,
            "structural node fingerprint changed",
        ));
    }
    Ok((handle.source, node))
}

fn typed_code_value(type_name: &str, value: RuntimeValue) -> RuntimeValue {
    RuntimeValue::Typed {
        type_ref: TypeRef::Named(type_name.to_string()),
        value: Box::new(value),
    }
}

fn code_result_err(
    sig: &crate::lower::FunctionSig,
    error: &crate::code::CodeError,
) -> RuntimeValue {
    let kind = serde_json::to_value(error.kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "code-error".to_string());
    let payload = RuntimeValue::Record(std::collections::BTreeMap::from([
        ("kind".to_string(), RuntimeValue::Str(kind)),
        (
            "message".to_string(),
            RuntimeValue::Str(error.message.clone()),
        ),
    ]));
    RuntimeValue::Enum {
        enum_key: result_enum_key(sig),
        tag: "err".to_string(),
        payload: Some(Box::new(typed_code_value("code.error", payload))),
    }
}

fn code_path_from_runtime(
    value: &RuntimeValue,
) -> std::result::Result<crate::code::Path, crate::code::CodeError> {
    let RuntimeValue::Array(segments) = untyped(value) else {
        return Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            "code path must be an array of key/index segments",
        ));
    };
    let mut path = crate::code::Path::root();
    for segment in segments {
        let RuntimeValue::Enum { tag, payload, .. } = untyped(segment) else {
            return Err(crate::code::CodeError::new(
                crate::code::CodeErrorKind::InvalidForm,
                "code path segment must be `segment.key` or `segment.index`",
            ));
        };
        let payload = payload.as_deref().ok_or_else(|| {
            crate::code::CodeError::new(
                crate::code::CodeErrorKind::InvalidForm,
                "code path segment requires a payload",
            )
        })?;
        match (tag.as_str(), untyped(payload)) {
            ("key", RuntimeValue::Str(key)) => {
                path.push(crate::code::Segment::key(key.clone()));
            }
            ("index", RuntimeValue::Int(index)) => {
                let index = usize::try_from(*index).map_err(|_| {
                    crate::code::CodeError::new(
                        crate::code::CodeErrorKind::InvalidForm,
                        "code path index must be non-negative",
                    )
                })?;
                path.push(crate::code::Segment::index(index));
            }
            _ => {
                return Err(crate::code::CodeError::new(
                    crate::code::CodeErrorKind::InvalidForm,
                    "code path segment payload has the wrong type",
                ));
            }
        }
    }
    Ok(path)
}

fn code_segment_from_runtime(
    value: &RuntimeValue,
) -> std::result::Result<crate::code::Segment, crate::code::CodeError> {
    let RuntimeValue::Enum { tag, payload, .. } = untyped(value) else {
        return Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            "code destination must be a key/index segment",
        ));
    };
    let payload = payload.as_deref().ok_or_else(|| {
        crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            "code destination segment requires a payload",
        )
    })?;
    match (tag.as_str(), untyped(payload)) {
        ("key", RuntimeValue::Str(key)) => Ok(crate::code::Segment::key(key.clone())),
        ("index", RuntimeValue::Int(index)) => {
            code_usize(*index, "code destination index").map(crate::code::Segment::index)
        }
        _ => Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            "code destination segment payload has the wrong type",
        )),
    }
}

fn code_usize(value: i64, label: &str) -> std::result::Result<usize, crate::code::CodeError> {
    usize::try_from(value).map_err(|_| {
        crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            format!("{label} must be non-negative"),
        )
    })
}

fn code_form_from_runtime(
    value: &RuntimeValue,
) -> std::result::Result<crate::code::Form, crate::code::CodeError> {
    let RuntimeValue::Enum { tag, payload, .. } = untyped(value) else {
        return Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            "code form must be a `code.form` value",
        ));
    };
    let payload = payload.as_deref().map(untyped);
    match (tag.as_str(), payload) {
        ("null", _) => Ok(crate::code::Form::Null),
        ("bool", Some(RuntimeValue::Bool(value))) => Ok(crate::code::Form::Bool(*value)),
        ("int", Some(RuntimeValue::Int(value))) => Ok(crate::code::Form::Int(*value)),
        ("float", Some(RuntimeValue::Float(value))) => Ok(crate::code::Form::Float(*value)),
        ("string", Some(RuntimeValue::Str(value))) => Ok(crate::code::Form::String(value.clone())),
        ("sequence", Some(RuntimeValue::Array(values))) => values
            .iter()
            .map(code_form_from_runtime)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(crate::code::Form::Sequence),
        ("mapping", Some(RuntimeValue::Array(entries))) => entries
            .iter()
            .map(|entry| {
                let RuntimeValue::Record(fields) = untyped(entry) else {
                    return Err(crate::code::CodeError::new(
                        crate::code::CodeErrorKind::InvalidForm,
                        "code mapping entry must be a record",
                    ));
                };
                let key = fields.get("key").and_then(|value| match untyped(value) {
                    RuntimeValue::Str(value) => Some(value.clone()),
                    _ => None,
                });
                let key = key.ok_or_else(|| {
                    crate::code::CodeError::new(
                        crate::code::CodeErrorKind::InvalidForm,
                        "code mapping entry key must be a string",
                    )
                })?;
                let value = fields.get("value").ok_or_else(|| {
                    crate::code::CodeError::new(
                        crate::code::CodeErrorKind::InvalidForm,
                        "code mapping entry requires a value",
                    )
                })?;
                Ok(crate::code::Entry {
                    key,
                    value: code_form_from_runtime(value)?,
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(crate::code::Form::Mapping),
        _ => Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            format!("invalid code form variant `{tag}`"),
        )),
    }
}

fn code_form_to_runtime(form: &crate::code::Form) -> RuntimeValue {
    let (tag, payload) = match form {
        crate::code::Form::Null => ("null", None),
        crate::code::Form::Bool(value) => ("bool", Some(Box::new(RuntimeValue::Bool(*value)))),
        crate::code::Form::Int(value) => ("int", Some(Box::new(RuntimeValue::Int(*value)))),
        crate::code::Form::Float(value) => ("float", Some(Box::new(RuntimeValue::Float(*value)))),
        crate::code::Form::String(value) => {
            ("string", Some(Box::new(RuntimeValue::Str(value.clone()))))
        }
        crate::code::Form::Sequence(values) => (
            "sequence",
            Some(Box::new(RuntimeValue::Array(
                values.iter().map(code_form_to_runtime).collect(),
            ))),
        ),
        crate::code::Form::Mapping(entries) => (
            "mapping",
            Some(Box::new(RuntimeValue::Array(
                entries
                    .iter()
                    .map(|entry| {
                        RuntimeValue::Record(std::collections::BTreeMap::from([
                            ("key".to_string(), RuntimeValue::Str(entry.key.clone())),
                            ("value".to_string(), code_form_to_runtime(&entry.value)),
                        ]))
                    })
                    .collect(),
            ))),
        ),
    };
    RuntimeValue::Enum {
        enum_key: "code.form".to_string(),
        tag: tag.to_string(),
        payload,
    }
}

fn code_pattern_from_runtime(
    value: &RuntimeValue,
) -> std::result::Result<crate::code::Pattern, crate::code::CodeError> {
    let RuntimeValue::Enum { tag, payload, .. } = untyped(value) else {
        return Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            "code pattern must be a `code.pattern` value",
        ));
    };
    let payload = payload.as_deref().map(untyped);
    match (tag.as_str(), payload) {
        ("any", _) => Ok(crate::code::Pattern::Any),
        ("exact", Some(value)) => Ok(crate::code::Pattern::Exact(code_form_from_runtime(value)?)),
        ("kind", Some(RuntimeValue::Str(kind))) => {
            Ok(crate::code::Pattern::Kind(code_node_kind(kind)?))
        }
        ("capture", Some(RuntimeValue::Record(fields))) => {
            let name = code_record_string(fields, "name")?;
            let pattern = fields.get("pattern").ok_or_else(|| {
                crate::code::CodeError::new(
                    crate::code::CodeErrorKind::InvalidForm,
                    "capture pattern requires `pattern`",
                )
            })?;
            Ok(crate::code::Pattern::capture(
                name,
                code_pattern_from_runtime(pattern)?,
            ))
        }
        ("sequence", Some(RuntimeValue::Array(patterns))) => patterns
            .iter()
            .map(code_pattern_from_runtime)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(crate::code::Pattern::Sequence),
        ("mapping", Some(RuntimeValue::Array(entries))) => entries
            .iter()
            .map(|entry| {
                let RuntimeValue::Record(fields) = untyped(entry) else {
                    return Err(crate::code::CodeError::new(
                        crate::code::CodeErrorKind::InvalidForm,
                        "mapping pattern entry must be a record",
                    ));
                };
                let key = code_record_string(fields, "key")?;
                let pattern = fields.get("pattern").ok_or_else(|| {
                    crate::code::CodeError::new(
                        crate::code::CodeErrorKind::InvalidForm,
                        "mapping pattern entry requires `pattern`",
                    )
                })?;
                Ok((key, code_pattern_from_runtime(pattern)?))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(crate::code::Pattern::Mapping),
        _ => Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            format!("invalid code pattern variant `{tag}`"),
        )),
    }
}

fn code_query_from_runtime(
    value: &RuntimeValue,
) -> std::result::Result<crate::code::Query, crate::code::CodeError> {
    let RuntimeValue::Record(fields) = untyped(value) else {
        return Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            "code query must be a record",
        ));
    };
    let root = fields
        .get("root")
        .ok_or_else(|| {
            crate::code::CodeError::new(
                crate::code::CodeErrorKind::InvalidForm,
                "code query requires `root`",
            )
        })
        .and_then(code_path_from_runtime)?;
    let include_root = matches!(
        fields.get("include-root").map(untyped),
        Some(RuntimeValue::Bool(true))
    );
    let mut query = if include_root {
        crate::code::Query::subtree(root)
    } else {
        crate::code::Query::descendants(root)
    };
    let mapping_key = code_record_string(fields, "mapping-key")?;
    if !mapping_key.is_empty() {
        query = query.with_mapping_key(mapping_key);
    }
    let kind = code_record_string(fields, "kind")?;
    if !kind.is_empty() {
        query = query.with_kind(code_node_kind(&kind)?);
    }
    if let Some(pattern) = fields.get("pattern") {
        let pattern = code_pattern_from_runtime(pattern)?;
        if pattern != crate::code::Pattern::Any {
            query = query.with_pattern(pattern);
        }
    }
    let limit = match fields.get("limit").map(untyped) {
        Some(RuntimeValue::Int(limit)) => code_usize(*limit, "code query limit")?,
        _ => {
            return Err(crate::code::CodeError::new(
                crate::code::CodeErrorKind::InvalidForm,
                "code query limit must be an integer",
            ));
        }
    };
    if limit > 0 {
        query = query.with_limit(limit);
    }
    Ok(query)
}

fn code_node_kind(
    kind: &str,
) -> std::result::Result<crate::code::NodeKind, crate::code::CodeError> {
    match kind {
        "scalar" => Ok(crate::code::NodeKind::Scalar),
        "mapping" => Ok(crate::code::NodeKind::Mapping),
        "sequence" => Ok(crate::code::NodeKind::Sequence),
        _ => Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            format!("unknown code node kind `{kind}`"),
        )),
    }
}

fn code_record_string(
    fields: &std::collections::BTreeMap<String, RuntimeValue>,
    name: &str,
) -> std::result::Result<String, crate::code::CodeError> {
    match fields.get(name).map(untyped) {
        Some(RuntimeValue::Str(value)) => Ok(value.clone()),
        _ => Err(crate::code::CodeError::new(
            crate::code::CodeErrorKind::InvalidForm,
            format!("code record field `{name}` must be a string"),
        )),
    }
}

fn result_err(
    sig: &crate::lower::FunctionSig,
    error_tag: &str,
    message: Option<String>,
) -> RuntimeValue {
    let fs_error_key = match &sig.return_type {
        TypeRef::Instantiated { type_args, .. } if type_args.len() >= 2 => match &type_args[1] {
            TypeRef::Named(name) => name.clone(),
            other => format!("{other:?}"),
        },
        _ => "fs-error".to_string(),
    };
    let payload = message.map(RuntimeValue::Str).map(Box::new);
    RuntimeValue::Enum {
        enum_key: result_enum_key(sig),
        tag: "err".to_string(),
        payload: Some(Box::new(RuntimeValue::Enum {
            enum_key: fs_error_key,
            tag: error_tag.to_string(),
            payload,
        })),
    }
}

fn fs_result<T>(
    sig: &crate::lower::FunctionSig,
    op: impl FnOnce() -> std::io::Result<T>,
    ok: impl FnOnce(T) -> RuntimeValue,
) -> RuntimeValue {
    match op() {
        Ok(value) => result_ok(sig, ok(value)),
        Err(err) => {
            let tag = match err.kind() {
                std::io::ErrorKind::NotFound => "not-found",
                std::io::ErrorKind::PermissionDenied => "permission-denied",
                std::io::ErrorKind::AlreadyExists => "already-exists",
                std::io::ErrorKind::InvalidInput => "invalid-path",
                _ => "io",
            };
            result_err(sig, tag, Some(err.to_string()))
        }
    }
}

/// Validate a program-controlled allocation length against `RunConfig`.
///
/// Returns the length as a `usize` when it is non-negative and within
/// [`RunConfig::max_alloc_len`]. On rejection returns a stable tag
/// (`"invalid-length"` for negatives, `"too-large"` for over-cap) plus a
/// human-readable message, so callers can surface a consistent error.
pub fn checked_alloc_len(
    len: i64,
    config: &RunConfig,
) -> std::result::Result<usize, (&'static str, String)> {
    let len = usize::try_from(len).map_err(|_| {
        (
            "invalid-length",
            format!("length {len} must not be negative"),
        )
    })?;
    if len > config.max_alloc_len {
        return Err((
            "too-large",
            format!(
                "length {len} exceeds max-alloc-len of {} bytes",
                config.max_alloc_len
            ),
        ));
    }
    Ok(len)
}

fn resolve_policy_path(
    path: &str,
    policy: &PolicyType,
    domain: CapabilityDomain,
) -> Result<PathBuf> {
    let abs = normalize_absolute_path(Path::new(path))?;
    let auth_path = nearest_existing_path(&abs)?;
    let canon_auth = auth_path.canonicalize().unwrap_or(auth_path);
    let Some(groups) = policy.domains.get(&domain) else {
        bail!("policy does not authorize `{domain}`");
    };
    for group in groups {
        for scope in &group.scopes {
            match scope {
                crate::lower::PolicyScope::Any => return Ok(abs),
                crate::lower::PolicyScope::Dir(root) => {
                    let root_path = PathBuf::from(root);
                    let canon_root = root_path.canonicalize().unwrap_or(root_path);
                    if canon_auth.starts_with(canon_root) {
                        return Ok(abs);
                    }
                }
                crate::lower::PolicyScope::File(file) => {
                    let file_path = PathBuf::from(file);
                    let canon_file = file_path.canonicalize().unwrap_or(file_path);
                    if abs == canon_file {
                        return Ok(abs);
                    }
                }
                _ => {}
            }
        }
    }
    bail!("path `{}` is outside approved policy", path)
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().context("current dir")?
    };

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }

    Ok(normalized)
}

fn nearest_existing_path(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Ok(current);
        }
        if !current.pop() {
            bail!("path `{}` has no existing ancestor", path.display());
        }
    }
}

/// Check that `policy` authorizes `requested` under `domain` using
/// non-filesystem scope selectors: `any` matches everything, `exact` requires
/// equality, `prefix` requires a leading match. Filesystem domains use
/// [`resolve_policy_path`] instead.
fn ensure_policy_scope(
    policy: &PolicyType,
    domain: CapabilityDomain,
    requested: &str,
) -> Result<()> {
    let Some(groups) = policy.domains.get(&domain) else {
        bail!("policy does not authorize `{domain}`");
    };
    for group in groups {
        for scope in &group.scopes {
            let matched = match scope {
                PolicyScope::Any => true,
                PolicyScope::Exact(value) => scope_value_eq(domain, value, requested),
                PolicyScope::Prefix(prefix) => requested.starts_with(prefix.as_str()),
                PolicyScope::Dir(_) | PolicyScope::File(_) => false,
            };
            if matched {
                return Ok(());
            }
        }
    }
    bail!("`{requested}` is outside the `{domain}` scopes of the provided policy")
}

fn scope_value_eq(domain: CapabilityDomain, scope: &str, requested: &str) -> bool {
    // Environment variable names are case-insensitive on Windows.
    if cfg!(windows)
        && matches!(
            domain,
            CapabilityDomain::EnvRead | CapabilityDomain::EnvWrite
        )
    {
        scope.eq_ignore_ascii_case(requested)
    } else {
        scope == requested
    }
}

fn env_get(name: &str) -> std::io::Result<String> {
    std::env::var(name).map_err(|err| {
        let kind = match err {
            std::env::VarError::NotPresent => std::io::ErrorKind::NotFound,
            std::env::VarError::NotUnicode(_) => std::io::ErrorKind::InvalidData,
        };
        std::io::Error::new(kind, err.to_string())
    })
}

fn is_valid_env_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0'])
}

pub(crate) fn exec_call(
    call: &Call,
    program: &LoweredProgram,
    env: &HashMap<String, RuntimeValue>,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<RuntimeValue> {
    let sig = program
        .functions
        .get(&call.callee_key)
        .with_context(|| format!("missing function `{}`", call.callee_key))?;

    match &sig.body {
        FunctionBody::User { statements } => {
            let mut fn_env: HashMap<String, RuntimeValue> = HashMap::new();
            for (idx, name) in sig.arg_names.iter().enumerate() {
                let val = eval_expr(&call.args[idx], env, program, files, config)?;
                fn_env.insert(format!("args.{name}"), val);
            }
            if let Some(v) = run_block(statements, program, &mut fn_env, files, config)? {
                return Ok(v);
            }
            if sig.return_type != TypeRef::Void {
                bail!(
                    "function `{}` finished without a value (expected non-void return)",
                    sig.symbol
                );
            }
            Ok(RuntimeValue::Void)
        }
        FunctionBody::Wasm { import, wasm_args } => {
            let entry =
                crate::host_abi::lookup(&import.module, &import.name).with_context(|| {
                    format!(
                        "E-WASM-002: `{}` targets unknown host import `{}.{}`",
                        sig.symbol, import.module, import.name
                    )
                })?;
            if wasm_args.len() != entry.params.len() {
                bail!(
                    "E-WASM-003: `{}` passes {} args to host import `{}.{}` which takes {}",
                    sig.symbol,
                    wasm_args.len(),
                    import.module,
                    import.name,
                    entry.params.len()
                );
            }
            // Evaluate the declared `$wasm.args` forwarding specs in the
            // import's parameter order. The specs -- never the wrapper's
            // symbol name -- are the binding between wrapper and host import.
            let mut host_args = Vec::with_capacity(wasm_args.len());
            for spec in wasm_args {
                host_args.push(match spec {
                    WasmArgSpec::Arg(name) => {
                        let idx =
                            sig.arg_names
                                .iter()
                                .position(|n| n == name)
                                .with_context(|| {
                                    format!(
                                        "`{}` forwards unknown argument `$args.{name}`",
                                        sig.symbol
                                    )
                                })?;
                        eval_expr(&call.args[idx], env, program, files, config)?
                    }
                    WasmArgSpec::ConstInt(value) => RuntimeValue::Int(*value),
                    WasmArgSpec::ConstStr(value) => RuntimeValue::Str(value.clone()),
                });
            }
            // Defense in depth behind the static `E-CAP-002` check: every
            // capability position must hold a genuine runtime-minted policy
            // value. `$policy` values cannot be forged, so presence here
            // proves the authority was threaded from a root signature.
            for (position, param) in entry.params.iter().enumerate() {
                if matches!(param, crate::host_abi::ParamKind::Capability(_)) {
                    value_policy(&host_args[position]).with_context(|| {
                        format!(
                            "host import `{}.{}` requires a `$policy` capability in position {position}",
                            entry.module, entry.name
                        )
                    })?;
                }
            }
            match entry.module {
                "vibra_v1" => exec_vibra_v1(entry.name, sig, &host_args, files, config),
                "vibra_test" => exec_vibra_test(entry.name, &host_args),
                "vibra_code" => exec_vibra_code(entry.name, sig, &host_args),
                other => bail!("unsupported host module `{other}`"),
            }
        }
    }
}

/// Resolve `path` against the `domain` scopes of `policy`, mapping a denial
/// into the wrapper's typed `permission-denied` error (payload-free, matching
/// the error enum's `$void` payload) instead of a hard runtime failure.
fn policy_path_or_denied(
    sig: &crate::lower::FunctionSig,
    path: &str,
    policy: &PolicyType,
    domain: CapabilityDomain,
) -> std::result::Result<PathBuf, Box<RuntimeValue>> {
    resolve_policy_path(path, policy, domain)
        .map_err(|_| Box::new(result_err(sig, "permission-denied", None)))
}

fn exec_vibra_v1(
    name: &str,
    sig: &crate::lower::FunctionSig,
    args: &[RuntimeValue],
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<RuntimeValue> {
    match name {
        "array_len" => match untyped(&args[0]) {
            RuntimeValue::Array(values) => Ok(RuntimeValue::Int(values.len() as i64)),
            other => bail!("array_len expects an array, got {other:?}"),
        },
        "array_get" => {
            let index = usize::try_from(value_i64(&args[1])?).context("array index too large")?;
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("array_get expects an array")
            };
            Ok(option_value(sig, values.get(index).cloned()))
        }
        "array_set" => {
            let index = usize::try_from(value_i64(&args[1])?).context("array index too large")?;
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("array_set expects an array")
            };
            if index >= values.len() {
                return Ok(collection_bounds_err(sig, index, values.len()));
            }
            let mut next = values.clone();
            next[index] = args[2].clone();
            Ok(result_ok(sig, RuntimeValue::Array(next)))
        }
        "array_slice" => {
            let start = usize::try_from(value_i64(&args[1])?).context("array start too large")?;
            let end = usize::try_from(value_i64(&args[2])?).context("array end too large")?;
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("array_slice expects an array")
            };
            if start > end || end > values.len() {
                return Ok(collection_bounds_err(
                    sig,
                    if start > end { start } else { end },
                    values.len(),
                ));
            }
            Ok(result_ok(
                sig,
                RuntimeValue::Array(values[start..end].to_vec()),
            ))
        }
        "array_append" => {
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("array_append expects an array")
            };
            if values.len() >= config.max_alloc_len {
                return Ok(collection_result_err(sig, "limit-exceeded", None));
            }
            let mut next = values.clone();
            next.push(args[1].clone());
            Ok(result_ok(sig, RuntimeValue::Array(next)))
        }
        "array_insert" => {
            let index = usize::try_from(value_i64(&args[1])?).context("array index too large")?;
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("array_insert expects an array")
            };
            if index > values.len() {
                return Ok(collection_bounds_err(sig, index, values.len()));
            }
            if values.len() >= config.max_alloc_len {
                return Ok(collection_result_err(sig, "limit-exceeded", None));
            }
            let mut next = values.clone();
            next.insert(index, args[2].clone());
            Ok(result_ok(sig, RuntimeValue::Array(next)))
        }
        "array_remove" => {
            let index = usize::try_from(value_i64(&args[1])?).context("array index too large")?;
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("array_remove expects an array")
            };
            if index >= values.len() {
                return Ok(collection_bounds_err(sig, index, values.len()));
            }
            let mut next = values.clone();
            let removed = next.remove(index);
            Ok(result_ok(
                sig,
                RuntimeValue::Record(BTreeMap::from([
                    ("values".to_string(), RuntimeValue::Array(next)),
                    ("removed".to_string(), removed),
                ])),
            ))
        }
        "array_contains" => {
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("array_contains expects an array")
            };
            Ok(RuntimeValue::Bool(
                values.iter().any(|value| runtime_value_eq(value, &args[1])),
            ))
        }
        "map_len" => match untyped(&args[0]) {
            RuntimeValue::Map(values) => Ok(RuntimeValue::Int(values.len() as i64)),
            other => bail!("map_len expects a map, got {other:?}"),
        },
        "map_get" => {
            let key = value_string(&args[1])?;
            let RuntimeValue::Map(values) = untyped(&args[0]) else {
                bail!("map_get expects a map")
            };
            Ok(option_value(
                sig,
                values
                    .iter()
                    .find(|(k, _)| value_string(k).is_ok_and(|candidate| candidate == key))
                    .map(|(_, value)| value.clone()),
            ))
        }
        "map_insert" => {
            let key = value_string(&args[1])?;
            let RuntimeValue::Map(values) = untyped(&args[0]) else {
                bail!("map_insert expects a map")
            };
            let mut next = values.clone();
            if let Some((_, current)) = next
                .iter_mut()
                .find(|(k, _)| value_string(k).is_ok_and(|candidate| candidate == key))
            {
                *current = args[2].clone();
            } else {
                if next.len() >= config.max_alloc_len {
                    return Ok(collection_result_err(sig, "limit-exceeded", None));
                }
                next.push((RuntimeValue::Str(key), args[2].clone()));
            }
            Ok(result_ok(sig, RuntimeValue::Map(next)))
        }
        "map_remove" => {
            let key = value_string(&args[1])?;
            let RuntimeValue::Map(values) = untyped(&args[0]) else {
                bail!("map_remove expects a map")
            };
            let mut next = values.clone();
            let removed = next
                .iter()
                .position(|(k, _)| value_string(k).is_ok_and(|candidate| candidate == key))
                .map(|index| {
                    next.remove(index);
                    RuntimeValue::Map(next)
                });
            Ok(option_value(sig, removed))
        }
        "map_contains_key" => {
            let key = value_string(&args[1])?;
            let RuntimeValue::Map(values) = untyped(&args[0]) else {
                bail!("map_contains_key expects a map")
            };
            Ok(RuntimeValue::Bool(values.iter().any(|(k, _)| {
                value_string(k).is_ok_and(|candidate| candidate == key)
            })))
        }
        "stdin_open" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::StdinRead, "*")?;
            Ok(RuntimeValue::HostHandle(HostHandle {
                id: files.insert_stdin(),
                access: HandleAccess::Read,
            }))
        }
        "stdout_open" => Ok(RuntimeValue::HostHandle(HostHandle {
            id: 1,
            access: HandleAccess::Write,
        })),
        "stderr_open" => Ok(RuntimeValue::HostHandle(HostHandle {
            id: 2,
            access: HandleAccess::Write,
        })),
        "fd_read" => {
            let handle = value_handle(&args[0])?;
            let value = match files.get_mut(handle)? {
                FileHandle::Stdin => {
                    let mut s = String::new();
                    std::io::stdin().read_to_string(&mut s).map(|_| s)
                }
                FileHandle::File(file) => {
                    let mut s = String::new();
                    file.read_to_string(&mut s).map(|_| s)
                }
                FileHandle::Stdout | FileHandle::Stderr => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "handle is not readable",
                )),
            };
            Ok(fs_result(sig, || value, RuntimeValue::Str))
        }
        "fd_read_bytes" => {
            let handle = value_handle(&args[0])?;
            let value = match files.get_mut(handle)? {
                FileHandle::Stdin => {
                    let mut bytes = Vec::new();
                    std::io::stdin().read_to_end(&mut bytes).map(|_| bytes)
                }
                FileHandle::File(file) => {
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes).map(|_| bytes)
                }
                FileHandle::Stdout | FileHandle::Stderr => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "handle is not readable",
                )),
            };
            Ok(fs_result(sig, || value, runtime_bytes))
        }
        "fd_read_line" => {
            let handle = value_handle(&args[0])?;
            let value = match files.get_mut(handle)? {
                FileHandle::Stdin => {
                    let mut line = String::new();
                    std::io::stdin().read_line(&mut line).map(|_| {
                        trim_line_ending(&mut line);
                        line
                    })
                }
                FileHandle::File(file) => read_line_from_file(file),
                FileHandle::Stdout | FileHandle::Stderr => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "handle is not readable",
                )),
            };
            Ok(fs_result(sig, || value, RuntimeValue::Str))
        }
        "fd_write" => {
            let handle = value_handle(&args[0])?;
            let contents = value_string(&args[1])?;
            files.get_mut(handle)?;
            let result = files.write_to_handle(handle, contents.as_bytes());
            Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
        }
        "fd_write_bytes" => {
            let handle = value_handle(&args[0])?;
            let contents = value_bytes(&args[1])?;
            files.get_mut(handle)?;
            let result = files.write_to_handle(handle, &contents);
            Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
        }
        "fd_sync" => {
            let handle = value_handle(&args[0])?;
            files.get_mut(handle)?;
            let result = files.flush_handle(handle);
            Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
        }
        "fd_close" => {
            let handle = value_handle(&args[0])?;
            files.close(handle);
            Ok(result_ok(sig, RuntimeValue::Void))
        }
        "path_new" => Ok(RuntimeValue::Str(value_string(&args[0])?)),
        "path_join" => {
            let base = value_string(&args[0])?;
            let segment = value_string(&args[1])?;
            Ok(RuntimeValue::Str(
                Path::new(&base).join(segment).display().to_string(),
            ))
        }
        "path_parent" => {
            let path = value_string(&args[0])?;
            let parent = Path::new(&path)
                .parent()
                .map(|p| p.display().to_string())
                .filter(|p| !p.is_empty());
            Ok(option_value(sig, parent.map(RuntimeValue::Str)))
        }
        "path_extension" => {
            let path = value_string(&args[0])?;
            let extension = Path::new(&path)
                .extension()
                .map(|e| e.to_string_lossy().to_string());
            Ok(option_value(sig, extension.map(RuntimeValue::Str)))
        }
        "bytes_len" => Ok(RuntimeValue::Int(
            i64::try_from(value_bytes(&args[0])?.len()).unwrap_or(i64::MAX),
        )),
        "bytes_slice" => {
            let bytes = value_bytes(&args[0])?;
            let start = usize::try_from(value_i64(&args[1])?).context("slice start < 0")?;
            let end = usize::try_from(value_i64(&args[2])?).context("slice end < 0")?;
            let slice = bytes
                .get(start..end)
                .context("byte slice range is out of bounds")?;
            Ok(runtime_bytes(slice.to_vec()))
        }
        "bytes_from_str" => Ok(runtime_bytes(value_string(&args[0])?.into_bytes())),
        "bytes_to_str" => Ok(RuntimeValue::Str(
            String::from_utf8(value_bytes(&args[0])?).context("bytes are not valid UTF-8")?,
        )),
        "fs_open_read" | "fs_open_write" | "fs_open_append" | "fs_open_read_write" => {
            let path = value_string(&args[0])?;
            let mut resolved: Option<PathBuf> = None;
            let checks: Vec<(&PolicyType, CapabilityDomain)> = match name {
                "fs_open_read" => vec![(value_policy(&args[1])?, CapabilityDomain::FsRead)],
                "fs_open_write" | "fs_open_append" => {
                    vec![(value_policy(&args[1])?, CapabilityDomain::FsWrite)]
                }
                _ => vec![
                    (value_policy(&args[1])?, CapabilityDomain::FsRead),
                    (value_policy(&args[2])?, CapabilityDomain::FsWrite),
                ],
            };
            for (policy, domain) in checks {
                match policy_path_or_denied(sig, &path, policy, domain) {
                    Ok(p) => resolved = Some(p),
                    Err(denied) => return Ok(*denied),
                }
            }
            let p = resolved.expect("open import checks at least one domain");
            if files.at_capacity() {
                return Ok(result_err(sig, "too-many-open-files", None));
            }
            let mut options = fs::OpenOptions::new();
            match name {
                "fs_open_read" => options.read(true),
                "fs_open_write" => options.create(true).truncate(true).write(true),
                "fs_open_append" => options.create(true).append(true),
                _ => options.create(true).truncate(false).read(true).write(true),
            };
            Ok(fs_result(
                sig,
                || options.open(p),
                |file| {
                    RuntimeValue::HostHandle(HostHandle {
                        id: files.insert(file),
                        access: match name {
                            "fs_open_read" => HandleAccess::Read,
                            "fs_open_read_write" => HandleAccess::ReadWrite,
                            _ => HandleAccess::Write,
                        },
                    })
                },
            ))
        }
        "fs_read_to_string" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsRead) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(sig, || fs::read_to_string(p), RuntimeValue::Str))
        }
        "fs_write_string_all" => {
            let path = value_string(&args[0])?;
            let contents = value_string(&args[1])?;
            let policy = value_policy(&args[2])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsWrite) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || fs::write(p, contents),
                |_| RuntimeValue::Void,
            ))
        }
        "fs_append_string" => {
            let path = value_string(&args[0])?;
            let contents = value_string(&args[1])?;
            let policy = value_policy(&args[2])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsWrite) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || {
                    let mut f = fs::OpenOptions::new().create(true).append(true).open(p)?;
                    f.write_all(contents.as_bytes())
                },
                |_| RuntimeValue::Void,
            ))
        }
        "fs_exists" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = resolve_policy_path(&path, policy, CapabilityDomain::FsRead)?;
            Ok(RuntimeValue::Bool(p.exists()))
        }
        "fs_create_dir_all" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsWrite) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || fs::create_dir_all(p),
                |_| RuntimeValue::Void,
            ))
        }
        "fs_remove_file" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsWrite) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || fs::remove_file(p),
                |_| RuntimeValue::Void,
            ))
        }
        "fs_remove_dir" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsWrite) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || fs::remove_dir_all(p),
                |_| RuntimeValue::Void,
            ))
        }
        "fs_read_dir" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsRead) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || {
                    let mut names = Vec::new();
                    for entry in fs::read_dir(p)? {
                        let entry = entry?;
                        names.push(entry.file_name().to_string_lossy().to_string());
                    }
                    Ok(names)
                },
                |names| {
                    RuntimeValue::Array(
                        names.into_iter().map(RuntimeValue::Str).collect::<Vec<_>>(),
                    )
                },
            ))
        }
        "fs_metadata" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsRead) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || fs::metadata(p),
                |md| RuntimeValue::Str(format!("size={},is_dir={}", md.len(), md.is_dir())),
            ))
        }
        "fs_canonicalize" => {
            let path = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsRead) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || p.canonicalize(),
                |c| RuntimeValue::Str(c.display().to_string()),
            ))
        }
        "env_get" => {
            let var = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::EnvRead, &var).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            Ok(fs_result(sig, || env_get(&var), RuntimeValue::Str))
        }
        "env_set" => {
            let var = value_string(&args[0])?;
            let value = value_string(&args[1])?;
            let policy = value_policy(&args[2])?;
            if ensure_policy_scope(policy, CapabilityDomain::EnvWrite, &var).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            if !is_valid_env_name(&var) {
                return Ok(result_err(sig, "invalid-name", None));
            }
            std::env::set_var(var, value);
            Ok(result_ok(sig, RuntimeValue::Void))
        }
        "net_connect" | "net_listen" | "process_run" => {
            let target = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            let (domain, what) = match name {
                "net_connect" => (CapabilityDomain::NetConnect, "network"),
                "net_listen" => (CapabilityDomain::NetListen, "network"),
                _ => (CapabilityDomain::ProcessRun, "process"),
            };
            if ensure_policy_scope(policy, domain, &target).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            Ok(result_err(
                sig,
                "unsupported",
                Some(format!("{what} runtime is not implemented yet")),
            ))
        }
        "clock_now_unix_millis" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::Clock, "*")?;
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock before unix epoch")?
                .as_millis();
            Ok(RuntimeValue::Int(i64::try_from(millis).unwrap_or(i64::MAX)))
        }
        "random_bytes" => {
            let len = value_i64(&args[0])?;
            let policy = value_policy(&args[1])?;
            ensure_policy_scope(policy, CapabilityDomain::Random, "*")?;
            let len = checked_alloc_len(len, config)
                .map_err(|(tag, msg)| anyhow::anyhow!("random_bytes {tag}: {msg}"))?;
            let mut buf = vec![0u8; len];
            getrandom::getrandom(&mut buf)
                .map_err(|err| anyhow::anyhow!("random_bytes unavailable: {err}"))?;
            Ok(RuntimeValue::Array(
                buf.into_iter()
                    .map(|byte| RuntimeValue::Int(i64::from(byte)))
                    .collect(),
            ))
        }
        "system_info" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::SystemInfo, "*")?;
            Ok(RuntimeValue::Str(format!(
                "{}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            )))
        }
        other => bail!("unsupported vibra_v1 import `{other}`"),
    }
}

/// Strip a trailing `\n` (and `\r\n`) from a line read from a stream.
fn trim_line_ending(line: &mut String) {
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
}

/// Read one line from a `File` without buffering past the newline, so the
/// handle's position stays consistent for subsequent reads.
fn read_line_from_file(file: &mut File) -> std::io::Result<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if file.read(&mut byte)? == 0 {
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
    }
    let mut line = String::from_utf8_lossy(&buf).to_string();
    if line.ends_with('\r') {
        line.pop();
    }
    Ok(line)
}

/// Construct an `option`-shaped enum value for the wrapper's return type.
fn option_value(sig: &crate::lower::FunctionSig, value: Option<RuntimeValue>) -> RuntimeValue {
    match value {
        Some(v) => RuntimeValue::Enum {
            enum_key: result_enum_key(sig),
            tag: "some".to_string(),
            payload: Some(Box::new(v)),
        },
        None => RuntimeValue::Enum {
            enum_key: result_enum_key(sig),
            tag: "none".to_string(),
            payload: None,
        },
    }
}

fn exec_vibra_test(name: &str, args: &[RuntimeValue]) -> Result<RuntimeValue> {
    match name {
        "assert" => {
            if value_bool(&args[0])? {
                Ok(RuntimeValue::Void)
            } else {
                bail!("assertion failed")
            }
        }
        "fail" => bail!("{}", value_string(&args[0])?),
        "assert-eq-bool" | "assert-eq-int" | "assert-eq-float" | "assert-eq-str" => {
            let actual = &args[0];
            let expected = &args[1];
            if actual == expected {
                Ok(RuntimeValue::Void)
            } else {
                bail!("assertion failed: expected {expected:?}, actual {actual:?}")
            }
        }
        other => bail!("unsupported vibra_test import `{other}`"),
    }
}

fn exec_vibra_code(
    name: &str,
    sig: &crate::lower::FunctionSig,
    args: &[RuntimeValue],
) -> Result<RuntimeValue> {
    match name {
        "make-query" => Ok(RuntimeValue::Record(std::collections::BTreeMap::from([
            ("root".to_string(), args[0].clone()),
            ("include-root".to_string(), args[1].clone()),
            ("mapping-key".to_string(), args[2].clone()),
            ("kind".to_string(), args[3].clone()),
            ("pattern".to_string(), args[4].clone()),
            ("limit".to_string(), args[5].clone()),
        ]))),
        "capture-pattern" => {
            let pattern = args[0].clone();
            let capture_name = value_string(&args[1])?;
            Ok(RuntimeValue::Enum {
                enum_key: "code.pattern".to_string(),
                tag: "capture".to_string(),
                payload: Some(Box::new(RuntimeValue::Record(
                    std::collections::BTreeMap::from([
                        ("name".to_string(), RuntimeValue::Str(capture_name)),
                        ("pattern".to_string(), pattern),
                    ]),
                ))),
            })
        }
        "parse" => {
            let source = value_string(&args[0])?;
            Ok(match code_document(&source) {
                Ok(_) => result_ok(
                    sig,
                    typed_code_value("code.document", RuntimeValue::Str(source)),
                ),
                Err(error) => code_result_err(sig, &error),
            })
        }
        "emit" => {
            let source = value_string(&args[0])?;
            Ok(RuntimeValue::Str(source))
        }
        "root" => {
            let source = value_string(&args[0])?;
            Ok(
                match code_document(&source).and_then(|(_, document)| document.root()) {
                    Ok(node) => result_ok(
                        sig,
                        typed_code_value(
                            "code.node",
                            RuntimeValue::Str(encode_code_node(&source, &node)?),
                        ),
                    ),
                    Err(error) => code_result_err(sig, &error),
                },
            )
        }
        "at" => {
            let source = value_string(&args[0])?;
            let path_value = &args[1];
            Ok(
                match code_path_from_runtime(path_value).and_then(|path| {
                    let (_, document) = code_document(&source)?;
                    document.at(&path)
                }) {
                    Ok(node) => result_ok(
                        sig,
                        typed_code_value(
                            "code.node",
                            RuntimeValue::Str(encode_code_node(&source, &node)?),
                        ),
                    ),
                    Err(error) => code_result_err(sig, &error),
                },
            )
        }
        "parent" => {
            let handle = value_string(&args[0])?;
            Ok(
                match decode_code_node(&handle).and_then(|(source, node)| {
                    let parent = node.path().parent().ok_or_else(|| {
                        crate::code::CodeError::new(
                            crate::code::CodeErrorKind::InvalidPath,
                            "root node has no parent",
                        )
                    })?;
                    let (_, document) = code_document(&source)?;
                    document.at(&parent).map(|node| (source, node))
                }) {
                    Ok((source, node)) => result_ok(
                        sig,
                        typed_code_value(
                            "code.node",
                            RuntimeValue::Str(encode_code_node(&source, &node)?),
                        ),
                    ),
                    Err(error) => code_result_err(sig, &error),
                },
            )
        }
        "children" => {
            let handle = value_string(&args[0])?;
            Ok(
                match decode_code_node(&handle)
                    .and_then(|(source, node)| node.children().map(|nodes| (source, nodes)))
                {
                    Ok((source, nodes)) => {
                        let handles = nodes
                            .iter()
                            .map(|node| {
                                encode_code_node(&source, node).map(|handle| {
                                    typed_code_value("code.node", RuntimeValue::Str(handle))
                                })
                            })
                            .collect::<std::result::Result<Vec<_>, _>>();
                        match handles {
                            Ok(handles) => result_ok(sig, RuntimeValue::Array(handles)),
                            Err(error) => code_result_err(sig, &error),
                        }
                    }
                    Err(error) => code_result_err(sig, &error),
                },
            )
        }
        "find" => {
            let source = value_string(&args[0])?;
            let query = &args[1];
            Ok(
                match code_query_from_runtime(query).and_then(|query| {
                    let (_, document) = code_document(&source)?;
                    query.execute(&document)
                }) {
                    Ok(matches) => {
                        let matches = matches
                            .into_iter()
                            .map(|matched| {
                                let node = encode_code_node(&source, &matched.node)?;
                                let captures = matched
                                    .captures
                                    .into_iter()
                                    .map(|(name, value)| {
                                        RuntimeValue::Record(std::collections::BTreeMap::from([
                                            ("name".to_string(), RuntimeValue::Str(name)),
                                            ("value".to_string(), code_form_to_runtime(&value)),
                                        ]))
                                    })
                                    .collect();
                                Ok(RuntimeValue::Record(std::collections::BTreeMap::from([
                                    (
                                        "node".to_string(),
                                        typed_code_value("code.node", RuntimeValue::Str(node)),
                                    ),
                                    ("captures".to_string(), RuntimeValue::Array(captures)),
                                ])))
                            })
                            .collect::<std::result::Result<Vec<_>, crate::code::CodeError>>();
                        match matches {
                            Ok(matches) => result_ok(sig, RuntimeValue::Array(matches)),
                            Err(error) => code_result_err(sig, &error),
                        }
                    }
                    Err(error) => code_result_err(sig, &error),
                },
            )
        }
        "source" => {
            let handle = value_string(&args[0])?;
            let (_, node) =
                decode_code_node(&handle).map_err(|error| anyhow::anyhow!(error.to_string()))?;
            Ok(RuntimeValue::Str(node.source().to_string()))
        }
        "to-form" => {
            let handle = value_string(&args[0])?;
            Ok(match decode_code_node(&handle) {
                Ok((_, node)) => result_ok(sig, code_form_to_runtime(node.form())),
                Err(error) => code_result_err(sig, &error),
            })
        }
        "render" => {
            let form = code_form_from_runtime(&args[0])
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            Ok(RuntimeValue::Str(
                form.render()
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            ))
        }
        "replace" | "delete" | "upsert-mapping" | "insert-mapping" | "rename-key"
        | "insert-sequence" | "splice-sequence" | "copy" | "move" => {
            let source = value_string(&args[0])?;
            let result = (|| {
                let (database, document) = code_document(&source)?;
                let handle = value_string(&args[1]).map_err(|error| {
                    crate::code::CodeError::new(
                        crate::code::CodeErrorKind::InvalidForm,
                        error.to_string(),
                    )
                })?;
                let (_, handle_node) = decode_code_node(&handle)?;
                if handle_node.revision() != document.revision() {
                    return Err(crate::code::CodeError::new(
                        crate::code::CodeErrorKind::StaleRevision,
                        "node belongs to a different document revision",
                    ));
                }
                let first = document.at(handle_node.path())?;
                let changes = match name {
                    "replace" => crate::code::ChangeSet::new()
                        .replace(first.locator(), code_form_from_runtime(&args[2])?),
                    "delete" => crate::code::ChangeSet::new().delete(first.locator()),
                    "upsert-mapping" | "insert-mapping" => {
                        let key = value_string(&args[2]).map_err(|error| {
                            crate::code::CodeError::new(
                                crate::code::CodeErrorKind::InvalidForm,
                                error.to_string(),
                            )
                        })?;
                        let value = code_form_from_runtime(&args[3])?;
                        if name == "upsert-mapping" {
                            crate::code::ChangeSet::new().upsert_mapping(
                                first.locator(),
                                key,
                                value,
                            )
                        } else {
                            crate::code::ChangeSet::new().insert_mapping(
                                first.locator(),
                                key,
                                value,
                            )
                        }
                    }
                    "rename-key" => {
                        let new_key = value_string(&args[2]).map_err(|error| {
                            crate::code::CodeError::new(
                                crate::code::CodeErrorKind::InvalidForm,
                                error.to_string(),
                            )
                        })?;
                        crate::code::ChangeSet::new().rename_mapping_key(first.locator(), new_key)
                    }
                    "insert-sequence" => {
                        let RuntimeValue::Int(index) = untyped(&args[2]) else {
                            return Err(crate::code::CodeError::new(
                                crate::code::CodeErrorKind::InvalidForm,
                                "sequence insertion index must be an integer",
                            ));
                        };
                        crate::code::ChangeSet::new().insert_sequence(
                            first.locator(),
                            code_usize(*index, "sequence insertion index")?,
                            code_form_from_runtime(&args[3])?,
                        )
                    }
                    "splice-sequence" => {
                        let (
                            RuntimeValue::Int(start),
                            RuntimeValue::Int(delete_count),
                            RuntimeValue::Array(values),
                        ) = (untyped(&args[2]), untyped(&args[3]), untyped(&args[4]))
                        else {
                            return Err(crate::code::CodeError::new(
                                crate::code::CodeErrorKind::InvalidForm,
                                "sequence splice expects integer bounds and form values",
                            ));
                        };
                        let values = values
                            .iter()
                            .map(code_form_from_runtime)
                            .collect::<std::result::Result<Vec<_>, _>>()?;
                        crate::code::ChangeSet::new().splice_sequence(
                            first.locator(),
                            code_usize(*start, "sequence splice start")?,
                            code_usize(*delete_count, "sequence splice delete count")?,
                            values,
                        )
                    }
                    "copy" | "move" => {
                        let target_handle = value_string(&args[2]).map_err(|error| {
                            crate::code::CodeError::new(
                                crate::code::CodeErrorKind::InvalidForm,
                                error.to_string(),
                            )
                        })?;
                        let (_, target_node) = decode_code_node(&target_handle)?;
                        if target_node.revision() != document.revision() {
                            return Err(crate::code::CodeError::new(
                                crate::code::CodeErrorKind::StaleRevision,
                                "target node belongs to a different document revision",
                            ));
                        }
                        let target = document.at(target_node.path())?;
                        let destination = code_segment_from_runtime(&args[3])?;
                        if name == "copy" {
                            crate::code::ChangeSet::new().copy(
                                first.locator(),
                                target.locator(),
                                destination,
                            )
                        } else {
                            crate::code::ChangeSet::new().move_node(
                                first.locator(),
                                target.locator(),
                                destination,
                            )
                        }
                    }
                    _ => unreachable!("matched code edit import"),
                };
                let applied = database.apply(&changes)?;
                Ok(applied
                    .database
                    .document("document.vibra")?
                    .source()
                    .to_string())
            })();
            Ok(match result {
                Ok(source) => result_ok(
                    sig,
                    typed_code_value("code.document", RuntimeValue::Str(source)),
                ),
                Err(error) => code_result_err(sig, &error),
            })
        }
        other => bail!("unsupported vibra_code import `{other}`"),
    }
}

#[cfg(test)]
mod collection_tests {
    use super::*;

    #[test]
    fn collection_operations_execute_in_interpreter() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let collections = root
            .join("stdlib/src/collections.vibra")
            .display()
            .to_string()
            .replace('\\', "/");
        let test = root
            .join("stdlib/src/test.vibra")
            .display()
            .to_string()
            .replace('\\', "/");
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vibra");
        std::fs::write(
            &entry,
            format!(
                r#"collections:
  $import: "{collections}"
test:
  $import: "{test}"
main:
  $function: $void
  return: $void
  do:
  - $let:
      found:
        $collections.array-contains:
          t: $int64
          values: {{$array: [1, 2, 3]}}
          value: 2
  - $test.assert: $found
"#
            ),
        )
        .unwrap();
        let loaded = crate::load::load_program(&entry).unwrap();
        let lowered = crate::lower::lower_program(&loaded).unwrap();
        run_lowered_interpreted(&lowered, &RunConfig::default()).unwrap();
    }
}
