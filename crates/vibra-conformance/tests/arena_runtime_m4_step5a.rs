//! The memory layer of a v1 module (milestone 4 Step 5a): allocation, reference
//! counts, the worklist release, the handle table, the accessors, and
//! exhaustion.
//!
//! The forms that build compound values are lowered in later steps, so these
//! tests build values from a hand-written entry body, in a module that is
//! otherwise exactly an emitted one (`vibra_wasm::support`). The values are
//! observed only through the exported accessors, as a host observes them.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use vibra_ir::boundary::TrapCode;
use vibra_wasm::layout::{
    self, CellClass, Kind, STRIDE_BYTES, STRIDE_CELLS, header, state,
};
use vibra_wasm::support::{
    BlockType, Instruction as I, MemArg, Routines, ValType, ValueClass,
    module_with_entry,
};
use vibra_wasm_run::{Instance, MemoryLimit, Outcome, Runner, Started, ValueId};

const MIB: usize = 1024 * 1024;
const PAGE: usize = 65_536;

// -- instruction helpers ----------------------------------------------------

type Ins = I<'static>;

const fn c32(value: i32) -> Ins {
    I::I32Const(value)
}

const fn get(local: u32) -> Ins {
    I::LocalGet(local)
}

const fn set(local: u32) -> Ins {
    I::LocalSet(local)
}

const fn mem(offset: u32, align: u32) -> MemArg {
    MemArg {
        offset: offset as u64,
        align,
        memory_index: 0,
    }
}

const fn index(value: u32) -> i32 {
    value.cast_signed()
}

/// Pushes a new object: `kind`, `stride`, `len`, and `variant`.
fn new_object(
    f: &Routines,
    kind: Kind,
    stride: u32,
    len: u32,
    variant: u32,
) -> Vec<Ins> {
    vec![
        c32(index(kind.code())),
        c32(index(stride)),
        c32(index(len)),
        c32(index(variant)),
        I::Call(f.new),
    ]
}

/// Writes the class byte of component `at` of the cells object in `object`.
fn class(object: u32, at: u32, class: CellClass) -> Vec<Ins> {
    vec![
        get(object),
        c32(i32::from(class.code())),
        I::I32Store8(mem(header::SIZE + at, 0)),
    ]
}

/// Writes an `i32` scalar into component `at` of a `len`-cell object.
fn cell_i32(object: u32, len: u32, at: u32, value: i32) -> Vec<Ins> {
    let mut code = class(object, at, CellClass::I32);
    code.extend([
        get(object),
        c32(value),
        I::I32Store(mem(layout::cells_offset(len) + 8 * at, 2)),
    ]);
    code
}

fn cell_i64(object: u32, len: u32, at: u32, value: i64) -> Vec<Ins> {
    let mut code = class(object, at, CellClass::I64);
    code.extend([
        get(object),
        I::I64Const(value),
        I::I64Store(mem(layout::cells_offset(len) + 8 * at, 3)),
    ]);
    code
}

fn cell_f32(object: u32, len: u32, at: u32, value: f32) -> Vec<Ins> {
    let mut code = class(object, at, CellClass::F32);
    code.extend([
        get(object),
        I::F32Const(value.into()),
        I::F32Store(mem(layout::cells_offset(len) + 8 * at, 2)),
    ]);
    code
}

fn cell_f64(object: u32, len: u32, at: u32, value: f64) -> Vec<Ins> {
    let mut code = class(object, at, CellClass::F64);
    code.extend([
        get(object),
        I::F64Const(value.into()),
        I::F64Store(mem(layout::cells_offset(len) + 8 * at, 3)),
    ]);
    code
}

/// Moves the reference in `child` into component `at` of `object`: the object
/// owns the count the local held, and the local is not used again.
fn cell_ref(object: u32, len: u32, at: u32, child: u32) -> Vec<Ins> {
    let mut code = class(object, at, CellClass::Ref);
    code.extend([
        get(object),
        get(child),
        I::I64ExtendI32U,
        I::I64Store(mem(layout::cells_offset(len) + 8 * at, 3)),
    ]);
    code
}

/// Writes the characters of `text` into a `str` object of `text.len()` scalars.
fn characters(object: u32, text: &str) -> Vec<Ins> {
    let mut code = Vec::new();
    for (at, character) in text.chars().enumerate() {
        code.extend([
            get(object),
            c32(index(u32::from(character))),
            I::I32Store(mem(header::SIZE + 4 * u32::try_from(at).expect("short"), 2)),
        ]);
    }
    code
}

/// `for i in 0..count { body }` over the local `counter`.
fn repeat(counter: u32, count: u32, body: Vec<Ins>) -> Vec<Ins> {
    let mut code = vec![
        c32(0),
        set(counter),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        get(counter),
        c32(index(count)),
        I::I32GeU,
        I::BrIf(1),
    ];
    code.extend(body);
    code.extend([
        get(counter),
        c32(1),
        I::I32Add,
        set(counter),
        I::Br(0),
        I::End,
        I::End,
    ]);
    code
}

// -- host helpers -----------------------------------------------------------

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * MIB)).expect("the engine configures")
}

fn started(runner: &Runner, bytes: &[u8]) -> Instance {
    match runner.start(bytes).expect("a v1 module") {
        Started::Ready(instance) => instance,
        Started::MemoryExhausted => panic!("the module's own memory exceeds the limit"),
    }
}

/// Runs the entry and returns the instance, the result slot's ID (when the entry's
/// class is a reference), and the live size the call reported.
fn run(bytes: &[u8]) -> (Instance, vibra_wasm_run::ResultSlot, u64) {
    let mut instance = started(&runner(), bytes);
    let Outcome::Completed { result, live_size } =
        instance.call_entry().expect("the entry runs")
    else {
        panic!("the entry did not complete");
    };
    (instance, result, live_size)
}

fn trapped_invalid(outcome: Outcome) {
    assert_eq!(
        outcome,
        Outcome::Trapped {
            code: TrapCode::InvalidHostValue,
            origin: None,
        }
    );
}

// -- a value of every scalar width and a reference -------------------------

/// A tuple of five components: an `i32`, an `i64`, an `f32`, an `f64`, and a
/// reference to the `str` "h\u{e9}!".
fn kitchen_sink() -> Vec<u8> {
    module_with_entry(ValueClass::Ref, &[ValType::I32, ValType::I32], |f| {
        let (tuple, text) = (0, 1);
        let mut code = new_object(f, Kind::Tuple, STRIDE_CELLS, 5, 0);
        code.push(set(tuple));
        code.extend(cell_i32(tuple, 5, 0, -7));
        code.extend(cell_i64(tuple, 5, 1, i64::MIN + 3));
        code.extend(cell_f32(tuple, 5, 2, 1.5));
        code.extend(cell_f64(tuple, 5, 3, -2.25));
        code.extend(new_object(f, Kind::Str, layout::STRIDE_CHARS, 3, 0));
        code.push(set(text));
        code.extend(characters(text, "h\u{e9}!"));
        code.extend(cell_ref(tuple, 5, 4, text));
        code.push(get(tuple));
        code
    })
}

#[test]
fn every_scalar_width_and_a_reference_read_back_through_the_accessors() {
    let (mut instance, slot, _) = run(&kitchen_sink());
    let tuple = instance.result_id(slot);
    assert_eq!(instance.length(&tuple), Ok(5));
    assert_eq!(instance.read_i32(&tuple, 0), Ok(-7));
    assert_eq!(instance.read_i64(&tuple, 1), Ok(i64::MIN + 3));
    assert_eq!(instance.read_f32(&tuple, 2), Ok(1.5));
    assert_eq!(instance.read_f64(&tuple, 3), Ok(-2.25));
    let text = instance.read_id(&tuple, 4).expect("a compound component");
    assert_eq!(instance.length(&text), Ok(3));
    let scalars = (0..3)
        .map(|at| instance.read_i32(&text, at).expect("a character"))
        .collect::<Vec<_>>();
    assert_eq!(scalars, [0x68, 0xe9, 0x21]);
    instance.release(text).expect("released");
    instance.release(tuple).expect("released");
    assert_eq!(instance.live_size(), Ok(0), "everything was released");
}

#[test]
fn an_enum_a_union_and_a_wrapper_read_through_variant_and_their_payload() {
    let bytes =
        module_with_entry(ValueClass::Ref, &[ValType::I32, ValType::I32], |f| {
            let (outer, inner) = (0, 1);
            // `(union ...)` member 2 holding an enum whose variant 3 carries 42.
            let mut code = new_object(f, Kind::Enum, STRIDE_CELLS, 1, 3);
            code.push(set(inner));
            code.extend(cell_i32(inner, 1, 0, 42));
            code.extend(new_object(f, Kind::Union, STRIDE_CELLS, 1, 2));
            code.push(set(outer));
            code.extend(cell_ref(outer, 1, 0, inner));
            code.push(get(outer));
            code
        });
    let (mut instance, slot, _) = run(&bytes);
    let union = instance.result_id(slot);
    assert_eq!(instance.variant(&union), Ok(2), "the member index");
    let payload = instance.read_id(&union, 0).expect("the enum");
    assert_eq!(instance.variant(&payload), Ok(3), "the variant index");
    assert_eq!(instance.read_i32(&payload, 0), Ok(42));
    // Neither kind has a length: the specification lists the kinds that do.
    trapped_invalid(instance.length(&union).expect_err("no length"));
    trapped_invalid(instance.length(&payload).expect_err("no length"));
    instance.release(payload).expect("released");
    instance.release(union).expect("released");
    assert_eq!(instance.live_size(), Ok(0));
}

#[test]
fn a_dict_reads_its_entries_as_tuples() {
    let bytes =
        module_with_entry(ValueClass::Ref, &[ValType::I32, ValType::I32], |f| {
            let (dict, entry) = (0, 1);
            // A dict holds each entry as a two-component tuple.
            let mut code = new_object(f, Kind::Tuple, STRIDE_CELLS, 2, 0);
            code.push(set(entry));
            code.extend(cell_i32(entry, 2, 0, 1));
            code.extend(cell_i32(entry, 2, 1, 10));
            code.extend(new_object(f, Kind::Dict, STRIDE_CELLS, 1, 0));
            code.push(set(dict));
            code.extend(cell_ref(dict, 1, 0, entry));
            code.push(get(dict));
            code
        });
    let (mut instance, slot, _) = run(&bytes);
    let dict = instance.result_id(slot);
    assert_eq!(instance.length(&dict), Ok(1));
    let entry = instance.read_id(&dict, 0).expect("the entry tuple");
    assert_eq!(instance.length(&entry), Ok(2));
    assert_eq!(instance.read_i32(&entry, 0), Ok(1));
    assert_eq!(instance.read_i32(&entry, 1), Ok(10));
    instance.release(entry).expect("released");
    instance.release(dict).expect("released");
    assert_eq!(instance.live_size(), Ok(0));
}

#[test]
fn bytes_and_chars_read_as_i32_and_no_other_scalar() {
    let bytes = module_with_entry(ValueClass::Ref, &[ValType::I32], |f| {
        let mut code = new_object(f, Kind::Bytes, STRIDE_BYTES, 3, 0);
        code.push(set(0));
        for (at, byte) in [0_i32, 128, 255].into_iter().enumerate() {
            code.extend([
                get(0),
                c32(byte),
                I::I32Store8(mem(header::SIZE + u32::try_from(at).expect("short"), 0)),
            ]);
        }
        code.push(get(0));
        code
    });
    let (mut instance, slot, _) = run(&bytes);
    let value = instance.result_id(slot);
    assert_eq!(instance.length(&value), Ok(3));
    assert_eq!(instance.read_i32(&value, 0), Ok(0));
    assert_eq!(instance.read_i32(&value, 1), Ok(128));
    assert_eq!(instance.read_i32(&value, 2), Ok(255));
    trapped_invalid(instance.read_i64(&value, 0).expect_err("a byte is an i32"));
    trapped_invalid(instance.read_f64(&value, 0).expect_err("a byte is an i32"));
    trapped_invalid(
        instance
            .read_id(&value, 0)
            .expect_err("a byte is no reference"),
    );
    trapped_invalid(instance.variant(&value).expect_err("bytes have no variant"));
}

// -- negative accessors, and recovery ---------------------------------------

#[test]
fn an_id_index_or_kind_the_instance_does_not_admit_is_invalid_host_value() {
    let (mut instance, slot, _) = run(&kitchen_sink());
    let tuple = instance.result_id(slot);

    // Zero, and a number the instance never issued.
    trapped_invalid(instance.length(&instance.forge_id(0)).expect_err("zero"));
    trapped_invalid(
        instance
            .length(&instance.forge_id((1 << 32) | 99))
            .expect_err("never issued"),
    );
    trapped_invalid(
        instance
            .length(&instance.forge_id(u64::MAX))
            .expect_err("never issued"),
    );
    // An index outside the value.
    trapped_invalid(instance.read_i32(&tuple, 5).expect_err("past the end"));
    trapped_invalid(
        instance
            .read_i32(&tuple, u64::MAX)
            .expect_err("far past the end"),
    );
    // A kind or a component class the accessor does not admit.
    trapped_invalid(
        instance
            .variant(&tuple)
            .expect_err("a tuple has no variant"),
    );
    trapped_invalid(
        instance
            .read_i64(&tuple, 0)
            .expect_err("component 0 is an i32"),
    );
    trapped_invalid(
        instance
            .read_i32(&tuple, 1)
            .expect_err("component 1 is an i64"),
    );
    trapped_invalid(
        instance
            .read_f64(&tuple, 2)
            .expect_err("component 2 is an f32"),
    );
    trapped_invalid(
        instance
            .read_id(&tuple, 0)
            .expect_err("component 0 is no reference"),
    );
    trapped_invalid(
        instance
            .read_i32(&tuple, 4)
            .expect_err("component 4 is a reference"),
    );

    // The instance still answers, and a stop recorded the code, not a status 0.
    assert_eq!(instance.length(&tuple), Ok(5));
    assert_eq!(instance.read_i32(&tuple, 0), Ok(-7));
    assert!(instance.live_size().expect("answers") > 0);
}

#[test]
fn a_released_id_and_another_instances_id_are_invalid_host_values() {
    let bytes = kitchen_sink();
    let runner = runner();
    let (mut one, slot_one) = {
        let mut instance = started(&runner, &bytes);
        let Outcome::Completed { result, .. } = instance.call_entry().expect("runs")
        else {
            panic!("completes");
        };
        (instance, result)
    };
    let (mut two, slot_two) = {
        let mut instance = started(&runner, &bytes);
        let Outcome::Completed { result, .. } = instance.call_entry().expect("runs")
        else {
            panic!("completes");
        };
        (instance, result)
    };
    let id_one = one.result_id(slot_one);
    let id_two = two.result_id(slot_two);

    // The two instances issued the same number: an ID is valid for one
    // instance alone, and the host tells them apart.
    assert_eq!(id_one.number_for_tests(), id_two.number_for_tests());
    trapped_invalid(two.length(&id_one).expect_err("another instance's ID"));
    trapped_invalid(one.length(&id_two).expect_err("another instance's ID"));
    assert_eq!(one.length(&id_one), Ok(5));
    assert_eq!(two.length(&id_two), Ok(5));

    // A released ID is invalid, though its slot may be reused.
    let again = id_one.clone();
    one.release(id_one).expect("released");
    trapped_invalid(one.length(&again).expect_err("released"));
    trapped_invalid(one.release(again).expect_err("released twice"));
}

#[test]
fn a_fresh_instance_runs_normally_after_a_stop() {
    let bytes = kitchen_sink();
    let runner = runner();
    let mut first = started(&runner, &bytes);
    let Outcome::Completed { result, .. } = first.call_entry().expect("runs") else {
        panic!("completes");
    };
    let tuple = first.result_id(result);
    trapped_invalid(first.read_i32(&tuple, 9).expect_err("past the end"));
    let mut second = started(&runner, &bytes);
    let Outcome::Completed { result, .. } = second.call_entry().expect("runs") else {
        panic!("completes");
    };
    let tuple = second.result_id(result);
    assert_eq!(second.length(&tuple), Ok(5));
    assert_eq!(second.read_i32(&tuple, 0), Ok(-7));
}

// -- ids --------------------------------------------------------------------

/// A tuple with one reference component, so `read_id` issues an ID.
fn holder() -> Vec<u8> {
    module_with_entry(ValueClass::Ref, &[ValType::I32, ValType::I32], |f| {
        let (tuple, leaf) = (0, 1);
        let mut code = new_object(f, Kind::Tuple, STRIDE_CELLS, 1, 0);
        code.push(set(leaf));
        code.extend(new_object(f, Kind::Tuple, STRIDE_CELLS, 1, 0));
        code.push(set(tuple));
        code.extend(cell_ref(tuple, 1, 0, leaf));
        code.push(get(tuple));
        code
    })
}

#[test]
fn a_released_id_stays_invalid_while_other_ids_and_a_reused_slot_are_live() {
    let (mut instance, slot, _) = run(&holder());
    let holder = instance.result_id(slot);
    let first = instance.read_id(&holder, 0).expect("an ID");
    let second = instance.read_id(&holder, 0).expect("an ID");
    let stale = first.clone();
    instance.release(first).expect("released");
    // The table still holds the holder and the second ID.
    trapped_invalid(instance.length(&stale).expect_err("released"));
    assert_eq!(instance.length(&second), Ok(1));
    // The next ID takes the released slot but not its number.
    let third = instance.read_id(&holder, 0).expect("an ID");
    assert_ne!(third.number_for_tests(), stale.number_for_tests());
    assert_eq!(
        third.number_for_tests() & 0xFFFF_FFFF,
        stale.number_for_tests() & 0xFFFF_FFFF,
        "the slot is reused"
    );
    trapped_invalid(instance.length(&stale).expect_err("still released"));
    assert_eq!(instance.length(&third), Ok(1));
    // A forged ID with the reused slot and the stale counter is no ID.
    trapped_invalid(
        instance
            .length(&instance.forge_id(stale.number_for_tests()))
            .expect_err("stale"),
    );
}

#[test]
fn ids_are_nonzero_strictly_increasing_and_never_reused() {
    let (mut instance, slot, _) = run(&holder());
    let holder = instance.result_id(slot);
    let mut issued = vec![holder.number_for_tests()];
    let mut live: Vec<ValueId> = Vec::new();
    for round in 0..40 {
        let id = instance.read_id(&holder, 0).expect("an ID");
        issued.push(id.number_for_tests());
        live.push(id);
        // Release every third ID, so slots are reused and numbers must not be.
        if round % 3 == 2 {
            let released = live.remove(0);
            instance.release(released).expect("released");
        }
    }
    assert!(
        issued.iter().all(|number| *number != 0),
        "zero is never valid"
    );
    assert!(
        issued.windows(2).all(|pair| pair[0] < pair[1]),
        "IDs strictly increase: {issued:?}"
    );
    let unique = issued.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), issued.len(), "no ID is reused");
}

#[test]
fn the_handle_table_grows_and_returns_the_live_size_to_its_start() {
    let (mut instance, slot, _) = run(&holder());
    let holder = instance.result_id(slot);
    let before = instance.live_size().expect("answers");
    let ids = (0..1000)
        .map(|_| instance.read_id(&holder, 0).expect("an ID"))
        .collect::<Vec<_>>();
    assert!(
        instance.live_size().expect("answers") > before,
        "the table grew"
    );
    // Every ID names a live value while the table moves under it.
    for id in &ids {
        assert_eq!(instance.length(id), Ok(1));
    }
    for id in ids {
        instance.release(id).expect("released");
    }
    // The table keeps its capacity while any ID is held, and it is storage the
    // live size counts.
    assert!(instance.live_size().expect("answers") >= before);
    instance.release(holder).expect("released");
    assert_eq!(
        instance.live_size(),
        Ok(0),
        "a balanced run leaves the live size where it started, and the table frees with its last ID"
    );
}

#[test]
fn an_id_counter_that_would_wrap_is_the_host_event_and_not_a_trap() {
    // The counter has 32 bits. Set it so the entry's own result ID is the last
    // that fits, and the next one would wrap.
    let bytes =
        module_with_entry(ValueClass::Ref, &[ValType::I32, ValType::I32], |f| {
            let (tuple, leaf) = (0, 1);
            let mut code = vec![
                c32(0),
                I::I64Const(0xFFFF_FFFE),
                I::I64Store(mem(state::ID_COUNTER, 3)),
            ];
            code.extend(new_object(f, Kind::Tuple, STRIDE_CELLS, 1, 0));
            code.push(set(leaf));
            code.extend(new_object(f, Kind::Tuple, STRIDE_CELLS, 1, 0));
            code.push(set(tuple));
            code.extend(cell_ref(tuple, 1, 0, leaf));
            code.push(get(tuple));
            code
        });
    let (mut instance, slot, _) = run(&bytes);
    let holder = instance.result_id(slot);
    assert_eq!(
        holder.number_for_tests() >> 32,
        0xFFFF_FFFF,
        "the last counter"
    );
    assert_eq!(instance.length(&holder), Ok(1), "that ID is valid");
    assert_eq!(
        instance.read_id(&holder, 0),
        Err(Outcome::MemoryExhausted),
        "the next would wrap"
    );
    // Still answering after the stop.
    assert_eq!(instance.length(&holder), Ok(1));

    // An entry that has no ID left for its own result stops the same way.
    let exhausted = module_with_entry(ValueClass::Ref, &[ValType::I32], |f| {
        let mut code = vec![
            c32(0),
            I::I64Const(0xFFFF_FFFF),
            I::I64Store(mem(state::ID_COUNTER, 3)),
        ];
        code.extend(new_object(f, Kind::Tuple, STRIDE_CELLS, 0, 0));
        code
    });
    assert_eq!(
        runner().run_entry(&exhausted).expect("runs"),
        Outcome::MemoryExhausted
    );
}

// -- reference counts and the free list -------------------------------------

#[test]
fn dup_and_drop_balance_to_a_live_size_equal_to_the_start() {
    let bytes =
        module_with_entry(ValueClass::Void, &[ValType::I32, ValType::I32], |f| {
            let (object, i) = (0, 1);
            let mut code = new_object(f, Kind::Tuple, STRIDE_CELLS, 3, 0);
            code.push(set(object));
            // The count is 1; take 50 more, drop 50, and the object is still live.
            code.extend(repeat(i, 50, vec![get(object), I::Call(f.dup)]));
            code.extend(repeat(i, 50, vec![get(object), I::Call(f.drop)]));
            // A live object has a count of one: its header says so.
            code.extend([
                get(object),
                I::I32Load(mem(header::COUNT, 2)),
                c32(1),
                I::I32Ne,
                I::If(BlockType::Empty),
                I::Unreachable,
                I::End,
                get(object),
                I::Call(f.drop),
            ]);
            code
        });
    let (mut instance, _, live_size) = run(&bytes);
    assert_eq!(
        live_size, 0,
        "the object was released when its count reached zero"
    );
    assert_eq!(instance.live_size(), Ok(0));
}

#[test]
fn the_free_list_serves_its_own_class_and_only_partly_satisfies_a_larger_request() {
    // `trap` stops with no recorded status, which the runner reports as a
    // defect, so a wrong answer fails the run.
    let bytes = module_with_entry(
        ValueClass::Void,
        &[ValType::I32, ValType::I32, ValType::I32],
        |f| {
            let (a, b, c) = (0, 1, 2);
            let different = |x: u32, y: u32| {
                vec![
                    get(x),
                    get(y),
                    I::I32Eq,
                    I::If(BlockType::Empty),
                    I::Unreachable,
                    I::End,
                ]
            };
            let same = |x: u32, y: u32| {
                vec![
                    get(x),
                    get(y),
                    I::I32Ne,
                    I::If(BlockType::Empty),
                    I::Unreachable,
                    I::End,
                ]
            };
            // A 32-byte block, freed.
            let mut code = new_object(f, Kind::Bytes, STRIDE_BYTES, 8, 0);
            code.push(set(a));
            code.extend([get(a), I::Call(f.drop)]);
            // A request one class up cannot use it.
            code.extend(new_object(f, Kind::Bytes, STRIDE_BYTES, 30, 0));
            code.push(set(b));
            code.extend(different(a, b));
            // A request of its own class takes it back.
            code.extend(new_object(f, Kind::Bytes, STRIDE_BYTES, 8, 0));
            code.push(set(c));
            code.extend(same(a, c));
            code.extend([get(b), I::Call(f.drop), get(c), I::Call(f.drop)]);
            code
        },
    );
    let (mut instance, _, live_size) = run(&bytes);
    assert_eq!(live_size, 0);
    assert_eq!(instance.live_size(), Ok(0));
}

// -- the worklist release ---------------------------------------------------

/// A chain of `depth` tuples, each holding the one before, the first holding a
/// scalar. The entry returns the head or drops it.
fn chain(depth: u32, keep: bool) -> Vec<u8> {
    let (head, i, node) = (0, 1, 2);
    module_with_entry(
        if keep {
            ValueClass::Ref
        } else {
            ValueClass::Void
        },
        &[ValType::I32, ValType::I32, ValType::I32],
        |f| {
            let mut code = new_object(f, Kind::Tuple, STRIDE_CELLS, 1, 0);
            code.push(set(head));
            let mut body = new_object(f, Kind::Tuple, STRIDE_CELLS, 1, 0);
            body.push(set(node));
            body.extend(cell_ref(node, 1, 0, head));
            body.extend([get(node), set(head)]);
            code.extend(repeat(i, depth, body));
            code.push(get(head));
            if !keep {
                code.push(I::Call(f.drop));
            }
            code
        },
    )
}

/// A block of one tuple of one reference cell: 40 bytes in a 64-byte class.
const CHAIN_NODE_BYTES: u64 = 64;
/// The handle table's first block: sixteen 16-byte entries.
const TABLE_BYTES: u64 = 256;

#[test]
fn a_value_nested_deep_releases_with_bounded_engine_stack() {
    // A 64 KiB engine stack: a release that recursed once per level could not
    // return from a few thousand levels.
    let runner = Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), 64 * 1024)
        .expect("the engine configures");
    for depth in [5_000_u32, 100_000] {
        // The entry itself drops the head: the release runs inside the module.
        let bytes = chain(depth, false);
        let mut instance = started(&runner, &bytes);
        let Outcome::Completed { live_size, .. } = instance.call_entry().expect("runs")
        else {
            panic!("a chain {depth} deep did not release on a small stack");
        };
        assert_eq!(live_size, 0, "depth {depth}: nothing stays live");

        // The host holds the head, then drops its hold: the release runs inside
        // `vibra_v1_release`.
        let bytes = chain(depth, true);
        let mut instance = started(&runner, &bytes);
        let Outcome::Completed { result, live_size } =
            instance.call_entry().expect("runs")
        else {
            panic!("a chain {depth} deep did not build");
        };
        let expected = (u64::from(depth) + 1) * CHAIN_NODE_BYTES + TABLE_BYTES;
        assert_eq!(
            live_size, expected,
            "depth {depth}: the live size while held"
        );
        let head = instance.result_id(result);
        instance.release(head).expect("released");
        assert_eq!(instance.live_size(), Ok(0), "depth {depth}: released");
        println!("deep release: depth {depth}: held {live_size} bytes, then 0");
    }
}

#[test]
fn the_same_depth_does_exhaust_the_same_stack_when_it_recurses() {
    // The control: a function that recurses once per level, in the same
    // engine, stops long before 100,000 levels. So the test above is
    // sensitive to a release that recursed.
    let bytes = module_with_entry(ValueClass::Void, &[ValType::I32], |_| {
        vec![
            c32(0),
            I::I32Load(mem(state::FRAME_DEPTH, 2)),
            set(0),
            get(0),
            c32(100_000),
            I::I32LtU,
            I::If(BlockType::Empty),
            c32(0),
            get(0),
            c32(1),
            I::I32Add,
            I::I32Store(mem(state::FRAME_DEPTH, 2)),
            I::Call(0),
            I::End,
        ]
    });
    let runner = Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), 64 * 1024)
        .expect("the engine configures");
    let Outcome::Defect { cause } = runner.run_entry(&bytes).expect("runs") else {
        panic!("a recursion 100,000 deep fits a 64 KiB stack");
    };
    assert!(cause.contains("no recorded status"), "{cause}");
}

#[test]
fn a_worklist_that_grows_past_a_page_releases_and_returns_every_byte() {
    // One block holding 100,000 references to blocks that die with it: when it
    // is released, all of them are waiting at once, which is more than a page
    // of worklist entries of any width.
    const WIDTH: u32 = 100_000;
    let (root, i, leaf) = (0, 1, 2);
    let bytes = module_with_entry(
        ValueClass::Ref,
        &[ValType::I32, ValType::I32, ValType::I32],
        |f| {
            let mut code = new_object(f, Kind::Array, STRIDE_CELLS, WIDTH, 0);
            code.push(set(root));
            // Every component starts as a scalar, so the root is safe to
            // release at any point; each store turns one into a reference.
            let mut body = new_object(f, Kind::Bytes, STRIDE_BYTES, 0, 0);
            body.push(set(leaf));
            body.extend([
                get(root),
                get(i),
                I::I32Add,
                c32(i32::from(CellClass::Ref.code())),
                I::I32Store8(mem(header::SIZE, 0)),
                get(root),
                get(i),
                c32(3),
                I::I32Shl,
                I::I32Add,
                get(leaf),
                I::I64ExtendI32U,
                I::I64Store(mem(layout::cells_offset(WIDTH), 3)),
            ]);
            code.extend(repeat(i, WIDTH, body));
            code.push(get(root));
            code
        },
    );
    let runner = Runner::with_wasm_stack(MemoryLimit::new(64 * MIB), 64 * 1024)
        .expect("the engine configures");
    let mut instance = started(&runner, &bytes);
    let Outcome::Completed { result, live_size } = instance.call_entry().expect("runs")
    else {
        panic!("the wide value did not build");
    };
    let root = instance.result_id(result);
    assert_eq!(instance.length(&root), Ok(u64::from(WIDTH)));
    assert!(live_size > 4 * u64::from(WIDTH) * 8, "{live_size}");
    instance.release(root).expect("released");
    assert_eq!(instance.live_size(), Ok(0));
    println!("wide release: {WIDTH} children, held {live_size} bytes, then 0");
}

// -- the live size of a loop ------------------------------------------------

/// A loop of `count` iterations that builds a fresh tuple holding the previous
/// one's replacement and keeps only the newest: the allocating tail loop of
/// "Reclamation", written by hand.
fn allocating_loop(count: u32) -> Vec<u8> {
    let (kept, i, fresh) = (0, 1, 2);
    module_with_entry(
        ValueClass::Ref,
        &[ValType::I32, ValType::I32, ValType::I32],
        |f| {
            let mut code = new_object(f, Kind::Tuple, STRIDE_CELLS, 2, 0);
            code.push(set(kept));
            let mut body = new_object(f, Kind::Tuple, STRIDE_CELLS, 2, 0);
            body.push(set(fresh));
            body.extend([get(kept), I::Call(f.drop), get(fresh), set(kept)]);
            code.extend(repeat(i, count, body));
            code.push(get(kept));
            code
        },
    )
}

#[test]
fn an_allocating_loop_has_a_live_arena_that_does_not_grow_with_its_iterations() {
    let sizes = [10_000_u32, 100_000].map(|count| run(&allocating_loop(count)).2);
    assert_eq!(
        sizes[0], sizes[1],
        "the live arena after 100,000 iterations exceeds the one after 10,000 by a constant, here zero"
    );
    println!(
        "allocating loop: live size {} after 10,000 and {} after 100,000",
        sizes[0], sizes[1]
    );
}

// -- exhaustion -------------------------------------------------------------

#[test]
fn running_out_of_memory_is_the_host_event_and_never_a_trap() {
    // A chain that is never released, in a 1 MiB instance.
    let bytes = chain(1_000_000, true);
    let small = Runner::new(MemoryLimit::new(MIB)).expect("engine");
    assert_eq!(
        small.run_entry(&bytes).expect("runs"),
        Outcome::MemoryExhausted
    );

    // The instance still answers afterwards, and a fresh one runs normally.
    let mut instance = started(&small, &bytes);
    assert_eq!(
        instance.call_entry().expect("runs"),
        Outcome::MemoryExhausted
    );
    assert!(instance.live_size().expect("answers") > 0);
    let fresh = chain(10, true);
    assert!(matches!(
        small.run_entry(&fresh).expect("runs"),
        Outcome::Completed { .. }
    ));
}

#[test]
fn the_largest_size_class_is_allocated_or_exhausts_and_nothing_above_it_exists() {
    let request = |len: u32| {
        module_with_entry(ValueClass::Void, &[ValType::I32], move |f| {
            let mut code = new_object(f, Kind::Bytes, STRIDE_BYTES, len, 0);
            code.extend([set(0), get(0), I::Call(f.drop)]);
            code
        })
    };
    let class_30 = (1_u32 << layout::MAX_CLASS) - header::SIZE;
    // 2^25 bytes fit the 64 MiB limit and are returned to the free list.
    let runner = runner();
    let mut instance = started(&runner, &request((1 << 25) - header::SIZE));
    assert!(matches!(
        instance.call_entry().expect("runs"),
        Outcome::Completed { live_size: 0, .. }
    ));
    // The largest class is a class: the engine's limit refuses its growth.
    assert_eq!(
        runner.run_entry(&request(class_30)).expect("runs"),
        Outcome::MemoryExhausted
    );
    // One byte more has no class at all, and stops the same way.
    assert_eq!(
        runner.run_entry(&request(class_30 + 1)).expect("runs"),
        Outcome::MemoryExhausted
    );
    // A length whose size overflows 32 bits is no wrapped small request.
    assert_eq!(
        runner.run_entry(&request(u32::MAX)).expect("runs"),
        Outcome::MemoryExhausted
    );
}

#[test]
fn a_block_that_exactly_fills_the_limit_runs_and_one_page_under_does_not() {
    // A 64 KiB request is a 128 KiB block (the header spills over a page).
    let bytes = module_with_entry(ValueClass::Void, &[ValType::I32], |f| {
        let mut code = new_object(f, Kind::Bytes, STRIDE_BYTES, 1 << 16, 0);
        code.extend([set(0), get(0), I::Call(f.drop)]);
        code
    });
    let runs = |pages: usize| {
        Runner::new(MemoryLimit::new(pages * PAGE))
            .expect("engine")
            .run_entry(&bytes)
            .expect("runs")
    };
    let need = (1..=8)
        .find(|pages| matches!(runs(*pages), Outcome::Completed { .. }))
        .expect("some limit admits it");
    assert_eq!(need, 3, "the arena start, a 128 KiB block, and no more");
    assert_eq!(runs(need - 1), Outcome::MemoryExhausted, "one page under");
    assert!(
        matches!(runs(need), Outcome::Completed { .. }),
        "exactly at the need"
    );
    assert!(matches!(runs(need + 1), Outcome::Completed { .. }));
}
