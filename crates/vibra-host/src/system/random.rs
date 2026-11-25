//! The random adapter: `random.generateBytes`, from the operating system's
//! generator.
//!
//! `docs/spec/06-runtime.md`, "Clocks and randomness": the call returns
//! exactly `count` bytes, or `unavailable` when there is no generator. The
//! broker reserved `count` against `inputBytes`, and compared it with the
//! caller's room, before this adapter is called, so the buffer it allocates
//! is one the caller can account.

use vibra_ir::authority::HostOk;

use super::{Answer, moved};
use crate::driver::{Measured, Operand, Progress};

/// Whether the operating system has a generator: one byte is drawn and
/// discarded. The failure's text when it has none.
pub(super) fn probe() -> Result<(), String> {
    let mut byte = [0_u8; 1];
    getrandom::fill(&mut byte)
        .map_err(|error| format!("the operating system has no random source: {error}"))
}

/// `random.generateBytes` with the operand `operand`.
pub(super) fn generate(operand: Operand<'_>) -> Answer {
    let Operand::Count(count) = operand else {
        return Answer::NotSubmitted("unsupported");
    };
    let Some(mut bytes) = usize::try_from(count).ok().and_then(|count| {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(count).ok()?;
        bytes.resize(count, 0);
        Some(bytes)
    }) else {
        // Nothing was drawn: the host could not hold the bytes the broker
        // admitted.
        return Answer::NotSubmitted("unsupported");
    };
    match getrandom::fill(&mut bytes) {
        Ok(()) => Answer::Ok(HostOk::Bytes(bytes), moved(count, 0)),
        // The generator may have produced part of the count before it failed.
        Err(_) => Answer::Err(
            "unavailable",
            Progress {
                input: Measured::Uncertain,
                output: Measured::Known(0),
            },
        ),
    }
}
