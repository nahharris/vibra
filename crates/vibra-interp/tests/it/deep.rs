//! A value nested far deeper than any host stack would allow recursion
//! over: it is built, observed, compared, and dropped without consuming host
//! stack in proportion to its depth (`docs/spec/06-runtime.md`, "Activations
//! and memory" and "Canonical value encoding").
//!
//! Every test here runs on a thread with a 256 KiB stack.

use vibra_interp::{Evaluation, MemoryBudget, run, run_test};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    Case, CheckedModuleSet, Construction, Expr, Node, Type, TypeDefinition, Value,
    Visibility,
};

use crate::{
    BASE, LIMITS, SEED, construct, direct, function, generic_row, id, if_else, node,
    program, set_with, signature, tail, u64_equal, u64_lit, u64_sub, var,
    variant_value, void,
};

const DEPTH: u64 = 200_000;

const STACK: usize = 256 * 1024;

/// Runs `work` on a thread whose stack is 256 KiB.
fn on_small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(work)
        .expect("a thread")
        .join()
        .expect("the thread finished")
}

const CHAIN: &str = "test.chain.Chain";

fn chain() -> Type {
    Type::Declared(id(CHAIN), Vec::new())
}

/// `Chain`: `link(next: Chain)` or `end`.
fn chain_definition() -> TypeDefinition {
    TypeDefinition::new(
        id(CHAIN),
        Vec::new(),
        vibra_ir::TypeBody::Variant(vec![
            Case::new("link", vec![("next".to_owned(), chain())]),
            Case::new("end", Vec::new()),
        ]),
        None,
        Visibility::Public,
    )
    .unwrap()
}

fn end() -> Node {
    construct(
        chain(),
        Construction::Case {
            case: "end".to_owned(),
            fields: Vec::new(),
        },
    )
}

/// `build(n: U64, chain: Chain) Chain`: `chain` under `n` more links, by
/// tail calls.
fn build() -> vibra_ir::CheckedFunction {
    let link = construct(
        chain(),
        Construction::Case {
            case: "link".to_owned(),
            fields: vec![("next".to_owned(), var(1, chain()))],
        },
    );
    function(
        "build",
        signature(vec![Type::U64, chain()], chain()),
        if_else(
            u64_equal(var(0, Type::U64), u64_lit(0)),
            var(1, chain()),
            tail(
                direct(BASE),
                vec![u64_sub(var(0, Type::U64), u64_lit(1)), link],
            ),
        ),
        2,
    )
}

/// `build(depth, end)`.
fn built(depth: u64) -> Node {
    node(Expr::Call {
        target: direct(BASE),
        arguments: crate::positional(vec![u64_lit(depth), end()]),
    })
}

fn set_with_entry(
    entry: vibra_ir::CheckedFunction,
) -> std::sync::Arc<CheckedModuleSet> {
    set_with(vec![build(), entry], Vec::new(), vec![chain_definition()])
}

/// The neutral `Chain` of `depth` links, built without recursion.
fn expected_value(depth: u64) -> Value {
    let mut value = variant_value("end", Vec::new());
    for _ in 0..depth {
        value = variant_value("link", vec![("next", value)]);
    }
    value
}

/// The `@value.v2` document of the `Chain` of `depth` links, written by
/// hand from the chapter's encoding of a variant.
fn expected_document(depth: u64) -> String {
    let ty =
        format!("{{kind: @declared path: @{CHAIN} seed: \"{SEED}\" arguments: []}}");
    let link = format!("{{kind: @enum typeInfo: {ty} variant: @link payload: {{next: ");
    let mut text = format!("{{format: @value.v2 typeInfo: {ty} value: ");
    for _ in 0..depth {
        text.push_str(&link);
    }
    text.push_str(&format!("{{kind: @enum typeInfo: {ty} variant: @end}}"));
    for _ in 0..depth {
        text.push_str("}}");
    }
    text.push('}');
    text
}

/// A result 200,000 levels deep is observed as its neutral value and its
/// canonical document, both compared with hand-built ones, and then all of
/// them are dropped.
#[test]
fn a_result_200_000_deep_is_observed_compared_and_dropped() {
    let (same_value, same_document) = on_small_stack(|| {
        let main = function("main", signature(Vec::new(), chain()), built(DEPTH), 0);
        let execution = run(
            &program(set_with_entry(main), BASE + 1),
            MemoryBudget::DEFAULT,
        )
        .unwrap();
        let same_value = *execution.value.value() == expected_value(DEPTH);
        let same_document = execution.canonical == expected_document(DEPTH);
        (same_value, same_document)
    });
    assert!(same_value);
    assert!(same_document);
}

/// A deep value passed into an evaluation is converted in and out, and
/// comes back equal; a chain one link shorter is not equal to it.
#[test]
fn a_value_200_000_deep_round_trips_through_an_evaluation() {
    let (round_trip, shorter_differs) = on_small_stack(|| {
        let set = set_with(vec![build()], Vec::new(), vec![chain_definition()]);
        let input = expected_value(DEPTH);
        let outcome = vibra_interp::evaluate(
            &set,
            &var(0, chain()).expr,
            std::slice::from_ref(&input),
            LIMITS,
        );
        let round_trip =
            matches!(&outcome, Evaluation::Value(value) if *value == input);
        let shorter_differs = input != expected_value(DEPTH - 1);
        (round_trip, shorter_differs)
    });
    assert!(round_trip);
    assert!(shorter_differs);
}

/// `assert.equal` compares two deep values by their encodings: equal
/// chains pass, and chains of different depth fail.
#[test]
fn assert_equal_compares_values_200_000_deep() {
    let (passed, failed) = on_small_stack(|| {
        let test = |expected: u64| {
            let body = node(Expr::Sequence(vec![
                generic_row(
                    CompilerIntrinsic::AssertEqual,
                    vec![chain()],
                    vec![built(expected), built(DEPTH)],
                ),
                void(),
            ]));
            let entry = function("test", signature(Vec::new(), Type::Void), body, 0);
            run_test(
                &program(set_with_entry(entry), BASE + 1),
                MemoryBudget::DEFAULT,
            )
            .unwrap()
            .failure
        };
        (test(DEPTH).is_none(), test(DEPTH - 1).is_some())
    });
    assert!(passed);
    assert!(failed);
}
