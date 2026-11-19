//! The `harness` launcher profile (decision N18), compiled only with the cargo
//! feature `harness`.
//!
//! It takes a `@host-world.v1` document and a quota, runs the program, and
//! writes one observation document. The document is internal to the
//! conformance runner and unversioned. The shipped archive does not contain
//! this module, so a shipped launcher has no way to select a host world.
//!
//! # The observation document
//!
//! ```text
//! (record outcome: @completed|@program-failed|@trapped|@memory-exhausted|@refused
//!         result: @void-or-ok|@err|@none
//!         trap: @runtime.<code>|@none  origin: <ordinal>
//!         audit: (record format: @audit-trace.v1 events: (array ...))
//!         counters: (record yields: <n> peak-frames: <n> tail-transfers: <n>
//!                           peak-live: <n> live: <n>)|@none
//!         value: <node>|@none)
//! ```
//!
//! `result` says only whether the entry returned `err`. `value` is the entry's
//! result exactly as the context's store holds it, described without types, and
//! is present when the run completed or ended in `program-failed`. `counters`
//! is present when a context ran: the number of times the dispatcher yielded
//! (with quota 1, one for each checkpoint but the last), the most frames held
//! at once, the tail transfers performed, the most bytes of values and frames
//! ever live, and the bytes live at the end (the result is still held then).
//! The context does not tell what its module values keep, so that counter is
//! absent.
//!
//! # The dump of a value
//!
//! A node is a VIBON record whose first field is `node:`. The bits of a scalar
//! are an unsigned decimal inside a string, because a `u64` does not fit a
//! VIBON integer; `variant` and `kind` are bare non-negative integers:
//!
//! ```text
//! (record node: @scalar class: @i32|@i64|@f32|@f64 bits: "<u64 decimal>")
//! (record node: @str value: "<text>")      ; the text is the canonical string
//! (record node: @atom value: "<text>")     ; of `vibra_ir::canonical_string`
//! (record node: @bytes hex: "<lowercase hex>")
//! (record node: @function)                 ; opaque: nothing of it is read
//! (record node: @object kind: @tuple|@array|@dict|@record|@enum|@wrapper|@union
//!         variant: <u32> cells: (array <node>...))
//! (record node: @unknown kind: <u8>)       ; a kind this dumper does not know
//! (record node: @fault)                    ; a handle or layout the store refuses
//! (record node: @truncated)                ; a bound was reached
//! ```
//!
//! The dump names a scalar's class and bits, never a type: a `bool` is an
//! `@object` of kind `@enum` and a `u8` is an `@i32` scalar, and it is the
//! reader of the conformance runner that gives them their meaning. Shared
//! structure is dumped once for each place it is reached.
//!
//! The dump is bounded by [`MAX_DEPTH`] levels, [`MAX_NODES`] nodes, and
//! [`MAX_PAYLOAD`] bytes of text (as UTF-8) and byte strings together. A node
//! that would pass a bound is written as `@truncated`, and an object that runs
//! out of budget between two cells ends its `cells` there with one `@truncated`
//! node. A reader treats a `@truncated` node as a result it cannot observe.

use std::fmt::Write as _;
use std::io::Write;

use vibra_host::{HostWorld, WorldProvider, trace_to_vibon};
use vibra_ir::arena::{class, kind};
use vibra_ir::canonical_string;
use vibra_nexus::{Cell, Context, Store};

use crate::abi::VibraProgram;
use crate::admission::admit;
use crate::launcher::{
    Execution, Finished, SYSTEM_MEMORY_LIMIT, refused, run_admitted, trap_atom,
};

/// Exit code of a harness launch whose input was not a host world.
pub const EXIT_INVALID_INPUT: i32 = 2;

/// The deepest level of a dump: the root is level 0. This is the depth the
/// conformance reader follows.
pub const MAX_DEPTH: usize = 64;
/// The most nodes a dump holds.
pub const MAX_NODES: usize = 65_536;
/// The most bytes of text (as UTF-8) and byte strings a dump holds, together.
pub const MAX_PAYLOAD: usize = 1 << 20;

/// The node that stands for what a bound cut off.
const TRUNCATED: &str = "(record node: @truncated)";
/// The node that stands for what the store does not admit.
const FAULT: &str = "(record node: @fault)";

/// The counters of one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counters {
    /// Times the dispatcher yielded.
    pub yields: u64,
    /// The most frames held at once.
    pub peak_frames: u64,
    /// Tail transfers performed.
    pub tail_transfers: u64,
    /// The most bytes of values and frames ever live.
    pub peak_live: u64,
    /// The bytes live now.
    pub live: u64,
}

/// What the harness reads from the context of a run that ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observed {
    /// The counters of the run.
    pub counters: Counters,
    /// The dump of the entry's result, for a run that returned one.
    pub value: Option<String>,
}

/// The counters of `ctx`.
#[must_use]
pub fn counters(ctx: &Context<'_>) -> Counters {
    let stats = ctx.stats();
    Counters {
        yields: stats.yields,
        peak_frames: u64::try_from(stats.max_depth).unwrap_or(u64::MAX),
        tail_transfers: stats.tail_transfers,
        peak_live: ctx.peak_size(),
        live: ctx.live_size(),
    }
}

/// Reads what the harness reports from a context whose run ended with
/// `finished`. Reading takes no reference and releases none: the context's
/// live size is the same before and after.
pub fn observe(ctx: &mut Context<'_>, finished: &Finished) -> Observed {
    let result = ctx.result();
    let value = matches!(finished, Finished::Completed | Finished::ProgramFailed)
        .then(|| dump_value(ctx.store(), result));
    Observed {
        counters: counters(ctx),
        value,
    }
}

/// The dump of the value `cell` holds, in the grammar of the module
/// documentation. The cell stays owned by the caller.
///
/// The walk terminates because a value is immutable once built and a value
/// cannot refer to itself (`docs/spec/06-runtime.md`; the store's own
/// reclamation depends on it), so following the cells of an object reaches
/// only values built before it. Termination does not rest on that: every step
/// down is counted against [`MAX_DEPTH`] and every node against [`MAX_NODES`].
#[must_use]
pub fn dump_value(store: &Store, cell: Cell) -> String {
    let mut dumper = Dumper {
        store,
        out: String::new(),
        nodes: 0,
        payload: 0,
    };
    dumper.cell(cell, 0);
    dumper.out
}

struct Dumper<'a> {
    store: &'a Store,
    out: String,
    nodes: usize,
    payload: usize,
}

impl Dumper<'_> {
    const fn spent(&self) -> bool {
        self.nodes >= MAX_NODES || self.payload > MAX_PAYLOAD
    }

    fn cell(&mut self, cell: Cell, depth: usize) {
        if depth > MAX_DEPTH || self.spent() {
            self.out.push_str(TRUNCATED);
            return;
        }
        self.nodes += 1;
        match cell.as_ref() {
            None => self.scalar(cell),
            Some(value) => self.value(value, depth),
        }
    }

    fn scalar(&mut self, cell: Cell) {
        let class = match cell.class {
            class::I32 => "i32",
            class::I64 => "i64",
            class::F32 => "f32",
            class::F64 => "f64",
            _ => {
                self.out.push_str(FAULT);
                return;
            }
        };
        let _ = write!(
            self.out,
            "(record node: @scalar class: @{class} bits: \"{}\")",
            cell.bits
        );
    }

    fn value(&mut self, value: vibra_nexus::Ref, depth: usize) {
        let Ok(code) = self.store.kind(value) else {
            self.out.push_str(FAULT);
            return;
        };
        match code {
            kind::ATOM | kind::STR => {
                let Ok(text) = self.store.text(value) else {
                    self.out.push_str(FAULT);
                    return;
                };
                if !self.take(text.len()) {
                    return;
                }
                let node = if code == kind::ATOM { "atom" } else { "str" };
                let _ = write!(
                    self.out,
                    "(record node: @{node} value: {})",
                    canonical_string(&text)
                );
            }
            kind::BYTES => {
                let Ok(bytes) = self.store.byte_values(value) else {
                    self.out.push_str(FAULT);
                    return;
                };
                if !self.take(bytes.len()) {
                    return;
                }
                self.out.push_str("(record node: @bytes hex: \"");
                for byte in bytes {
                    let _ = write!(self.out, "{byte:02x}");
                }
                self.out.push_str("\")");
            }
            kind::FUNCTION => self.out.push_str("(record node: @function)"),
            kind::TUPLE
            | kind::ARRAY
            | kind::DICT
            | kind::RECORD
            | kind::ENUM
            | kind::WRAPPER
            | kind::UNION => self.object(value, code, depth),
            other => {
                let _ = write!(self.out, "(record node: @unknown kind: {other})");
            }
        }
    }

    /// Counts `length` characters or bytes of payload, or writes the truncated
    /// node when they do not fit.
    fn take(&mut self, length: usize) -> bool {
        match self.payload.checked_add(length) {
            Some(total) if total <= MAX_PAYLOAD => {
                self.payload = total;
                true
            }
            _ => {
                self.out.push_str(TRUNCATED);
                false
            }
        }
    }

    fn object(&mut self, value: vibra_nexus::Ref, code: u8, depth: usize) {
        let (Ok(len), Ok(variant)) = (self.store.len(value), self.store.variant(value))
        else {
            self.out.push_str(FAULT);
            return;
        };
        let name = match code {
            kind::TUPLE => "tuple",
            kind::ARRAY => "array",
            kind::DICT => "dict",
            kind::RECORD => "record",
            kind::ENUM => "enum",
            kind::WRAPPER => "wrapper",
            _ => "union",
        };
        let _ = write!(
            self.out,
            "(record node: @object kind: @{name} variant: {variant} cells: (array"
        );
        for index in 0..len {
            self.out.push(' ');
            if self.spent() {
                self.out.push_str(TRUNCATED);
                break;
            }
            match self.store.cell(value, index) {
                Ok(child) => self.cell(child, depth + 1),
                Err(_) => self.out.push_str(FAULT),
            }
        }
        self.out.push_str("))");
    }
}

/// The observation document of an execution: the module documentation's
/// record. `observed` is what [`observe`] read, absent when no context ran.
#[must_use]
pub fn observation(execution: &Execution, observed: Option<&Observed>) -> String {
    let (outcome, result, trap, origin) = match &execution.finished {
        Finished::Completed => ("completed", "void-or-ok", "@none".to_owned(), 0),
        Finished::ProgramFailed => ("program-failed", "err", "@none".to_owned(), 0),
        Finished::Trapped(trap) => (
            "trapped",
            "none",
            trap_atom(trap.code).to_owned(),
            trap.origin,
        ),
        Finished::MemoryExhausted => {
            ("memory-exhausted", "none", "@none".to_owned(), 0)
        }
        Finished::Refused(_) => ("refused", "none", "@none".to_owned(), 0),
    };
    let counters = observed.map_or_else(
        || "@none".to_owned(),
        |observed| {
            let counters = observed.counters;
            format!(
                "(record yields: {} peak-frames: {} tail-transfers: {} peak-live: {} live: {})",
                counters.yields,
                counters.peak_frames,
                counters.tail_transfers,
                counters.peak_live,
                counters.live
            )
        },
    );
    let value = observed
        .and_then(|observed| observed.value.as_deref())
        .unwrap_or("@none");
    format!(
        "(record outcome: @{outcome} result: @{result} trap: {trap} origin: {origin} audit: {} counters: {counters} value: {value})",
        trace_to_vibon(&execution.audit)
    )
}

/// Runs `program` in the host world `world_text` under `quota`, writes the
/// observation document to `out`, and returns the process exit code: `0` when
/// the document was written, whatever the program did.
///
/// # Safety
///
/// As [`crate::execute`].
pub unsafe fn launch_harness(
    program: &VibraProgram,
    world_text: &str,
    quota: u64,
    out: &mut dyn Write,
    diagnostics: &mut dyn Write,
) -> i32 {
    let world = match HostWorld::decode("host-world.vibon", world_text) {
        Ok(world) => world,
        Err(errors) => {
            let _ =
                writeln!(diagnostics, "invalid host world: {} error(s)", errors.len());
            return EXIT_INVALID_INPUT;
        }
    };
    let mut host = WorldProvider::new(&world);
    // SAFETY: forwarded; the caller's contract is that of `admit`.
    let (execution, observed) = match unsafe { admit(program) } {
        Ok(admitted) => {
            let (execution, observed) =
                run_admitted(admitted, &mut host, SYSTEM_MEMORY_LIMIT, quota, observe);
            (execution, Some(observed))
        }
        Err(refusal) => (refused(refusal), None),
    };
    let _ = writeln!(out, "{}", observation(&execution, observed.as_ref()));
    0
}
