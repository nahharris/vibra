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
}

#[derive(Debug)]
pub struct JoinHandle {
    task: TaskId,
    joined: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    pub cancelled_tasks: usize,
    pub closed_resources: usize,
}

/// Fake-clock scheduler. Task IDs are creation order and therefore the stable
/// tie-breaker for completions at the same instant.
#[derive(Debug)]
pub struct Scheduler {
    now: u64,
    next_scope: u64,
    next_task: u64,
    next_resource: u64,
    scopes: BTreeMap<ScopeId, Scope>,
    tasks: BTreeMap<TaskId, Task>,
    open_resources: BTreeSet<ResourceId>,
    trace: Vec<TraceEvent>,
}

impl Scheduler {
    pub fn new(grants: impl IntoIterator<Item = CapabilityGrant>) -> Self {
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
            scopes,
            tasks: BTreeMap::new(),
            open_resources: BTreeSet::new(),
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
        let requested: BTreeSet<_> = grants.into_iter().collect();
        let parent_scope = self.scopes.get(&parent).ok_or(RuntimeError::UnknownScope)?;
        if parent_scope.closed {
            return Err(RuntimeError::ScopeClosed);
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
            reason,
        ));
        for child in children {
            self.cancel_scope(child, CancelReason::ParentCancelled);
        }
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
        };
        for child in children {
            let child_report = self.close_scope(child)?;
            report.cancelled_tasks += child_report.cancelled_tasks;
            report.closed_resources += child_report.closed_resources;
        }
        for resource in resources.into_iter().rev() {
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
            self.trace.push(TraceEvent::scope(
                self.now,
                EventKind::ScopeCompleted,
                scope,
            ));
        }
        Ok(report)
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
            reason,
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                closed_resources: 2
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
}
