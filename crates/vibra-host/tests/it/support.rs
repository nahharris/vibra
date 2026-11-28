//! The helpers several subject modules share: replies, policies, names,
//! grants, values, and a thread with a small stack.

use vibra_host::broker::{Context, Counters};
use vibra_host::policy::{self, Policy};
use vibra_host::recipe::LeafName;
use vibra_host::trace::declaration_identity;
use vibra_ir::authority::{GrantToken, HostErrorCase, HostOk, HostReply, Narrowed};
use vibra_ir::external::host::{HostInterface, HostOp};
use vibra_ir::{EntityRef, Value};

pub(crate) const ANY_NAME: &str = "{kind: @anyName}";

pub(crate) const VOID: HostReply = HostReply::Ok(HostOk::Void);

pub(crate) fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

pub(crate) fn name(atom: &str) -> LeafName {
    LeafName::new(atom)
}

pub(crate) fn policy(expression: &str) -> Policy {
    policy::decode(
        format!(
            "{{format: @scopePolicy.v1 family: @filesystem expression: {expression}}}"
        )
        .as_bytes(),
    )
    .expect("a policy")
}

pub(crate) fn suffix(value: &str) -> String {
    format!("{{kind: @suffix value: \"{value}\"}}")
}

pub(crate) fn err(op: HostOp, case: &str) -> HostReply {
    HostReply::Err(HostErrorCase::new(op.row().error, case).expect("a case"))
}

pub(crate) fn text(value: &str) -> HostReply {
    HostReply::Ok(HostOk::Text(value.to_owned()))
}

pub(crate) fn bytes(value: &[u8]) -> HostReply {
    HostReply::Ok(HostOk::Bytes(value.to_vec()))
}

pub(crate) fn counters(cx: &Context<'_>, grant: GrantToken) -> Counters {
    cx.counters_for_tests(grant).expect("counters")
}

pub(crate) fn granted(narrowed: Narrowed) -> GrantToken {
    match narrowed {
        Narrowed::Grant(token) => token,
        Narrowed::Refused(refusal) => panic!("refused: {refusal:?}"),
    }
}

pub(crate) fn variant(case: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::Variant {
        case: case.to_owned(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    }
}

pub(crate) fn entity(interface: HostInterface) -> EntityRef {
    let owner = interface.owner();
    EntityRef::new(
        owner.path().trim_start_matches('@'),
        declaration_identity(
            "std",
            "0.0.2",
            owner.module,
            "",
            "@interface",
            owner.name,
        ),
    )
    .expect("an entity reference")
}

/// Runs `work` on a thread with a 256 KiB stack: a stack overflow there
/// aborts the test binary.
pub(crate) fn on_small_stack<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(work)
        .expect("the thread starts")
        .join()
        .expect("no stack overflow")
}
