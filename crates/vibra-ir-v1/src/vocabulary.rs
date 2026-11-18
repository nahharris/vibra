//! The variant names of the closed `std` types that every backend builds and
//! inspects: `option`, `result`, and the ordering `std.core.compare` returns.

use std::cmp::Ordering;

/// The `option` variant that holds a value.
pub const SOME: &str = "some";
/// The `option` variant that holds none.
pub const NONE: &str = "none";
/// The `result` variant that holds a success.
pub const OK: &str = "ok";
/// The `result` variant that holds a failure.
pub const ERR: &str = "err";
/// The ordering of a left operand before its right.
pub const LESS: &str = "less";
/// The ordering of equal operands.
pub const EQUAL: &str = "equal";
/// The ordering of a left operand after its right.
pub const GREATER: &str = "greater";

/// The ordering variants, in the order of their [`Ordering`] counterparts.
pub const ORDERING: [&str; 3] = [LESS, EQUAL, GREATER];

/// The `option` variant for a value that is present or absent.
#[must_use]
pub const fn option_variant(present: bool) -> &'static str {
    if present { SOME } else { NONE }
}

/// The `result` variant for a success or a failure.
#[must_use]
pub const fn result_variant(success: bool) -> &'static str {
    if success { OK } else { ERR }
}

/// The ordering variant that spells `order`.
#[must_use]
pub const fn ordering_variant(order: Ordering) -> &'static str {
    match order {
        Ordering::Less => LESS,
        Ordering::Equal => EQUAL,
        Ordering::Greater => GREATER,
    }
}

/// The ordering a variant name spells.
#[must_use]
pub fn ordering_from_variant(variant: &str) -> Option<Ordering> {
    match variant {
        LESS => Some(Ordering::Less),
        EQUAL => Some(Ordering::Equal),
        GREATER => Some(Ordering::Greater),
        _ => None,
    }
}
