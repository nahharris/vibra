//! Pure wasm32 ABI layout planning for copied values, mutable cells, and references.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiType {
    I32,
    I64,
    F32,
    F64,
    String,
    Array(Box<AbiType>),
    Map(Box<AbiType>, Box<AbiType>),
    Record(Vec<AbiType>),
    Tuple(Vec<AbiType>),
    Enum(Vec<AbiType>),
    Mutable(Box<AbiType>),
    Reference(Box<AbiType>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageClass {
    Direct,
    CopiedPointer,
    ArenaAddress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiLayout {
    pub size: u32,
    pub align: u32,
    pub field_offsets: Vec<u32>,
    pub storage: StorageClass,
}

pub fn layout_of(ty: &AbiType) -> AbiLayout {
    match ty {
        AbiType::I32 | AbiType::F32 => scalar(4, 4),
        AbiType::I64 | AbiType::F64 => scalar(8, 8),
        AbiType::Mutable(_) | AbiType::Reference(_) => AbiLayout {
            size: 4,
            align: 4,
            field_offsets: Vec::new(),
            storage: StorageClass::ArenaAddress,
        },
        AbiType::String | AbiType::Array(_) | AbiType::Map(_, _) => AbiLayout {
            size: 8,
            align: 4,
            field_offsets: vec![0, 4],
            storage: StorageClass::CopiedPointer,
        },
        AbiType::Record(fields) | AbiType::Tuple(fields) => aggregate(fields),
        AbiType::Enum(payloads) => {
            let payload = payloads
                .iter()
                .map(layout_of)
                .max_by_key(|layout| layout.size)
                .unwrap_or_else(|| scalar(0, 1));
            let payload_offset = align_up(4, payload.align);
            AbiLayout {
                size: align_up(payload_offset + payload.size, payload.align.max(4)),
                align: payload.align.max(4),
                field_offsets: vec![0, payload_offset],
                storage: StorageClass::CopiedPointer,
            }
        }
    }
}

fn scalar(size: u32, align: u32) -> AbiLayout {
    AbiLayout {
        size,
        align,
        field_offsets: Vec::new(),
        storage: StorageClass::Direct,
    }
}

fn aggregate(fields: &[AbiType]) -> AbiLayout {
    let layouts: Vec<_> = fields.iter().map(layout_of).collect();
    let align = layouts.iter().map(|layout| layout.align).max().unwrap_or(1);
    let mut cursor = 0;
    let mut offsets = Vec::with_capacity(layouts.len());
    for layout in layouts {
        cursor = align_up(cursor, layout.align);
        offsets.push(cursor);
        cursor += layout.size;
    }
    AbiLayout {
        size: align_up(cursor, align),
        align,
        field_offsets: offsets,
        storage: StorageClass::CopiedPointer,
    }
}

fn align_up(value: u32, align: u32) -> u32 {
    (value + align - 1) & !(align - 1)
}
