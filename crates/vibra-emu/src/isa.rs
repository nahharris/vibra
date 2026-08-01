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
