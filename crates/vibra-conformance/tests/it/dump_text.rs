//! A second writer of the harness launcher's structural dump, for tests.
//!
//! The runner reads the dump with `native_read` and cannot depend on the
//! bridge that writes it, so the tests hold the two ends to the grammar of
//! `vibra_nexus_bridge::harness` from both sides: this writer produces the dump
//! of a value in a store of the reference stepper, and the linked-executable
//! tests compare it, byte for byte, with the document that the real launcher
//! wrote. It is also compiled into the library's unit tests
//! (`#[path]` in `native_read.rs`), which need a dump of a value they build.
//! The bounds are the launcher's.

use std::fmt::Write as _;

use vibra_ir::arena::{class, kind};
use vibra_ir::canonical_string;
use vibra_nexus::{Cell, Ref, Store};

/// The deepest level of a dump.
pub(crate) const MAX_DEPTH: usize = 64;
/// The most nodes a dump holds.
pub(crate) const MAX_NODES: usize = 65_536;
/// The most bytes of text and byte strings a dump holds, together.
pub(crate) const MAX_PAYLOAD: usize = 1 << 20;

const TRUNCATED: &str = "(record node: @truncated)";
const FAULT: &str = "(record node: @fault)";

/// The dump of the value `cell` holds.
pub(crate) fn dump(store: &Store, cell: Cell) -> String {
    let mut writer = Writer {
        store,
        out: String::new(),
        nodes: 0,
        payload: 0,
    };
    writer.cell(cell, 0);
    writer.out
}

struct Writer<'a> {
    store: &'a Store,
    out: String,
    nodes: usize,
    payload: usize,
}

impl Writer<'_> {
    fn cell(&mut self, cell: Cell, depth: usize) {
        if depth > MAX_DEPTH || self.nodes >= MAX_NODES || self.payload > MAX_PAYLOAD {
            self.out.push_str(TRUNCATED);
            return;
        }
        self.nodes += 1;
        match cell.as_ref() {
            Some(value) => self.value(value, depth),
            None => {
                let name = match cell.class {
                    class::I32 => "i32",
                    class::I64 => "i64",
                    class::F32 => "f32",
                    class::F64 => "f64",
                    _ => return self.out.push_str(FAULT),
                };
                let _ = write!(
                    self.out,
                    "(record node: @scalar class: @{name} bits: \"{}\")",
                    cell.bits
                );
            }
        }
    }

    fn value(&mut self, value: Ref, depth: usize) {
        let Ok(code) = self.store.kind(value) else {
            return self.out.push_str(FAULT);
        };
        match code {
            kind::ATOM | kind::STR => {
                let Ok(text) = self.store.text(value) else {
                    return self.out.push_str(FAULT);
                };
                if self.payload + text.len() > MAX_PAYLOAD {
                    return self.out.push_str(TRUNCATED);
                }
                self.payload += text.len();
                let name = if code == kind::ATOM { "atom" } else { "str" };
                let _ = write!(
                    self.out,
                    "(record node: @{name} value: {})",
                    canonical_string(&text)
                );
            }
            kind::BYTES => {
                let Ok(bytes) = self.store.byte_values(value) else {
                    return self.out.push_str(FAULT);
                };
                if self.payload + bytes.len() > MAX_PAYLOAD {
                    return self.out.push_str(TRUNCATED);
                }
                self.payload += bytes.len();
                let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
                let _ = write!(self.out, "(record node: @bytes hex: \"{hex}\")");
            }
            kind::FUNCTION => self.out.push_str("(record node: @function)"),
            kind::TUPLE
            | kind::ARRAY
            | kind::DICT
            | kind::RECORD
            | kind::ENUM
            | kind::WRAPPER
            | kind::UNION => {
                let (Ok(len), Ok(variant)) =
                    (self.store.len(value), self.store.variant(value))
                else {
                    return self.out.push_str(FAULT);
                };
                let name = match code {
                    kind::TUPLE => "tuple",
                    kind::ARRAY => "array",
                    kind::DICT => "dict",
                    kind::RECORD => "record",
                    kind::ENUM => "enum",
                    kind::WRAPPER => "wrapper",
                    _ => "union",
                };
                let _ = write!(
                    self.out,
                    "(record node: @object kind: @{name} variant: {variant} cells: (array"
                );
                for index in 0..len {
                    self.out.push(' ');
                    if self.nodes >= MAX_NODES || self.payload > MAX_PAYLOAD {
                        self.out.push_str(TRUNCATED);
                        break;
                    }
                    match self.store.cell(value, index) {
                        Ok(child) => self.cell(child, depth + 1),
                        Err(_) => self.out.push_str(FAULT),
                    }
                }
                self.out.push_str("))");
            }
            other => {
                let _ = write!(self.out, "(record node: @unknown kind: {other})");
            }
        }
    }
}
