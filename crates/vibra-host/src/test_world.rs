//! The rows of the test-world registry, over a [`WorldDriver`].
//!
//! `docs/spec/04-programs-and-packages.md`, "Test worlds and grants", and
//! `docs/spec/06-runtime.md`, "Host registry" and "Audit trace", fix what
//! this module does. `member` runs one member of `TestWorld` on the
//! world: it makes the member's resources and names the roots to issue on
//! them, each with the policy the chapter states, by the field names of the
//! member's row. The broker checks that the token is the test root of its
//! context, issues every root with the origin `{kind: @testWorld}` in the
//! canonical order of the field names, and appends no event.
//!
//! - `directory` makes a new virtual directory holding the fixture's UTF-8
//!   files. Each of its six roots permits its own action everywhere in the
//!   directory: a file rule with `prefix: []`, `depth: @tree`, and
//!   `@anyName` for the read and the write grants, a directory rule of
//!   `depth: @tree` for `list`, and for `stat` the `anyOf` of that directory
//!   rule and that file rule, each with `@stat`.
//! - `streams` makes new standard streams whose input is its `stdin`.
//! - `environment` sets the world's one environment; its variable grant
//!   permits every name.
//! - `clocks` and `random` hold the world's one clock pair and one random
//!   state, so two calls share them.
//! - `output` gives everything the world's streams have received.

use std::collections::BTreeMap;

use vibra_ir::Value;
use vibra_ir::authority::HostFault;
use vibra_ir::external::host::{HostInterface, HostType, TestWorldOp};

use crate::driver::{Acquire, Driver, Resolved, ResourceId};
use crate::path::RelativeNames;
use crate::policy::{
    DirAction, DirDepth, DirectoryRule, Expr, FileAction, FileDepth, FileRule,
    NameTest, Policy,
};
use crate::recipe::Provider;
use crate::trace::GrantOrigin;
use crate::world::{WorldDriver, WorldError};

/// One root a member asks the broker to issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Root {
    /// The operation interface of the grant.
    pub(crate) interface: HostInterface,
    /// The resource the member made or acquired.
    pub(crate) resource: ResourceId,
    /// The policy of a directory grant, in its normal form.
    pub(crate) policy: Option<Policy>,
}

/// What one member made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Made {
    /// The roots of a result record, by field name, in the row's order.
    Roots(Vec<(&'static str, Root)>),
    /// The one root of `random`.
    Root(Root),
    /// What the world's streams have received, per stream.
    Output {
        /// Standard output.
        stdout: Vec<u8>,
        /// Standard error.
        stderr: Vec<u8>,
    },
}

/// A defect: a value checked source cannot hand a member.
fn invalid(message: impl Into<String>) -> HostFault {
    HostFault::InvalidHostValue(message.into())
}

/// The fault of a primitive of the world that made nothing. A fixture
/// whose paths conflict is the test's setup fault, not a defect.
fn world_fault(error: WorldError) -> HostFault {
    match error {
        WorldError::Conflict(conflict) => HostFault::ConflictingFixture(conflict.names),
        WorldError::Exhausted => HostFault::MemoryExhausted,
    }
}

/// Runs the member `op` on `world` with its labelled arguments, in the
/// canonical order of their labels.
///
/// # Errors
///
/// `InvalidHostValue` for arguments that are not the member's;
/// [`HostFault::ConflictingFixture`] for a fixture whose paths conflict;
/// [`HostFault::MemoryExhausted`] when the world has no resource identity
/// left.
pub(crate) fn member(
    op: TestWorldOp,
    world: &mut WorldDriver,
    arguments: &[Value],
) -> Result<Made, HostFault> {
    match op {
        TestWorldOp::Directory => {
            let [files] = arguments else {
                return Err(arity(op));
            };
            let resource =
                world.make_directory(fixture(files)?).map_err(world_fault)?;
            roots(op, |interface| {
                Ok(Root {
                    interface,
                    resource,
                    policy: Some(directory_policy(interface)?),
                })
            })
            .map(Made::Roots)
        }
        TestWorldOp::Streams => {
            let [Value::Str(stdin)] = arguments else {
                return Err(arity(op));
            };
            let streams = world
                .make_streams(stdin.clone().into_bytes())
                .map_err(world_fault)?;
            roots(op, |interface| {
                let resource = match interface {
                    HostInterface::StdinText | HostInterface::StdinBytes => {
                        streams.stdin
                    }
                    HostInterface::StdoutText | HostInterface::StdoutBytes => {
                        streams.stdout
                    }
                    HostInterface::StderrText | HostInterface::StderrBytes => {
                        streams.stderr
                    }
                    _ => return Err(foreign(op, interface)),
                };
                Ok(plain(interface, resource))
            })
            .map(Made::Roots)
        }
        TestWorldOp::Environment => {
            let [Value::List(arguments), Value::Dict(variables)] = arguments else {
                return Err(arity(op));
            };
            let arguments = arguments
                .iter()
                .map(|argument| text(op, argument))
                .collect::<Result<Vec<_>, _>>()?;
            let variables = variables
                .iter()
                .map(|(name, value)| Ok((text(op, name)?, text(op, value)?)))
                .collect::<Result<BTreeMap<_, _>, HostFault>>()?;
            let environment = world
                .set_environment(variables, arguments)
                .map_err(world_fault)?;
            roots(op, |interface| {
                let resource = match interface {
                    HostInterface::EnvReadVar => environment.variables,
                    HostInterface::EnvReadArgs => environment.arguments,
                    _ => return Err(foreign(op, interface)),
                };
                Ok(plain(interface, resource))
            })
            .map(Made::Roots)
        }
        TestWorldOp::Clocks => {
            none(op, arguments)?;
            roots(op, |interface| {
                let provider = match interface {
                    HostInterface::WallClock => Provider::TimeWall,
                    HostInterface::MonotonicClock => Provider::TimeMonotonic,
                    _ => return Err(foreign(op, interface)),
                };
                let resource = acquire(world, provider, &Resolved::SystemClock)?;
                Ok(plain(interface, resource))
            })
            .map(Made::Roots)
        }
        TestWorldOp::Random => {
            none(op, arguments)?;
            let resource = acquire(world, Provider::Random, &Resolved::SystemRandom)?;
            Ok(Made::Root(plain(HostInterface::RandomBytes, resource)))
        }
        TestWorldOp::Output => {
            none(op, arguments)?;
            let output = world.output();
            Ok(Made::Output {
                stdout: output.stdout.to_vec(),
                stderr: output.stderr.to_vec(),
            })
        }
    }
}

fn arity(op: TestWorldOp) -> HostFault {
    invalid(format!("`{}` with arguments it does not take", op.symbol()))
}

fn foreign(op: TestWorldOp, interface: HostInterface) -> HostFault {
    invalid(format!(
        "`{}` issues no `{}` grant",
        op.symbol(),
        interface.name()
    ))
}

fn none(op: TestWorldOp, arguments: &[Value]) -> Result<(), HostFault> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(arity(op))
    }
}

fn text(op: TestWorldOp, value: &Value) -> Result<String, HostFault> {
    match value {
        Value::Str(text) => Ok(text.clone()),
        _ => Err(arity(op)),
    }
}

/// A root that adds no policy.
const fn plain(interface: HostInterface, resource: ResourceId) -> Root {
    Root {
        interface,
        resource,
        policy: None,
    }
}

/// One root per field of the member's result record, in the row's order.
fn roots(
    op: TestWorldOp,
    mut root: impl FnMut(HostInterface) -> Result<Root, HostFault>,
) -> Result<Vec<(&'static str, Root)>, HostFault> {
    let HostType::Record(fields) = op.row().result else {
        return Err(invalid(format!("`{}` returns no record", op.symbol())));
    };
    fields
        .iter()
        .map(|field| match field.ty {
            HostType::Interface(interface) => Ok((field.name, root(interface)?)),
            _ => Err(invalid(format!(
                "the field `{}` of `{}` is not a grant",
                field.name,
                op.symbol()
            ))),
        })
        .collect()
}

/// A clock or the random state of the world.
fn acquire(
    world: &mut WorldDriver,
    provider: Provider,
    resource: &Resolved,
) -> Result<ResourceId, HostFault> {
    match world.acquire(provider, resource) {
        Acquire::Acquired(resource) => Ok(resource),
        Acquire::Refused => {
            Err(invalid(format!("the world refused `{}`", provider.atom())))
        }
        Acquire::Failed(reason) => Err(invalid(reason)),
    }
}

/// One file of a fixture: its names from the root, and its contents.
type File = (Vec<String>, Vec<u8>);

/// The files of a `files` fixture: each `RelativePath` key by its names,
/// each `Str` value by its UTF-8 bytes.
fn fixture(files: &Value) -> Result<Vec<File>, HostFault> {
    let wrong = || invalid("the `files` of `testWorld.directory` is not a fixture");
    let Value::Dict(entries) = files else {
        return Err(wrong());
    };
    entries
        .iter()
        .map(|(path, contents)| {
            let (Value::Wrapper(names), Value::Str(contents)) = (path, contents) else {
                return Err(wrong());
            };
            let Value::List(names) = names.as_ref() else {
                return Err(wrong());
            };
            let names = names
                .iter()
                .map(|name| match name {
                    Value::Str(name) => Ok(name.clone()),
                    _ => Err(wrong()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            RelativeNames::new(names.iter().cloned()).map_err(|fault| {
                invalid(format!(
                    "a `files` path is not a `RelativePath`: {}",
                    fault.error
                ))
            })?;
            Ok((names, contents.clone().into_bytes()))
        })
        .collect()
}

/// The policy of the `directory` root of `interface`: its own action
/// everywhere in the directory.
fn directory_policy(interface: HostInterface) -> Result<Policy, HostFault> {
    let root = GrantOrigin::TestWorld.policy_root().to_owned();
    let files = |actions: Vec<FileAction>| {
        Expr::FileRule(FileRule {
            root: root.clone(),
            prefix: RelativeNames::default(),
            depth: FileDepth::Tree,
            actions,
            names: NameTest::AnyName,
        })
    };
    let directories = |actions: Vec<DirAction>| {
        Expr::DirectoryRule(DirectoryRule {
            root: root.clone(),
            prefix: RelativeNames::default(),
            depth: DirDepth::Tree,
            actions,
        })
    };
    let expression = match interface {
        HostInterface::DirReadBytes | HostInterface::DirReadText => {
            files(vec![FileAction::Read])
        }
        HostInterface::DirWriteBytes | HostInterface::DirWriteText => {
            files(vec![FileAction::Replace, FileAction::Create])
        }
        HostInterface::DirList => directories(vec![DirAction::List]),
        // 03 "Filesystem policies": a `stat` of a directory is decided by the
        // directory rules and of any other entry by the file rules.
        HostInterface::DirStat => Expr::AnyOf(vec![
            directories(vec![DirAction::Stat]),
            files(vec![FileAction::Stat]),
        ]),
        _ => return Err(foreign(TestWorldOp::Directory, interface)),
    };
    Policy::new(&expression, 0)
        .map_err(|_| invalid("the policy of a test directory is over the limits"))
}
