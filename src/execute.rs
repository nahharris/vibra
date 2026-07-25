//! Execute lowered Vibra programs with stdlib io/fs support.

use crate::async_runtime::{JoinHandle as RuntimeJoinHandle, Scheduler, TaskOutcome};
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
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{cell::RefCell, rc::Rc};

pub fn run_lowered(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    crate::wasm_backend::run_lowered(program, config)
}

#[cfg(test)]
pub(crate) fn run_lowered_interpreted(program: &LoweredProgram, config: &RunConfig) -> Result<()> {
    reset_source_tasks();
    let mut env: HashMap<String, RuntimeValue> = HashMap::new();
    seed_main_args(program, config, &mut env)?;
    let mut files = FileTable::new(config.max_open_files);
    for stmt in &program.statements {
        if !matches!(
            exec_statement(stmt, program, &mut env, &mut files, config)?,
            ExecFlow::Next
        ) {
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
        if !matches!(
            exec_statement(stmt, program, &mut env, &mut files, config)?,
            ExecFlow::Next
        ) {
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
    Child(Child),
    ChildStdin(ChildStdin),
    ChildStdout(ChildStdout),
    ChildStderr(ChildStderr),
    TcpStream(TcpStream),
    TcpListener(TcpListener),
    UdpSocket(UdpSocket),
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
    Child,
    ReadPipe,
    WritePipe,
    TcpStream,
    TcpListener,
    UdpSocket,
}

impl FileHandle {
    fn kind(&self) -> HandleKind {
        match self {
            FileHandle::Stdin => HandleKind::Stdin,
            FileHandle::Stdout => HandleKind::Stdout,
            FileHandle::Stderr => HandleKind::Stderr,
            FileHandle::File(_) => HandleKind::File,
            FileHandle::Child(_) => HandleKind::Child,
            FileHandle::ChildStdin(_) => HandleKind::WritePipe,
            FileHandle::ChildStdout(_) | FileHandle::ChildStderr(_) => HandleKind::ReadPipe,
            FileHandle::TcpStream(_) => HandleKind::TcpStream,
            FileHandle::TcpListener(_) => HandleKind::TcpListener,
            FileHandle::UdpSocket(_) => HandleKind::UdpSocket,
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

    /// Count of live owned host resources, excluding borrowed standard streams.
    fn open_resource_count(&self) -> usize {
        self.handles
            .values()
            .filter(|handle| {
                matches!(
                    handle,
                    FileHandle::File(_)
                        | FileHandle::Child(_)
                        | FileHandle::ChildStdin(_)
                        | FileHandle::ChildStdout(_)
                        | FileHandle::ChildStderr(_)
                        | FileHandle::TcpStream(_)
                        | FileHandle::TcpListener(_)
                        | FileHandle::UdpSocket(_)
                )
            })
            .count()
    }

    /// Whether opening another file would exceed the configured limit.
    fn at_capacity(&self) -> bool {
        self.limit != 0 && self.open_resource_count() >= self.limit
    }

    fn insert(&mut self, file: File) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::File(file));
        id
    }

    fn insert_child(&mut self, child: Child) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::Child(child));
        id
    }

    fn insert_child_stdin(&mut self, pipe: ChildStdin) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::ChildStdin(pipe));
        id
    }

    fn insert_child_stdout(&mut self, pipe: ChildStdout) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::ChildStdout(pipe));
        id
    }

    fn insert_child_stderr(&mut self, pipe: ChildStderr) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::ChildStderr(pipe));
        id
    }

    fn insert_tcp_stream(&mut self, stream: TcpStream) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::TcpStream(stream));
        id
    }

    fn insert_tcp_listener(&mut self, listener: TcpListener) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::TcpListener(listener));
        id
    }

    fn insert_udp_socket(&mut self, socket: UdpSocket) -> u64 {
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, FileHandle::UdpSocket(socket));
        id
    }

    /// Mint a stdin handle. Only reachable through the capability-checked
    /// `stdin_open` host import.
    fn insert_stdin(&mut self) -> u64 {
        if let Some(id) = self
            .handles
            .iter()
            .find_map(|(id, handle)| matches!(handle, FileHandle::Stdin).then_some(*id))
        {
            return id;
        }
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

    fn close(&mut self, id: u64) -> std::result::Result<(), HandleLifecycleError> {
        // Standard streams are borrowed process resources. They are never
        // invalidated by guest code; close is a deterministic no-op.
        if matches!(
            self.handles.get(&id),
            Some(FileHandle::Stdin | FileHandle::Stdout | FileHandle::Stderr)
        ) {
            return Ok(());
        }
        if self.handles.remove(&id).is_none() {
            return Err(self.classify_absent(id));
        }
        Ok(())
    }

    fn lifecycle_error(&self, id: u64) -> Option<HandleLifecycleError> {
        if self.handles.contains_key(&id) {
            None
        } else {
            Some(self.classify_absent(id))
        }
    }

    /// Monotonic IDs make tombstones unnecessary: every dynamic ID below
    /// `next` was minted by this instance, so an absent one is closed. IDs
    /// outside that half-open range were never minted and are invalid.
    fn classify_absent(&self, id: u64) -> HandleLifecycleError {
        if (3..self.next).contains(&id) {
            HandleLifecycleError::Closed
        } else {
            HandleLifecycleError::Invalid
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
            Some(HandleKind::WritePipe) => match self.handles.get_mut(&id) {
                Some(FileHandle::ChildStdin(pipe)) => pipe.write_all(bytes),
                _ => unreachable!("handle kind changed between lookups"),
            },
            Some(HandleKind::ReadPipe) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "handle is not writable",
            )),
            Some(HandleKind::TcpStream) => match self.handles.get_mut(&id) {
                Some(FileHandle::TcpStream(stream)) => stream.write_all(bytes),
                _ => unreachable!("handle kind changed between lookups"),
            },
            Some(HandleKind::TcpListener | HandleKind::UdpSocket) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "handle is not a byte stream",
            )),
            Some(HandleKind::Child) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "process handle is not writable",
            )),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid file handle `{id}`"),
            )),
        }
    }

    fn write_some_to_handle(&mut self, id: u64, bytes: &[u8]) -> std::io::Result<usize> {
        match self.handles.get(&id).map(FileHandle::kind) {
            Some(HandleKind::Stdout) => self.write_std(StdStream::Out, bytes).map(|()| bytes.len()),
            Some(HandleKind::Stderr) => self.write_std(StdStream::Err, bytes).map(|()| bytes.len()),
            Some(HandleKind::Stdin) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "handle is not writable",
            )),
            Some(HandleKind::File) => match self.handles.get_mut(&id) {
                Some(FileHandle::File(file)) => file.write(bytes),
                _ => unreachable!("handle kind changed between lookups"),
            },
            Some(HandleKind::WritePipe) => match self.handles.get_mut(&id) {
                Some(FileHandle::ChildStdin(pipe)) => pipe.write(bytes),
                _ => unreachable!("handle kind changed between lookups"),
            },
            Some(HandleKind::ReadPipe) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "handle is not writable",
            )),
            Some(HandleKind::TcpStream) => match self.handles.get_mut(&id) {
                Some(FileHandle::TcpStream(stream)) => stream.write(bytes),
                _ => unreachable!("handle kind changed between lookups"),
            },
            Some(HandleKind::TcpListener | HandleKind::UdpSocket) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "handle is not a byte stream",
            )),
            Some(HandleKind::Child) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "process handle is not writable",
            )),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid file handle",
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
            Some(HandleKind::WritePipe) => match self.handles.get_mut(&id) {
                Some(FileHandle::ChildStdin(pipe)) => pipe.flush(),
                _ => unreachable!("handle kind changed between lookups"),
            },
            Some(HandleKind::ReadPipe) => Ok(()),
            Some(HandleKind::TcpStream) => match self.handles.get_mut(&id) {
                Some(FileHandle::TcpStream(stream)) => stream.flush(),
                _ => unreachable!("handle kind changed between lookups"),
            },
            Some(HandleKind::TcpListener | HandleKind::UdpSocket) => Ok(()),
            Some(HandleKind::Child) => Ok(()),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid file handle `{id}`"),
            )),
        }
    }
}

impl Drop for FileTable {
    fn drop(&mut self) {
        // Dropping each File closes its OS descriptor. Clear explicitly to
        // make the instance-boundary cleanup contract visible and immediate.
        for handle in self.handles.values_mut() {
            if let FileHandle::Child(child) = handle {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.handles.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HandleLifecycleError {
    Closed,
    Invalid,
}

impl HandleLifecycleError {
    const fn tag(self) -> &'static str {
        match self {
            Self::Closed => "resource-closed",
            Self::Invalid => "invalid-handle",
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
        Expr::Range { start, end, step } => {
            let start = value_i64(&eval_expr(start, env, program, files, config)?)?;
            let end = value_i64(&eval_expr(end, env, program, files, config)?)?;
            let step = value_i64(&eval_expr(step, env, program, files, config)?)?;
            if step == 0 {
                bail!("E-ITER-002: `$range.step` must not be zero");
            }
            Ok(RuntimeValue::Range { start, end, step })
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

#[derive(Debug)]
enum ExecFlow {
    Next,
    Return(RuntimeValue),
    Break,
    Continue,
}

struct SourceTaskRuntime {
    scheduler: Scheduler,
    next_handle: u64,
    handles: HashMap<u64, RuntimeJoinHandle>,
    results: HashMap<u64, RuntimeValue>,
}

impl SourceTaskRuntime {
    fn new() -> Self {
        Self {
            scheduler: Scheduler::new([]),
            next_handle: 1,
            handles: HashMap::new(),
            results: HashMap::new(),
        }
    }

    fn spawn(&mut self, result: RuntimeValue) -> Result<u64> {
        let id = self.next_handle;
        self.next_handle += 1;
        let handle = self
            .scheduler
            .spawn_scripted(
                self.scheduler.root(),
                self.scheduler.now(),
                TaskOutcome::Completed(format!("source-task-{id}")),
            )
            .map_err(|error| anyhow::anyhow!("E-TASK-004: task admission failed: {error:?}"))?;
        self.handles.insert(id, handle);
        self.results.insert(id, result);
        Ok(id)
    }

    fn join(&mut self, id: u64) -> Result<RuntimeValue> {
        let mut handle = self.handles.remove(&id).with_context(|| {
            format!("E-TASK-003: task handle `{id}` is unknown or already joined")
        })?;
        self.scheduler.advance_to(self.scheduler.now());
        self.scheduler
            .join(&mut handle)
            .map_err(|error| anyhow::anyhow!("E-TASK-004: task join failed: {error:?}"))?;
        self.results
            .remove(&id)
            .with_context(|| format!("E-TASK-004: task `{id}` completed without a retained result"))
    }
}

thread_local! {
    static SOURCE_TASKS: RefCell<SourceTaskRuntime> = RefCell::new(SourceTaskRuntime::new());
}

#[allow(dead_code)]
fn reset_source_tasks() {
    SOURCE_TASKS.with(|tasks| *tasks.borrow_mut() = SourceTaskRuntime::new());
}

fn exec_statement(
    stmt: &Statement,
    program: &LoweredProgram,
    env: &mut HashMap<String, RuntimeValue>,
    files: &mut FileTable,
    config: &RunConfig,
) -> Result<ExecFlow> {
    match stmt {
        Statement::Return(expr) => Ok(ExecFlow::Return(eval_expr(
            expr, env, program, files, config,
        )?)),
        Statement::Call(call) => {
            let _ = exec_call(call, program, env, files, config)?;
            Ok(ExecFlow::Next)
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
            Ok(ExecFlow::Next)
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
                    Ok(ExecFlow::Next)
                }
                _ => bail!("E-SET-002: symbol `{var}` is not writable"),
            }
        }
        Statement::Match { target, arms } => {
            let value = eval_expr(target, env, program, files, config)?;
            for arm in arms {
                let mut scoped = env.clone();
                if pattern_matches(&arm.pattern, &value, program, &mut scoped)? {
                    return run_block(&arm.body, program, &mut scoped, files, config);
                }
            }
            bail!("non-exhaustive $match reached runtime with value `{value:?}`")
        }
        Statement::Eval(expr) => {
            eval_expr(expr, env, program, files, config)?;
            Ok(ExecFlow::Next)
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
                RuntimeValue::Bool(true) => match run_block(body, program, env, files, config)? {
                    ExecFlow::Next | ExecFlow::Continue => {}
                    ExecFlow::Break => return Ok(ExecFlow::Next),
                    flow @ ExecFlow::Return(_) => return Ok(flow),
                },
                RuntimeValue::Bool(false) => return Ok(ExecFlow::Next),
                other => bail!("`$while` condition must be `$bool`, got {other:?}"),
            }
        },
        Statement::For { var, source, body } => {
            let source = eval_expr(source, env, program, files, config)?;
            let items = iteration_items(source, config.max_alloc_len)?;
            for item in items {
                let mut scoped = env.clone();
                scoped.insert(var.clone(), item);
                match run_block(body, program, &mut scoped, files, config)? {
                    ExecFlow::Next | ExecFlow::Continue => {}
                    ExecFlow::Break => return Ok(ExecFlow::Next),
                    flow @ ExecFlow::Return(_) => return Ok(flow),
                }
            }
            Ok(ExecFlow::Next)
        }
        Statement::Task { captures, body } => {
            let mut child = HashMap::new();
            for name in captures {
                let value = env
                    .get(name)
                    .with_context(|| format!("E-TASK-001: missing captured value `{name}`"))?;
                child.insert(name.clone(), value.clone());
            }
            match run_block(body, program, &mut child, files, config)? {
                ExecFlow::Next => Ok(ExecFlow::Next),
                ExecFlow::Return(_) | ExecFlow::Break | ExecFlow::Continue => {
                    bail!("E-TASK-002: task control escaped its structured boundary")
                }
            }
        }
        Statement::Spawn {
            handle,
            captures,
            value,
            ..
        } => {
            let mut child = HashMap::new();
            for name in captures {
                let value = env
                    .get(name)
                    .with_context(|| format!("E-TASK-001: missing captured value `{name}`"))?;
                child.insert(name.clone(), value.clone());
            }
            let result = eval_expr(value, &child, program, files, config)?;
            let id = SOURCE_TASKS.with(|tasks| tasks.borrow_mut().spawn(result))?;
            env.insert(handle.clone(), RuntimeValue::JoinHandle(id));
            Ok(ExecFlow::Next)
        }
        Statement::Join { handle, var } => {
            let id = match env.remove(handle) {
                Some(RuntimeValue::JoinHandle(id)) => id,
                Some(_) => bail!("E-TASK-003: symbol `{handle}` is not a task join handle"),
                None => bail!("E-TASK-003: task handle `{handle}` is unknown or already joined"),
            };
            let result = SOURCE_TASKS.with(|tasks| tasks.borrow_mut().join(id))?;
            env.insert(var.clone(), result);
            Ok(ExecFlow::Next)
        }
        Statement::Break => Ok(ExecFlow::Break),
        Statement::Continue => Ok(ExecFlow::Continue),
    }
}

pub(crate) fn iteration_items(value: RuntimeValue, limit: usize) -> Result<Vec<RuntimeValue>> {
    let mut items = match value {
        RuntimeValue::Array(items) => items,
        RuntimeValue::Map(items) => items
            .into_iter()
            .map(|(key, value)| RuntimeValue::Tuple(vec![key, value]))
            .collect(),
        RuntimeValue::Str(text) => text
            .chars()
            .map(|scalar| RuntimeValue::Str(scalar.to_string()))
            .collect(),
        RuntimeValue::Range { start, end, step } => {
            if step == 0 {
                bail!("E-ITER-002: `$range.step` must not be zero");
            }
            let mut out = Vec::new();
            let mut current = start;
            while (step > 0 && current < end) || (step < 0 && current > end) {
                if out.len() >= limit {
                    bail!("E-ITER-003: traversal exceeds configured limit `{limit}`");
                }
                out.push(RuntimeValue::Int(current));
                current = current
                    .checked_add(step)
                    .context("E-ITER-004: integer range overflow")?;
            }
            out
        }
        other => bail!("E-ITER-001: value is not traversable: {other:?}"),
    };
    if items.len() > limit {
        bail!("E-ITER-003: traversal exceeds configured limit `{limit}`");
    }
    Ok(std::mem::take(&mut items))
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
) -> Result<ExecFlow> {
    for stmt in stmts {
        let flow = exec_statement(stmt, program, env, files, config)?;
        if !matches!(flow, ExecFlow::Next) {
            return Ok(flow);
        }
    }
    Ok(ExecFlow::Next)
}

/// Unwrap a runtime value to an `i64`, looking through newtype wrappers.
fn value_i64(value: &RuntimeValue) -> Result<i64> {
    match untyped(value) {
        RuntimeValue::Int(i) => Ok(*i),
        RuntimeValue::Float(_) => bail!("expected integer, got float"),
        other => bail!("expected integer, got {other:?}"),
    }
}

/// Unwrap a runtime value to an `f64`, looking through newtype wrappers.
fn value_f64(value: &RuntimeValue) -> Result<f64> {
    match untyped(value) {
        RuntimeValue::Float(value) => Ok(*value),
        other => bail!("expected float, got {other:?}"),
    }
}

#[cfg(test)]
mod iteration_tests {
    use super::*;

    fn lower(source: &str) -> (tempfile::TempDir, LoweredProgram) {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vibra");
        std::fs::write(&entry, source).unwrap();
        let loaded = crate::load::load_program(&entry).unwrap();
        let lowered = crate::lower::lower_program(&loaded).unwrap();
        (dir, lowered)
    }

    fn lower_error(source: &str) -> String {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vibra");
        std::fs::write(&entry, source).unwrap();
        let loaded = crate::load::load_program(&entry).unwrap();
        format!("{:#}", crate::lower::lower_program(&loaded).unwrap_err())
    }

    #[test]
    fn canonical_iteration_runs_in_interpreter_and_wasm() {
        let (_dir, program) = lower(
            r#"main:
  $function: $void
  return: $void
  do:
  - $let:
      sum:
        $mut: 0
  - $for: value
    in:
      $range:
        start: 0
        end: 4
        step: 1
    do:
    - $if:
        $equal: [$value, 1]
      then:
      - $continue: null
      else: []
    - $set:
        sum:
          $add: [$sum, $value]
  - $for: scalar
    in: "é🙂"
    do:
    - $break: null
"#,
        );
        run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        run_lowered(&program, &RunConfig::default()).unwrap();
    }

    #[test]
    fn zero_step_fails_consistently_in_both_runtimes() {
        let (_dir, program) = lower(
            r#"main:
  $function: $void
  return: $void
  do:
  - $for: value
    in:
      $range:
        start: 0
        end: 2
        step: 0
    do: []
"#,
        );
        let interpreted = run_lowered_interpreted(&program, &RunConfig::default()).unwrap_err();
        let wasm = run_lowered(&program, &RunConfig::default()).unwrap_err();
        assert!(format!("{interpreted:#}").contains("E-ITER-002"));
        assert!(format!("{wasm:#}").contains("E-ITER-002"));
    }

    #[test]
    fn loop_control_requires_a_loop_and_canonical_null_envelopes() {
        for statement in ["$break: null", "$continue: null"] {
            let error = lower_error(&format!(
                "main:\n  $function: $void\n  return: $void\n  do:\n  - {statement}\n"
            ));
            assert!(
                error.contains("only valid inside `$for` or `$while`"),
                "{error}"
            );
        }
        for statement in [
            "$break: false",
            "$continue: {}",
            "$break: null\n    extra: null",
        ] {
            let error = lower_error(&format!(
                "main:\n  $function: $void\n  return: $void\n  do:\n  - {statement}\n"
            ));
            assert!(error.contains("must use canonical form"), "{error}");
        }
    }

    #[test]
    fn while_is_an_existing_loop_control_target() {
        let (_dir, program) = lower(
            "main:\n  $function: $void\n  return: $void\n  do:\n  - $while: true\n    do:\n    - $break: null\n",
        );
        run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        run_lowered(&program, &RunConfig::default()).unwrap();
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

fn value_str(value: &RuntimeValue) -> Result<&str> {
    match untyped(value) {
        RuntimeValue::Str(value) => Ok(value),
        other => bail!("expected string, got {other:?}"),
    }
}

fn value_byte_items(value: &RuntimeValue) -> Result<&[RuntimeValue]> {
    match untyped(value) {
        RuntimeValue::Array(items) => Ok(items),
        other => bail!("expected bytes, got {other:?}"),
    }
}

fn runtime_byte(value: &RuntimeValue) -> Result<u8> {
    match untyped(value) {
        RuntimeValue::Int(value) => u8::try_from(*value).context("byte outside uint8 range"),
        other => bail!("expected byte integer, got {other:?}"),
    }
}

fn push_bounded_char(value: &mut String, scalar: char, limit: usize) -> bool {
    let width = scalar.len_utf8();
    let Some(next_len) = value.len().checked_add(width) else {
        return false;
    };
    if next_len > limit || value.try_reserve_exact(width).is_err() {
        return false;
    }
    value.push(scalar);
    true
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

fn process_output_value(
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> RuntimeValue {
    RuntimeValue::Record(BTreeMap::from([
        ("success".to_string(), RuntimeValue::Bool(status.success())),
        (
            "code".to_string(),
            RuntimeValue::Int(i64::from(status.code().unwrap_or(-1))),
        ),
        ("stdout".to_string(), runtime_bytes(stdout)),
        ("stderr".to_string(), runtime_bytes(stderr)),
    ]))
}

fn process_io_error(sig: &crate::lower::FunctionSig, error: std::io::Error) -> RuntimeValue {
    let tag = match error.kind() {
        std::io::ErrorKind::NotFound => "not-found",
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        std::io::ErrorKind::InvalidInput => "invalid-input",
        _ => "io",
    };
    result_err(sig, tag, (tag == "io").then(|| error.to_string()))
}

fn net_io_error(sig: &crate::lower::FunctionSig, error: std::io::Error) -> RuntimeValue {
    let tag = match error.kind() {
        std::io::ErrorKind::AddrInUse => "address-in-use",
        std::io::ErrorKind::AddrNotAvailable => "address-not-available",
        std::io::ErrorKind::ConnectionRefused => "connection-refused",
        std::io::ErrorKind::ConnectionReset => "connection-reset",
        std::io::ErrorKind::ConnectionAborted => "connection-aborted",
        std::io::ErrorKind::NotConnected => "not-connected",
        std::io::ErrorKind::TimedOut => "timed-out",
        std::io::ErrorKind::WouldBlock => "would-block",
        std::io::ErrorKind::InvalidInput => "invalid-input",
        _ => "io",
    };
    result_err(sig, tag, Some(error.to_string()))
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
            match run_block(statements, program, &mut fn_env, files, config)? {
                ExecFlow::Return(value) => return Ok(value),
                ExecFlow::Next => {}
                ExecFlow::Break | ExecFlow::Continue => bail!("loop control escaped its loop"),
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
    let checked_handle = |value: &RuntimeValue, files: &FileTable| {
        let handle = value_handle(value)?;
        Ok::<_, anyhow::Error>((handle, files.lifecycle_error(handle)))
    };
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
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            let value = match files.get_mut(handle)? {
                FileHandle::Stdin => {
                    let mut s = String::new();
                    std::io::stdin().read_to_string(&mut s).map(|_| s)
                }
                FileHandle::File(file) => {
                    let mut s = String::new();
                    file.read_to_string(&mut s).map(|_| s)
                }
                FileHandle::TcpStream(stream) => {
                    let mut s = String::new();
                    stream.read_to_string(&mut s).map(|_| s)
                }
                FileHandle::ChildStdout(pipe) => {
                    let mut s = String::new();
                    pipe.read_to_string(&mut s).map(|_| s)
                }
                FileHandle::ChildStderr(pipe) => {
                    let mut s = String::new();
                    pipe.read_to_string(&mut s).map(|_| s)
                }
                FileHandle::Stdout
                | FileHandle::Stderr
                | FileHandle::Child(_)
                | FileHandle::ChildStdin(_)
                | FileHandle::TcpListener(_)
                | FileHandle::UdpSocket(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "handle is not readable",
                )),
            };
            Ok(fs_result(sig, || value, RuntimeValue::Str))
        }
        "fd_read_bytes" => {
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            let value = match files.get_mut(handle)? {
                FileHandle::Stdin => {
                    let mut bytes = Vec::new();
                    std::io::stdin().read_to_end(&mut bytes).map(|_| bytes)
                }
                FileHandle::File(file) => {
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes).map(|_| bytes)
                }
                FileHandle::TcpStream(stream) => {
                    let mut bytes = Vec::new();
                    stream.read_to_end(&mut bytes).map(|_| bytes)
                }
                FileHandle::ChildStdout(pipe) => {
                    let mut bytes = Vec::new();
                    pipe.read_to_end(&mut bytes).map(|_| bytes)
                }
                FileHandle::ChildStderr(pipe) => {
                    let mut bytes = Vec::new();
                    pipe.read_to_end(&mut bytes).map(|_| bytes)
                }
                FileHandle::Stdout
                | FileHandle::Stderr
                | FileHandle::Child(_)
                | FileHandle::ChildStdin(_)
                | FileHandle::TcpListener(_)
                | FileHandle::UdpSocket(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "handle is not readable",
                )),
            };
            Ok(fs_result(sig, || value, runtime_bytes))
        }
        "fd_read_line" => {
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            let value = match files.get_mut(handle)? {
                FileHandle::Stdin => {
                    let mut line = String::new();
                    std::io::stdin().read_line(&mut line).map(|_| {
                        trim_line_ending(&mut line);
                        line
                    })
                }
                FileHandle::File(file) => read_line_from_file(file),
                FileHandle::TcpStream(stream) => read_line_from_file(stream),
                FileHandle::ChildStdout(pipe) => read_line_from_file(pipe),
                FileHandle::ChildStderr(pipe) => read_line_from_file(pipe),
                FileHandle::Stdout
                | FileHandle::Stderr
                | FileHandle::Child(_)
                | FileHandle::ChildStdin(_)
                | FileHandle::TcpListener(_)
                | FileHandle::UdpSocket(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "handle is not readable",
                )),
            };
            Ok(fs_result(sig, || value, RuntimeValue::Str))
        }
        "fd_read_bytes_up_to" => {
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            let requested = usize::try_from(value_i64(&args[1])?).unwrap_or(usize::MAX);
            let limit = requested.min(config.max_alloc_len);
            let value = match files.get_mut(handle)? {
                FileHandle::Stdin => {
                    let mut bytes = Vec::new();
                    std::io::stdin()
                        .take(limit as u64)
                        .read_to_end(&mut bytes)
                        .map(|_| bytes)
                }
                FileHandle::File(file) => {
                    let mut bytes = Vec::new();
                    file.take(limit as u64)
                        .read_to_end(&mut bytes)
                        .map(|_| bytes)
                }
                FileHandle::TcpStream(stream) => {
                    let mut bytes = vec![0; limit];
                    stream.read(&mut bytes).map(|count| {
                        bytes.truncate(count);
                        bytes
                    })
                }
                FileHandle::ChildStdout(pipe) => {
                    let mut bytes = Vec::new();
                    pipe.take(limit as u64)
                        .read_to_end(&mut bytes)
                        .map(|_| bytes)
                }
                FileHandle::ChildStderr(pipe) => {
                    let mut bytes = Vec::new();
                    pipe.take(limit as u64)
                        .read_to_end(&mut bytes)
                        .map(|_| bytes)
                }
                FileHandle::Stdout
                | FileHandle::Stderr
                | FileHandle::Child(_)
                | FileHandle::ChildStdin(_)
                | FileHandle::TcpListener(_)
                | FileHandle::UdpSocket(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "handle is not readable",
                )),
            };
            Ok(fs_result(sig, || value, runtime_bytes))
        }
        "fd_write" => {
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            let contents = value_string(&args[1])?;
            files.get_mut(handle)?;
            let result = files.write_to_handle(handle, contents.as_bytes());
            Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
        }
        "fd_write_bytes" => {
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            let contents = value_bytes(&args[1])?;
            files.get_mut(handle)?;
            let result = files.write_to_handle(handle, &contents);
            Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
        }
        "fd_write_bytes_some" => {
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            let contents = value_bytes(&args[1])?;
            files.get_mut(handle)?;
            let result = files.write_some_to_handle(handle, &contents);
            Ok(fs_result(
                sig,
                || result,
                |written| RuntimeValue::Int(written.try_into().unwrap_or(i64::MAX)),
            ))
        }
        "fd_sync" => {
            let (handle, lifecycle_error) = checked_handle(&args[0], files)?;
            if let Some(error) = lifecycle_error {
                return Ok(result_err(sig, error.tag(), None));
            }
            files.get_mut(handle)?;
            let result = files.flush_handle(handle);
            Ok(fs_result(sig, || result, |_| RuntimeValue::Void))
        }
        "fd_close" => {
            let handle = value_handle(&args[0])?;
            Ok(match files.close(handle) {
                Ok(()) => result_ok(sig, RuntimeValue::Void),
                Err(error) => result_err(sig, error.tag(), None),
            })
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
        "path_file_name" => {
            let path = value_string(&args[0])?;
            let name = Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string());
            Ok(option_value(sig, name.map(RuntimeValue::Str)))
        }
        "path_components" => {
            let path = value_string(&args[0])?;
            let mut components = Vec::new();
            for component in Path::new(&path).components() {
                let component = component.as_os_str().to_string_lossy().to_string();
                if component.len() > config.max_alloc_len
                    || components.len() >= config.max_alloc_len
                {
                    return Ok(result_err(sig, "limit-exceeded", None));
                }
                components.push(RuntimeValue::Str(component));
            }
            Ok(result_ok(sig, RuntimeValue::Array(components)))
        }
        "str_scalar_len" => Ok(RuntimeValue::Int(
            i64::try_from(value_string(&args[0])?.chars().count()).unwrap_or(i64::MAX),
        )),
        "str_byte_len" => Ok(RuntimeValue::Int(
            i64::try_from(value_string(&args[0])?.len()).unwrap_or(i64::MAX),
        )),
        "str_is_empty" => Ok(RuntimeValue::Bool(value_string(&args[0])?.is_empty())),
        "str_contains" => Ok(RuntimeValue::Bool(
            value_string(&args[0])?.contains(&value_string(&args[1])?),
        )),
        "str_starts_with" => Ok(RuntimeValue::Bool(
            value_string(&args[0])?.starts_with(&value_string(&args[1])?),
        )),
        "str_ends_with" => Ok(RuntimeValue::Bool(
            value_string(&args[0])?.ends_with(&value_string(&args[1])?),
        )),
        "str_find" => {
            let text = value_string(&args[0])?;
            let needle = value_string(&args[1])?;
            let found = text.find(&needle).map(|byte| {
                RuntimeValue::Int(text[..byte].chars().count().try_into().unwrap_or(i64::MAX))
            });
            Ok(option_value(sig, found))
        }
        "str_scalar_at" => {
            let index = usize::try_from(value_i64(&args[1])?).context("scalar index too large")?;
            Ok(option_value(
                sig,
                value_string(&args[0])?
                    .chars()
                    .nth(index)
                    .map(|value| RuntimeValue::Str(value.to_string())),
            ))
        }
        "str_split" => {
            let text = value_str(&args[0])?;
            let separator = value_str(&args[1])?;
            let mut values = Vec::new();
            let mut bytes = 0usize;
            for value in text.split(separator) {
                let Some(next_count) = values.len().checked_add(1) else {
                    return Ok(result_err(sig, "limit-exceeded", None));
                };
                let Some(next_bytes) = bytes.checked_add(value.len()) else {
                    return Ok(result_err(sig, "limit-exceeded", None));
                };
                if next_count > config.max_alloc_len || next_bytes > config.max_alloc_len {
                    return Ok(result_err(sig, "limit-exceeded", None));
                }
                if values.try_reserve_exact(1).is_err() {
                    return Ok(result_err(sig, "limit-exceeded", None));
                }
                values.push(RuntimeValue::Str(value.to_string()));
                bytes = next_bytes;
            }
            Ok(result_ok(sig, RuntimeValue::Array(values)))
        }
        "str_trim" => {
            let value = value_str(&args[0])?.trim();
            if value.len() > config.max_alloc_len {
                return Ok(result_err(sig, "limit-exceeded", None));
            }
            Ok(result_ok(sig, RuntimeValue::Str(value.to_string())))
        }
        "str_replace" => {
            let text = value_str(&args[0])?;
            let from = value_str(&args[1])?;
            let to = value_str(&args[2])?;
            if from.is_empty() {
                return Ok(result_err(sig, "empty-pattern", None));
            }
            let replacements = text.matches(from).count();
            let Some(removed) = replacements.checked_mul(from.len()) else {
                return Ok(result_err(sig, "limit-exceeded", None));
            };
            let Some(added) = replacements.checked_mul(to.len()) else {
                return Ok(result_err(sig, "limit-exceeded", None));
            };
            let Some(output_len) = text
                .len()
                .checked_sub(removed)
                .and_then(|len| len.checked_add(added))
            else {
                return Ok(result_err(sig, "limit-exceeded", None));
            };
            if output_len > config.max_alloc_len {
                return Ok(result_err(sig, "limit-exceeded", None));
            }
            Ok(result_ok(sig, RuntimeValue::Str(text.replace(from, to))))
        }
        "str_join" => {
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("str_join expects an array")
            };
            let separator = value_str(&args[1])?;
            let separators = values.len().saturating_sub(1);
            let Some(mut output_len) = separators.checked_mul(separator.len()) else {
                return Ok(result_err(sig, "limit-exceeded", None));
            };
            for value in values {
                let Some(next) = output_len.checked_add(value_str(value)?.len()) else {
                    return Ok(result_err(sig, "limit-exceeded", None));
                };
                output_len = next;
            }
            if output_len > config.max_alloc_len {
                return Ok(result_err(sig, "limit-exceeded", None));
            }
            let mut value = String::with_capacity(output_len);
            for (index, item) in values.iter().enumerate() {
                if index != 0 {
                    value.push_str(separator);
                }
                value.push_str(value_str(item)?);
            }
            Ok(result_ok(sig, RuntimeValue::Str(value)))
        }
        "str_lowercase" | "str_uppercase" => {
            let text = value_str(&args[0])?;
            let mut value = String::new();
            for scalar in text.chars() {
                if name == "str_lowercase" {
                    for mapped_scalar in scalar.to_lowercase() {
                        if !push_bounded_char(&mut value, mapped_scalar, config.max_alloc_len) {
                            return Ok(result_err(sig, "limit-exceeded", None));
                        }
                    }
                } else {
                    for mapped_scalar in scalar.to_uppercase() {
                        if !push_bounded_char(&mut value, mapped_scalar, config.max_alloc_len) {
                            return Ok(result_err(sig, "limit-exceeded", None));
                        }
                    }
                }
            }
            Ok(result_ok(sig, RuntimeValue::Str(value)))
        }
        "parse_int64" => match value_string(&args[0])?.parse::<i64>() {
            Ok(value) => Ok(result_ok(sig, RuntimeValue::Int(value))),
            Err(error)
                if matches!(
                    error.kind(),
                    std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow
                ) =>
            {
                Ok(result_err(sig, "overflow", None))
            }
            Err(_) => Ok(result_err(sig, "invalid", None)),
        },
        "parse_uint64" => match value_string(&args[0])?.parse::<u64>() {
            Ok(value) if value <= i64::MAX as u64 => {
                Ok(result_ok(sig, RuntimeValue::Int(value as i64)))
            }
            Ok(_) => Ok(result_err(sig, "overflow", None)),
            Err(error)
                if matches!(
                    error.kind(),
                    std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow
                ) =>
            {
                Ok(result_err(sig, "overflow", None))
            }
            Err(_) => Ok(result_err(sig, "invalid", None)),
        },
        "parse_float64" => {
            let text = value_string(&args[0])?;
            let parsed = match text.as_str() {
                "nan" => Some(f64::NAN),
                "inf" => Some(f64::INFINITY),
                "-inf" => Some(f64::NEG_INFINITY),
                _ if text.trim() != text || text.is_empty() => None,
                _ => text.parse::<f64>().ok(),
            };
            Ok(match parsed {
                Some(value) => result_ok(sig, RuntimeValue::Float(value)),
                None => result_err(sig, "invalid", None),
            })
        }
        "parse_bool" => Ok(match value_string(&args[0])?.as_str() {
            "true" => result_ok(sig, RuntimeValue::Bool(true)),
            "false" => result_ok(sig, RuntimeValue::Bool(false)),
            _ => result_err(sig, "invalid", None),
        }),
        "format_int64" | "format_uint64" => Ok(RuntimeValue::Str(value_i64(&args[0])?.to_string())),
        "format_float64" => {
            let value = value_f64(&args[0])?;
            Ok(RuntimeValue::Str(if value.is_nan() {
                "nan".to_string()
            } else if value == f64::INFINITY {
                "inf".to_string()
            } else if value == f64::NEG_INFINITY {
                "-inf".to_string()
            } else {
                value.to_string()
            }))
        }
        "format_bool" => Ok(RuntimeValue::Str(value_bool(&args[0])?.to_string())),
        "bytes_len" => Ok(RuntimeValue::Int(
            i64::try_from(value_byte_items(&args[0])?.len()).unwrap_or(i64::MAX),
        )),
        "bytes_slice" => {
            let bytes = value_byte_items(&args[0])?;
            let start = usize::try_from(value_i64(&args[1])?).context("slice start < 0")?;
            let end = usize::try_from(value_i64(&args[2])?).context("slice end < 0")?;
            let slice = bytes
                .get(start..end)
                .context("byte slice range is out of bounds")?;
            Ok(RuntimeValue::Array(slice.to_vec()))
        }
        "bytes_from_str" => {
            let value = value_str(&args[0])?;
            Ok(runtime_bytes(value.as_bytes().to_vec()))
        }
        "bytes_get" => {
            let index = usize::try_from(value_i64(&args[1])?).context("byte index too large")?;
            Ok(option_value(
                sig,
                value_byte_items(&args[0])?
                    .get(index)
                    .map(runtime_byte)
                    .transpose()?
                    .map(|value| RuntimeValue::Int(i64::from(value))),
            ))
        }
        "bytes_concat" => {
            let left = value_byte_items(&args[0])?;
            let right = value_byte_items(&args[1])?;
            let Some(len) = left.len().checked_add(right.len()) else {
                return Ok(result_err(sig, "limit-exceeded", None));
            };
            if len > config.max_alloc_len {
                return Ok(result_err(sig, "limit-exceeded", None));
            }
            let mut joined = Vec::with_capacity(len);
            joined.extend_from_slice(left);
            joined.extend_from_slice(right);
            Ok(result_ok(sig, RuntimeValue::Array(joined)))
        }
        "bytes_find" => {
            let haystack = value_byte_items(&args[0])?;
            let needle = value_byte_items(&args[1])?;
            let found = if needle.is_empty() {
                Some(0)
            } else {
                haystack.windows(needle.len()).position(|window| {
                    window
                        .iter()
                        .zip(needle)
                        .all(|(left, right)| runtime_byte(left).ok() == runtime_byte(right).ok())
                })
            };
            Ok(option_value(
                sig,
                found.map(|value| RuntimeValue::Int(value.try_into().unwrap_or(i64::MAX))),
            ))
        }
        "bytes_from_array" => {
            let RuntimeValue::Array(values) = untyped(&args[0]) else {
                bail!("bytes_from_array expects an array")
            };
            if values.len() > config.max_alloc_len {
                return Ok(result_err(sig, "limit-exceeded", None));
            }
            let mut bytes = Vec::with_capacity(values.len());
            for value in values {
                let value = value_i64(value)?;
                let Ok(value) = u8::try_from(value) else {
                    return Ok(result_err(sig, "invalid-byte", None));
                };
                bytes.push(value);
            }
            Ok(result_ok(sig, runtime_bytes(bytes)))
        }
        "bytes_decode_utf8" => {
            let items = value_byte_items(&args[0])?;
            if items.len() > config.max_alloc_len {
                return Ok(result_err(sig, "limit-exceeded", None));
            }
            let mut bytes = Vec::with_capacity(items.len());
            for item in items {
                bytes.push(runtime_byte(item)?);
            }
            Ok(match String::from_utf8(bytes) {
                Ok(value) => result_ok(sig, RuntimeValue::Str(value)),
                Err(_) => result_err(sig, "invalid-utf8", None),
            })
        }
        "fs_open_read"
        | "fs_open_write"
        | "fs_open_write_options"
        | "fs_open_append"
        | "fs_open_read_write" => {
            let path = value_string(&args[0])?;
            let mut resolved: Option<PathBuf> = None;
            let checks: Vec<(&PolicyType, CapabilityDomain)> = match name {
                "fs_open_read" => vec![(value_policy(&args[1])?, CapabilityDomain::FsRead)],
                "fs_open_write" | "fs_open_append" => {
                    vec![(value_policy(&args[1])?, CapabilityDomain::FsWrite)]
                }
                "fs_open_write_options" => {
                    vec![(value_policy(&args[4])?, CapabilityDomain::FsWrite)]
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
                "fs_open_write_options" => options
                    .create(value_bool(&args[1])?)
                    .create_new(value_bool(&args[2])?)
                    .truncate(value_bool(&args[3])?)
                    .write(true),
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
        "fs_read_dir_entries" => {
            let path = value_string(&args[0])?;
            let logical_path = PathBuf::from(&path);
            let policy = value_policy(&args[1])?;
            let p = match policy_path_or_denied(sig, &path, policy, CapabilityDomain::FsRead) {
                Ok(p) => p,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || {
                    let mut entries = Vec::new();
                    for entry in fs::read_dir(p)? {
                        let entry = entry?;
                        let metadata = entry.metadata()?;
                        let name = entry.file_name();
                        entries.push(RuntimeValue::Record(BTreeMap::from([
                            (
                                "path".to_string(),
                                RuntimeValue::Str(logical_path.join(&name).display().to_string()),
                            ),
                            (
                                "name".to_string(),
                                RuntimeValue::Str(name.to_string_lossy().to_string()),
                            ),
                            ("is-dir".to_string(), RuntimeValue::Bool(metadata.is_dir())),
                            (
                                "size".to_string(),
                                RuntimeValue::Int(metadata.len().try_into().unwrap_or(i64::MAX)),
                            ),
                        ])));
                    }
                    Ok(entries)
                },
                RuntimeValue::Array,
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
        "fs_rename" => {
            let from = value_string(&args[0])?;
            let to = value_string(&args[1])?;
            let policy = value_policy(&args[2])?;
            let from = match policy_path_or_denied(sig, &from, policy, CapabilityDomain::FsWrite) {
                Ok(path) => path,
                Err(denied) => return Ok(*denied),
            };
            let to = match policy_path_or_denied(sig, &to, policy, CapabilityDomain::FsWrite) {
                Ok(path) => path,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || fs::rename(from, to),
                |_| RuntimeValue::Void,
            ))
        }
        "fs_copy" => {
            let from = value_string(&args[0])?;
            let to = value_string(&args[1])?;
            let read_policy = value_policy(&args[2])?;
            let write_policy = value_policy(&args[3])?;
            let from =
                match policy_path_or_denied(sig, &from, read_policy, CapabilityDomain::FsRead) {
                    Ok(path) => path,
                    Err(denied) => return Ok(*denied),
                };
            let to = match policy_path_or_denied(sig, &to, write_policy, CapabilityDomain::FsWrite)
            {
                Ok(path) => path,
                Err(denied) => return Ok(*denied),
            };
            Ok(fs_result(
                sig,
                || fs::copy(from, to),
                |_| RuntimeValue::Void,
            ))
        }
        "env_get" => {
            let var = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::EnvRead, &var).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            if let Some(environment) = &config.injected_environment {
                let environment = environment.lock().expect("injected environment poisoned");
                return Ok(match environment.get(&var) {
                    Some(value) => result_ok(sig, RuntimeValue::Str(value.clone())),
                    None => result_err(sig, "not-found", None),
                });
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
            if let Some(environment) = &config.injected_environment {
                environment
                    .lock()
                    .expect("injected environment poisoned")
                    .insert(var, value);
            } else {
                std::env::set_var(var, value);
            }
            Ok(result_ok(sig, RuntimeValue::Void))
        }
        "env_remove" => {
            let var = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::EnvWrite, &var).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            if !is_valid_env_name(&var) {
                return Ok(result_err(sig, "invalid-name", None));
            }
            if let Some(environment) = &config.injected_environment {
                environment
                    .lock()
                    .expect("injected environment poisoned")
                    .remove(&var);
            } else {
                std::env::remove_var(var);
            }
            Ok(result_ok(sig, RuntimeValue::Void))
        }
        "env_list" => {
            let policy = value_policy(&args[0])?;
            let mut names = if let Some(environment) = &config.injected_environment {
                environment
                    .lock()
                    .expect("injected environment poisoned")
                    .keys()
                    .filter(|name| {
                        ensure_policy_scope(policy, CapabilityDomain::EnvRead, name).is_ok()
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                std::env::vars_os()
                    .filter_map(|(name, _)| name.into_string().ok())
                    .filter(|name| {
                        ensure_policy_scope(policy, CapabilityDomain::EnvRead, name).is_ok()
                    })
                    .collect::<Vec<_>>()
            };
            names.sort();
            Ok(RuntimeValue::Array(
                names.into_iter().map(RuntimeValue::Str).collect(),
            ))
        }
        "net_address_parse" => match value_string(&args[0])?.parse::<SocketAddr>() {
            Ok(address) => Ok(result_ok(sig, RuntimeValue::Str(address.to_string()))),
            Err(error) => Ok(result_err(sig, "invalid-address", Some(error.to_string()))),
        },
        "net_resolve" => {
            let target = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::NetConnect, &target).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            match target.to_socket_addrs() {
                Ok(addresses) => Ok(result_ok(
                    sig,
                    RuntimeValue::Array(
                        addresses
                            .map(|a| RuntimeValue::Str(a.to_string()))
                            .collect(),
                    ),
                )),
                Err(error) => Ok(result_err(sig, "dns-failed", Some(error.to_string()))),
            }
        }
        "net_connect" => {
            let target = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::NetConnect, &target).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            if files.at_capacity() {
                return Ok(result_err(
                    sig,
                    "resource-limit",
                    Some("host resource limit reached".into()),
                ));
            }
            match TcpStream::connect(&target) {
                Ok(stream) => {
                    let id = files.insert_tcp_stream(stream);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id,
                            access: HandleAccess::ReadWrite,
                        }),
                    ))
                }
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_listen" => {
            let target = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::NetListen, &target).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            if files.at_capacity() {
                return Ok(result_err(
                    sig,
                    "resource-limit",
                    Some("host resource limit reached".into()),
                ));
            }
            match TcpListener::bind(&target) {
                Ok(listener) => {
                    let id = files.insert_tcp_listener(listener);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id,
                            access: HandleAccess::Read,
                        }),
                    ))
                }
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_accept" => {
            let id = value_handle(&args[0])?;
            if let Some(error) = files.lifecycle_error(id) {
                return Ok(result_err(sig, error.tag(), None));
            }
            if files.at_capacity() {
                return Ok(result_err(
                    sig,
                    "resource-limit",
                    Some("host resource limit reached".into()),
                ));
            }
            let accepted = match files.get_mut(id)? {
                FileHandle::TcpListener(listener) => listener.accept(),
                _ => return Ok(result_err(sig, "invalid-handle", None)),
            };
            match accepted {
                Ok((stream, _)) => {
                    let id = files.insert_tcp_stream(stream);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id,
                            access: HandleAccess::ReadWrite,
                        }),
                    ))
                }
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_local_address" => {
            let id = value_handle(&args[0])?;
            if let Some(error) = files.lifecycle_error(id) {
                return Ok(result_err(sig, error.tag(), None));
            }
            let address = match files.get_mut(id)? {
                FileHandle::TcpStream(v) => v.local_addr(),
                FileHandle::TcpListener(v) => v.local_addr(),
                FileHandle::UdpSocket(v) => v.local_addr(),
                _ => return Ok(result_err(sig, "invalid-handle", None)),
            };
            match address {
                Ok(v) => Ok(result_ok(sig, RuntimeValue::Str(v.to_string()))),
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_set_deadline" => {
            let id = value_handle(&args[0])?;
            let millis =
                u64::try_from(value_i64(&args[1])?).context("deadline must be non-negative")?;
            let timeout = (millis != 0).then(|| Duration::from_millis(millis));
            let result = match files.get_mut(id)? {
                FileHandle::TcpStream(v) => v
                    .set_read_timeout(timeout)
                    .and_then(|_| v.set_write_timeout(timeout)),
                _ => return Ok(result_err(sig, "invalid-handle", None)),
            };
            match result {
                Ok(()) => Ok(result_ok(sig, RuntimeValue::Void)),
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_shutdown" => {
            let id = value_handle(&args[0])?;
            let direction = match value_string(&args[1])?.as_str() {
                "read" => Shutdown::Read,
                "write" => Shutdown::Write,
                "both" => Shutdown::Both,
                _ => {
                    return Ok(result_err(
                        sig,
                        "invalid-input",
                        Some("expected read, write, or both".into()),
                    ))
                }
            };
            let result = match files.get_mut(id)? {
                FileHandle::TcpStream(v) => v.shutdown(direction),
                _ => return Ok(result_err(sig, "invalid-handle", None)),
            };
            match result {
                Ok(()) => Ok(result_ok(sig, RuntimeValue::Void)),
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_udp_bind" => {
            let target = value_string(&args[0])?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::NetListen, &target).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            if files.at_capacity() {
                return Ok(result_err(
                    sig,
                    "resource-limit",
                    Some("host resource limit reached".into()),
                ));
            }
            match UdpSocket::bind(&target) {
                Ok(socket) => {
                    let id = files.insert_udp_socket(socket);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id,
                            access: HandleAccess::ReadWrite,
                        }),
                    ))
                }
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_udp_connect" => {
            let id = value_handle(&args[0])?;
            let target = value_string(&args[1])?;
            let policy = value_policy(&args[2])?;
            if ensure_policy_scope(policy, CapabilityDomain::NetConnect, &target).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            let result = match files.get_mut(id)? {
                FileHandle::UdpSocket(v) => v.connect(&target),
                _ => return Ok(result_err(sig, "invalid-handle", None)),
            };
            match result {
                Ok(()) => Ok(result_ok(sig, RuntimeValue::Void)),
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_udp_send_to" => {
            let id = value_handle(&args[0])?;
            let bytes = value_bytes(&args[1])?;
            let target = value_string(&args[2])?;
            let policy = value_policy(&args[3])?;
            if ensure_policy_scope(policy, CapabilityDomain::NetConnect, &target).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            let result = match files.get_mut(id)? {
                FileHandle::UdpSocket(v) => v.send_to(&bytes, &target),
                _ => return Ok(result_err(sig, "invalid-handle", None)),
            };
            match result {
                Ok(count) => Ok(result_ok(
                    sig,
                    RuntimeValue::Int(count.try_into().unwrap_or(i64::MAX)),
                )),
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "net_udp_recv_from" => {
            let id = value_handle(&args[0])?;
            let len = match checked_alloc_len(value_i64(&args[1])?, config) {
                Ok(v) => v,
                Err((tag, message)) => return Ok(result_err(sig, tag, Some(message))),
            };
            let mut bytes = vec![0; len];
            let result = match files.get_mut(id)? {
                FileHandle::UdpSocket(v) => v.recv_from(&mut bytes),
                _ => return Ok(result_err(sig, "invalid-handle", None)),
            };
            match result {
                Ok((count, from)) => {
                    bytes.truncate(count);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::Record(BTreeMap::from([
                            ("data".into(), runtime_bytes(bytes)),
                            ("from".into(), RuntimeValue::Str(from.to_string())),
                        ])),
                    ))
                }
                Err(error) => Ok(net_io_error(sig, error)),
            }
        }
        "process_run" | "process_spawn" => {
            let RuntimeValue::Record(command) = untyped(&args[0]) else {
                bail!("process command must be a record")
            };
            let field = |name: &str| {
                command
                    .get(name)
                    .with_context(|| format!("process command is missing `{name}`"))
            };
            let executable = value_string(field("executable")?)?;
            let policy = value_policy(&args[1])?;
            if ensure_policy_scope(policy, CapabilityDomain::ProcessRun, &executable).is_err() {
                return Ok(result_err(sig, "permission-denied", None));
            }
            let RuntimeValue::Array(argv) = untyped(field("arguments")?) else {
                bail!("process arguments must be an array")
            };
            let RuntimeValue::Map(environment) = untyped(field("environment")?) else {
                bail!("process environment must be a string map")
            };
            let cwd = value_string(field("cwd")?)?;
            let RuntimeValue::Enum { tag: stdio, .. } = untyped(field("stdio")?) else {
                bail!("process stdio must be a policy enum")
            };
            if name == "process_run" && stdio == "stream" {
                return Ok(result_err(
                    sig,
                    "unsupported",
                    Some("stream stdio requires process.spawn".to_string()),
                ));
            }
            let mut command = Command::new(&executable);
            for arg in argv {
                command.arg(value_str(arg)?);
            }
            command.env_clear();
            for (key, value) in environment {
                command.env(value_str(key)?, value_str(value)?);
            }
            if !cwd.is_empty() {
                command.current_dir(cwd);
            }
            match stdio.as_str() {
                "capture" => {
                    command
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                }
                "stream" => {
                    command
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                }
                "inherit" => {
                    command
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());
                }
                "null" => {
                    command
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());
                }
                _ => return Ok(result_err(sig, "invalid-input", None)),
            }
            if name == "process_run" {
                Ok(match command.output() {
                    Ok(output) => result_ok(
                        sig,
                        process_output_value(output.status, output.stdout, output.stderr),
                    ),
                    Err(error) => process_io_error(sig, error),
                })
            } else if files.at_capacity() {
                Ok(result_err(sig, "resource-exhausted", None))
            } else {
                Ok(match command.spawn() {
                    Ok(child) => result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id: files.insert_child(child),
                            access: HandleAccess::Process,
                        }),
                    ),
                    Err(error) => process_io_error(sig, error),
                })
            }
        }
        "process_wait" => {
            let handle = value_handle(&args[0])?;
            let Some(FileHandle::Child(child)) = files.handles.remove(&handle) else {
                return Ok(result_err(sig, "resource-closed", None));
            };
            Ok(match child.wait_with_output() {
                Ok(output) => result_ok(
                    sig,
                    process_output_value(output.status, output.stdout, output.stderr),
                ),
                Err(error) => process_io_error(sig, error),
            })
        }
        "process_kill" => {
            let handle = value_handle(&args[0])?;
            let Some(FileHandle::Child(child)) = files.handles.get_mut(&handle) else {
                return Ok(result_err(sig, "resource-closed", None));
            };
            Ok(match child.kill() {
                Ok(()) => result_ok(sig, RuntimeValue::Void),
                Err(error) => process_io_error(sig, error),
            })
        }
        "process_child_stdin" | "process_child_stdout" | "process_child_stderr" => {
            let handle = value_handle(&args[0])?;
            if files.at_capacity() {
                return Ok(result_err(sig, "resource-exhausted", None));
            }
            let Some(FileHandle::Child(child)) = files.handles.get_mut(&handle) else {
                return Ok(result_err(sig, "resource-closed", None));
            };
            match name {
                "process_child_stdin" => {
                    let Some(pipe) = child.stdin.take() else {
                        return Ok(result_err(sig, "unavailable", None));
                    };
                    let id = files.insert_child_stdin(pipe);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id,
                            access: HandleAccess::Write,
                        }),
                    ))
                }
                "process_child_stdout" => {
                    let Some(pipe) = child.stdout.take() else {
                        return Ok(result_err(sig, "unavailable", None));
                    };
                    let id = files.insert_child_stdout(pipe);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id,
                            access: HandleAccess::Read,
                        }),
                    ))
                }
                _ => {
                    let Some(pipe) = child.stderr.take() else {
                        return Ok(result_err(sig, "unavailable", None));
                    };
                    let id = files.insert_child_stderr(pipe);
                    Ok(result_ok(
                        sig,
                        RuntimeValue::HostHandle(HostHandle {
                            id,
                            access: HandleAccess::Read,
                        }),
                    ))
                }
            }
        }
        "clock_now_unix_millis" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::Clock, "*")?;
            if let Some(clock) = &config.injected_clock {
                let millis = clock.lock().expect("injected clock poisoned").unix_millis;
                return Ok(RuntimeValue::Int(i64::try_from(millis).unwrap_or(i64::MAX)));
            }
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock before unix epoch")?
                .as_millis();
            Ok(RuntimeValue::Int(i64::try_from(millis).unwrap_or(i64::MAX)))
        }
        "clock_monotonic_millis" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::Clock, "*")?;
            if let Some(clock) = &config.injected_clock {
                let millis = clock
                    .lock()
                    .expect("injected clock poisoned")
                    .monotonic_millis;
                return Ok(RuntimeValue::Int(i64::try_from(millis).unwrap_or(i64::MAX)));
            }
            static EPOCH: OnceLock<Instant> = OnceLock::new();
            let millis = EPOCH.get_or_init(Instant::now).elapsed().as_millis();
            Ok(RuntimeValue::Int(i64::try_from(millis).unwrap_or(i64::MAX)))
        }
        "clock_duration_from_millis" => Ok(RuntimeValue::Int(value_i64(&args[0])?)),
        "clock_duration_add" => {
            let left = value_i64(&args[0])?;
            let right = value_i64(&args[1])?;
            match left.checked_add(right) {
                Some(value) if value >= 0 => Ok(result_ok(sig, RuntimeValue::Int(value))),
                _ => Ok(result_err(sig, "overflow", None)),
            }
        }
        "clock_duration_between" => {
            let earlier = value_i64(&args[0])?;
            let later = value_i64(&args[1])?;
            match later.checked_sub(earlier) {
                Some(value) if value >= 0 => Ok(result_ok(sig, RuntimeValue::Int(value))),
                _ => Ok(result_err(sig, "invalid-order", None)),
            }
        }
        "clock_sleep_millis" => {
            let millis = value_i64(&args[0])?;
            let policy = value_policy(&args[1])?;
            ensure_policy_scope(policy, CapabilityDomain::Clock, "*")?;
            let millis = u64::try_from(millis).context("sleep duration must be non-negative")?;
            if let Some(clock) = &config.injected_clock {
                let mut clock = clock.lock().expect("injected clock poisoned");
                clock.monotonic_millis = clock.monotonic_millis.saturating_add(millis);
                clock.unix_millis = clock.unix_millis.saturating_add(millis);
            } else {
                std::thread::sleep(Duration::from_millis(millis));
            }
            Ok(RuntimeValue::Void)
        }
        "random_bytes" => {
            let len = value_i64(&args[0])?;
            let policy = value_policy(&args[1])?;
            ensure_policy_scope(policy, CapabilityDomain::Random, "*")?;
            let len = checked_alloc_len(len, config)
                .map_err(|(tag, msg)| anyhow::anyhow!("random_bytes {tag}: {msg}"))?;
            let mut buf = vec![0u8; len];
            if let Some(random) = &config.injected_random {
                random
                    .lock()
                    .expect("injected random source poisoned")
                    .fill_bytes(&mut buf);
            } else {
                getrandom::getrandom(&mut buf)
                    .map_err(|err| anyhow::anyhow!("random_bytes unavailable: {err}"))?;
            }
            Ok(RuntimeValue::Array(
                buf.into_iter()
                    .map(|byte| RuntimeValue::Int(i64::from(byte)))
                    .collect(),
            ))
        }
        "system_info" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::SystemInfo, "*")?;
            Ok(RuntimeValue::Record(BTreeMap::from([
                (
                    "architecture".to_string(),
                    RuntimeValue::Str(std::env::consts::ARCH.to_string()),
                ),
                (
                    "family".to_string(),
                    RuntimeValue::Str(std::env::consts::FAMILY.to_string()),
                ),
                (
                    "operating-system".to_string(),
                    RuntimeValue::Str(std::env::consts::OS.to_string()),
                ),
            ])))
        }
        "system_args" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::SystemInfo, "*")?;
            Ok(RuntimeValue::Array(
                std::iter::once(config.program_name.clone())
                    .chain(config.argv.iter().cloned())
                    .map(RuntimeValue::Str)
                    .collect(),
            ))
        }
        "system_current_dir" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::SystemInfo, "*")?;
            Ok(fs_result(sig, std::env::current_dir, |path| {
                RuntimeValue::Str(path.display().to_string())
            }))
        }
        "system_executable" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::SystemInfo, "*")?;
            Ok(fs_result(sig, std::env::current_exe, |path| {
                RuntimeValue::Str(path.display().to_string())
            }))
        }
        "system_temp_dir" => {
            let policy = value_policy(&args[0])?;
            ensure_policy_scope(policy, CapabilityDomain::SystemInfo, "*")?;
            Ok(RuntimeValue::Str(
                std::env::temp_dir().display().to_string(),
            ))
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
fn read_line_from_file(file: &mut impl Read) -> std::io::Result<String> {
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

#[cfg(test)]
mod source_task_tests {
    use super::*;
    use crate::async_runtime::EventKind;

    #[test]
    fn source_spawn_join_uses_scheduler_and_consumes_handles() {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vibra");
        std::fs::write(
            &entry,
            r#"main:
  $function: $void
  return: $void
  do:
    - $spawn: first
      captures: []
      value: 41
    - $spawn: second
      captures: []
      value: 42
    - $join: second
      into: second-result
    - $join: first
      into: first-result
"#,
        )
        .unwrap();
        let loaded = crate::load::load_program(&entry).unwrap();
        let lowered = crate::lower::lower_program(&loaded).unwrap();
        run_lowered_interpreted(&lowered, &RunConfig::default()).unwrap();

        SOURCE_TASKS.with(|tasks| {
            let tasks = tasks.borrow();
            assert!(tasks.handles.is_empty());
            assert!(tasks.results.is_empty());
            let kinds: Vec<_> = tasks
                .scheduler
                .trace()
                .iter()
                .map(|event| event.kind.clone())
                .collect();
            assert_eq!(
                kinds
                    .iter()
                    .filter(|kind| **kind == EventKind::TaskCreated)
                    .count(),
                2
            );
            assert_eq!(
                kinds
                    .iter()
                    .filter(|kind| **kind == EventKind::JoinCompleted)
                    .count(),
                2
            );
        });
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

#[cfg(test)]
mod text_conversion_tests {
    use super::*;

    fn lower(body: &str) -> LoweredProgram {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let import = |name: &str| {
            root.join(format!("stdlib/src/{name}.vibra"))
                .display()
                .to_string()
                .replace('\\', "/")
        };
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("entry.vibra");
        std::fs::write(
            &entry,
            format!(
                "text:\n  $import: \"{}\"\nconvert:\n  $import: \"{}\"\nbytes:\n  $import: \"{}\"\noption:\n  $import: \"{}\"\nresult:\n  $import: \"{}\"\ntest:\n  $import: \"{}\"\nmain:\n  $function: $void\n  return: $void\n  do:\n{}",
                import("text"),
                import("convert"),
                import("bytes"),
                import("option"),
                import("result"),
                import("test"),
                body
            ),
        )
        .unwrap();
        let loaded = crate::load::load_program(&entry).unwrap();
        crate::lower::lower_program(&loaded).unwrap()
    }

    #[test]
    fn unicode_parse_and_invalid_utf8_match_in_interpreter_and_wasm() {
        let program = lower(
            r#"  - $test.assert-eq-int:
      actual: {$text.scalar-len: aé🙂}
      expected: 3
  - $let:
      parsed: {$convert.parse-int64: '-42'}
  - $match: $parsed
    when:
    - case: {$result.result.ok: -42}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: parse-failed}]
  - $let:
      raw:
        $cast: {$array: [255]}
        into: $bytes.bytes
  - $let:
      decoded: {$bytes.bytes.decode-utf8: $raw}
  - $match: $decoded
    when:
    - case: {$result.result.err: {$bytes.bytes-error.invalid-utf8: null}}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: invalid-utf8-was-accepted}]
"#,
        );
        run_lowered_interpreted(&program, &RunConfig::default()).unwrap();
        run_lowered(&program, &RunConfig::default()).unwrap();
    }

    #[test]
    fn text_growth_limit_is_a_typed_result_in_both_backends() {
        let program = lower(
            r#"  - $let:
      replaced:
        $text.replace: a
        from: a
        to: xx
  - $match: $replaced
    when:
    - case: {$result.result.err: {$text.text-error.limit-exceeded: null}}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: replace-limit-was-not-enforced}]
  - $let:
      joined:
        $text.join: {$array: [a, a]}
        separator: ''
  - $match: $joined
    when:
    - case: {$result.result.err: {$text.text-error.limit-exceeded: null}}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: join-limit-was-not-enforced}]
  - $let:
      uppercase: {$text.uppercase: ß}
  - $match: $uppercase
    when:
    - case: {$result.result.err: {$text.text-error.limit-exceeded: null}}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: case-limit-was-not-enforced}]
  - $let:
      split:
        $text.split: 'a,'
        separator: ','
  - $match: $split
    when:
    - case: {$result.result.err: {$text.text-error.limit-exceeded: null}}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: split-element-limit-was-not-enforced}]
  - $let:
      left:
        $cast: {$array: [97]}
        into: $bytes.bytes
  - $let:
      right:
        $cast: {$array: [98]}
        into: $bytes.bytes
  - $let:
      concatenated:
        $bytes.bytes.concat: $left
        other: $right
  - $match: $concatenated
    when:
    - case: {$result.result.err: {$bytes.bytes-error.limit-exceeded: null}}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: bytes-concat-limit-was-not-enforced}]
  - $let:
      raw:
        $cast: {$array: [97, 98]}
        into: $bytes.bytes
  - $let:
      decoded: {$bytes.bytes.decode-utf8: $raw}
  - $match: $decoded
    when:
    - case: {$result.result.err: {$bytes.bytes-error.limit-exceeded: null}}
      do: [{$test.assert: true}]
    - case: {$wildcard: null}
      do: [{$test.fail: bytes-decode-limit-was-not-enforced}]
"#,
        );
        let config = RunConfig {
            max_alloc_len: 1,
            ..RunConfig::default()
        };
        run_lowered_interpreted(&program, &config).unwrap();
        run_lowered(&program, &config).unwrap();
    }
}

#[cfg(test)]
mod resource_lifecycle_tests {
    use super::{FileHandle, FileTable, HandleLifecycleError};
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream, UdpSocket};

    #[test]
    fn stream_write_some_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.bin");
        let mut table = FileTable::new(1);
        let id = table.insert(std::fs::File::create(&path).unwrap());

        let written = table.write_some_to_handle(id, b"abcdef").unwrap();
        assert!(written > 0 && written <= 6);
        assert_eq!(std::fs::read(path).unwrap(), b"abcdef"[..written]);
    }

    #[test]
    fn child_wait_consumes_the_process_resource() {
        let executable = std::env::current_exe().unwrap();
        let child = std::process::Command::new(executable)
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut table = FileTable::new(1);
        let id = table.insert_child(child);
        let super::FileHandle::Child(child) = table.handles.remove(&id).unwrap() else {
            panic!("inserted child changed resource kind");
        };
        let status = child.wait_with_output().unwrap().status;
        assert!(status.success());
        assert_eq!(
            table.lifecycle_error(id),
            Some(HandleLifecycleError::Closed)
        );
    }

    #[test]
    fn child_pipes_become_independent_stream_resources() {
        let executable = std::env::current_exe().unwrap();
        let child = std::process::Command::new(executable)
            .arg("--list")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut table = FileTable::new(8);
        let child_id = table.insert_child(child);
        let super::FileHandle::Child(child) = table.handles.get_mut(&child_id).unwrap() else {
            panic!("inserted child changed resource kind");
        };
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdin_id = table.insert_child_stdin(stdin);
        let stdout_id = table.insert_child_stdout(stdout);
        let stderr_id = table.insert_child_stderr(stderr);

        table.close(stdin_id).unwrap();
        let mut stdout = Vec::new();
        let super::FileHandle::ChildStdout(pipe) = table.handles.get_mut(&stdout_id).unwrap()
        else {
            panic!("stdout pipe changed resource kind");
        };
        pipe.read_to_end(&mut stdout).unwrap();
        assert!(!stdout.is_empty());
        table.close(stdout_id).unwrap();
        table.close(stderr_id).unwrap();
        let super::FileHandle::Child(child) = table.handles.remove(&child_id).unwrap() else {
            panic!("child changed resource kind");
        };
        assert!(child.wait_with_output().unwrap().status.success());
    }

    #[test]
    fn close_is_linear_and_classifies_minted_ids_as_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resource.txt");
        let mut table = FileTable::new(1);
        let id = table.insert(std::fs::File::create(&path).unwrap());

        assert_eq!(table.open_resource_count(), 1);
        assert_eq!(table.close(id), Ok(()));
        assert_eq!(table.open_resource_count(), 0);
        assert_eq!(
            table.lifecycle_error(id),
            Some(HandleLifecycleError::Closed)
        );
        assert_eq!(table.close(id), Err(HandleLifecycleError::Closed));
        assert_eq!(
            table.lifecycle_error(u64::MAX),
            Some(HandleLifecycleError::Invalid)
        );
    }

    #[test]
    fn stdin_is_singleton_and_cannot_grow_the_table() {
        let mut table = FileTable::new(1);
        let first = table.insert_stdin();
        let second = table.insert_stdin();
        assert_eq!(first, second);
        assert_eq!(table.open_resource_count(), 0);
    }

    #[test]
    fn reserved_output_close_is_a_noop() {
        let mut table = FileTable::new(1);
        assert_eq!(table.close(1), Ok(()));
        assert_eq!(table.close(1), Ok(()));
        assert_eq!(table.lifecycle_error(1), None);
    }

    #[test]
    fn dropping_table_releases_live_owned_resources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("leaked.txt");
        let mut table = FileTable::new(1);
        table.insert(std::fs::File::create(&path).unwrap());
        drop(table);

        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .expect("instance teardown must release owned OS files");
    }

    #[test]
    fn repeated_open_close_uses_constant_lifecycle_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("churn.txt");
        let mut table = FileTable::new(1);

        for _ in 0..10_000 {
            let id = table.insert(std::fs::File::create(&path).unwrap());
            assert_eq!(table.close(id), Ok(()));
            assert_eq!(
                table.lifecycle_error(id),
                Some(HandleLifecycleError::Closed)
            );
        }

        // Only the two borrowed output streams remain. Closed-ID tracking is
        // represented by the single monotonic `next` counter, not tombstones.
        assert_eq!(table.handles.len(), 2);
        assert_eq!(table.open_resource_count(), 0);
        assert_eq!(
            table.lifecycle_error(table.next),
            Some(HandleLifecycleError::Invalid)
        );
    }

    #[test]
    fn tcp_streams_share_io_and_instance_cleanup() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.write_all(b"ping").unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            let mut reply = Vec::new();
            stream.read_to_end(&mut reply).unwrap();
            reply
        });

        let mut table = FileTable::new(4);
        let listener_id = table.insert_tcp_listener(listener);
        let stream = match table.get_mut(listener_id).unwrap() {
            FileHandle::TcpListener(listener) => listener.accept().unwrap().0,
            _ => unreachable!(),
        };
        let stream_id = table.insert_tcp_stream(stream);
        let mut request = Vec::new();
        match table.get_mut(stream_id).unwrap() {
            FileHandle::TcpStream(stream) => stream.read_to_end(&mut request).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(request, b"ping");
        table.write_to_handle(stream_id, b"pong").unwrap();
        table.close(stream_id).unwrap();
        table.close(listener_id).unwrap();
        assert_eq!(client.join().unwrap(), b"pong");
        assert_eq!(table.open_resource_count(), 0);
    }

    #[test]
    fn udp_resources_exchange_datagrams_and_close() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let receiver_address = receiver.local_addr().unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();

        let mut table = FileTable::new(2);
        let receiver_id = table.insert_udp_socket(receiver);
        let sender_id = table.insert_udp_socket(sender);
        match table.get_mut(sender_id).unwrap() {
            FileHandle::UdpSocket(socket) => {
                socket.send_to(b"datagram", receiver_address).unwrap();
            }
            _ => unreachable!(),
        }
        let mut bytes = [0; 32];
        let count = match table.get_mut(receiver_id).unwrap() {
            FileHandle::UdpSocket(socket) => socket.recv_from(&mut bytes).unwrap().0,
            _ => unreachable!(),
        };
        assert_eq!(&bytes[..count], b"datagram");
        table.close(sender_id).unwrap();
        table.close(receiver_id).unwrap();
        assert_eq!(table.open_resource_count(), 0);
    }
}
