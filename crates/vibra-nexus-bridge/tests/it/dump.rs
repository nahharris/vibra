//! The harness launcher's dump of the entry's result, and its counters
//! (feature `harness`). The grammar is the module documentation of
//! `vibra_nexus_bridge::harness`.

use vibra_host::{HostWorld, WorldProvider};
use vibra_ir::arena::{STRIDE_CELLS, class, kind};
use vibra_nexus::{Cell, Context, Store};
use vibra_nexus_bridge::harness::{
    MAX_DEPTH, MAX_NODES, MAX_PAYLOAD, dump_value, launch_harness,
};

const LIMIT: u64 = 512 * 1024 * 1024;

/// A cell of `class` and `bits`.
fn scalar(class: u8, bits: u64) -> Cell {
    Cell { class, bits }
}

/// A value of `kind` with `variant` that takes over the references of `cells`.
fn object(store: &mut Store, kind: u8, variant: u32, cells: &[Cell]) -> Cell {
    let len = u32::try_from(cells.len()).unwrap();
    let value = store.alloc(kind, STRIDE_CELLS, len, variant).unwrap();
    for (index, cell) in cells.iter().enumerate() {
        store
            .set_cell(value, u32::try_from(index).unwrap(), *cell)
            .unwrap();
    }
    Cell::reference(value)
}

fn with_context<T>(body: impl FnOnce(&mut Context<'_>) -> T) -> T {
    let world = HostWorld::default();
    let mut host = WorldProvider::new(&world);
    let mut ctx = Context::new(LIMIT, 0, &mut host);
    body(&mut ctx)
}

#[test]
fn the_dump_of_each_value_kind_is_stable() {
    with_context(|ctx| {
        let store = ctx.store();
        let dump = |store: &Store, cell: Cell| dump_value(store, cell);

        // Scalars: the class and the bits as an unsigned decimal.
        assert_eq!(
            dump(store, Cell::i32(7)),
            r#"(record node: @scalar class: @i32 bits: "7")"#
        );
        assert_eq!(
            dump(store, Cell::i64(u64::MAX)),
            r#"(record node: @scalar class: @i64 bits: "18446744073709551615")"#
        );
        assert_eq!(
            dump(store, scalar(class::F32, 0x3f80_0000)),
            r#"(record node: @scalar class: @f32 bits: "1065353216")"#
        );
        assert_eq!(
            dump(store, scalar(class::F64, 0x7ff8_0000_0000_0000)),
            r#"(record node: @scalar class: @f64 bits: "9221120237041090560")"#
        );
        // The empty cell is the `void` value: a scalar zero.
        assert_eq!(
            dump(store, Cell::EMPTY),
            r#"(record node: @scalar class: @i32 bits: "0")"#
        );

        // Text, with the escapes of the canonical string; an atom; bytes.
        let text =
            Cell::reference(store.new_text(kind::STR, "a\"b\\c\n\u{e9}").unwrap());
        assert_eq!(
            dump(store, text),
            "(record node: @str value: \"a\\\"b\\\\c\\n\u{e9}\")"
        );
        let atom = Cell::reference(store.new_text(kind::ATOM, "@ok").unwrap());
        assert_eq!(dump(store, atom), r#"(record node: @atom value: "@ok")"#);
        let bytes =
            Cell::reference(store.new_bytes(kind::BYTES, &[0, 16, 255]).unwrap());
        assert_eq!(dump(store, bytes), r#"(record node: @bytes hex: "0010ff")"#);
        let none = Cell::reference(store.new_bytes(kind::BYTES, &[]).unwrap());
        assert_eq!(dump(store, none), r#"(record node: @bytes hex: "")"#);

        // A function value is opaque: neither its header nor its captures are
        // read.
        let function = object(
            store,
            kind::FUNCTION,
            0,
            &[Cell::i64(3), Cell::i64(4), Cell::i64(0), Cell::i32(9)],
        );
        assert_eq!(dump(store, function), "(record node: @function)");

        // An object of each kind: the kind, the variant, and the cells in order.
        let inner =
            |bits| format!(r#"(record node: @scalar class: @i32 bits: "{bits}")"#);
        for (code, name, variant) in [
            (kind::TUPLE, "tuple", 0),
            (kind::ARRAY, "array", 0),
            (kind::DICT, "dict", 0),
            (kind::RECORD, "record", 0),
            (kind::ENUM, "enum", 2),
            (kind::WRAPPER, "wrapper", 0),
            (kind::UNION, "union", 1),
        ] {
            let value = object(store, code, variant, &[Cell::i32(1), Cell::i32(2)]);
            assert_eq!(
                dump(store, value),
                format!(
                    "(record node: @object kind: @{name} variant: {variant} cells: (array {} {}))",
                    inner(1),
                    inner(2)
                )
            );
        }
        let empty = object(store, kind::ENUM, 1, &[]);
        assert_eq!(
            dump(store, empty),
            "(record node: @object kind: @enum variant: 1 cells: (array))"
        );

        // Nesting: the reference is followed, and shared structure is written
        // once for each place it is reached.
        let shared = object(store, kind::TUPLE, 0, &[Cell::i32(5)]);
        store.dup_cell(shared).unwrap();
        let outer = object(store, kind::RECORD, 0, &[shared, shared]);
        let leaf = r#"(record node: @object kind: @tuple variant: 0 cells: (array (record node: @scalar class: @i32 bits: "5")))"#;
        assert_eq!(
            dump(store, outer),
            format!(
                "(record node: @object kind: @record variant: 0 cells: (array {leaf} {leaf}))"
            )
        );

        // What the dumper does not know and what the store refuses: a handle of
        // another store, and a cell of no class.
        let foreign = {
            let world = HostWorld::default();
            let mut host = WorldProvider::new(&world);
            let mut other = Context::new(LIMIT, 0, &mut host);
            let mut last = None;
            for _ in 0..400 {
                last = Some(other.store().new_bytes(kind::BYTES, &[]).unwrap());
            }
            Cell::reference(last.unwrap())
        };
        let unknown = object(store, 200, 0, &[]);
        assert_eq!(dump(store, unknown), "(record node: @unknown kind: 200)");
        assert_eq!(dump(store, foreign), "(record node: @fault)");
        assert_eq!(dump(store, scalar(77, 1)), "(record node: @fault)");
    });
}

#[test]
fn a_dump_past_the_bound_is_truncated_not_failed() {
    with_context(|ctx| {
        let store = ctx.store();

        // Depth: tuples nested 70 deep. Levels 0 to MAX_DEPTH are written and
        // the one below is the truncated node.
        let mut value = Cell::i32(1);
        for _ in 0..70 {
            value = object(store, kind::TUPLE, 0, &[value]);
        }
        let dump = dump_value(store, value);
        assert_eq!(dump.matches("@object").count(), MAX_DEPTH + 1);
        assert_eq!(dump.matches("@truncated").count(), 1);
        assert!(!dump.contains("@scalar"));

        // Exactly MAX_DEPTH levels of objects below the root fit.
        let mut value = Cell::i32(1);
        for _ in 0..MAX_DEPTH {
            value = object(store, kind::TUPLE, 0, &[value]);
        }
        let dump = dump_value(store, value);
        assert_eq!(dump.matches("@truncated").count(), 0);
        assert_eq!(dump.matches("@scalar").count(), 1);

        // Width: an array of more cells than the node bound.
        let cells = vec![Cell::i32(0); MAX_NODES + 10];
        let wide = object(store, kind::ARRAY, 0, &cells);
        let dump = dump_value(store, wide);
        assert_eq!(dump.matches("@scalar").count(), MAX_NODES - 1);
        assert_eq!(dump.matches("@truncated").count(), 1);
        assert!(
            dump.ends_with("(record node: @truncated)))"),
            "{}",
            &dump[dump.len() - 80..]
        );

        // Shared structure is dumped by value, so a value that refers twice to
        // a child at each of forty levels would be 2^40 nodes: the node bound
        // ends the walk.
        let mut value = Cell::i32(1);
        for _ in 0..40 {
            store.dup_cell(value).unwrap();
            value = object(store, kind::TUPLE, 0, &[value, value]);
        }
        let dump = dump_value(store, value);
        assert!(dump.contains("@truncated"));
        assert!(dump.matches("@object").count() <= MAX_NODES);

        // Payload: text and bytes past the bound.
        let big = Cell::reference(
            store.new_bytes(kind::BYTES, &vec![7; MAX_PAYLOAD]).unwrap(),
        );
        assert!(dump_value(store, big).contains("@bytes"));
        let bigger = Cell::reference(
            store
                .new_bytes(kind::BYTES, &vec![7; MAX_PAYLOAD + 1])
                .unwrap(),
        );
        assert_eq!(dump_value(store, bigger), "(record node: @truncated)");
        let long = Cell::reference(
            store
                .new_text(kind::STR, &"x".repeat(MAX_PAYLOAD + 1))
                .unwrap(),
        );
        assert_eq!(dump_value(store, long), "(record node: @truncated)");
        let pair = object(store, kind::TUPLE, 0, &[big, long]);
        let dump = dump_value(store, pair);
        assert!(
            dump.contains("@bytes") && dump.ends_with("(record node: @truncated)))")
        );
    });
}

#[test]
fn dumping_leaves_the_store_balanced() {
    with_context(|ctx| {
        let before = ctx.live_size();
        let store = ctx.store();
        let text = Cell::reference(store.new_text(kind::STR, "held").unwrap());
        let inner = object(store, kind::RECORD, 0, &[text, Cell::i64(3)]);
        let outer = object(store, kind::ARRAY, 0, &[inner, Cell::i32(1)]);
        let held = ctx.live_size();
        assert!(held > before);
        let peak = ctx.peak_size();
        let first = dump_value(ctx.store(), outer);
        assert_eq!(ctx.live_size(), held, "a dump takes no reference");
        assert_eq!(first, dump_value(ctx.store(), outer));
        assert_eq!(ctx.live_size(), held);
        assert_eq!(ctx.peak_size(), peak, "a dump allocates nothing");
        // Releasing the value once frees everything: no reference was left
        // behind.
        ctx.store().drop_cell(outer).unwrap();
        assert_eq!(ctx.live_size(), before);
    });
}

#[test]
fn the_document_carries_the_value_and_the_counters() {
    use crate::fixture;

    let mut out = Vec::new();
    let fixture = fixture::entry_enum(1, 0);
    let descriptor = fixture.descriptor();
    // SAFETY: the fixture owns every table the descriptor addresses.
    let code = unsafe {
        launch_harness(
            &descriptor,
            "(record format: @host-world.v1)",
            u64::MAX,
            &mut out,
            &mut Vec::new(),
        )
    };
    assert_eq!(code, 0);
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains(
            "counters: (record yields: 0 peak-frames: 1 tail-transfers: 0 peak-live: "
        ),
        "{text}"
    );
    assert!(
        text.ends_with(
            " value: (record node: @object kind: @enum variant: 1 cells: (array)))\n"
        ),
        "{text}"
    );

    // A trap has counters and no value; a refusal has neither.
    let fixture = fixture::entry_trap();
    let descriptor = fixture.descriptor();
    let mut out = Vec::new();
    // SAFETY: as above.
    unsafe {
        launch_harness(
            &descriptor,
            "(record format: @host-world.v1)",
            1,
            &mut out,
            &mut Vec::new(),
        )
    };
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.ends_with(" value: @none)\n") && text.contains("counters: (record "),
        "{text}"
    );
}
