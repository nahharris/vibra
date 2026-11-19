//! Running one test of a test module in a fresh instance.
//!
//! `docs/spec/06-runtime.md`, "Wasm transition": `vibra_v1_test` runs the
//! test at a zero-based position in canonical discovery order, and a host
//! runs each test in a fresh instance. A call that returns has passed; a call
//! that stops did so because generated code recorded a status first. A false
//! assertion is status `3`, with the assertion and the origin ordinal of its
//! call, and, for `assert.equal`, the two operands, which the host reads by
//! the type the origin table records and renders with the one canonical
//! encoder of `vibra-ir`. A trap is status `1`, and the memory host event
//! status `2`.

use vibra_ir::boundary::{Failure, TrapCode};
use vibra_ir::{Type, TypeTable, encode_value};

use crate::observe::defect;
use crate::{LiveSizes, Outcome, ResultSlot, Runner, RunnerError, Started};

/// How one test ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestRun {
    /// The test completed.
    Passed {
        /// The live arena size before the test ran and after it completed.
        live: LiveSizes,
    },
    /// An assertion was false, and stopped only this test.
    AssertionFailed {
        /// The assertion that failed.
        failure: Failure,
        /// The origin ordinal of the assertion call, `None` for none.
        origin: Option<u32>,
        /// The canonical value encoding of `assert.equal`'s expected operand.
        expected: Option<String>,
        /// The canonical value encoding of `assert.equal`'s actual operand.
        actual: Option<String>,
    },
    /// The test stopped with a registered trap.
    Trapped {
        /// The trap's code.
        code: TrapCode,
        /// The origin ordinal, `None` for none.
        origin: Option<u32>,
    },
    /// The instance exhausted its memory: the host event.
    MemoryExhausted,
    /// The protocol was broken, which is the toolchain's defect and is
    /// reported as `@runtime.invalidCheckedProgram` with no origin.
    Defect {
        /// What the runner saw.
        cause: String,
    },
}

impl Runner {
    /// Validates and instantiates `bytes` as a fresh instance, runs the test
    /// at `index`, and reads how it ended. `types` are the declared types of
    /// the program and the library bindings, and `operand_of` maps the origin
    /// ordinal of an assertion call to the type its operands are read by.
    ///
    /// # Errors
    ///
    /// A [`RunnerError`] when the module is not a transition module or lacks
    /// an export.
    pub fn run_test(
        &self,
        bytes: &[u8],
        index: u32,
        types: &TypeTable,
        operand_of: &dyn Fn(u32) -> Option<Type>,
    ) -> Result<TestRun, RunnerError> {
        let mut instance = match self.start(bytes)? {
            Started::MemoryExhausted => return Ok(TestRun::MemoryExhausted),
            Started::Ready(instance) => instance,
        };
        let start = match instance.live_size() {
            Ok(start) => start,
            Err(stop) => return Ok(stopped(stop)),
        };
        Ok(match instance.call_test(index)? {
            Outcome::Completed { live_size, .. } => TestRun::Passed {
                live: LiveSizes {
                    start,
                    with_result: live_size,
                    end: live_size,
                },
            },
            Outcome::AssertionFailed {
                failure,
                origin,
                operands,
            } => {
                let Some((expected, actual)) = operands else {
                    return Ok(TestRun::AssertionFailed {
                        failure,
                        origin,
                        expected: None,
                        actual: None,
                    });
                };
                let Some(ty) = origin.and_then(operand_of) else {
                    return Ok(TestRun::Defect {
                        cause: format!(
                            "the origin table records no operand type at {origin:?}"
                        ),
                    });
                };
                let mut read = |bits: u64| {
                    let value =
                        instance.observe(ResultSlot::from_bits(bits), &ty, types)?;
                    encode_value(&value, &ty, types).map_err(|error| {
                        defect(format!("an operand has no encoding: {error}"))
                    })
                };
                let expected = match read(expected) {
                    Ok(text) => text,
                    Err(stop) => return Ok(stopped(stop)),
                };
                let actual = match read(actual) {
                    Ok(text) => text,
                    Err(stop) => return Ok(stopped(stop)),
                };
                TestRun::AssertionFailed {
                    failure,
                    origin,
                    expected: Some(expected),
                    actual: Some(actual),
                }
            }
            other => stopped(other),
        })
    }
}

/// A stop that is not a failed assertion.
fn stopped(outcome: Outcome) -> TestRun {
    match outcome {
        Outcome::Trapped { code, origin } => TestRun::Trapped { code, origin },
        Outcome::MemoryExhausted => TestRun::MemoryExhausted,
        Outcome::Defect { cause } => TestRun::Defect { cause },
        Outcome::Completed { .. } | Outcome::AssertionFailed { .. } => {
            TestRun::Defect {
                cause: "a call was read twice".to_owned(),
            }
        }
    }
}
