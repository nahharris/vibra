//! Deterministic, single-threaded structured-concurrency runtime prototype.
//!
//! This module deliberately exposes no native-thread assumptions. Embedders can
//! script completions against an injected monotonic clock while the language
//! surface and host adapter are developed independently.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScopeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OperationId(pub u64);

/// Host-visible token. Reused slots always receive a new generation, so a late
/// completion can never complete a newer operation (the ABA problem).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OperationToken {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CancellationMode {
    Guaranteed,
    DrainRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "command")]
pub enum HostCommand {
    Start {
        token: OperationToken,
        resource: ResourceId,
        request: String,
    },
    Cancel {
        token: OperationToken,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCompletion {
    pub token: OperationToken,
    pub result: HostOperationResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "status", content = "value")]
pub enum HostOperationResult {
    Completed(String),
    Failed(String),
}

/// An adapter consumes scheduler commands and returns whichever completions
/// are ready at the injected monotonic instant. It need not use a thread.
pub trait AsyncHostAdapter {
    fn dispatch(&mut self, commands: Vec<HostCommand>);
    fn poll(&mut self, now: u64) -> Vec<HostCompletion>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeLimits {
    pub live_tasks: usize,
    pub pending_operations: usize,
    pub open_resources: usize,
    pub draining_operations: usize,
}

impl Default for ScopeLimits {
    fn default() -> Self {
        Self {
            live_tasks: 1024,
            pending_operations: 1024,
            open_resources: 256,
            draining_operations: 256,
        }
    }
}

impl ScopeLimits {
    fn is_within(self, parent: Self) -> bool {
        self.live_tasks <= parent.live_tasks
            && self.pending_operations <= parent.pending_operations
            && self.open_resources <= parent.open_resources
            && self.draining_operations <= parent.draining_operations
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceLimit {
    LiveTasks,
    PendingOperations,
    OpenResources,
    DrainingOperations,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CapabilityGrant {
    pub domain: String,
    /// Hierarchical resource prefix. Empty means the whole domain.
    pub resource_prefix: String,
}

impl CapabilityGrant {
    fn is_within(&self, parent: &Self) -> bool {
        if self.domain != parent.domain {
            return false;
        }
        parent.resource_prefix.is_empty()
            || self.resource_prefix == parent.resource_prefix
            || self
                .resource_prefix
                .strip_prefix(&parent.resource_prefix)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CancelReason {
    Explicit,
    ParentCancelled,
    Deadline,
    SiblingFailed,
    CapabilityRevoked,
    ScopeClosed,
    ResourceLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "status", content = "value")]
pub enum TaskOutcome {
    Completed(String),
    Failed(String),
    Cancelled(CancelReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TaskState {
    Waiting { at: u64, outcome: TaskOutcome },
    Terminal(TaskOutcome),
}

#[derive(Debug)]
struct Task {
    scope: ScopeId,
    state: TaskState,
}

#[derive(Debug)]
struct Scope {
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    tasks: Vec<TaskId>,
    resources: Vec<ResourceId>,
    grants: BTreeSet<CapabilityGrant>,
    limits: ScopeLimits,
    deadline: Option<u64>,
    cancelled: Option<CancelReason>,
    closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    ScopeOpened,
    ScopeCancelled,
    ScopeCompleted,
    TaskCreated,
    TaskWaiting,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
    JoinCompleted,
    ResourceOpened,
    ResourceClosed,
    OperationStarted,
    OperationCompleted,
    OperationCancelled,
    LateCompletionDrained,
    AdmissionRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub at: u64,
    pub kind: EventKind,
    pub scope: ScopeId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<CancelReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TraceDocument<'a> {
    #[serde(rename = "contract-version")]
    pub contract_version: &'static str,
    pub events: &'a [TraceEvent],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    UnknownScope,
    ScopeClosed,
    CapabilityAmplification(CapabilityGrant),
    DeadlineExceedsParent,
    UnknownTask,
    AlreadyJoined,
    TaskPending,
    UnknownResource,
    UnknownOperation,
    ResourceLimit(ResourceLimit),
    LimitsExceedParent,
}

#[derive(Debug, PartialEq, Eq)]
pub struct JoinHandle {
    task: TaskId,
    joined: bool,
}

impl JoinHandle {
    pub fn task_id(&self) -> TaskId {
        self.task
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    pub cancelled_tasks: usize,
    pub closed_resources: usize,
    pub draining_operations: usize,
    pub leaked_resources: usize,
}

#[derive(Debug)]
enum OperationState {
    Pending,
    Draining,
}

#[derive(Debug)]
struct Operation {
    id: OperationId,
    token: OperationToken,
    scope: ScopeId,
    task: TaskId,
    resource: ResourceId,
    cancellation: CancellationMode,
    state: OperationState,
}

/// Fake-clock scheduler. Task IDs are creation order and therefore the stable
/// tie-breaker for completions at the same instant.
#[derive(Debug)]
pub struct Scheduler {
    now: u64,
    next_scope: u64,
    next_task: u64,
    next_resource: u64,
    next_operation: u64,
    scopes: BTreeMap<ScopeId, Scope>,
    tasks: BTreeMap<TaskId, Task>,
    open_resources: BTreeSet<ResourceId>,
    operations: BTreeMap<OperationToken, Operation>,
    slot_generations: Vec<u32>,
    free_operation_slots: BTreeSet<u32>,
    host_commands: Vec<HostCommand>,
    trace: Vec<TraceEvent>,
}

impl Scheduler {
    pub fn new(grants: impl IntoIterator<Item = CapabilityGrant>) -> Self {
        Self::new_with_limits(grants, ScopeLimits::default())
    }

    pub fn new_with_limits(
        grants: impl IntoIterator<Item = CapabilityGrant>,
        limits: ScopeLimits,
    ) -> Self {
        let root = ScopeId(1);
        let mut scopes = BTreeMap::new();
        scopes.insert(
            root,
            Scope {
                parent: None,
                children: vec![],
                tasks: vec![],
                resources: vec![],
                grants: grants.into_iter().collect(),
                limits,
                deadline: None,
                cancelled: None,
                closed: false,
            },
        );
        Self {
            now: 0,
            next_scope: 2,
            next_task: 1,
            next_resource: 1,
            next_operation: 1,
            scopes,
            tasks: BTreeMap::new(),
            open_resources: BTreeSet::new(),
            operations: BTreeMap::new(),
            slot_generations: vec![],
            free_operation_slots: BTreeSet::new(),
            host_commands: vec![],
            trace: vec![TraceEvent::scope(0, EventKind::ScopeOpened, root)],
        }
    }

    pub fn root(&self) -> ScopeId {
        ScopeId(1)
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    pub fn trace(&self) -> &[TraceEvent] {
        &self.trace
    }

    pub fn trace_document(&self) -> TraceDocument<'_> {
        TraceDocument {
            contract_version: "1",
            events: &self.trace,
        }
    }

    pub fn parent_scope(&self, scope: ScopeId) -> Result<Option<ScopeId>, RuntimeError> {
        self.scopes
            .get(&scope)
            .map(|scope| scope.parent)
            .ok_or(RuntimeError::UnknownScope)
    }

    pub fn open_scope(
        &mut self,
        parent: ScopeId,
        grants: impl IntoIterator<Item = CapabilityGrant>,
        deadline: Option<u64>,
    ) -> Result<ScopeId, RuntimeError> {
        let parent_limits = self
            .scopes
            .get(&parent)
            .ok_or(RuntimeError::UnknownScope)?
            .limits;
        self.open_scope_with_limits(parent, grants, deadline, parent_limits)
    }

    pub fn open_scope_with_limits(
        &mut self,
        parent: ScopeId,
        grants: impl IntoIterator<Item = CapabilityGrant>,
        deadline: Option<u64>,
        limits: ScopeLimits,
    ) -> Result<ScopeId, RuntimeError> {
        let requested: BTreeSet<_> = grants.into_iter().collect();
        let parent_scope = self.scopes.get(&parent).ok_or(RuntimeError::UnknownScope)?;
        if parent_scope.closed {
            return Err(RuntimeError::ScopeClosed);
        }
        if !limits.is_within(parent_scope.limits) {
            return Err(RuntimeError::LimitsExceedParent);
        }
        for grant in &requested {
            if !parent_scope
                .grants
                .iter()
                .any(|allowed| grant.is_within(allowed))
            {
                return Err(RuntimeError::CapabilityAmplification(grant.clone()));
            }
        }
        if deadline
            .zip(parent_scope.deadline)
            .is_some_and(|(child, parent)| child > parent)
        {
            return Err(RuntimeError::DeadlineExceedsParent);
        }
        let id = ScopeId(self.next_scope);
        self.next_scope += 1;
        self.scopes.insert(
            id,
            Scope {
                parent: Some(parent),
                children: vec![],
                tasks: vec![],
                resources: vec![],
                grants: requested,
                limits,
                deadline,
                cancelled: None,
                closed: false,
            },
        );
        self.scopes.get_mut(&parent).unwrap().children.push(id);
        self.trace
            .push(TraceEvent::scope(self.now, EventKind::ScopeOpened, id));
        Ok(id)
    }

    pub fn spawn_scripted(
        &mut self,
        scope: ScopeId,
        at: u64,
        outcome: TaskOutcome,
    ) -> Result<JoinHandle, RuntimeError> {
        let owner = self
            .scopes
            .get_mut(&scope)
            .ok_or(RuntimeError::UnknownScope)?;
        if owner.closed || owner.cancelled.is_some() {
            return Err(RuntimeError::ScopeClosed);
        }
        let live = owner
            .tasks
            .iter()
            .filter(|id| {
                self.tasks
                    .get(id)
                    .is_some_and(|task| !matches!(task.state, TaskState::Terminal(_)))
            })
            .count();
        if live >= owner.limits.live_tasks {
            self.trace.push(TraceEvent::admission(
                self.now,
                scope,
                ResourceLimit::LiveTasks,
            ));
            return Err(RuntimeError::ResourceLimit(ResourceLimit::LiveTasks));
        }
        let id = TaskId(self.next_task);
        self.next_task += 1;
        owner.tasks.push(id);
        self.tasks.insert(
            id,
            Task {
                scope,
                state: TaskState::Waiting { at, outcome },
            },
        );
        self.trace.push(TraceEvent::task(
            self.now,
            EventKind::TaskCreated,
            scope,
            id,
        ));
        self.trace.push(TraceEvent::task(
            self.now,
            EventKind::TaskWaiting,
            scope,
            id,
        ));
        Ok(JoinHandle {
            task: id,
            joined: false,
        })
    }

    pub fn open_resource(&mut self, scope: ScopeId) -> Result<ResourceId, RuntimeError> {
        let owner = self
            .scopes
            .get_mut(&scope)
            .ok_or(RuntimeError::UnknownScope)?;
        if owner.closed {
            return Err(RuntimeError::ScopeClosed);
        }
        let open = owner
            .resources
            .iter()
            .filter(|id| self.open_resources.contains(id))
            .count();
        if open >= owner.limits.open_resources {
            self.trace.push(TraceEvent::admission(
                self.now,
                scope,
                ResourceLimit::OpenResources,
            ));
            return Err(RuntimeError::ResourceLimit(ResourceLimit::OpenResources));
        }
        let id = ResourceId(self.next_resource);
        self.next_resource += 1;
        owner.resources.push(id);
        self.open_resources.insert(id);
        self.trace.push(TraceEvent::resource(
            self.now,
            EventKind::ResourceOpened,
            scope,
            id,
        ));
        Ok(id)
    }

    pub fn start_operation(
        &mut self,
        scope: ScopeId,
        task: TaskId,
        resource: ResourceId,
        request: impl Into<String>,
        cancellation: CancellationMode,
    ) -> Result<OperationToken, RuntimeError> {
        let owner = self.scopes.get(&scope).ok_or(RuntimeError::UnknownScope)?;
        if owner.closed || owner.cancelled.is_some() {
            return Err(RuntimeError::ScopeClosed);
        }
        if !owner.tasks.contains(&task) {
            return Err(RuntimeError::UnknownTask);
        }
        if !owner.resources.contains(&resource) || !self.open_resources.contains(&resource) {
            return Err(RuntimeError::UnknownResource);
        }
        let pending = self
            .operations
            .values()
            .filter(|op| op.scope == scope && matches!(op.state, OperationState::Pending))
            .count();
        if pending >= owner.limits.pending_operations {
            self.trace.push(TraceEvent::admission(
                self.now,
                scope,
                ResourceLimit::PendingOperations,
            ));
            return Err(RuntimeError::ResourceLimit(
                ResourceLimit::PendingOperations,
            ));
        }
        if cancellation == CancellationMode::DrainRequired {
            let drain_capable = self
                .operations
                .values()
                .filter(|op| {
                    op.scope == scope && op.cancellation == CancellationMode::DrainRequired
                })
                .count();
            if drain_capable >= owner.limits.draining_operations {
                self.trace.push(TraceEvent::admission(
                    self.now,
                    scope,
                    ResourceLimit::DrainingOperations,
                ));
                return Err(RuntimeError::ResourceLimit(
                    ResourceLimit::DrainingOperations,
                ));
            }
        }
        let token = self.allocate_operation_token();
        let id = OperationId(self.next_operation);
        self.next_operation += 1;
        self.operations.insert(
            token,
            Operation {
                id,
                token,
                scope,
                task,
                resource,
                cancellation,
                state: OperationState::Pending,
            },
        );
        self.host_commands.push(HostCommand::Start {
            token,
            resource,
            request: request.into(),
        });
        self.trace.push(TraceEvent::operation(
            self.now,
            EventKind::OperationStarted,
            scope,
            task,
            resource,
            id,
            None,
        ));
        Ok(token)
    }

    pub fn take_host_commands(&mut self) -> Vec<HostCommand> {
        std::mem::take(&mut self.host_commands)
    }

    pub fn drive_host(&mut self, adapter: &mut impl AsyncHostAdapter) {
        adapter.dispatch(self.take_host_commands());
        let mut completions = adapter.poll(self.now);
        completions.sort_by_key(|completion| {
            self.operations
                .get(&completion.token)
                .map(|operation| operation.id)
                .unwrap_or(OperationId(u64::MAX))
        });
        for completion in completions {
            self.complete_operation(completion);
        }
        adapter.dispatch(self.take_host_commands());
    }

    pub fn complete_operation(&mut self, completion: HostCompletion) {
        let Some(operation) = self.operations.remove(&completion.token) else {
            self.trace
                .push(TraceEvent::stale_completion(self.now, completion.token));
            return;
        };
        match operation.state {
            OperationState::Pending => {
                let task_outcome = match completion.result {
                    HostOperationResult::Completed(value) => TaskOutcome::Completed(value),
                    HostOperationResult::Failed(error) => TaskOutcome::Failed(error),
                };
                let detail = match &task_outcome {
                    TaskOutcome::Completed(value) => value.clone(),
                    TaskOutcome::Failed(error) => format!("error:{error}"),
                    TaskOutcome::Cancelled(_) => unreachable!(),
                };
                self.trace.push(TraceEvent::operation(
                    self.now,
                    EventKind::OperationCompleted,
                    operation.scope,
                    operation.task,
                    operation.resource,
                    operation.id,
                    Some(detail),
                ));
                if let Some(task) = self.tasks.get_mut(&operation.task) {
                    if !matches!(task.state, TaskState::Terminal(_)) {
                        task.state = TaskState::Terminal(task_outcome.clone());
                        self.trace.push(TraceEvent::outcome(
                            self.now,
                            operation.scope,
                            operation.task,
                            &task_outcome,
                        ));
                    }
                }
            }
            OperationState::Draining => {
                self.trace.push(TraceEvent::operation(
                    self.now,
                    EventKind::LateCompletionDrained,
                    operation.scope,
                    operation.task,
                    operation.resource,
                    operation.id,
                    None,
                ));
            }
        }
        self.release_operation_token(operation.token);
        self.finalize_closed_scope(operation.scope);
    }

    fn allocate_operation_token(&mut self) -> OperationToken {
        let slot = if let Some(slot) = self.free_operation_slots.pop_first() {
            slot
        } else {
            self.slot_generations.push(0);
            (self.slot_generations.len() - 1) as u32
        };
        let generation = self.slot_generations[slot as usize]
            .checked_add(1)
            .expect("operation generation exhausted");
        self.slot_generations[slot as usize] = generation;
        OperationToken { slot, generation }
    }

    fn release_operation_token(&mut self, token: OperationToken) {
        self.free_operation_slots.insert(token.slot);
    }

    fn cancel_operations(&mut self, scope: ScopeId, reason: CancelReason) {
        let tokens: Vec<_> = self
            .operations
            .iter()
            .filter(|(_, op)| op.scope == scope && matches!(op.state, OperationState::Pending))
            .map(|(token, _)| *token)
            .collect();
        for token in tokens {
            let operation = self.operations.get_mut(&token).unwrap();
            let id = operation.id;
            let task = operation.task;
            let resource = operation.resource;
            let mode = operation.cancellation;
            self.host_commands.push(HostCommand::Cancel { token });
            self.trace.push(TraceEvent::operation(
                self.now,
                EventKind::OperationCancelled,
                scope,
                task,
                resource,
                id,
                Some(format!("{reason:?}").to_lowercase()),
            ));
            if mode == CancellationMode::Guaranteed {
                self.operations.remove(&token);
                self.release_operation_token(token);
            } else {
                self.operations.get_mut(&token).unwrap().state = OperationState::Draining;
            }
        }
    }

    /// Advance the injected monotonic clock. Deadlines at an instant are
    /// applied before task completions; completions then use TaskId order.
    pub fn advance_to(&mut self, at: u64) {
        assert!(at >= self.now, "fake monotonic clock cannot move backwards");
        self.now = at;
        let expired: Vec<_> = self
            .scopes
            .iter()
            .filter(|(_, scope)| {
                !scope.closed
                    && scope.cancelled.is_none()
                    && scope.deadline.is_some_and(|d| d <= at)
            })
            .map(|(id, _)| *id)
            .collect();
        for scope in expired {
            self.cancel_scope(scope, CancelReason::Deadline);
        }
        let ready: Vec<_> = self
            .tasks
            .iter()
            .filter_map(|(id, task)| match task.state {
                TaskState::Waiting { at: due, .. } if due <= at => Some(*id),
                _ => None,
            })
            .collect();
        for id in ready {
            let (scope, outcome) = {
                let task = self.tasks.get_mut(&id).unwrap();
                let TaskState::Waiting { outcome, .. } = &task.state else {
                    continue;
                };
                (task.scope, outcome.clone())
            };
            self.tasks.get_mut(&id).unwrap().state = TaskState::Terminal(outcome.clone());
            self.trace
                .push(TraceEvent::outcome(at, scope, id, &outcome));
        }
    }

    pub fn cancel_scope(&mut self, scope: ScopeId, reason: CancelReason) {
        let Some(current) = self.scopes.get_mut(&scope) else {
            return;
        };
        if current.cancelled.is_some() || current.closed {
            return;
        }
        current.cancelled = Some(reason.clone());
        let children = current.children.clone();
        let tasks = current.tasks.clone();
        self.trace.push(TraceEvent::cancel(
            self.now,
            EventKind::ScopeCancelled,
            scope,
            reason.clone(),
        ));
        for child in children {
            self.cancel_scope(child, CancelReason::ParentCancelled);
        }
        self.cancel_operations(scope, reason.clone());
        for id in tasks {
            let Some(task) = self.tasks.get_mut(&id) else {
                continue;
            };
            if matches!(task.state, TaskState::Terminal(_)) {
                continue;
            }
            let cancelled = TaskOutcome::Cancelled(CancelReason::ParentCancelled);
            task.state = TaskState::Terminal(cancelled.clone());
            self.trace
                .push(TraceEvent::outcome(self.now, scope, id, &cancelled));
        }
    }

    pub fn join(&mut self, handle: &mut JoinHandle) -> Result<TaskOutcome, RuntimeError> {
        if handle.joined {
            return Err(RuntimeError::AlreadyJoined);
        }
        let task = self
            .tasks
            .get(&handle.task)
            .ok_or(RuntimeError::UnknownTask)?;
        let TaskState::Terminal(outcome) = &task.state else {
            return Err(RuntimeError::TaskPending);
        };
        let outcome = outcome.clone();
        handle.joined = true;
        self.trace.push(TraceEvent::task(
            self.now,
            EventKind::JoinCompleted,
            task.scope,
            handle.task,
        ));
        Ok(outcome)
    }

    pub fn close_scope(&mut self, scope: ScopeId) -> Result<CleanupReport, RuntimeError> {
        if !self.scopes.contains_key(&scope) {
            return Err(RuntimeError::UnknownScope);
        }
        let before = self
            .tasks
            .values()
            .filter(|task| task.scope == scope && !matches!(task.state, TaskState::Terminal(_)))
            .count();
        self.cancel_scope(scope, CancelReason::ScopeClosed);
        let (children, resources) = {
            let owner = self.scopes.get(&scope).unwrap();
            (owner.children.clone(), owner.resources.clone())
        };
        let mut report = CleanupReport {
            cancelled_tasks: before,
            closed_resources: 0,
            draining_operations: 0,
            leaked_resources: 0,
        };
        for child in children {
            let child_report = self.close_scope(child)?;
            report.cancelled_tasks += child_report.cancelled_tasks;
            report.closed_resources += child_report.closed_resources;
            report.draining_operations += child_report.draining_operations;
            report.leaked_resources += child_report.leaked_resources;
        }
        let retained: BTreeSet<_> = self
            .operations
            .values()
            .filter(|op| op.scope == scope && matches!(op.state, OperationState::Draining))
            .map(|op| op.resource)
            .collect();
        report.draining_operations += self
            .operations
            .values()
            .filter(|op| op.scope == scope && matches!(op.state, OperationState::Draining))
            .count();
        for resource in resources.into_iter().rev() {
            if retained.contains(&resource) {
                report.leaked_resources += 1;
                continue;
            }
            if self.open_resources.remove(&resource) {
                report.closed_resources += 1;
                self.trace.push(TraceEvent::resource(
                    self.now,
                    EventKind::ResourceClosed,
                    scope,
                    resource,
                ));
            }
        }
        let owner = self.scopes.get_mut(&scope).unwrap();
        if !owner.closed {
            owner.closed = true;
        }
        if report.draining_operations == 0 {
            self.trace.push(TraceEvent::scope(
                self.now,
                EventKind::ScopeCompleted,
                scope,
            ));
        }
        Ok(report)
    }

    fn finalize_closed_scope(&mut self, scope: ScopeId) {
        if self
            .operations
            .values()
            .any(|op| op.scope == scope && matches!(op.state, OperationState::Draining))
        {
            return;
        }
        let Some(owner) = self.scopes.get(&scope) else {
            return;
        };
        if !owner.closed {
            return;
        }
        let resources = owner.resources.clone();
        let parent = owner.parent;
        for resource in resources.into_iter().rev() {
            if self.open_resources.remove(&resource) {
                self.trace.push(TraceEvent::resource(
                    self.now,
                    EventKind::ResourceClosed,
                    scope,
                    resource,
                ));
            }
        }
        if !self
            .trace
            .iter()
            .any(|event| event.scope == scope && event.kind == EventKind::ScopeCompleted)
        {
            self.trace.push(TraceEvent::scope(
                self.now,
                EventKind::ScopeCompleted,
                scope,
            ));
        }
        if let Some(parent) = parent {
            self.finalize_closed_scope(parent);
        }
    }
}

impl TraceEvent {
    fn scope(at: u64, kind: EventKind, scope: ScopeId) -> Self {
        Self {
            at,
            kind,
            scope,
            task: None,
            resource: None,
            operation: None,
            reason: None,
            detail: None,
        }
    }
    fn task(at: u64, kind: EventKind, scope: ScopeId, task: TaskId) -> Self {
        Self {
            at,
            kind,
            scope,
            task: Some(task),
            resource: None,
            operation: None,
            reason: None,
            detail: None,
        }
    }
    fn resource(at: u64, kind: EventKind, scope: ScopeId, resource: ResourceId) -> Self {
        Self {
            at,
            kind,
            scope,
            task: None,
            resource: Some(resource),
            operation: None,
            reason: None,
            detail: None,
        }
    }
    fn cancel(at: u64, kind: EventKind, scope: ScopeId, reason: CancelReason) -> Self {
        Self {
            at,
            kind,
            scope,
            task: None,
            resource: None,
            operation: None,
            reason: Some(reason),
            detail: None,
        }
    }
    fn outcome(at: u64, scope: ScopeId, task: TaskId, outcome: &TaskOutcome) -> Self {
        let (kind, reason, detail) = match outcome {
            TaskOutcome::Completed(value) => (EventKind::TaskCompleted, None, Some(value.clone())),
            TaskOutcome::Failed(error) => (EventKind::TaskFailed, None, Some(error.clone())),
            TaskOutcome::Cancelled(reason) => {
                (EventKind::TaskCancelled, Some(reason.clone()), None)
            }
        };
        Self {
            at,
            kind,
            scope,
            task: Some(task),
            resource: None,
            operation: None,
            reason,
            detail,
        }
    }
    fn operation(
        at: u64,
        kind: EventKind,
        scope: ScopeId,
        task: TaskId,
        resource: ResourceId,
        operation: OperationId,
        detail: Option<String>,
    ) -> Self {
        Self {
            at,
            kind,
            scope,
            task: Some(task),
            resource: Some(resource),
            operation: Some(operation),
            reason: None,
            detail,
        }
    }
    fn admission(at: u64, scope: ScopeId, limit: ResourceLimit) -> Self {
        Self {
            at,
            kind: EventKind::AdmissionRejected,
            scope,
            task: None,
            resource: None,
            operation: None,
            reason: Some(CancelReason::ResourceLimit),
            detail: Some(format!("{limit:?}").to_lowercase()),
        }
    }
    fn stale_completion(at: u64, token: OperationToken) -> Self {
        Self {
            at,
            kind: EventKind::LateCompletionDrained,
            scope: ScopeId(1),
            task: None,
            resource: None,
            operation: None,
            reason: None,
            detail: Some(format!("stale-token:{}:{}", token.slot, token.generation)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeIoAdapter {
        commands: Vec<HostCommand>,
        scheduled: Vec<(u64, HostCompletion)>,
        /// Return ready events in reverse order to prove scheduler ordering.
        reverse_ready: bool,
    }

    impl FakeIoAdapter {
        fn at(&mut self, at: u64, token: OperationToken, result: Result<&str, &str>) {
            self.scheduled.push((
                at,
                HostCompletion {
                    token,
                    result: match result {
                        Ok(value) => HostOperationResult::Completed(value.to_owned()),
                        Err(error) => HostOperationResult::Failed(error.to_owned()),
                    },
                },
            ));
        }
    }

    impl AsyncHostAdapter for FakeIoAdapter {
        fn dispatch(&mut self, commands: Vec<HostCommand>) {
            self.commands.extend(commands);
        }
        fn poll(&mut self, now: u64) -> Vec<HostCompletion> {
            let mut ready = vec![];
            let mut pending = vec![];
            for (at, completion) in self.scheduled.drain(..) {
                if at <= now {
                    ready.push(completion);
                } else {
                    pending.push((at, completion));
                }
            }
            self.scheduled = pending;
            if self.reverse_ready {
                ready.reverse();
            }
            ready
        }
    }

    fn grant(domain: &str, prefix: &str) -> CapabilityGrant {
        CapabilityGrant {
            domain: domain.into(),
            resource_prefix: prefix.into(),
        }
    }

    #[test]
    fn completions_are_deterministic_and_join_is_affine() {
        let mut scheduler = Scheduler::new([]);
        let root = scheduler.root();
        let mut first = scheduler
            .spawn_scripted(root, 5, TaskOutcome::Completed("first".into()))
            .unwrap();
        let mut second = scheduler
            .spawn_scripted(root, 5, TaskOutcome::Completed("second".into()))
            .unwrap();
        assert_eq!(scheduler.join(&mut first), Err(RuntimeError::TaskPending));
        scheduler.advance_to(5);
        assert_eq!(
            scheduler.join(&mut first).unwrap(),
            TaskOutcome::Completed("first".into())
        );
        assert_eq!(scheduler.join(&mut first), Err(RuntimeError::AlreadyJoined));
        assert_eq!(
            scheduler.join(&mut second).unwrap(),
            TaskOutcome::Completed("second".into())
        );
        let completed: Vec<_> = scheduler
            .trace()
            .iter()
            .filter(|event| event.kind == EventKind::TaskCompleted)
            .map(|event| event.task.unwrap())
            .collect();
        assert_eq!(completed, [TaskId(1), TaskId(2)]);
    }

    #[test]
    fn deadline_wins_over_completion_and_propagates() {
        let mut scheduler = Scheduler::new([]);
        let child = scheduler
            .open_scope(scheduler.root(), [], Some(10))
            .unwrap();
        let grandchild = scheduler.open_scope(child, [], Some(9)).unwrap();
        assert_eq!(scheduler.parent_scope(grandchild).unwrap(), Some(child));
        let mut task = scheduler
            .spawn_scripted(grandchild, 10, TaskOutcome::Completed("late".into()))
            .unwrap();
        scheduler.advance_to(10);
        assert_eq!(
            scheduler.join(&mut task).unwrap(),
            TaskOutcome::Cancelled(CancelReason::ParentCancelled)
        );
        let deadline = scheduler
            .trace()
            .iter()
            .position(|event| {
                event.kind == EventKind::ScopeCancelled
                    && event.reason == Some(CancelReason::Deadline)
            })
            .unwrap();
        let task_cancel = scheduler
            .trace()
            .iter()
            .position(|event| event.kind == EventKind::TaskCancelled)
            .unwrap();
        assert!(deadline < task_cancel);
    }

    #[test]
    fn capabilities_can_only_be_attenuated() {
        let mut scheduler = Scheduler::new([grant("filesystem-read", "/safe")]);
        let root = scheduler.root();
        assert!(scheduler
            .open_scope(root, [grant("filesystem-read", "/safe/cache")], None)
            .is_ok());
        assert_eq!(
            scheduler.open_scope(root, [grant("filesystem-read", "/")], None),
            Err(RuntimeError::CapabilityAmplification(grant(
                "filesystem-read",
                "/"
            )))
        );
        assert_eq!(
            scheduler.open_scope(root, [grant("network", "")], None),
            Err(RuntimeError::CapabilityAmplification(grant("network", "")))
        );
    }

    #[test]
    fn scope_close_cancels_tasks_and_closes_resources_in_reverse_order() {
        let mut scheduler = Scheduler::new([]);
        let root = scheduler.root();
        let mut task = scheduler
            .spawn_scripted(root, 100, TaskOutcome::Completed("never".into()))
            .unwrap();
        let first = scheduler.open_resource(root).unwrap();
        let second = scheduler.open_resource(root).unwrap();
        assert_eq!(
            scheduler.close_scope(root).unwrap(),
            CleanupReport {
                cancelled_tasks: 1,
                closed_resources: 2,
                draining_operations: 0,
                leaked_resources: 0,
            }
        );
        assert_eq!(
            scheduler.join(&mut task).unwrap(),
            TaskOutcome::Cancelled(CancelReason::ParentCancelled)
        );
        let closed: Vec<_> = scheduler
            .trace()
            .iter()
            .filter(|event| event.kind == EventKind::ResourceClosed)
            .map(|event| event.resource.unwrap())
            .collect();
        assert_eq!(closed, [second, first]);
    }

    #[test]
    fn trace_serializes_as_the_versioned_interchange_document() {
        let scheduler = Scheduler::new([]);
        let document = serde_json::to_value(scheduler.trace_document()).unwrap();
        assert_eq!(document["contract-version"], "1");
        assert_eq!(document["events"][0]["kind"], "scope-opened");
        assert_eq!(document["events"][0]["scope"], 1);
    }

    #[test]
    fn multiple_concurrent_io_completions_are_deterministic_and_wake_tasks() {
        let mut scheduler = Scheduler::new([]);
        let root = scheduler.root();
        let mut first = scheduler
            .spawn_scripted(root, 100, TaskOutcome::Failed("timeout".into()))
            .unwrap();
        let mut second = scheduler
            .spawn_scripted(root, 100, TaskOutcome::Failed("timeout".into()))
            .unwrap();
        let left = scheduler.open_resource(root).unwrap();
        let right = scheduler.open_resource(root).unwrap();
        let left_token = scheduler
            .start_operation(
                root,
                first.task_id(),
                left,
                "read-left",
                CancellationMode::Guaranteed,
            )
            .unwrap();
        let right_token = scheduler
            .start_operation(
                root,
                second.task_id(),
                right,
                "read-right",
                CancellationMode::Guaranteed,
            )
            .unwrap();
        let mut host = FakeIoAdapter {
            reverse_ready: true,
            ..Default::default()
        };
        scheduler.drive_host(&mut host);
        assert_eq!(host.commands.len(), 2);
        host.at(5, right_token, Ok("right"));
        host.at(5, left_token, Ok("left"));
        scheduler.advance_to(5);
        scheduler.drive_host(&mut host);
        assert_eq!(
            scheduler.join(&mut first).unwrap(),
            TaskOutcome::Completed("left".into())
        );
        assert_eq!(
            scheduler.join(&mut second).unwrap(),
            TaskOutcome::Completed("right".into())
        );
        let completed: Vec<_> = scheduler
            .trace()
            .iter()
            .filter(|event| event.kind == EventKind::OperationCompleted)
            .map(|event| event.operation.unwrap())
            .collect();
        assert_eq!(completed, [OperationId(1), OperationId(2)]);
    }

    #[test]
    fn deadline_cancellation_drains_late_completion_before_resource_cleanup() {
        let limits = ScopeLimits {
            draining_operations: 1,
            ..ScopeLimits::default()
        };
        let mut scheduler = Scheduler::new_with_limits([], limits);
        let scope = scheduler
            .open_scope(scheduler.root(), [], Some(10))
            .unwrap();
        let mut task = scheduler
            .spawn_scripted(scope, 100, TaskOutcome::Completed("never".into()))
            .unwrap();
        let resource = scheduler.open_resource(scope).unwrap();
        let token = scheduler
            .start_operation(
                scope,
                task.task_id(),
                resource,
                "read",
                CancellationMode::DrainRequired,
            )
            .unwrap();
        let mut host = FakeIoAdapter::default();
        scheduler.drive_host(&mut host);
        host.at(10, token, Ok("too-late"));
        scheduler.advance_to(10);
        let report = scheduler.close_scope(scope).unwrap();
        assert_eq!(report.draining_operations, 1);
        assert_eq!(report.leaked_resources, 1);
        assert_eq!(report.closed_resources, 0);
        scheduler.drive_host(&mut host);
        assert_eq!(
            scheduler.join(&mut task).unwrap(),
            TaskOutcome::Cancelled(CancelReason::ParentCancelled)
        );
        assert!(scheduler
            .trace()
            .iter()
            .any(|event| event.kind == EventKind::LateCompletionDrained
                && event.operation == Some(OperationId(1))));
        assert!(scheduler.trace().iter().any(
            |event| event.kind == EventKind::ResourceClosed && event.resource == Some(resource)
        ));
    }

    #[test]
    fn operation_tokens_change_generation_and_stale_completion_is_discarded() {
        let mut scheduler = Scheduler::new([]);
        let root = scheduler.root();
        let first_task = scheduler
            .spawn_scripted(root, 100, TaskOutcome::Completed("never".into()))
            .unwrap();
        let resource = scheduler.open_resource(root).unwrap();
        let first = scheduler
            .start_operation(
                root,
                first_task.task_id(),
                resource,
                "first",
                CancellationMode::Guaranteed,
            )
            .unwrap();
        scheduler.complete_operation(HostCompletion {
            token: first,
            result: HostOperationResult::Completed("done".into()),
        });
        let mut second_task = scheduler
            .spawn_scripted(root, 100, TaskOutcome::Completed("never".into()))
            .unwrap();
        let second = scheduler
            .start_operation(
                root,
                second_task.task_id(),
                resource,
                "second",
                CancellationMode::Guaranteed,
            )
            .unwrap();
        assert_eq!(first.slot, second.slot);
        assert!(second.generation > first.generation);
        scheduler.complete_operation(HostCompletion {
            token: first,
            result: HostOperationResult::Completed("stale".into()),
        });
        assert_eq!(
            scheduler.join(&mut second_task),
            Err(RuntimeError::TaskPending)
        );
    }

    #[test]
    fn admission_limits_reject_atomically_and_are_traced() {
        let limits = ScopeLimits {
            live_tasks: 1,
            pending_operations: 1,
            open_resources: 1,
            draining_operations: 1,
        };
        let mut scheduler = Scheduler::new_with_limits([], limits);
        let root = scheduler.root();
        let task = scheduler
            .spawn_scripted(root, 100, TaskOutcome::Completed("never".into()))
            .unwrap();
        assert_eq!(
            scheduler.spawn_scripted(root, 100, TaskOutcome::Completed("no".into())),
            Err(RuntimeError::ResourceLimit(ResourceLimit::LiveTasks))
        );
        let resource = scheduler.open_resource(root).unwrap();
        assert_eq!(
            scheduler.open_resource(root),
            Err(RuntimeError::ResourceLimit(ResourceLimit::OpenResources))
        );
        scheduler
            .start_operation(
                root,
                task.task_id(),
                resource,
                "one",
                CancellationMode::Guaranteed,
            )
            .unwrap();
        assert_eq!(
            scheduler.start_operation(
                root,
                task.task_id(),
                resource,
                "two",
                CancellationMode::Guaranteed
            ),
            Err(RuntimeError::ResourceLimit(
                ResourceLimit::PendingOperations
            ))
        );
        assert_eq!(
            scheduler
                .trace()
                .iter()
                .filter(|event| event.kind == EventKind::AdmissionRejected)
                .count(),
            3
        );
    }
}
