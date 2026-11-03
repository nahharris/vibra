//! The host boundary in both backends, for what a corpus case cannot state.
//!
//! The operations themselves (non-ASCII text, bytes, standard input read once,
//! invalid text) are the corpus cases `V1-RUNTIME-io-*`, which the conformance
//! gate runs in both backends. What remains is proved with hand-built checked IR
//! whose functions are the registry rows of `io`: a one-mebibyte input and a
//! million-byte output cross the boundary, and a provider that exhausts the
//! host's memory is the host event in both backends.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_diagnostics::ByteSpan;
use vibra_host::{
    AuditEvent, Done, Failure, HostProvider, HostWorld, IoError, WorldProvider,
};
use vibra_interp::{Interpreter, MemoryBudget, RuntimeError};
use vibra_ir::external::host::{HostRow, host_row};
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type,
    TypeBody, TypeDefinition, TypeId, Value,
};
use vibra_wasm_run::{MemoryLimit, Observed, Outcome, Runner};

const MIB: usize = 1024 * 1024;

fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

// -- the program --------------------------------------------------------------

fn result_id() -> TypeId {
    TypeId::new("@vibra-stdlib@0.3.0/std.result.result", "std.result.result")
}

fn io_error_id() -> TypeId {
    TypeId::new("@vibra-stdlib@0.3.0/std.io.io-error", "std.io.io-error")
}

fn types() -> Vec<TypeDefinition> {
    vec![
        TypeDefinition::new(
            result_id(),
            TypeBody::Enum(vec![
                ("ok".to_owned(), Type::Param("t".to_owned())),
                ("err".to_owned(), Type::Param("e".to_owned())),
            ]),
        )
        .with_parameters(vec!["t".to_owned(), "e".to_owned()]),
        TypeDefinition::new(
            io_error_id(),
            TypeBody::Enum(vec![
                ("invalid-text".to_owned(), Type::Void),
                ("closed".to_owned(), Type::Void),
                ("other".to_owned(), Type::Void),
            ]),
        ),
    ]
}

fn result_of(ok: Type) -> Type {
    Type::Applied(result_id(), vec![ok, Type::Declared(io_error_id())])
}

fn row_of(symbol: &str) -> &'static HostRow {
    host_row(symbol).expect("a registry row")
}

/// The function of a registry row, with the signature the standard library
/// would declare for it.
fn host_function(symbol: &str) -> CheckedFunction {
    let row = row_of(symbol);
    let parameters = row
        .parameters
        .iter()
        .map(|parameter| parameter.ty.primitive().expect("a primitive parameter"))
        .collect();
    let ok = row.ok.primitive().expect("a primitive payload");
    CheckedFunction::new_host(
        symbol,
        FunctionSignature::new(parameters, result_of(ok)),
        row,
        origin(),
    )
    .expect("a host function")
}

/// An entry that applies each `(symbol, arguments)` in order and answers the
/// result of the last one.
fn program(calls: &[(&str, Vec<Value>)]) -> CheckedProgram {
    let mut functions = Vec::new();
    let mut applied = Vec::new();
    for (symbol, arguments) in calls {
        let index = functions
            .iter()
            .position(|function: &CheckedFunction| function.name() == *symbol)
            .unwrap_or_else(|| {
                functions.push(host_function(symbol));
                functions.len() - 1
            });
        let row = row_of(symbol);
        let ok = row.ok.primitive().expect("a primitive payload");
        applied.push(Expr::call(
            index,
            arguments
                .iter()
                .map(|value| Expr::literal(value.clone(), origin()))
                .collect(),
            result_of(ok),
            origin(),
        ));
    }
    let result = applied.last().expect("a call").result_type();
    let entry = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), result),
        Expr::sequence(applied, origin()),
        origin(),
    )
    .expect("an entry");
    functions.push(entry);
    let entry = functions.len() - 1;
    CheckedProgram::try_new_with_types(types(), Vec::new(), functions, entry)
        .expect("a checked program")
}

fn text(value: &str) -> Value {
    Value::Str(value.to_owned())
}

fn bytes(values: &[u8]) -> Value {
    Value::Bytes(values.to_vec())
}

// -- the two backends ----------------------------------------------------------

fn world(stdin: &str) -> HostWorld {
    HostWorld::decode(
        "world.vibon",
        &format!("(record format: @host-world.v1 stdin: {stdin})\n"),
    )
    .expect("a world")
}

struct Seen {
    result: String,
    trace: Vec<AuditEvent>,
}

fn interpreted(program: &CheckedProgram, world: &HostWorld) -> Seen {
    let mut provider = WorldProvider::new(world);
    let execution =
        Interpreter::run_with_host(program, MemoryBudget::new(64 * MIB), &mut provider)
            .expect("the interpreter runs");
    Seen {
        result: execution.canonical_result(),
        trace: execution.audit_trace().to_vec(),
    }
}

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * MIB)).expect("the engine configures")
}

fn emitted(program: &CheckedProgram) -> Vec<u8> {
    vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}"))
        .into_bytes()
}

fn webassembly(program: &CheckedProgram, world: &HostWorld) -> Seen {
    let result = program.entry().signature().result();
    match runner()
        .run_observed_with_host(
            &emitted(program),
            &result,
            program.types(),
            &|_| None,
            Box::new(WorldProvider::new(world)),
        )
        .expect("the module runs")
    {
        Observed::Completed {
            value,
            live,
            audit_trace,
        } => {
            assert_eq!(
                live.end, live.start,
                "the run returns the arena to where it started"
            );
            Seen {
                result: value.canonical_observation(&result),
                trace: audit_trace,
            }
        }
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

/// Runs `program` in both backends over copies of `world`, requires them to
/// agree, and answers what they saw.
fn agree(program: &CheckedProgram, world: &HostWorld) -> Seen {
    let oracle = interpreted(program, world);
    let wasm = webassembly(program, world);
    assert_eq!(wasm.result, oracle.result, "the results differ");
    assert_eq!(
        wasm.trace
            .iter()
            .map(AuditEvent::to_vibon)
            .collect::<Vec<_>>(),
        oracle
            .trace
            .iter()
            .map(AuditEvent::to_vibon)
            .collect::<Vec<_>>(),
        "the traces differ"
    );
    oracle
}

#[test]
fn a_one_mebibyte_input_and_a_million_byte_output_cross_the_boundary() {
    let big = "x".repeat(MIB);
    let world = HostWorld::decode(
        "world.vibon",
        &format!("(record format: @host-world.v1 stdin: \"{big}\")\n"),
    )
    .expect("a world");
    let seen = agree(&program(&[("io.stdin-text", vec![])]), &world);
    assert!(seen.result.contains(&big));
    let output = vec![7_u8; 1_000_000];
    let seen = agree(
        &program(&[("io.stdout-bytes", vec![bytes(&output)])]),
        &world,
    );
    assert_eq!(seen.trace.len(), 1);
}

// -- a provider that cannot answer ----------------------------------------------

/// A provider that exhausts the host's memory on standard input, so the call
/// is the host event.
struct Exhausted(WorldProvider);

impl HostProvider for Exhausted {
    fn stdin_text(&mut self) -> Done<String, IoError> {
        Err(Failure::MemoryExhausted)
    }
    fn stdin_bytes(&mut self) -> Done<Vec<u8>, IoError> {
        self.0.stdin_bytes()
    }
    fn stdout_text(&mut self, value: &str) -> Done<(), IoError> {
        self.0.stdout_text(value)
    }
    fn stdout_bytes(&mut self, value: &[u8]) -> Done<(), IoError> {
        self.0.stdout_bytes(value)
    }
    fn stderr_text(&mut self, value: &str) -> Done<(), IoError> {
        self.0.stderr_text(value)
    }
    fn stderr_bytes(&mut self, value: &[u8]) -> Done<(), IoError> {
        self.0.stderr_bytes(value)
    }
    fn read_file(&mut self, at: &str) -> Done<Vec<u8>, vibra_host::FsError> {
        self.0.read_file(at)
    }
    fn read_text(&mut self, at: &str) -> Done<String, vibra_host::FsError> {
        self.0.read_text(at)
    }
    fn read_entries(&mut self, at: &str) -> Done<Vec<String>, vibra_host::FsError> {
        self.0.read_entries(at)
    }
    fn write_file(
        &mut self,
        at: &str,
        contents: &[u8],
    ) -> Done<(), vibra_host::FsError> {
        self.0.write_file(at, contents)
    }
    fn write_text(
        &mut self,
        at: &str,
        contents: &str,
    ) -> Done<(), vibra_host::FsError> {
        self.0.write_text(at, contents)
    }
    fn stat(&mut self, at: &str) -> Done<vibra_host::Metadata, vibra_host::FsError> {
        self.0.stat(at)
    }
    fn env_var(&mut self, name: &str) -> Done<String, vibra_host::EnvError> {
        self.0.env_var(name)
    }
    fn env_args(&mut self) -> Done<Vec<String>, vibra_host::EnvError> {
        self.0.env_args()
    }
    fn now_wall(&mut self) -> Done<i64, vibra_host::TimeError> {
        self.0.now_wall()
    }
    fn now_monotonic(&mut self) -> Done<u64, vibra_host::TimeError> {
        self.0.now_monotonic()
    }
    fn random_bytes(&mut self, count: u64) -> Done<Vec<u8>, vibra_host::RandomError> {
        self.0.random_bytes(count)
    }
}

#[test]
fn memory_exhaustion_of_the_provider_is_the_host_event_in_both_backends() {
    let program = program(&[
        ("io.stdout-text", vec![text("before")]),
        ("io.stdin-text", vec![]),
    ]);
    let world = world("\"\"");
    let mut provider = Exhausted(WorldProvider::new(&world));
    let error = Interpreter::run_with_host(
        &program,
        MemoryBudget::new(64 * MIB),
        &mut provider,
    )
    .expect_err("the host event ends the run");
    assert!(
        matches!(error, RuntimeError::MemoryExhausted { .. }),
        "{error:?}"
    );
    let result = program.entry().signature().result();
    let observed = runner()
        .run_observed_with_host(
            &emitted(&program),
            &result,
            program.types(),
            &|_| None,
            Box::new(Exhausted(WorldProvider::new(&world))),
        )
        .expect("the module runs");
    assert!(
        matches!(observed, Observed::Stopped(Outcome::MemoryExhausted)),
        "{observed:?}"
    );
}
