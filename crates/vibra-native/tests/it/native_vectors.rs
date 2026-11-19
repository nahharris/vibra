//! `native_vectors`: the shared sample vectors of `vibra-ir`, run through the
//! natives over arena memory.
//!
//! Every native row that has vectors is called on each sample's operands, laid
//! out by hand in the arena layout, as a runtime calls it: a building row
//! measures with no destination and then fills one of that length, and a
//! `parse` row writes its cell. The answer is read back as a value and
//! compared with the outcome the vectors specify, which no executor wrote.

use vibra_ir::Value;
use vibra_ir::arena::{self, kind};
use vibra_ir::external::{
    Answer, CompilerIntrinsic, FloatType, IntegerType, NATIVE_ROWS, NativeRow, Numeric,
    ObjectKind, Operand, Totality,
};
use vibra_native::arena::{call, memory};

/// A memory that objects are bump-allocated in, each in a block of its own.
struct Arena {
    bytes: Vec<u8>,
    next: usize,
}

impl Arena {
    fn new() -> Self {
        Self {
            bytes: vec![0; 1 << 20],
            next: 256,
        }
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        self.bytes[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// A block for an object of `kind`, `stride`, and `len` components.
    fn object(&mut self, object_kind: u8, stride: u32, len: u32) -> usize {
        let payload = if stride == arena::STRIDE_CELLS {
            arena::cells_offset(len) as usize + 8 * len as usize
        } else {
            arena::header::SIZE as usize + stride as usize * len as usize
        };
        let size = payload.next_power_of_two().max(32);
        let at = self.next;
        self.next += size;
        self.put(at, &1_u32.to_le_bytes());
        self.put(at + 4, &u32::try_from(size).unwrap().to_le_bytes());
        self.put(at + 8, &[object_kind, u8::try_from(stride).unwrap()]);
        self.put(at + 12, &len.to_le_bytes());
        at
    }

    fn text(&mut self, text: &str) -> usize {
        let scalars = text.chars().map(u32::from).collect::<Vec<_>>();
        let at = self.object(kind::STR, 4, u32::try_from(scalars.len()).unwrap());
        for (index, scalar) in scalars.iter().enumerate() {
            self.put(at + 24 + 4 * index, &scalar.to_le_bytes());
        }
        at
    }

    fn byte_string(&mut self, bytes: &[u8]) -> usize {
        let at = self.object(kind::BYTES, 1, u32::try_from(bytes.len()).unwrap());
        self.put(at + 24, bytes);
        at
    }

    fn read_text(&self, at: usize) -> String {
        (0..self.len(at))
            .map(|index| {
                let place = at + 24 + 4 * index;
                char::from_u32(u32::from_le_bytes(
                    self.bytes[place..place + 4].try_into().unwrap(),
                ))
                .unwrap()
            })
            .collect()
    }

    fn read_bytes(&self, at: usize) -> Vec<u8> {
        self.bytes[at + 24..at + 24 + self.len(at)].to_vec()
    }

    fn len(&self, at: usize) -> usize {
        u32::from_le_bytes(self.bytes[at + 12..at + 16].try_into().unwrap()) as usize
    }

    fn cell(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.bytes[at..at + 8].try_into().unwrap())
    }

    fn run(&mut self, row: &NativeRow, scalars: &[i64]) -> Result<i64, String> {
        call(row, memory(&mut self.bytes), scalars)
            .map_err(|fault| format!("{}: {fault}", row.name()))
    }
}

/// The 64 bits of the cell of a number value.
fn number_bits(value: &Value) -> Result<u64, String> {
    let low = |pattern: i32| u64::from(pattern.cast_unsigned());
    Ok(match value {
        Value::I8(value) => low(i32::from(*value)),
        Value::I16(value) => low(i32::from(*value)),
        Value::I32(value) => low(*value),
        Value::U8(value) => u64::from(*value),
        Value::U16(value) => u64::from(*value),
        Value::U32(value) | Value::F32(value) => u64::from(*value),
        Value::I64(value) => value.cast_unsigned(),
        Value::U64(value) | Value::F64(value) => *value,
        other => return Err(format!("not a number: {other:?}")),
    })
}

/// The value a cell of `numeric` holds.
fn number_value(numeric: Numeric, bits: u64) -> Value {
    let low = u32::try_from(bits & 0xFFFF_FFFF).unwrap();
    match numeric {
        Numeric::Integer(IntegerType::I8) => Value::I8(low.cast_signed() as i8),
        Numeric::Integer(IntegerType::I16) => Value::I16(low.cast_signed() as i16),
        Numeric::Integer(IntegerType::I32) => Value::I32(low.cast_signed()),
        Numeric::Integer(IntegerType::I64) => Value::I64(bits.cast_signed()),
        Numeric::Integer(IntegerType::U8) => Value::U8(low as u8),
        Numeric::Integer(IntegerType::U16) => Value::U16(low as u16),
        Numeric::Integer(IntegerType::U32) => Value::U32(low),
        Numeric::Integer(IntegerType::U64) => Value::U64(bits),
        Numeric::Float(FloatType::F32) => Value::F32(low),
        Numeric::Float(FloatType::F64) => Value::F64(bits),
    }
}

fn case(name: &str, fields: Vec<(String, Value)>) -> Value {
    Value::Variant {
        case: name.to_owned(),
        fields,
    }
}

fn some(value: Value) -> Value {
    case("some", vec![("value".to_owned(), value)])
}

fn ok(value: Value) -> Value {
    case("ok", vec![("value".to_owned(), value)])
}

fn err(error: &str) -> Value {
    case("err", vec![("error".to_owned(), case(error, Vec::new()))])
}

/// Calls `row` on `operands` as a runtime does, and reads the answer back.
fn answer(row: &NativeRow, operands: &[Value]) -> Result<Value, String> {
    let mut heap = Arena::new();
    let mut scalars = Vec::new();
    for (operand, value) in row.operands.iter().zip(operands) {
        match (operand, value) {
            (Operand::Text, Value::Str(text)) => {
                scalars.push(i64::try_from(heap.text(text)).unwrap());
            }
            (Operand::Bytes, Value::Bytes(bytes)) => {
                scalars.push(i64::try_from(heap.byte_string(bytes)).unwrap());
            }
            (Operand::Index, Value::U64(index)) => scalars.push(index.cast_signed()),
            (Operand::Number(_), number) => {
                scalars.push(number_bits(number)?.cast_signed());
            }
            (operand, value) => {
                return Err(format!("{}: {operand:?} of {value:?}", row.name()));
            }
        }
    }
    Ok(match row.answer {
        Answer::Length => Value::U64(heap.run(row, &scalars)?.cast_unsigned()),
        Answer::Same => Value::bool(heap.run(row, &scalars)? == 1),
        Answer::Order => case(
            match heap.run(row, &scalars)? {
                -1 => "less",
                0 => "equal",
                1 => "greater",
                other => return Err(format!("{}: order {other}", row.name())),
            },
            Vec::new(),
        ),
        Answer::Parsed => {
            let cell = heap.object(kind::LIST, 8, 1) + 32;
            scalars.push(i64::try_from(cell).unwrap());
            let numeric = parsed_type(row)
                .ok_or_else(|| format!("{}: a parse row of no number", row.name()))?;
            match heap.run(row, &scalars)? {
                0 => ok(number_value(numeric, heap.cell(cell))),
                1 => err("invalidFormat"),
                2 => err("outOfRange"),
                other => return Err(format!("{}: status {other}", row.name())),
            }
        }
        Answer::Object {
            kind: object_kind,
            totality,
        } => {
            let mut measure = scalars.clone();
            measure.push(0);
            let length = heap.run(row, &measure)?;
            if length == -1 {
                return match totality {
                    Totality::Total => {
                        Err(format!("{}: a total row built nothing", row.name()))
                    }
                    Totality::Option => Ok(case("none", Vec::new())),
                    Totality::Conversion => Ok(err("invalidFormat")),
                };
            }
            let len = u32::try_from(length).unwrap();
            let (code, stride) = match object_kind {
                ObjectKind::Str => (kind::STR, arena::STRIDE_CHARS),
                ObjectKind::Bytes => (kind::BYTES, arena::STRIDE_BYTES),
                ObjectKind::List => (kind::LIST, arena::STRIDE_CELLS),
            };
            let out = heap.object(code, stride, len);
            scalars.push(i64::try_from(out).unwrap());
            assert_eq!(heap.run(row, &scalars)?, length, "{}", row.name());
            let built = match object_kind {
                ObjectKind::Str => Value::Str(heap.read_text(out)),
                ObjectKind::Bytes => Value::Bytes(heap.read_bytes(out)),
                ObjectKind::List => {
                    return Err(format!("{}: no list vectors", row.name()));
                }
            };
            match totality {
                Totality::Total => built,
                Totality::Option => some(built),
                Totality::Conversion => ok(built),
            }
        }
    })
}

/// The number type a `parse` row reads.
fn parsed_type(row: &NativeRow) -> Option<Numeric> {
    match row.intrinsic {
        CompilerIntrinsic::Integer(integer, _) => Some(Numeric::Integer(integer)),
        CompilerIntrinsic::Float(float, _) => Some(Numeric::Float(float)),
        _ => None,
    }
}

#[test]
fn native_vectors() {
    let mut checked = 0;
    for row in &NATIVE_ROWS {
        for vector in row.intrinsic.vectors() {
            let got = answer(row, &vector.operands).unwrap();
            assert!(
                got == vector.outcome.value(),
                "{} on {:?}: got {got:?}, the vectors say {:?}",
                row.name(),
                vector.operands,
                vector.outcome
            );
            checked += 1;
        }
    }
    // The number-text rows of ten types and the text and bytes rows.
    assert!(checked > 1_000, "{checked} vectors");
    for intrinsic in [
        CompilerIntrinsic::StrConcat,
        CompilerIntrinsic::StrSlice,
        CompilerIntrinsic::StrFromUtf8,
        CompilerIntrinsic::BytesCompare,
        CompilerIntrinsic::Float(FloatType::F64, vibra_ir::external::FloatOp::ToStr),
        CompilerIntrinsic::Integer(
            IntegerType::I8,
            vibra_ir::external::IntegerOp::Parse,
        ),
    ] {
        assert!(!intrinsic.vectors().is_empty(), "{}", intrinsic.symbol());
    }
}
