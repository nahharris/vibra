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

use vibra_diagnostics::ByteSpan;
use vibra_ir::external::host::host_row;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type,
    TypeBody, TypeDefinition, TypeId, Value,
};
use wasmtime::{Caller, Engine, Linker, Module, Store, TypedFunc};

fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

/// A program whose entry applies `io.stdout-text` to `"hi"` and answers `void`.
fn module() -> Vec<u8> {
    let result_id =
        TypeId::new("@vibra-stdlib@0.3.0/std.result.result", "std.result.result");
    let error_id = TypeId::new("@vibra-stdlib@0.3.0/std.io.io-error", "std.io.io-error");
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

/// The builders and accessors of a module, as a host calls them.
struct Host<'c, 'd> {
    caller: &'c mut Caller<'d, ()>,
}

impl Host<'_, '_> {
    fn func<P, R>(&mut self, name: &str) -> wasmtime::Result<TypedFunc<P, R>>
    where
        P: wasmtime::WasmParams,
        R: wasmtime::WasmResults,
    {
        let export = self
            .caller
            .get_export(name)
            .and_then(wasmtime::Extern::into_func)
            .ok_or_else(|| wasmtime::Error::msg(format!("no export {name}")))?;
        export.typed(&*self.caller)
    }

    fn new_value(&mut self, kind: i32, length: i64, variant: i32) -> wasmtime::Result<i64> {
        let f = self.func::<(i32, i64, i32), i64>("vibra_v1_new")?;
        f.call(&mut *self.caller, (kind, length, variant))
    }

    fn write_i32(&mut self, id: i64, index: i64, value: i32) -> wasmtime::Result<()> {
        let f = self.func::<(i64, i64, i32), ()>("vibra_v1_write_i32")?;
        f.call(&mut *self.caller, (id, index, value))
    }

    fn write_i64(&mut self, id: i64, index: i64, value: i64) -> wasmtime::Result<()> {
        let f = self.func::<(i64, i64, i64), ()>("vibra_v1_write_i64")?;
        f.call(&mut *self.caller, (id, index, value))
    }

    fn write_f32(&mut self, id: i64, index: i64, value: f32) -> wasmtime::Result<()> {
        let f = self.func::<(i64, i64, f32), ()>("vibra_v1_write_f32")?;
        f.call(&mut *self.caller, (id, index, value))
    }

    fn write_f64(&mut self, id: i64, index: i64, value: f64) -> wasmtime::Result<()> {
        let f = self.func::<(i64, i64, f64), ()>("vibra_v1_write_f64")?;
        f.call(&mut *self.caller, (id, index, value))
    }

    fn write_id(&mut self, id: i64, index: i64, child: i64) -> wasmtime::Result<()> {
        let f = self.func::<(i64, i64, i64), ()>("vibra_v1_write_id")?;
        f.call(&mut *self.caller, (id, index, child))
    }

    fn seal(&mut self, id: i64) -> wasmtime::Result<()> {
        let f = self.func::<i64, ()>("vibra_v1_seal")?;
        f.call(&mut *self.caller, id)
    }

    fn release(&mut self, id: i64) -> wasmtime::Result<()> {
        let f = self.func::<i64, ()>("vibra_v1_release")?;
        f.call(&mut *self.caller, id)
    }

    fn length(&mut self, id: i64) -> wasmtime::Result<i64> {
        let f = self.func::<i64, i64>("vibra_v1_length")?;
        f.call(&mut *self.caller, id)
    }

    /// A sealed `ok(void)`: the result a faithful host answers.
    fn ok_void(&mut self) -> wasmtime::Result<i64> {
        let id = self.new_value(7, 0, 0)?;
        self.seal(id)?;
        Ok(id)
    }
}

const KIND_STR: i32 = 1;
const KIND_BYTES: i32 = 2;
const KIND_ARRAY: i32 = 4;
const KIND_ENUM: i32 = 7;

#[derive(Debug, PartialEq, Eq)]
enum Ended {
    /// The entry returned; the live arena size before and after.
    Completed { start: i64, end: i64 },
    /// The module recorded this trap code.
    Trapped(i32),
    /// The module recorded another status.
    Other(i32),
}

/// Runs the module with `import` as the host's `io.stdout-text`, which receives
/// the ID of the borrowed argument.
fn run(
    import: impl Fn(&mut Host<'_, '_>, i64) -> wasmtime::Result<i64> + Send + Sync + 'static,
) -> Ended {
    let engine = Engine::default();
    let module = Module::new(&engine, module()).expect("a module");
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "vibra_v1",
            "io.stdout-text",
            move |mut caller: Caller<'_, ()>, argument: i64| -> wasmtime::Result<i64> {
                import(&mut Host { caller: &mut caller }, argument)
            },
        )
        .expect("an import");
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &module).expect("instantiates");
    let live = instance
        .get_typed_func::<(), i64>(&mut store, "vibra_v1_live_size")
        .expect("live size");
    let start = live.call(&mut store, ()).expect("live size");
    let entry = instance
        .get_typed_func::<(), ()>(&mut store, "vibra_v1_entry")
        .expect("entry");
    let call = entry.call(&mut store, ());
    let status = instance
        .get_typed_func::<(), i32>(&mut store, "vibra_v1_status")
        .expect("status")
        .call(&mut store, ())
        .expect("status");
    match (call, status) {
        (Ok(()), 0) => Ended::Completed {
            start,
            end: live.call(&mut store, ()).expect("live size"),
        },
        (Err(_), 1) => Ended::Trapped(
            instance
                .get_typed_func::<(), i32>(&mut store, "vibra_v1_trap_code")
                .expect("trap code")
                .call(&mut store, ())
                .expect("trap code"),
        ),
        (_, other) => Ended::Other(other),
    }
}

const INVALID_HOST_VALUE: i32 = 3;

fn trapped(ended: &Ended) {
    assert_eq!(
        *ended,
        Ended::Trapped(INVALID_HOST_VALUE),
        "a builder misuse is @runtime.invalid-host-value"
    );
}

/// A misuse made by `misuse` inside the import: the builder call fails, and the
/// failure is the call's, so the import returns it.
fn misused(
    misuse: impl Fn(&mut Host<'_, '_>) -> wasmtime::Result<()> + Send + Sync + 'static,
) -> Ended {
    run(move |host, _argument| {
        misuse(host)?;
        host.ok_void()
    })
}

// -- a faithful host --------------------------------------------------------------

#[test]
fn a_faithful_host_completes_and_the_arena_returns_to_where_it_started() {
    let ended = run(|host, _| host.ok_void());
    let Ended::Completed { start, end } = ended else {
        panic!("{ended:?}");
    };
    assert_eq!(end, start, "the borrowed argument and the adopted result balance");
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
        host.ok_void()
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
        host.ok_void()
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
    for (kind, length, variant) in
        [(5, 0, 0), (10, 0, 0), (11, 0, 0), (-1, 0, 0), (KIND_ENUM, 2, 0), (8, 0, 0), (9, 2, 0), (KIND_STR, 0, 1)]
    {
        trapped(&misused(move |host| host.new_value(kind, length, variant).map(drop)));
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
        let id = host.ok_void()?;
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
        host.ok_void()
    }));
}

#[test]
fn a_parameter_is_readable_through_the_accessors_during_the_call() {
    let ended = run(|host, argument| {
        assert_eq!(host.length(argument)?, 2, "\"hi\" has two scalars");
        host.ok_void()
    });
    assert!(matches!(ended, Ended::Completed { .. }), "{ended:?}");
}

