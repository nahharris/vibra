//! Helpers shared by the test modules.
//!
//! Each helper lives here once. Where several modules once carried their own
//! copy, this is the one form that serves every caller: a module that needs a
//! variation builds it from these, and a module that needs something no other
//! module needs keeps it beside its tests.

use std::collections::BTreeSet;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{
    Case, CaseObservation, ConformanceOperation, Corpus, HandlerError, ProfileHandler,
};
use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type, Value,
};
use vibra_types::check_source;
use vibra_wasm::layout::{self, header, state};
use vibra_wasm_run::{
    Instance, LiveSizes, MemoryLimit, Observed, Outcome, Runner, Started,
};

pub(crate) use vibra_conformance::{corpus_root, workspace_root};

pub(crate) const MIB: usize = 1024 * 1024;

// -- the registry in source ----------------------------------------------------

/// The imports a program that spells registry rows with [`call_head`] needs.
pub(crate) const ROW_IMPORTS: &str = "(import core @std.core)\n(import from @std.core.from)\n(import try-from @std.core.try-from)\n";

/// How source reaches a registry row: a static method through its type path,
/// such as `i32.add` or `str.concat`, or, for a row that is the member of an
/// implementation, its contract, such as `equatable.equal` for `i32.equal` and
/// `from.convert` for `i8.to-i16` or `i32.to-str`. A contract call selects the
/// implementation from its operands or from the written result type.
pub(crate) fn call_head(intrinsic: vibra_ir::external::CompilerIntrinsic) -> String {
    use vibra_ir::external::{CompilerIntrinsic as C, FloatOp, IntegerOp};
    match intrinsic {
        C::Integer(_, IntegerOp::Equal) | C::TextEqual | C::BytesEqual => {
            "equatable.equal".to_owned()
        }
        C::Integer(_, IntegerOp::Compare) | C::TextCompare | C::BytesCompare => {
            "ordered.compare".to_owned()
        }
        C::Integer(_, IntegerOp::ToStr) | C::Float(_, FloatOp::ToStr) => {
            "from.convert".to_owned()
        }
        C::Integer(_, IntegerOp::Parse) | C::Float(_, FloatOp::Parse) => {
            "try-from.convert".to_owned()
        }
        C::Convert(source, target) if C::conversion_is_total(source, target) => {
            "from.convert".to_owned()
        }
        C::Convert(..) => "try-from.convert".to_owned(),
        other => match other.symbol().strip_prefix("text.") {
            Some(member) => format!("str.{member}"),
            None => other.symbol().to_owned(),
        },
    }
}

// -- the filesystem -----------------------------------------------------------

/// The one process-wide serial that keeps every temporary directory of this
/// test binary apart, whichever module made it.
static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

/// A fresh, empty directory under the system temporary directory, removed when
/// it drops. It dereferences to its path.
#[derive(Debug)]
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// A directory named for `label`, this process, and a serial.
    pub(crate) fn new(label: &str) -> Self {
        Self::under(&std::env::temp_dir(), label)
    }

    /// Like [`TempDir::new`], under the canonical form of the temporary
    /// directory, for the checks that confine a tree to its canonical root.
    pub(crate) fn canonical(label: &str) -> Self {
        let parent = std::fs::canonicalize(std::env::temp_dir())
            .expect("a canonical temporary directory");
        Self::under(&parent, label)
    }

    fn under(parent: &Path, label: &str) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "vibra-conformance-{label}-{}-{serial}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temporary directory");
        Self { path }
    }

    /// The directory.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Copies the tree `from` to `to`, creating `to`.
pub(crate) fn copy_directory(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("a directory") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// Every `.rs` file below `directory`, appended to `found`.
pub(crate) fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The production code of a Rust source: everything before its test module,
/// without line comments.
pub(crate) fn production(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("readable source");
    let end = text.find("\n#[cfg(test)]").unwrap_or(text.len());
    text[..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `(workspace-relative path, production code)` of every source of the crate
/// `name`.
pub(crate) fn crate_sources(name: &str) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    rust_sources(
        &workspace_root().join("crates").join(name).join("src"),
        &mut paths,
    );
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(workspace_root())
                .expect("inside the workspace")
                .to_string_lossy()
                .replace('\\', "/");
            (relative, production(&path))
        })
        .collect()
}

// -- the corpus ---------------------------------------------------------------

/// The repository corpus, loaded once for the whole test binary.
pub(crate) fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS.get_or_init(|| Corpus::discover(corpus_root()).expect("the corpus loads"))
}

/// The one case of the corpus directory `id`.
pub(crate) fn corpus_case(id: &str) -> Case {
    Corpus::discover(corpus_root().join(id))
        .expect("a corpus case directory")
        .cases()
        .first()
        .cloned()
        .expect("a corpus case")
}

/// The text of the one source document of a case: the source of an `interpret`
/// case, or the only module of the one-module workspace of a `workspace-check`
/// case, which is `src/app/main.vib` of its tree. `None` for any other case,
/// and for a workspace of several modules.
pub(crate) fn single_source(case: &Case) -> Option<String> {
    match case.manifest().operation() {
        ConformanceOperation::Interpret => {
            let source_id = case.manifest().inputs.source.as_deref()?;
            Some(case.read_file(source_id).expect("the source reads"))
        }
        ConformanceOperation::WorkspaceCheck => {
            let files = case.tree_files().expect("the tree reads");
            let mut modules =
                files.iter().filter(|file| file.source_id.ends_with(".vib"));
            let module = modules.next()?;
            if modules.next().is_some() || module.source_id != "tree/src/app/main.vib" {
                return None;
            }
            Some(String::from_utf8(module.bytes.clone()).expect("a UTF-8 source"))
        }
        _ => None,
    }
}

/// The source of the one-source corpus case `id`, so that a host test and its
/// case cannot drift.
pub(crate) fn corpus_source(id: &str) -> String {
    single_source(&corpus_case(id)).unwrap_or_else(|| panic!("{id} has no one source"))
}

/// A source program of the corpus and the canonical form of what the
/// checker made of it.
#[derive(Debug)]
pub(crate) struct CheckedSource {
    pub(crate) case_id: String,
    pub(crate) source: String,
    /// `None` when the checker rejected the source.
    pub(crate) canonical: Option<String>,
}

/// Checks the source document of every `interpret` case and the module of every
/// one-module `workspace-check` case, in corpus order. This is the whole-corpus check; most callers want
/// [`checked_sources`], which does it once.
pub(crate) fn check_every_source() -> Vec<CheckedSource> {
    let mut forms = Vec::new();
    for case in corpus().cases() {
        let Some(source) = single_source(case) else {
            continue;
        };
        let result = check_source("input.vib", &source);
        forms.push(CheckedSource {
            case_id: case.manifest().id.clone(),
            canonical: result.program().map(CheckedProgram::canonical_vibon),
            source,
        });
    }
    forms
}

/// [`check_every_source`], computed once for the whole test binary.
pub(crate) fn checked_sources() -> &'static [CheckedSource] {
    static CHECKED: OnceLock<Vec<CheckedSource>> = OnceLock::new();
    CHECKED.get_or_init(check_every_source)
}

/// A handler that reports one fixed observation for every case.
pub(crate) struct FixedHandler {
    pub(crate) observation: CaseObservation,
}

impl FixedHandler {
    pub(crate) fn new(observation: CaseObservation) -> Self {
        Self { observation }
    }
}

impl ProfileHandler for FixedHandler {
    fn run(&self, _case: &Case) -> Result<CaseObservation, HandlerError> {
        Ok(self.observation.clone())
    }
}

// -- running the test binary again --------------------------------------------

/// Runs the `#[ignore]`d test `name` of the module `module_path` in a child
/// process of this test binary, with its output captured.
///
/// `module_path` is the caller's `module_path!()`. A test is selected by its
/// path from the crate root, so the crate's own name is dropped from it. That is
/// what `cargo test` selects by, and nextest runs the same binary through the
/// same harness, so one name serves both.
pub(crate) fn run_ignored_child(module_path: &str, name: &str) -> Output {
    let in_crate = module_path.split_once("::").map_or("", |(_, path)| path);
    Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--ignored",
            "--exact",
            &format!("{in_crate}::{name}"),
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("the child process runs")
}

// -- checked programs and the two backends -------------------------------------

/// The checked program of `source`.
pub(crate) fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

/// How many calls in tail position the functions and module values of the
/// source `source_id` hold, not counting those of the standard library the
/// program also carries.
pub(crate) fn tail_calls(program: &CheckedProgram, source_id: &str) -> usize {
    let count = |body: &Expr| format!("{body:?}").matches("tail: true").count();
    let functions = program
        .functions()
        .iter()
        .filter(|function| function.origin().source_id() == source_id)
        .map(|function| count(function.body()));
    let globals = program
        .globals()
        .iter()
        .filter(|global| global.origin().source_id() == source_id)
        .map(|global| count(global.initializer()));
    functions.chain(globals).sum()
}

pub(crate) fn entry_type(program: &CheckedProgram) -> Type {
    program.entry().signature().result()
}

pub(crate) fn emitted(program: &CheckedProgram) -> Vec<u8> {
    vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}"))
        .into_bytes()
}

pub(crate) fn runner_with(limit: usize) -> Runner {
    Runner::new(MemoryLimit::new(limit)).expect("the engine configures")
}

/// A runner with a 64 MiB memory limit.
pub(crate) fn runner() -> Runner {
    runner_with(64 * MIB)
}

/// The source position the IR constructors of a test use.
pub(crate) fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

/// A program whose entry `main` takes nothing and returns `body`.
pub(crate) fn returning(result: Type, body: Expr) -> CheckedProgram {
    let main = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), result),
        body,
        origin(),
    )
    .expect("a checked function");
    CheckedProgram::try_new(vec![main], 0).expect("a checked program")
}

/// A program that returns the literal `value`.
pub(crate) fn literal(value: Value) -> CheckedProgram {
    let ty = value.ty();
    returning(ty, Expr::literal(value, origin()))
}

/// The interpreter's canonical result, the oracle.
pub(crate) fn oracle(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

/// What the Wasm backend observes on `runner`, with the live sizes around the
/// run.
pub(crate) fn wasm_on(
    runner: &Runner,
    program: &CheckedProgram,
) -> (String, LiveSizes) {
    let ty = entry_type(program);
    match runner
        .run_observed(&emitted(program), &ty, program.types())
        .expect("the module runs")
    {
        Observed::Completed { value, live, .. } => {
            (value.canonical_observation(&ty), live)
        }
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

/// What the Wasm backend observes, with the live sizes around the run.
pub(crate) fn wasm(program: &CheckedProgram) -> (String, LiveSizes) {
    wasm_on(&runner(), program)
}

/// Runs `source` in both backends, asserts they agree, and returns the result
/// and the live sizes.
pub(crate) fn agree(source: &str) -> (String, LiveSizes) {
    let program = checked(source);
    let expected = oracle(&program);
    let (observed, live) = wasm(&program);
    assert_eq!(observed, expected, "the backends disagree on:\n{source}");
    (observed, live)
}

/// Runs `source` in both backends, and holds the result to `expected`, which
/// is written from the canonical value encoding of the runtime chapter.
pub(crate) fn agree_on(source: &str, expected: &str) -> LiveSizes {
    let (observed, live) = agree(source);
    assert_eq!(
        observed, expected,
        "not the specified encoding for:\n{source}"
    );
    live
}

// -- a started instance ---------------------------------------------------------

pub(crate) fn word(instance: &mut Instance, address: u32) -> u32 {
    let bytes = instance
        .read_memory_for_tests(address, 4)
        .expect("the state is in memory");
    u32::from_le_bytes(bytes.try_into().expect("four bytes"))
}

/// A started instance of `program` under a memory `limit`.
pub(crate) fn start(program: &CheckedProgram, limit: usize) -> Instance {
    let bytes = emitted(program);
    let Started::Ready(instance) =
        runner_with(limit).start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    instance
}

/// The variant number of the enum or union value the entry of `program`
/// returns.
pub(crate) fn result_variant(program: &CheckedProgram) -> u32 {
    let mut instance = start(program, 64 * MIB);
    let Outcome::Completed { result, .. } = instance.call_entry().expect("runs") else {
        panic!("the entry did not complete");
    };
    let id = instance.result_id(result);
    let variant = instance.variant(&id).expect("an enum or union");
    instance.release(id).expect("release");
    variant
}

/// The bytes of every block the instance's module-value table reaches, the
/// table's own block included: what an instance legitimately keeps for its
/// lifetime after the host has released everything it held.
pub(crate) fn module_values_bytes(instance: &mut Instance) -> u64 {
    let table = word(instance, state::MODULE_VALUES);
    if table == 0 {
        return 0;
    }
    let mut seen = BTreeSet::new();
    let mut pending = vec![table];
    let mut bytes = 0_u64;
    while let Some(block) = pending.pop() {
        if !seen.insert(block) {
            continue;
        }
        let size = word(instance, block + header::BLOCK_SIZE);
        bytes += u64::from(size);
        let kind_and_stride = word(instance, block + header::KIND);
        let stride = (kind_and_stride >> 8) & 0xFF;
        if stride != layout::STRIDE_CELLS {
            continue;
        }
        let len = word(instance, block + header::LEN);
        let classes = instance
            .read_memory_for_tests(block + header::SIZE, len as usize)
            .expect("class bytes");
        let cells = block + layout::cells_offset(len);
        for (position, class) in (0_u32..).zip(classes) {
            if class == layout::CellClass::Ref.code() {
                pending.push(word(instance, cells + 8 * position));
            }
        }
    }
    bytes
}

/// What a run left: the bytes of live arena after the host released the result,
/// the high-water mark of the arena, and the frames that were left.
pub(crate) struct Footprint {
    /// The live size when the entry completed, while the host held the result.
    pub(crate) with_result: u64,
    pub(crate) live: u64,
    /// The bytes the module values reach, which an instance keeps for its life.
    pub(crate) kept: u64,
    pub(crate) arena_used: u32,
    pub(crate) frame_depth: u32,
}

/// Runs the entry once on a fresh instance under a memory `limit` and reads the
/// result, then measures.
pub(crate) fn footprint(program: &CheckedProgram, limit: usize) -> Footprint {
    let mut instance = start(program, limit);
    let Outcome::Completed {
        result,
        live_size: with_result,
    } = instance.call_entry().expect("runs")
    else {
        panic!("the entry did not complete");
    };
    instance
        .observe(result, &entry_type(program), program.types())
        .expect("the host reads the result");
    let live = instance.live_size().expect("live size");
    let kept = module_values_bytes(&mut instance);
    Footprint {
        with_result,
        live,
        kept,
        arena_used: word(&mut instance, state::ARENA_USED),
        frame_depth: word(&mut instance, state::FRAME_DEPTH),
    }
}

// -- canonical value encodings and a counter -------------------------------------

/// The observation of a `value` of the type written `ty`.
pub(crate) fn result(ty: &str, value: &str) -> String {
    format!("(record type: {ty} value: {value})\n")
}

pub(crate) fn tuple_type(components: &[&str]) -> String {
    format!(
        "(record type: @tuple arguments: (array {}))",
        components.join(" ")
    )
}

pub(crate) fn tuple_value(components: &[&str]) -> String {
    format!(
        "(record kind: @tuple values: (array {}))",
        components.join(" ")
    )
}

/// The definitions of a counter that counts to `n`: `state`, `(zero)`,
/// `(inc s)`, which makes the next state, and `(done s)`, true exactly when the
/// state is `n`. A state is a record of one `u32`, so every round allocates a
/// fresh record, which the loops that use it rely on to show that the garbage is
/// freed.
pub(crate) fn counter(n: u32) -> String {
    format!(
        "(deftype state (record n u32))\n\
(defn zero () state (state n: 0u32))\n\
(defn inc (s state) state\n  (state n: (match (u32.add (s @n) 1u32) (result.ok next) next (result.err -) 0u32)))\n\
(defn done (s state) bool (equatable.equal (s @n) {n}u32))\n"
    )
}

/// The most the live arena may grow between two iteration counts of a loop,
/// and the most the arena's high-water mark may: constants of the programs that
/// use [`counter`], which allocates a fresh `state` record in every round. A
/// loop that grew with its rounds would grow by megabytes between ten thousand
/// and a hundred thousand of them.
pub(crate) const LIVE_GROWTH_BOUND: u64 = 64;
pub(crate) const ARENA_GROWTH_BOUND: u32 = 4 * 1024;

/// Non-tail recursion that never reaches a base case, with no arithmetic.
pub(crate) const FOREVER: &str = "(defn main () void (do (forever) void))\n(defn forever () void (do (forever) void))\n";
