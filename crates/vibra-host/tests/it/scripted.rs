//! A scripted driver for the broker's tests.
//!
//! It records each `call`, each gate answer, each of its own steps (`read`,
//! `create`, `truncate`, `write`), each completion it returns, and each
//! release. It answers from its files, its standard input, and its
//! variables, with `Known` progress; a queued [`Tweak`] changes what it
//! reports to the gate or how it completes the next call.

use std::cell::{RefCell, RefMut};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use vibra_host::driver::{
    Acquire, Action, Completion, Driver, DriverIdentity, Found, FoundAction, Gate,
    Measured, Operand, Progress, Resolved, ResourceId,
};
use vibra_host::recipe::Provider;
use vibra_ir::authority::{HostErrorCase, HostFsKind, HostOk, HostReply};
use vibra_ir::external::host::HostOp;

/// One of the driver's own steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Content was read.
    Read,
    /// A file was created.
    Create,
    /// A file was truncated.
    Truncate,
    /// Content was written.
    Write,
}

/// An operand, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Owned {
    /// No operand.
    None,
    /// A text.
    Text(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// A count.
    Count(u64),
}

/// What the driver reported to the gate, owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Asked {
    /// The real names.
    pub real_names: Vec<String>,
    /// The action found.
    pub action: FoundAction,
    /// The size.
    pub size: Option<u64>,
    /// The link count.
    pub links: Option<u32>,
}

/// One entry of the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The broker called the driver.
    Call {
        /// The resource.
        resource: ResourceId,
        /// The operation.
        op: HostOp,
        /// The names, in the coordinates of the resource.
        names: Vec<String>,
        /// The other operand.
        operand: Owned,
    },
    /// The driver asked the gate; `answer` is the reserved count, or `None`
    /// for a refusal.
    Gate {
        /// What it reported.
        found: Asked,
        /// The answer.
        answer: Option<u64>,
    },
    /// One of the driver's steps.
    Step(Step),
    /// The driver returned: `refused`, `notSubmitted`, or `completed`.
    Returned(&'static str),
    /// The broker released a resource.
    Release(ResourceId),
}

/// One file of the driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct File {
    /// Its contents.
    pub contents: Vec<u8>,
    /// The link count it reports.
    pub links: Option<u32>,
}

/// What the next call reports or answers differently.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tweak {
    /// The real names reported to the gate instead of the request's.
    pub real_names: Option<Vec<String>>,
    /// The size reported to the gate instead of the object's.
    pub size: Option<Option<u64>>,
    /// The link count reported instead of the file's.
    pub links: Option<Option<u32>>,
    /// The completion returned after the gate admitted, or without a gate.
    pub completion: Option<Completion>,
    /// Go on as if the gate had admitted, whatever it answered.
    pub ignore_gate: bool,
}

/// The driver's state, which the test inspects.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// Everything that happened, in order.
    pub log: Vec<Event>,
    /// The files, by their names from the held directory.
    pub files: BTreeMap<Vec<String>, File>,
    /// What remains of standard input.
    pub stdin: Vec<u8>,
    /// Whether standard input reports its size.
    pub stdin_sized: bool,
    /// The variables.
    pub env: BTreeMap<String, String>,
    /// The tweaks of the next calls, in order.
    pub tweaks: VecDeque<Tweak>,
    next_resource: u64,
}

/// The scripted driver: a handle on shared state, so the test keeps one
/// handle while the context holds the other.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scripted(Rc<RefCell<State>>);

impl Scripted {
    /// A driver with no files, no input, and no variables.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The state.
    pub(crate) fn state(&self) -> RefMut<'_, State> {
        self.0.borrow_mut()
    }

    /// Adds a file with a link count of one.
    pub(crate) fn file(&self, names: &[&str], contents: &str) {
        self.state().files.insert(
            names.iter().map(|name| (*name).to_owned()).collect(),
            File {
                contents: contents.as_bytes().to_vec(),
                links: Some(1),
            },
        );
    }

    /// Queues a tweak for the next call.
    pub(crate) fn tweak(&self, tweak: Tweak) {
        self.state().tweaks.push_back(tweak);
    }

    /// Sets standard input.
    pub(crate) fn stdin(&self, input: &str, sized: bool) {
        let mut state = self.state();
        state.stdin = input.as_bytes().to_vec();
        state.stdin_sized = sized;
    }

    /// The calls the broker made, in order.
    pub(crate) fn calls(&self) -> Vec<Event> {
        self.state()
            .log
            .iter()
            .filter(|event| matches!(event, Event::Call { .. }))
            .cloned()
            .collect()
    }

    /// Whether the log holds `step`.
    pub(crate) fn stepped(&self, step: Step) -> bool {
        self.state().log.contains(&Event::Step(step))
    }

    /// The last gate answer of the log.
    pub(crate) fn last_gate(&self) -> Option<Option<u64>> {
        self.state().log.iter().rev().find_map(|event| match event {
            Event::Gate { answer, .. } => Some(*answer),
            _ => None,
        })
    }

    /// The last completion the driver returned.
    pub(crate) fn last_returned(&self) -> Option<&'static str> {
        self.state().log.iter().rev().find_map(|event| match event {
            Event::Returned(kind) => Some(*kind),
            _ => None,
        })
    }

    /// Clears the log.
    pub(crate) fn clear(&self) {
        self.state().log.clear();
    }
}

fn owned(operand: Operand<'_>) -> Owned {
    match operand {
        Operand::None => Owned::None,
        Operand::Text(text) => Owned::Text(text.to_owned()),
        Operand::Bytes(bytes) => Owned::Bytes(bytes.to_vec()),
        Operand::Count(count) => Owned::Count(count),
    }
}

fn error(op: HostOp, name: &str) -> HostReply {
    HostReply::Err(HostErrorCase::new(op.row().error, name).expect("a case of the row"))
}

fn completed(reply: HostReply, input: u64, output: u64) -> Completion {
    Completion::Completed {
        reply,
        progress: Progress {
            input: Measured::Known(input),
            output: Measured::Known(output),
        },
    }
}

fn length(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).expect("a small test value")
}

/// Asks the gate; whether the driver goes on.
fn ask(
    state: &mut State,
    gate: &mut dyn Gate,
    found: Found<'_>,
    ignore: bool,
) -> Option<u64> {
    let answer = gate.admit(&found).ok().map(|reserved| reserved.input);
    state.log.push(Event::Gate {
        found: Asked {
            real_names: found.real_names.to_vec(),
            action: found.action,
            size: found.size,
            links: found.links,
        },
        answer,
    });
    match answer {
        Some(input) => Some(input),
        None if ignore => Some(u64::MAX),
        None => None,
    }
}

impl Scripted {
    fn answer(
        state: &mut State,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        let tweak = state.tweaks.pop_front().unwrap_or_default();
        let names = action.names.to_vec();
        let op = action.op;
        let real = tweak.real_names.clone().unwrap_or_else(|| names.clone());
        let then = |default: Completion| tweak.completion.clone().unwrap_or(default);
        match op {
            HostOp::FsReadFile | HostOp::FsReadText => {
                let Some(file) = state.files.get(&names).cloned() else {
                    return then(completed(error(op, "notFound"), 0, 0));
                };
                let found = Found {
                    real_names: &real,
                    action: FoundAction::Read,
                    size: tweak.size.unwrap_or(Some(length(&file.contents))),
                    links: tweak.links.unwrap_or(file.links),
                };
                if ask(state, gate, found, tweak.ignore_gate).is_none() {
                    return Completion::Refused;
                }
                state.log.push(Event::Step(Step::Read));
                let reply = if op == HostOp::FsReadText {
                    HostOk::Text(
                        String::from_utf8(file.contents.clone()).expect("text"),
                    )
                } else {
                    HostOk::Bytes(file.contents.clone())
                };
                then(completed(HostReply::Ok(reply), length(&file.contents), 0))
            }
            HostOp::FsWriteFile | HostOp::FsWriteText => {
                let contents = match action.operand {
                    Operand::Text(text) => text.as_bytes().to_vec(),
                    Operand::Bytes(bytes) => bytes.to_vec(),
                    Operand::None | Operand::Count(_) => Vec::new(),
                };
                let existing = state.files.get(&names).cloned();
                let found = Found {
                    real_names: &real,
                    action: if existing.is_some() {
                        FoundAction::Replace
                    } else {
                        FoundAction::Create
                    },
                    size: None,
                    links: tweak.links.unwrap_or_else(|| {
                        existing.as_ref().and_then(|file| file.links)
                    }),
                };
                if ask(state, gate, found, tweak.ignore_gate).is_none() {
                    return Completion::Refused;
                }
                if existing.is_some() {
                    state.log.push(Event::Step(Step::Truncate));
                } else {
                    state.log.push(Event::Step(Step::Create));
                }
                state.log.push(Event::Step(Step::Write));
                let written = length(&contents);
                state.files.insert(
                    names,
                    File {
                        contents,
                        links: Some(1),
                    },
                );
                then(completed(HostReply::Ok(HostOk::Void), 0, written))
            }
            HostOp::FsReadEntries => {
                let found = Found {
                    real_names: &real,
                    action: FoundAction::List,
                    size: None,
                    links: None,
                };
                if ask(state, gate, found, tweak.ignore_gate).is_none() {
                    return Completion::Refused;
                }
                then(completed(HostReply::Ok(HostOk::Names(Vec::new())), 0, 0))
            }
            HostOp::FsMetadataStat => {
                let file = state.files.get(&names).cloned();
                let found = Found {
                    real_names: &real,
                    action: if file.is_some() {
                        FoundAction::StatFile
                    } else {
                        FoundAction::StatDirectory
                    },
                    size: None,
                    links: None,
                };
                if ask(state, gate, found, tweak.ignore_gate).is_none() {
                    return Completion::Refused;
                }
                let (kind, size) = file.map_or((HostFsKind::Directory, 0), |file| {
                    (HostFsKind::File, length(&file.contents))
                });
                then(completed(
                    HostReply::Ok(HostOk::Metadata { kind, size }),
                    0,
                    0,
                ))
            }
            HostOp::IoStdinText | HostOp::IoStdinBytes => {
                let Operand::Count(max) = action.operand else {
                    return Completion::Refused;
                };
                let found = Found {
                    real_names: &[],
                    action: FoundAction::Read,
                    size: tweak.size.unwrap_or_else(|| {
                        state.stdin_sized.then(|| length(&state.stdin))
                    }),
                    links: None,
                };
                let Some(reserved) = ask(state, gate, found, tweak.ignore_gate) else {
                    return Completion::Refused;
                };
                state.log.push(Event::Step(Step::Read));
                let take = usize::try_from(reserved.min(max))
                    .unwrap_or(usize::MAX)
                    .min(state.stdin.len());
                let taken = state.stdin.drain(..take).collect::<Vec<_>>();
                let reply = if op == HostOp::IoStdinText {
                    HostOk::Text(String::from_utf8(taken.clone()).expect("text"))
                } else {
                    HostOk::Bytes(taken.clone())
                };
                then(completed(HostReply::Ok(reply), length(&taken), 0))
            }
            HostOp::IoStdoutText
            | HostOp::IoStdoutBytes
            | HostOp::IoStderrText
            | HostOp::IoStderrBytes => {
                let written = match action.operand {
                    Operand::Text(text) => length(text.as_bytes()),
                    Operand::Bytes(bytes) => length(bytes),
                    Operand::None | Operand::Count(_) => 0,
                };
                state.log.push(Event::Step(Step::Write));
                then(completed(HostReply::Ok(HostOk::Void), 0, written))
            }
            HostOp::EnvReadVar => {
                let Operand::Text(name) = action.operand else {
                    return Completion::Refused;
                };
                let Some(value) = state.env.get(name).cloned() else {
                    return then(completed(error(op, "notPresent"), 0, 0));
                };
                let found = Found {
                    real_names: &[],
                    action: FoundAction::Read,
                    size: Some(length(value.as_bytes())),
                    links: None,
                };
                if ask(state, gate, found, tweak.ignore_gate).is_none() {
                    return Completion::Refused;
                }
                state.log.push(Event::Step(Step::Read));
                let size = length(value.as_bytes());
                then(completed(HostReply::Ok(HostOk::Text(value)), size, 0))
            }
            HostOp::EnvReadArgs => {
                then(completed(HostReply::Ok(HostOk::Names(Vec::new())), 0, 0))
            }
            HostOp::TimeNowWall => then(completed(HostReply::Ok(HostOk::I64(0)), 0, 0)),
            HostOp::TimeNowMonotonic => {
                then(completed(HostReply::Ok(HostOk::U64(0)), 0, 0))
            }
            HostOp::RandomGenerateBytes => {
                let Operand::Count(count) = action.operand else {
                    return Completion::Refused;
                };
                let bytes = vec![0; usize::try_from(count).expect("a small count")];
                then(completed(HostReply::Ok(HostOk::Bytes(bytes)), count, 0))
            }
        }
    }
}

impl Driver for Scripted {
    fn identity(&self, provider: Provider) -> Option<DriverIdentity> {
        Some(DriverIdentity {
            provider: provider.atom().to_owned(),
            version: "0.0.0".to_owned(),
        })
    }

    fn acquire(&mut self, _provider: Provider, _resource: &Resolved) -> Acquire {
        let mut state = self.state();
        state.next_resource += 1;
        Acquire::Acquired(ResourceId(state.next_resource))
    }

    fn release(&mut self, resource: ResourceId) {
        self.state().log.push(Event::Release(resource));
    }

    fn call(
        &mut self,
        resource: ResourceId,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Completion {
        let mut state = self.state();
        state.log.push(Event::Call {
            resource,
            op: action.op,
            names: action.names.to_vec(),
            operand: owned(action.operand),
        });
        let completion = Self::answer(&mut state, action, gate);
        state.log.push(Event::Returned(match completion {
            Completion::Refused => "refused",
            Completion::NotSubmitted(_) => "notSubmitted",
            Completion::Completed { .. } => "completed",
        }));
        completion
    }
}
