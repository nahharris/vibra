use serde::Serialize;
use std::fmt;
use std::ops::{BitAnd, BitOr};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[repr(u8)]
pub enum Tag {
    Unit = 0,
    Int = 1,
    Bool = 2,
    Char = 3,
    Ptr = 4,
    Code = 5,
    Header = 6,
    CapIdx = 7,
    Sealed = 8,
    Poison = 14,
    Null = 15,
}

impl TryFrom<u8> for Tag {
    type Error = ValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unit),
            1 => Ok(Self::Int),
            2 => Ok(Self::Bool),
            3 => Ok(Self::Char),
            4 => Ok(Self::Ptr),
            5 => Ok(Self::Code),
            6 => Ok(Self::Header),
            7 => Ok(Self::CapIdx),
            8 => Ok(Self::Sealed),
            14 => Ok(Self::Poison),
            15 => Ok(Self::Null),
            other => Err(ValueError::ReservedTag(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Word {
    tag: Tag,
    payload: u32,
}

impl Word {
    pub const fn int(value: i32) -> Self {
        Self {
            tag: Tag::Int,
            payload: value as u32,
        }
    }

    pub const fn bool(value: bool) -> Self {
        Self {
            tag: Tag::Bool,
            payload: value as u32,
        }
    }

    pub fn char(value: char) -> Result<Self, ValueError> {
        Self::try_new(Tag::Char, value as u32)
    }

    pub const fn poison() -> Self {
        Self {
            tag: Tag::Poison,
            payload: 0,
        }
    }

    pub const fn unit() -> Self {
        Self {
            tag: Tag::Unit,
            payload: 0,
        }
    }

    pub const fn null() -> Self {
        Self {
            tag: Tag::Null,
            payload: 0,
        }
    }

    pub fn try_new(tag: Tag, payload: u32) -> Result<Self, ValueError> {
        match tag {
            Tag::Unit | Tag::Null if payload != 0 => {
                Err(ValueError::NonZeroPayload { tag, payload })
            }
            Tag::Bool if payload > 1 => Err(ValueError::InvalidBoolean(payload)),
            Tag::Char if char::from_u32(payload).is_none() => {
                Err(ValueError::InvalidChar(payload))
            }
            _ => Ok(Self { tag, payload }),
        }
    }

    pub const fn tag(self) -> Tag {
        self.tag
    }

    pub const fn payload(self) -> u32 {
        self.payload
    }

    pub fn as_i32(self) -> Result<i32, ValueError> {
        if self.tag == Tag::Int {
            Ok(self.payload as i32)
        } else {
            Err(ValueError::ExpectedTag {
                expected: Tag::Int,
                actual: self.tag,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueError {
    ReservedTag(u8),
    NonZeroPayload { tag: Tag, payload: u32 },
    InvalidBoolean(u32),
    InvalidChar(u32),
    ExpectedTag { expected: Tag, actual: Tag },
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReservedTag(tag) => write!(formatter, "reserved data tag {tag}"),
            Self::NonZeroPayload { tag, payload } => {
                write!(formatter, "tag {tag:?} requires a zero payload, got {payload}")
            }
            Self::InvalidBoolean(payload) => write!(formatter, "invalid boolean payload {payload}"),
            Self::InvalidChar(payload) => write!(formatter, "invalid Unicode scalar value {payload}"),
            Self::ExpectedTag { expected, actual } => {
                write!(formatter, "expected tag {expected:?}, got {actual:?}")
            }
        }
    }
}

impl std::error::Error for ValueError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Permissions(u8);

impl Permissions {
    pub const READ: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const EXECUTE: Self = Self(1 << 2);
    pub const ALLOCATE: Self = Self(1 << 3);
    pub const MMIO: Self = Self(1 << 4);
    pub const SEAL: Self = Self(1 << 5);
    pub const DERIVE: Self = Self(1 << 6);
    pub const GLOBAL: Self = Self(1 << 7);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl BitOr for Permissions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitAnd for Permissions {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Capability {
    base: u32,
    len: u32,
    permissions: Permissions,
    otype: u8,
}

impl Capability {
    pub fn new(
        base: u32,
        len: u32,
        permissions: Permissions,
        otype: u8,
    ) -> Result<Self, CapabilityError> {
        if len > 0x00ff_ffff {
            return Err(CapabilityError::LengthTooLarge(len));
        }
        Ok(Self {
            base,
            len,
            permissions,
            otype,
        })
    }

    pub const fn null() -> Self {
        Self {
            base: 0,
            len: 0,
            permissions: Permissions::empty(),
            otype: 0,
        }
    }

    pub const fn base(self) -> u32 {
        self.base
    }

    pub const fn len(self) -> u32 {
        self.len
    }

    pub const fn permissions(self) -> Permissions {
        self.permissions
    }

    pub const fn otype(self) -> u8 {
        self.otype
    }

    pub const fn is_null(self) -> bool {
        self.len == 0
    }

    pub fn derive(self, offset: u32, len: u32) -> Result<Self, CapabilityError> {
        let new_base = self
            .base
            .checked_add(offset)
            .ok_or(CapabilityError::Bounds)?;
        let new_end = u64::from(new_base)
            .checked_add(u64::from(len))
            .ok_or(CapabilityError::Bounds)?;
        let parent_end = u64::from(self.base) + u64::from(self.len);
        if new_base < self.base || new_end > parent_end {
            return Err(CapabilityError::Bounds);
        }
        Self::new(new_base, len, self.permissions, self.otype)
    }

    pub const fn attenuate(self, permissions: Permissions) -> Self {
        Self {
            permissions: self.permissions.intersect(permissions),
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    LengthTooLarge(u32),
    Bounds,
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthTooLarge(len) => write!(formatter, "capability length {len} exceeds 24 bits"),
            Self::Bounds => write!(formatter, "derived capability is outside its parent bounds"),
        }
    }
}

impl std::error::Error for CapabilityError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Format {
    None,
    R,
    I,
    L,
    J,
    B,
    M,
    C,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[repr(u8)]
pub enum Opcode {
    Nop = 0x00,
    Mov = 0x01,
    MovI = 0x02,
    MovH = 0x03,
    Add = 0x04,
    Sub = 0x05,
    Mul = 0x06,
    And = 0x07,
    Or = 0x08,
    Xor = 0x09,
    Shl = 0x0a,
    Shr = 0x0b,
    Sar = 0x0c,
    AddI = 0x0d,
    AndI = 0x0e,
    OrI = 0x0f,
    XorI = 0x10,
    ShlI = 0x11,
    ShrI = 0x12,
    Cmp = 0x13,
    BNot = 0x14,
    BAnd = 0x15,
    BOr = 0x16,
    Tag = 0x17,
    IsTag = 0x18,
    Poison = 0x1a,
    Mktag = 0x1b,
    Load = 0x1c,
    Store = 0x1d,
    Alloc = 0x1e,
    Hdr = 0x1f,
    CDerive = 0x20,
    CAndPerm = 0x21,
    CDrop = 0x22,
    CSeal = 0x23,
    CUnseal = 0x24,
    CMov = 0x25,
    CInc = 0x26,
    CGet = 0x27,
    CSpecial = 0x28,
    Branch = 0x29,
    BranchTrue = 0x2a,
    BranchFalse = 0x2b,
    Jump = 0x2c,
    Call = 0x2d,
    CallC = 0x2e,
    CallCL = 0x2f,
    Ret = 0x30,
    Withe = 0x31,
    EndE = 0x32,
    GetE = 0x33,
    ReqE = 0x34,
    Trap = 0x35,
    TRet = 0x36,
    Halt = 0x37,
    Div = 0x38,
    Rem = 0x39,
}

impl Opcode {
    pub const fn format(self) -> Format {
        match self {
            Self::Nop | Self::Ret | Self::EndE | Self::TRet | Self::Halt => Format::None,
            Self::Mov
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::And
            | Self::Or
            | Self::Xor
            | Self::Shl
            | Self::Shr
            | Self::Sar
            | Self::Cmp
            | Self::BNot
            | Self::BAnd
            | Self::BOr
            | Self::Tag
            | Self::Div
            | Self::Rem
            | Self::Hdr
            | Self::Jump
            | Self::CallCL => Format::R,
            Self::MovH
            | Self::AddI
            | Self::AndI
            | Self::OrI
            | Self::XorI
            | Self::ShlI
            | Self::ShrI
            | Self::IsTag
            | Self::Alloc => Format::I,
            Self::MovI | Self::Poison | Self::Withe | Self::GetE | Self::ReqE => Format::L,
            Self::Branch | Self::Call | Self::Trap => Format::J,
            Self::BranchTrue | Self::BranchFalse => Format::B,
            Self::Load | Self::Store => Format::M,
            Self::Mktag => Format::I,
            Self::CDerive
            | Self::CAndPerm
            | Self::CDrop
            | Self::CSeal
            | Self::CUnseal
            | Self::CMov
            | Self::CInc
            | Self::CGet
            | Self::CSpecial
            | Self::CallC => Format::C,
        }
    }

    pub const fn all() -> &'static [(Self, Format)] {
        &[
            (Self::Nop, Format::None),
            (Self::Mov, Format::R),
            (Self::MovI, Format::L),
            (Self::MovH, Format::I),
            (Self::Add, Format::R),
            (Self::Sub, Format::R),
            (Self::Mul, Format::R),
            (Self::And, Format::R),
            (Self::Or, Format::R),
            (Self::Xor, Format::R),
            (Self::Shl, Format::R),
            (Self::Shr, Format::R),
            (Self::Sar, Format::R),
            (Self::AddI, Format::I),
            (Self::AndI, Format::I),
            (Self::OrI, Format::I),
            (Self::XorI, Format::I),
            (Self::ShlI, Format::I),
            (Self::ShrI, Format::I),
            (Self::Cmp, Format::R),
            (Self::BNot, Format::R),
            (Self::BAnd, Format::R),
            (Self::BOr, Format::R),
            (Self::Tag, Format::R),
            (Self::IsTag, Format::I),
            (Self::Poison, Format::L),
            (Self::Mktag, Format::I),
            (Self::Load, Format::M),
            (Self::Store, Format::M),
            (Self::Alloc, Format::I),
            (Self::Hdr, Format::R),
            (Self::CDerive, Format::C),
            (Self::CAndPerm, Format::C),
            (Self::CDrop, Format::C),
            (Self::CSeal, Format::C),
            (Self::CUnseal, Format::C),
            (Self::CMov, Format::C),
            (Self::CInc, Format::C),
            (Self::CGet, Format::C),
            (Self::CSpecial, Format::C),
            (Self::Branch, Format::J),
            (Self::BranchTrue, Format::B),
            (Self::BranchFalse, Format::B),
            (Self::Jump, Format::R),
            (Self::Call, Format::J),
            (Self::CallC, Format::C),
            (Self::CallCL, Format::R),
            (Self::Ret, Format::None),
            (Self::Withe, Format::L),
            (Self::EndE, Format::None),
            (Self::GetE, Format::L),
            (Self::ReqE, Format::L),
            (Self::Trap, Format::J),
            (Self::TRet, Format::None),
            (Self::Halt, Format::None),
            (Self::Div, Format::R),
            (Self::Rem, Format::R),
        ]
    }
}

impl TryFrom<u8> for Opcode {
    type Error = InstructionError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let opcode = match value {
            0x00 => Self::Nop,
            0x01 => Self::Mov,
            0x02 => Self::MovI,
            0x03 => Self::MovH,
            0x04 => Self::Add,
            0x05 => Self::Sub,
            0x06 => Self::Mul,
            0x07 => Self::And,
            0x08 => Self::Or,
            0x09 => Self::Xor,
            0x0a => Self::Shl,
            0x0b => Self::Shr,
            0x0c => Self::Sar,
            0x0d => Self::AddI,
            0x0e => Self::AndI,
            0x0f => Self::OrI,
            0x10 => Self::XorI,
            0x11 => Self::ShlI,
            0x12 => Self::ShrI,
            0x13 => Self::Cmp,
            0x14 => Self::BNot,
            0x15 => Self::BAnd,
            0x16 => Self::BOr,
            0x17 => Self::Tag,
            0x18 => Self::IsTag,
            0x1a => Self::Poison,
            0x1b => Self::Mktag,
            0x1c => Self::Load,
            0x1d => Self::Store,
            0x1e => Self::Alloc,
            0x1f => Self::Hdr,
            0x20 => Self::CDerive,
            0x21 => Self::CAndPerm,
            0x22 => Self::CDrop,
            0x23 => Self::CSeal,
            0x24 => Self::CUnseal,
            0x25 => Self::CMov,
            0x26 => Self::CInc,
            0x27 => Self::CGet,
            0x28 => Self::CSpecial,
            0x29 => Self::Branch,
            0x2a => Self::BranchTrue,
            0x2b => Self::BranchFalse,
            0x2c => Self::Jump,
            0x2d => Self::Call,
            0x2e => Self::CallC,
            0x2f => Self::CallCL,
            0x30 => Self::Ret,
            0x31 => Self::Withe,
            0x32 => Self::EndE,
            0x33 => Self::GetE,
            0x34 => Self::ReqE,
            0x35 => Self::Trap,
            0x36 => Self::TRet,
            0x37 => Self::Halt,
            0x38 => Self::Div,
            0x39 => Self::Rem,
            other => return Err(InstructionError::UnknownOpcode(other)),
        };
        Ok(opcode)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Instruction {
    opcode: Opcode,
    eff: u8,
    a: u8,
    b: u8,
    c: u8,
    imm10: u16,
}

impl Instruction {
    pub fn decode(raw: u32) -> Result<Self, InstructionError> {
        Self::new(
            Opcode::try_from((raw >> 26) as u8 & 0x3f)?,
            ((raw >> 22) & 0x0f) as u8,
            ((raw >> 18) & 0x0f) as u8,
            ((raw >> 14) & 0x0f) as u8,
            ((raw >> 10) & 0x0f) as u8,
            (raw & 0x03ff) as u16,
        )
    }

    pub const fn encode(self) -> u32 {
        ((self.opcode as u32) << 26)
            | ((self.eff as u32) << 22)
            | ((self.a as u32) << 18)
            | ((self.b as u32) << 14)
            | ((self.c as u32) << 10)
            | self.imm10 as u32
    }

    pub fn r(
        opcode: Opcode,
        eff: u8,
        rd: u8,
        rs1: u8,
        rs2: u8,
        aux10: u16,
    ) -> Result<Self, InstructionError> {
        Self::ensure_format(opcode, Format::R)?;
        if aux10 > 0x03ff {
            return Err(InstructionError::FieldOutOfRange("aux10", aux10 as u32));
        }
        Self::new(opcode, eff, rd, rs1, rs2, aux10)
    }

    pub fn i(
        opcode: Opcode,
        eff: u8,
        rd: u8,
        rs1: u8,
        immediate: i32,
    ) -> Result<Self, InstructionError> {
        Self::ensure_format(opcode, Format::I)?;
        let immediate = encode_signed(immediate, 14, "imm14")? as u32;
        Self::new(
            opcode,
            eff,
            rd,
            rs1,
            ((immediate >> 10) & 0x0f) as u8,
            (immediate & 0x03ff) as u16,
        )
    }

    pub fn l(
        opcode: Opcode,
        eff: u8,
        rd: u8,
        immediate: i32,
    ) -> Result<Self, InstructionError> {
        Self::ensure_format(opcode, Format::L)?;
        let immediate = encode_signed(immediate, 18, "imm18")? as u32;
        Self::new(
            opcode,
            eff,
            rd,
            ((immediate >> 14) & 0x0f) as u8,
            ((immediate >> 10) & 0x0f) as u8,
            (immediate & 0x03ff) as u16,
        )
    }

    pub fn j(opcode: Opcode, eff: u8, immediate: i32) -> Result<Self, InstructionError> {
        Self::ensure_format(opcode, Format::J)?;
        let immediate = encode_signed(immediate, 22, "imm22")? as u32;
        Self::new(
            opcode,
            eff,
            ((immediate >> 18) & 0x0f) as u8,
            ((immediate >> 14) & 0x0f) as u8,
            ((immediate >> 10) & 0x0f) as u8,
            (immediate & 0x03ff) as u16,
        )
    }

    pub fn b(opcode: Opcode, eff: u8, rs: u8, immediate: i32) -> Result<Self, InstructionError> {
        Self::ensure_format(opcode, Format::B)?;
        let immediate = encode_signed(immediate, 18, "imm18")? as u32;
        Self::new(
            opcode,
            eff,
            rs,
            ((immediate >> 14) & 0x0f) as u8,
            ((immediate >> 10) & 0x0f) as u8,
            (immediate & 0x03ff) as u16,
        )
    }

    pub fn m(
        opcode: Opcode,
        eff: u8,
        rd: u8,
        capability: u8,
        base_reg: u8,
        immediate: i32,
    ) -> Result<Self, InstructionError> {
        Self::ensure_format(opcode, Format::M)?;
        let immediate = encode_signed(immediate, 10, "imm10")? as u32;
        Self::new(
            opcode,
            eff,
            rd,
            capability,
            base_reg,
            immediate as u16,
        )
    }

    pub fn c(
        opcode: Opcode,
        eff: u8,
        a: u8,
        b: u8,
        c: u8,
        aux10: u16,
    ) -> Result<Self, InstructionError> {
        Self::ensure_format(opcode, Format::C)?;
        if aux10 > 0x03ff {
            return Err(InstructionError::FieldOutOfRange("aux10", aux10 as u32));
        }
        Self::new(opcode, eff, a, b, c, aux10)
    }

    pub const fn halt() -> Self {
        Self {
            opcode: Opcode::Halt,
            eff: 0,
            a: 0,
            b: 0,
            c: 0,
            imm10: 0,
        }
    }

    pub const fn nop() -> Self {
        Self {
            opcode: Opcode::Nop,
            eff: 0,
            a: 0,
            b: 0,
            c: 0,
            imm10: 0,
        }
    }

    pub const fn format(self) -> Format {
        self.opcode.format()
    }

    pub const fn opcode(self) -> Opcode {
        self.opcode
    }

    pub const fn eff(self) -> u8 {
        self.eff
    }

    pub const fn rd(self) -> u8 {
        self.a
    }

    pub const fn rs1(self) -> u8 {
        self.b
    }

    pub const fn rs2(self) -> u8 {
        self.c
    }

    pub const fn imm10(self) -> i32 {
        sign_extend(self.imm10 as u32, 10)
    }

    pub const fn imm14(self) -> i32 {
        sign_extend(((self.c as u32) << 10) | self.imm10 as u32, 14)
    }

    pub const fn imm18(self) -> i32 {
        sign_extend(
            ((self.b as u32) << 14) | ((self.c as u32) << 10) | self.imm10 as u32,
            18,
        )
    }

    pub const fn imm22(self) -> i32 {
        sign_extend(
            ((self.a as u32) << 18)
                | ((self.b as u32) << 14)
                | ((self.c as u32) << 10)
                | self.imm10 as u32,
            22,
        )
    }

    pub const fn cap(self) -> u8 {
        self.b & 0x07
    }

    pub const fn base_reg(self) -> u8 {
        self.c
    }

    pub const fn cd(self) -> u8 {
        self.a & 0x07
    }

    pub const fn cs(self) -> u8 {
        self.b & 0x07
    }

    pub const fn aux_reg(self) -> u8 {
        (self.imm10 & 0x0f) as u8
    }

    fn new(
        opcode: Opcode,
        eff: u8,
        a: u8,
        b: u8,
        c: u8,
        imm10: u16,
    ) -> Result<Self, InstructionError> {
        if eff > 0x0f {
            return Err(InstructionError::FieldOutOfRange("eff", eff as u32));
        }
        if a > 0x0f {
            return Err(InstructionError::FieldOutOfRange("A", a as u32));
        }
        if b > 0x0f {
            return Err(InstructionError::FieldOutOfRange("B", b as u32));
        }
        if c > 0x0f {
            return Err(InstructionError::FieldOutOfRange("C", c as u32));
        }
        if imm10 > 0x03ff {
            return Err(InstructionError::FieldOutOfRange("imm10", imm10 as u32));
        }
        Ok(Self {
            opcode,
            eff,
            a,
            b,
            c,
            imm10,
        })
    }

    fn ensure_format(opcode: Opcode, expected: Format) -> Result<(), InstructionError> {
        let actual = opcode.format();
        if actual == expected {
            Ok(())
        } else {
            Err(InstructionError::WrongFormat { opcode, expected, actual })
        }
    }
}

fn encode_signed(value: i32, bits: u32, field: &'static str) -> Result<u32, InstructionError> {
    let min = -(1_i64 << (bits - 1));
    let max = (1_i64 << (bits - 1)) - 1;
    let value = i64::from(value);
    if value < min || value > max {
        return Err(InstructionError::ImmediateOutOfRange { field, value: value as i32 });
    }
    Ok((value as i64 & ((1_i64 << bits) - 1)) as u32)
}

const fn sign_extend(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionError {
    UnknownOpcode(u8),
    WrongFormat {
        opcode: Opcode,
        expected: Format,
        actual: Format,
    },
    FieldOutOfRange(&'static str, u32),
    ImmediateOutOfRange { field: &'static str, value: i32 },
}

impl fmt::Display for InstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOpcode(opcode) => write!(formatter, "undefined opcode 0x{opcode:02x}"),
            Self::WrongFormat { opcode, expected, actual } => {
                write!(formatter, "opcode {opcode:?} requires {expected:?}, got {actual:?}")
            }
            Self::FieldOutOfRange(field, value) => write!(formatter, "{field} field value {value} is out of range"),
            Self::ImmediateOutOfRange { field, value } => {
                write!(formatter, "{field} immediate {value} is out of range")
            }
        }
    }
}

impl std::error::Error for InstructionError {}
