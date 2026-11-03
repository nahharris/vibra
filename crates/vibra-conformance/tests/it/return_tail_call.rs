//! The operand of `return` is a tail call in the reference interpreter
//! (`docs/roadmap/pre-m4/01-bindings-return-never.md`, section F07).
//!
//! The results of the rest of section F are corpus cases that run in both
//! backends: `V1-RUNTIME-let-binds-in-order`, `V1-RUNTIME-let-else`,
//! `V1-RUNTIME-return`, and `V1-RUNTIME-never-unreached`. The transfer count
//! and the activation depth are the interpreter's own measures, which a case
//! cannot state.

use vibra_interp::Interpreter;
use vibra_types::check_source;

fn run(source: &str) -> vibra_interp::Execution {
    let checked = check_source("pre-m4.vib", source);
    assert!(checked.accepted(), "{source}: {:?}", checked.diagnostics());
    Interpreter::run(checked.program().expect("program")).expect("execution")
}

fn walk(count: usize) -> vibra_interp::Execution {
    let items = vec!["1i32"; count].join(" ");
    run(&format!(
        "(defn answer () i32 (walk (array.of {items})))\n(defn walk (items (array i32)) i32\n  (let-else (option.some rest) (array.slice items 1u64 (array.length items)) (return 7i32))\n  (let-else (option.none) (items 0u64) (return (walk rest)))\n  7i32)"
    ))
}

#[test]
fn the_operand_of_return_is_a_tail_call() {
    // F07: the entry call and one transfer per element, and no growth in live
    // activations.
    let short = walk(10);
    let long = walk(200);
    assert_eq!(short.tail_transfer_count(), 11);
    assert_eq!(long.tail_transfer_count(), 201);
    assert_eq!(short.max_activation_depth(), long.max_activation_depth());
}
