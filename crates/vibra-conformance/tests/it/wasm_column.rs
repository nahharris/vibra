//! The WebAssembly executor column (`docs/spec/07-diagnostics-and-conformance.md`,
//! "Differential execution"): an executable case the interpreter accepted
//! runs on the backend too, an unsupported emit is unavailable there, a
//! mismatch names the executor and fails the run, and a missing executor is
//! reported missing.

use std::sync::Arc;

use vibra_conformance::{
    Case, CaseObservation, CaseStatus, ConformanceOperation, ConformanceProfile,
    ConformanceRunner, Corpus, Executable, HandlerError, ProfileCounts,
    ProfileDispatcher, ProfileHandler, RunReport, Verdict, WasmExecutor,
};
use vibra_diagnostics::ByteSpan;
use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{
    Case as VariantCase, CheckedFunction, CheckedModuleSet, CheckedProgram, Constant,
    Expr, FnType, FunctionBody, Guard, Library, LibraryTypes, ModuleSetParts, Node,
    SignatureRecord, SourceOrigin, Type, TypeBody, TypeDefinition, TypeId, Value,
    Visibility,
};

use crate::runner::{conformance, text};
use crate::support::{rules, scratch_directory, write};

const ID: &str = "V2-SRC-READER-x";

/// A corpus of one `interpret` case under `interpreterV2`.
fn one_interpret_case() -> std::path::PathBuf {
    let base = scratch_directory("wasm-column");
    write(&base, "rules.tsv", &rules(&[(ID, "active")]));
    write(
        &base,
        &format!("cases/SRC-READER/{ID}/case.toml"),
        &format!(
            "id = \"{ID}\"\n\
             rule = \"{ID}\"\n\
             profile = \"interpreterV2\"\n\
             operation = \"interpret\"\n\
             description = \"07-diagnostics-and-conformance.md#differential-execution\"\n\
             \n\
             [inputs]\n\
             source = \"main.vib\"\n\
             \n\
             [expect]\n\
             verdict = \"accepted\"\n"
        ),
    );
    write(&base, &format!("cases/SRC-READER/{ID}/main.vib"), "");
    base.join("cases")
}

/// A handler that accepts every `interpret` case and hands over `program`.
struct Interpreter(CheckedProgram);

impl ProfileHandler for Interpreter {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Interpret
    }

    fn run(&self, _case: &Case) -> Result<CaseObservation, HandlerError> {
        let mut observation = CaseObservation::new(Verdict::Accepted);
        observation.executable = Some(Executable::Program(self.0.clone()));
        Ok(observation)
    }
}

fn origin_at(start: usize, end: usize) -> SourceOrigin {
    SourceOrigin::new("main.vib", ByteSpan::new(start, end))
}

fn node(expr: Expr) -> Node {
    Node::new(expr, origin_at(0, 0))
}

fn int(value: i32) -> Node {
    node(Expr::Literal(
        Constant::new(Type::I32, Value::I32(value)).unwrap(),
    ))
}

fn integer(op: IntegerOp, left: Node, right: Node) -> Node {
    node(Expr::Intrinsic {
        intrinsic: CompilerIntrinsic::Integer(IntegerType::I32, op),
        type_arguments: Vec::new(),
        arguments: vec![left, right],
    })
}

/// `main` returning `body`, with `guard` as its `requires … else`.
fn main_program(body: Node, guard: Option<Guard>) -> CheckedProgram {
    let bool_id = TypeId::new(
        "std.bool.Bool",
        "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad",
    )
    .unwrap();
    let bool_type = TypeDefinition::new(
        bool_id.clone(),
        Vec::new(),
        TypeBody::Variant(vec![
            VariantCase::new("false", Vec::new()),
            VariantCase::new("true", Vec::new()),
        ]),
        None,
        Visibility::Public,
    )
    .unwrap();
    let main = CheckedFunction {
        name: "main".to_owned(),
        signature: SignatureRecord::new(
            Vec::new(),
            Type::function(FnType::new(Vec::new(), Type::I32)),
        )
        .unwrap(),
        defaults: Vec::new(),
        body: FunctionBody::Expr(Box::new(body)),
        entry_guard: guard,
        exit_guard: None,
        slot_count: 0,
        origin: origin_at(0, 0),
    };
    let set = CheckedModuleSet::try_new(ModuleSetParts {
        library: LibraryTypes::new().with(Library::Bool, bool_id),
        types: vec![bool_type],
        functions: vec![main],
        ..ModuleSetParts::default()
    })
    .unwrap();
    CheckedProgram::for_entry(Arc::<CheckedModuleSet>::clone(&set), 0).unwrap()
}

/// `I32.add(1i32 2i32)`.
fn scalar_program() -> CheckedProgram {
    main_program(integer(IntegerOp::Add, int(1), int(2)), None)
}

/// `requires 1i32 == 1i32 else 0i32`, then `1i32`.
fn guarded_program() -> CheckedProgram {
    let guard = Guard {
        predicate: integer(IntegerOp::Equal, int(1), int(1)),
        consequence: Node::new(
            Expr::Literal(Constant::new(Type::I32, Value::I32(0)).unwrap()),
            origin_at(20, 24),
        ),
    };
    main_program(int(1), Some(guard))
}

fn run(program: CheckedProgram, executor: WasmExecutor) -> RunReport {
    let corpus = Corpus::discover(one_interpret_case()).expect("the corpus loads");
    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::InterpreterV2, Interpreter(program));
    ConformanceRunner::new(dispatcher)
        .with_executor(executor)
        .run(&corpus)
}

fn executor() -> WasmExecutor {
    WasmExecutor::new().expect("the engine configures")
}

#[test]
fn wasm_column_runs_an_accepted_case() {
    let report = run(scalar_program(), executor());
    assert_eq!(report.cases()[0].status, CaseStatus::Passed);
    assert_eq!(report.cases()[0].executor, Some(CaseStatus::Passed));
    assert!(report.is_success());
}

#[test]
fn wasm_column_reports_unavailable() {
    let report = run(guarded_program(), executor());
    let case = &report.cases()[0];
    assert_eq!(case.status, CaseStatus::Passed, "the interpreter column");
    let Some(CaseStatus::Unavailable { reason }) = &case.executor else {
        panic!("unavailable on the executor: {:?}", case.executor);
    };
    assert!(reason.contains("`entryGuard`"), "{reason}");
    assert!(reason.contains("@tool.unavailable"), "{reason}");
    assert_eq!(
        report.executor_counts(),
        vec![(
            ConformanceProfile::InterpreterV2,
            ProfileCounts {
                unavailable: 1,
                ..ProfileCounts::default()
            }
        )]
    );
    assert_eq!(report.profile_counts()[0].1.passed, 1);
    assert!(
        report.is_success(),
        "an unavailable case does not fail the run"
    );
}

#[test]
fn wasm_column_mismatch_fails() {
    let report = run(scalar_program(), executor().with_flipped_result());
    let case = &report.cases()[0];
    assert_eq!(case.status, CaseStatus::Passed, "the interpreter column");
    let Some(CaseStatus::Failed { reason }) = &case.executor else {
        panic!("failed on the executor: {:?}", case.executor);
    };
    assert!(reason.starts_with("wasm executor: "), "{reason}");
    assert!(
        reason.contains("the entry ended with the value flipped")
            && reason.contains("@value.v2"),
        "the first difference, with both endings: {reason}"
    );
    assert_eq!(report.executor_total().failed, 1);
    assert!(!report.is_success(), "a mismatch fails the run");
}

#[test]
fn a_missing_executor_is_reported_missing() {
    let base = scratch_directory("missing-executor");
    write(&base, "rules.tsv", &rules(&[(ID, "active")]));
    write(
        &base,
        &format!("cases/SRC-READER/{ID}/case.toml"),
        &crate::support::reader_case(ID, ID),
    );
    write(&base, &format!("cases/SRC-READER/{ID}/input.vibon"), "{}");
    let root = base.join("cases");
    let output = conformance(&[
        "--root".as_ref(),
        root.as_os_str(),
        "--executor".as_ref(),
        "native".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = text(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line == "native executor: missing"),
        "{stdout}"
    );
    // The backend runs no case of a corpus without executable cases.
    let output = conformance(&[
        "--root".as_ref(),
        root.as_os_str(),
        "--executor".as_ref(),
        "wasm".as_ref(),
    ]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert!(
        stdout.lines().any(|line| line
            == "wasm executor total: 0 passed, 0 failed, 0 unknown, 0 unavailable"),
        "{stdout}"
    );
}
