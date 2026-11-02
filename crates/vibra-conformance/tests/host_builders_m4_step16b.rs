//! The builders and the ownership rules of the host boundary (milestone 4
//! Step 16b-1).
//!
//! The runner's own host never misuses a builder, so a misuse is forged: the
//! emitted module of a program that applies `io.stdout-text` is linked to an
//! import written here, which calls the exported builders and accessors the way
//! a faulty host would. Every condition of the specification's trap
//! `@runtime.invalid-host-value` is exercised, and so is the discipline around
//! it: parameters are borrowed, the result is adopted exactly once, and a
//! balanced run returns the arena to where it started.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::sync::Arc;
use vibra_diagnostics::ByteSpan;
use vibra_ir::external::host::host_row;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type,
    TypeBody, TypeDefinition, TypeId, Value,
};

use vibra_ir::boundary::TrapCode;
use vibra_wasm_run::{Builders, Forged, MemoryLimit, Outcome, Runner, Started, Stop};

fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

/// A program whose entry applies `io.stdout-text` to `"hi"` and answers `void`.
fn module() -> Vec<u8> {
    let result_id =
        TypeId::new("@vibra-stdlib@0.3.0/std.result.result", "std.result.result");
    let error_id =
        TypeId::new("@vibra-stdlib@0.3.0/std.io.io-error", "std.io.io-error");
    let result = Type::Applied(
        result_id.clone(),
        vec![Type::Void, Type::Declared(error_id.clone())],
    );
    let host = CheckedFunction::new_host(
        "io.stdout-text",
        FunctionSignature::new(vec![Type::Str], result.clone()),
        host_row("io.stdout-text").expect("a row"),
        origin(),
    )
    .expect("a host function");
    let entry = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), Type::Void),
        Expr::sequence(
            vec![
                Expr::call(
                    0,
                    vec![Expr::literal(Value::Str("hi".to_owned()), origin())],
                    result,
                    origin(),
                ),
                Expr::literal(Value::Void, origin()),
            ],
            origin(),
        ),
        origin(),
    )
    .expect("an entry");
    let types = vec![
        TypeDefinition::new(
            result_id,
            TypeBody::Enum(vec![
                ("ok".to_owned(), Type::Param("t".to_owned())),
                ("err".to_owned(), Type::Param("e".to_owned())),
            ]),
        )
        .with_parameters(vec!["t".to_owned(), "e".to_owned()]),
        TypeDefinition::new(
            error_id,
            TypeBody::Enum(vec![
                ("invalid-text".to_owned(), Type::Void),
                ("closed".to_owned(), Type::Void),
                ("other".to_owned(), Type::Void),
            ]),
        ),
    ];
    let program =
        CheckedProgram::try_new_with_types(types, Vec::new(), vec![host, entry], 1)
            .expect("a program");
    vibra_wasm::emit(&program).expect("it lowers").into_bytes()
}

// -- the forged host -----------------------------------------------------------

/// A sealed `ok(void)`: the result a faithful host answers.
fn ok_void(host: &mut Builders<'_, '_>) -> Result<i64, Stop> {
    let id = host.new_value(KIND_ENUM, 0, 0)?;
    host.seal(id)?;
    Ok(id)
}

const KIND_STR: i32 = 1;
const KIND_BYTES: i32 = 2;
const KIND_ARRAY: i32 = 4;
const KIND_ENUM: i32 = 7;

#[derive(Debug, PartialEq, Eq)]
enum Ended {
    /// The entry returned; the live arena size before and after.
    Completed { start: u64, end: u64 },
    /// The module recorded this trap.
    Trapped(TrapCode),
    /// The run ended some other way.
    Other(String),
}

/// Runs the module with `import` as the host's `io.stdout-text`, which receives
/// the ID of the borrowed argument.
fn run(
    import: impl Fn(&mut Builders<'_, '_>, i64) -> Result<i64, Stop> + Send + Sync + 'static,
) -> Ended {
    let runner = Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("an engine");
    let forged: Forged = Arc::new(move |host, ids| import(host, ids[0]));
    let Started::Ready(mut instance) = runner
        .start_forged(&module(), "io.stdout-text", forged)
        .expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    let start = instance.live_size().expect("live size");
    match instance.call_entry().expect("the protocol holds") {
        Outcome::Completed { .. } => Ended::Completed {
            start,
            end: instance.live_size().expect("live size"),
        },
        Outcome::Trapped { code, .. } => Ended::Trapped(code),
        other => Ended::Other(format!("{other:?}")),
    }
}

fn trapped(ended: &Ended) {
    assert_eq!(
        *ended,
        Ended::Trapped(TrapCode::InvalidHostValue),
        "a builder misuse is @runtime.invalid-host-value"
    );
}

/// A misuse made by `misuse` inside the import: the builder call fails, and the
/// failure is the call's, so the import returns it.
fn misused(
    misuse: impl Fn(&mut Builders<'_, '_>) -> Result<(), Stop> + Send + Sync + 'static,
) -> Ended {
    run(move |host, _argument| {
        misuse(host)?;
        ok_void(host)
    })
}

// -- a faithful host --------------------------------------------------------------

#[test]
fn a_faithful_host_completes_and_the_arena_returns_to_where_it_started() {
    let ended = run(|host, _| ok_void(host));
    let Ended::Completed { start, end } = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(
        end, start,
        "the borrowed argument and the adopted result balance"
    );
}

#[test]
fn a_result_with_an_error_payload_is_adopted_with_its_children() {
    let ended = run(|host, _| {
        let error = host.new_value(KIND_ENUM, 0, 1)?;
        host.seal(error)?;
        let result = host.new_value(KIND_ENUM, 1, 1)?;
        host.write_id(result, 0, error)?;
        host.release(error)?;
        host.seal(result)?;
        Ok(result)
    });
    let Ended::Completed { start, end } = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(end, start);
}

#[test]
fn a_slot_written_twice_keeps_its_last_write_and_releases_what_it_replaced() {
    let ended = run(|host, _| {
        let array = host.new_value(KIND_ARRAY, 1, 0)?;
        let first = host.new_value(KIND_STR, 0, 0)?;
        host.seal(first)?;
        let second = host.new_value(KIND_STR, 1, 0)?;
        host.write_i32(second, 0, 120)?;
        host.seal(second)?;
        host.write_id(array, 0, first)?;
        host.write_id(array, 0, second)?;
        host.release(first)?;
        host.release(second)?;
        host.seal(array)?;
        // The last write is the one the array holds.
        assert_eq!(host.length(array)?, 1);
        host.release(array)?;
        ok_void(host)
    });
    let Ended::Completed { start, end } = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(end, start);
}

#[test]
fn a_value_nested_five_thousand_deep_is_built_adopted_and_released() {
    let ended = run(|host, _| {
        let mut inner = host.new_value(KIND_ENUM, 0, 0)?;
        host.seal(inner)?;
        for _ in 0..5_000 {
            let outer = host.new_value(KIND_ARRAY, 1, 0)?;
            host.write_id(outer, 0, inner)?;
            host.release(inner)?;
            host.seal(outer)?;
            inner = outer;
        }
        host.release(inner)?;
        ok_void(host)
    });
    let Ended::Completed { start, end } = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(end, start, "the worklist release frees the whole chain");
}

// -- the traps ----------------------------------------------------------------------

#[test]
fn a_write_to_a_sealed_value_traps() {
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 1, 0)?;
        host.write_i32(id, 0, 97)?;
        host.seal(id)?;
        host.write_i32(id, 0, 98)
    }));
}

#[test]
fn sealing_twice_traps() {
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 0, 0)?;
        host.seal(id)?;
        host.seal(id)
    }));
}

#[test]
fn an_index_outside_the_value_traps() {
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 1, 0)?;
        host.write_i32(id, 1, 97)
    }));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_ARRAY, 0, 0)?;
        host.write_i64(id, 0, 1)
    }));
}

#[test]
fn a_class_the_kind_does_not_hold_traps() {
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 1, 0)?;
        host.write_i64(id, 0, 97)
    }));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 1, 0)?;
        host.write_f32(id, 0, 1.0)
    }));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_BYTES, 1, 0)?;
        host.write_f64(id, 0, 1.0)
    }));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 1, 0)?;
        let child = host.new_value(KIND_STR, 0, 0)?;
        host.seal(child)?;
        host.write_id(id, 0, child)
    }));
}

#[test]
fn a_byte_outside_zero_to_255_traps() {
    trapped(&misused(|host| {
        let id = host.new_value(KIND_BYTES, 1, 0)?;
        host.write_i32(id, 0, 256)
    }));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_BYTES, 1, 0)?;
        host.write_i32(id, 0, -1)
    }));
}

#[test]
fn a_char_that_is_not_a_unicode_scalar_traps() {
    for scalar in [0xD800, 0xDFFF, 0x11_0000, -1] {
        trapped(&misused(move |host| {
            let id = host.new_value(KIND_STR, 1, 0)?;
            host.write_i32(id, 0, scalar)
        }));
    }
}

#[test]
fn a_seal_with_a_slot_unwritten_traps() {
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 2, 0)?;
        host.write_i32(id, 0, 97)?;
        host.seal(id)
    }));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_BYTES, 9, 0)?;
        for index in 0..8 {
            host.write_i32(id, index, 1)?;
        }
        host.seal(id)
    }));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_ENUM, 1, 1)?;
        host.seal(id)
    }));
}

#[test]
fn a_child_that_is_not_sealed_traps() {
    trapped(&misused(|host| {
        let parent = host.new_value(KIND_ARRAY, 1, 0)?;
        let child = host.new_value(KIND_STR, 0, 0)?;
        host.write_id(parent, 0, child)
    }));
}

#[test]
fn a_kind_the_builders_do_not_build_traps() {
    for (kind, length, variant) in [
        (5, 0, 0),
        (10, 0, 0),
        (11, 0, 0),
        (-1, 0, 0),
        (KIND_ENUM, 2, 0),
        (8, 0, 0),
        (9, 2, 0),
        (KIND_STR, 0, 1),
    ] {
        trapped(&misused(move |host| {
            host.new_value(kind, length, variant).map(drop)
        }));
    }
}

#[test]
fn an_id_the_instance_did_not_issue_traps() {
    trapped(&misused(|host| host.seal(0)));
    trapped(&misused(|host| host.write_i32(0x1_0000_0001, 0, 1)));
    trapped(&misused(|host| {
        let id = host.new_value(KIND_STR, 0, 0)?;
        host.seal(id)?;
        host.release(id)?;
        host.seal(id)
    }));
}

// -- the result ---------------------------------------------------------------------

#[test]
fn an_unsealed_result_traps() {
    trapped(&run(|host, _| host.new_value(KIND_ENUM, 0, 0)));
}

#[test]
fn a_result_that_is_not_an_enum_traps() {
    trapped(&run(|host, _| {
        let id = host.new_value(KIND_STR, 0, 0)?;
        host.seal(id)?;
        Ok(id)
    }));
}

#[test]
fn a_result_id_that_was_never_issued_or_was_released_traps() {
    trapped(&run(|_, _| Ok(0)));
    trapped(&run(|_, _| Ok(0x7_0000_0001)));
    trapped(&run(|host, _| {
        let id = ok_void(host)?;
        host.release(id)?;
        Ok(id)
    }));
}

// -- ownership ----------------------------------------------------------------------

#[test]
fn a_parameter_is_borrowed_and_a_host_that_releases_it_breaks_the_module() {
    // The module releases the ID it issued for the argument after the call, so
    // a host that released it first leaves the module with an invalid ID.
    trapped(&run(|host, argument| {
        host.release(argument)?;
        ok_void(host)
    }));
}

#[test]
fn a_parameter_is_readable_through_the_accessors_during_the_call() {
    let ended = run(|host, argument| {
        assert_eq!(host.length(argument)?, 2, "\"hi\" has two scalars");
        ok_void(host)
    });
    assert!(matches!(ended, Ended::Completed { .. }), "{ended:?}");
}
