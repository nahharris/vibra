//! The shape of the generated Cranelift IR, read from `Function::display()`
//! before object emission. Nothing runs: these are properties of the text.

use std::collections::BTreeMap;

use vibra_codegen::emit_with_listing;
use vibra_exec::{Callee, Function, Inst, Terminator, verified};

use crate::programs;
use crate::{TARGETS, inputs};

/// The routine an instruction is, in the order the stepper's `Context` call
/// for it is made. Written out here, not derived from the generator.
pub fn inst_routine(inst: &Inst) -> &'static str {
    match inst {
        Inst::Scalar { .. } => "nexus_scalar",
        Inst::Constant { .. } => "nexus_constant",
        Inst::Copy { .. } => "nexus_copy",
        Inst::Move { .. } => "nexus_move",
        Inst::Drop { .. } => "nexus_drop",
        Inst::Op { .. } => "nexus_op",
        Inst::Intrinsic { .. } => "nexus_intrinsic",
        Inst::Build { .. } => "nexus_build",
        Inst::Cell { .. } => "nexus_cell",
        Inst::Variant { .. } => "nexus_variant",
        Inst::Function { .. } => "nexus_function",
    }
}

/// The routine a terminator calls; none for a `Jump`.
pub fn terminator_routine(term: &Terminator) -> Option<&'static str> {
    Some(match term {
        Terminator::Jump(_) => return None,
        Terminator::Branch { .. } => "nexus_branch",
        Terminator::Switch { .. } => "nexus_switch",
        Terminator::Call {
            callee: Callee::Direct(_),
            ..
        } => "nexus_call",
        Terminator::Call {
            callee: Callee::Indirect(_),
            ..
        } => "nexus_call_value",
        Terminator::TailCall {
            callee: Callee::Direct(_),
            ..
        } => "nexus_tail_call",
        Terminator::TailCall {
            callee: Callee::Indirect(_),
            ..
        } => "nexus_tail_call_value",
        Terminator::Host { .. } => "nexus_host",
        Terminator::Global { .. } => "nexus_global",
        Terminator::Return { .. } => "nexus_ret",
        Terminator::Trap { .. } => "nexus_trap",
    })
}

/// The routines a step function calls, in the order of its layout: the trap
/// of an unknown block first, then each block's instructions and terminator
/// in block order.
fn expected_calls(function: &Function) -> Vec<&'static str> {
    let mut calls = vec!["nexus_trap"];
    for block in &function.blocks {
        calls.extend(block.insts.iter().map(inst_routine));
        calls.extend(terminator_routine(&block.term));
    }
    calls
}

/// The `fnN = ... u0:ID ...` declarations of the header of a function.
fn declarations(
    text: &str,
    routines: &BTreeMap<u32, &'static str>,
) -> BTreeMap<String, &'static str> {
    let mut named = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        let Some((reference, rest)) = line.split_once(" = ") else {
            continue;
        };
        if !reference.starts_with("fn") {
            continue;
        }
        let id = rest
            .split_whitespace()
            .find_map(|word| word.strip_prefix("u0:"))
            .and_then(|id| id.parse::<u32>().ok())
            .unwrap_or_else(|| panic!("no external name in `{line}`"));
        let routine = routines
            .get(&id)
            .unwrap_or_else(|| panic!("`{line}` calls no routine of the listing"));
        named.insert(reference.to_owned(), *routine);
    }
    named
}

/// The routine each `call fnN(...)` of the text calls, in textual order.
fn calls_in_order(
    text: &str,
    named: &BTreeMap<String, &'static str>,
) -> Vec<&'static str> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| {
            let (_, call) = line
                .split_once(" = call ")
                .or_else(|| line.strip_prefix("call ").map(|rest| ("", rest)))?;
            let callee = call.split('(').next()?;
            Some(
                *named
                    .get(callee)
                    .unwrap_or_else(|| panic!("`{line}` is no routine call")),
            )
        })
        .collect()
}

/// The text of each CLIF block of a function.
fn clif_blocks(text: &str) -> Vec<Vec<&str>> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with("block") && line.ends_with(':') {
            blocks.push(Vec::new());
        } else if let Some(open) = blocks.last_mut()
            && !line.is_empty()
            && line != "}"
        {
            open.push(line);
        }
    }
    blocks
}

fn check_function(
    label: &str,
    function: &Function,
    text: &str,
    routines: &BTreeMap<u32, &'static str>,
) {
    let named = declarations(text, routines);
    assert_eq!(
        calls_in_order(text, &named),
        expected_calls(function),
        "{label}: the routines are called in the order the stepper executes them"
    );
    assert_eq!(
        text.matches("br_table").count(),
        1,
        "{label}: exactly one switch on `resume`"
    );
    for forbidden in [
        "call_indirect",
        "load",
        "store",
        "stack_addr",
        "return_call",
    ] {
        assert!(
            !text.lines().any(|line| line
                .split_whitespace()
                .any(|word| word.starts_with(forbidden))),
            "{label}: `{forbidden}` in the generated function:\n{text}"
        );
    }
    // Every routine call is followed by a status check: the CLIF block that
    // holds a call holds exactly one, and ends in a branch on, or a return of,
    // what it answered.
    for body in clif_blocks(text) {
        let calls = body.iter().filter(|line| line.contains("call fn")).count();
        assert!(calls <= 1, "{label}: two calls in one block: {body:?}");
        if calls == 1 {
            let last = body.last().expect("a terminator");
            assert!(
                last.starts_with("brif ") || last.starts_with("return "),
                "{label}: a call is not followed by a status check: {body:?}"
            );
            let call = body
                .iter()
                .find(|line| line.contains("call fn"))
                .expect("call");
            let result = call.split(" = ").next().expect("a result").trim();
            let used = body
                .iter()
                .skip_while(|line| !line.contains("call fn"))
                .skip(1)
                .any(|line| {
                    line.split(|c: char| !c.is_alphanumeric())
                        .any(|word| word == result)
                })
                || last
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|word| word == result);
            assert!(
                used,
                "{label}: the status `{result}` is never inspected: {body:?}"
            );
        }
    }
}

#[test]
fn the_generated_ir_matches_the_fixture_shape() {
    let programs = [
        ("demo", programs::demo(1000, false)),
        ("demo-trap", programs::demo(2, true)),
        ("deep", programs::deep(10)),
        ("module-value", programs::module_value()),
        ("everything", programs::everything()),
    ];
    for (name, program) in &programs {
        let proof = verified(program).expect("verified");
        for target in TARGETS {
            let (_, listing) =
                emit_with_listing(proof, &target, &inputs(program)).expect("listing");
            assert_eq!(listing.functions.len(), program.functions.len());
            for (index, (function, text)) in
                program.functions.iter().zip(&listing.functions).enumerate()
            {
                let label = format!(
                    "{name} function {index} ({}) for {}",
                    function.name,
                    target.triple()
                );
                check_function(&label, function, text, &listing.routines);
            }
        }
    }
}

#[test]
fn the_demo_main_is_the_fixture_main() {
    // The hand-written fixture of the bridge (`tests/it/fixture.rs`,
    // `demo_main`) runs these routines in this order for `main` of the demo:
    // block 0 builds the function value, writes the rounds, calls through the
    // value; block 1 drops it, builds the constant, performs the host
    // operation; the last block returns. The generated function does the same.
    let program = programs::demo(1000, false);
    let proof = verified(&program).expect("verified");
    let (_, listing) =
        emit_with_listing(proof, &TARGETS[0], &inputs(&program)).expect("listing");
    let main = &listing.functions[0];
    let named = declarations(main, &listing.routines);
    assert_eq!(
        calls_in_order(main, &named),
        [
            "nexus_trap",
            "nexus_function",
            "nexus_scalar",
            "nexus_call_value",
            "nexus_drop",
            "nexus_constant",
            "nexus_host",
            "nexus_drop",
            "nexus_drop",
            "nexus_ret",
        ]
    );
}
