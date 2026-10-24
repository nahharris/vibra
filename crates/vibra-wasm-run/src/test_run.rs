//! Running one test of a test module in a fresh instance.
//!
//! `docs/spec/06-runtime.md`, "WebAssembly boundary": `vibra_v1_test` runs the
//! test at a zero-based position in canonical discovery order, and a host runs
//! each test in a fresh instance. A call that returns has passed; a call that
//! stops did so because generated code recorded a status first. A false
//! assertion is status `3`, with the assertion and the origin ordinal of its
//! call, and, for `assert.equal`, the two operands, which the host reads by the
//! type the origin table records and renders with the one canonical encoder the
//! results use. A trap is status `1`, and the memory host event status `2`.

use vibra_host::{AuditEvent, HostProvider, HostWorld, WorldProvider};
use vibra_ir::boundary::{Failure, TEST_EXPORT, TrapCode};
use vibra_ir::{Type, TypeDefinition, Value};

use crate::{
    Instance, LiveSizes, Outcome, ResultSlot, Runner, RunnerError, Started,
    export_error, interpret,
};

/// How one test ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestRun {
    /// The test completed. `live` is the live arena size before it ran and
    /// after it completed.
    Passed {
        /// The live sizes around the run.
        live: LiveSizes,
        /// The events of the host calls the test completed, in call order.
        audit_trace: Vec<AuditEvent>,
    },
    /// An assertion was false, and stopped only this test.
    AssertionFailed {
        /// The assertion that failed.
        failure: Failure,
        /// The origin ordinal of the assertion call, `None` for none.
        origin: Option<u32>,
        /// The canonical encoding of the expected operand, or of the required
        /// boolean.
        expected: String,
        /// The canonical encoding of the actual operand.
        actual: String,
        /// The events of the host calls the test completed before it failed.
        audit_trace: Vec<AuditEvent>,
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
    /// reported as `@runtime.invalid-checked-program` with no origin.
    Defect {
        /// What the runner saw.
        cause: String,
    },
}

impl Instance {
    /// Calls `vibra_v1_test` for the test at `index` and reads what the module
    /// recorded.
    ///
    /// # Errors
    ///
    /// [`RunnerError::Export`] when the module lacks the export.
    pub fn call_test(&mut self, index: u32) -> Result<Outcome, RunnerError> {
        let (store, instance) = self.parts();
        let test = instance
            .get_typed_func::<i32, ()>(&mut *store, TEST_EXPORT)
            .map_err(|error| export_error(TEST_EXPORT, &error))?;
        let call = test.call(&mut *store, index.cast_signed());
        let stop = call.err().map(|error| format!("{error:#}"));
        interpret(store, instance, stop)
    }
}

impl Runner {
    /// Validates and instantiates `bytes` as a fresh instance, runs the test at
    /// `index`, and reads how it ended. `types` are the declared types of the
    /// program, `heads` names the type a head of the module's descriptors
    /// stands for, and `operand_of` maps the origin ordinal of an assertion call
    /// to the type its operands are read by.
    ///
    /// # Errors
    ///
    /// A [`RunnerError`] when the module is not a v1 module or lacks an export.
    pub fn run_test(
        &self,
        bytes: &[u8],
        index: u32,
        types: &[TypeDefinition],
        heads: &dyn Fn(u32) -> Option<Type>,
        operand_of: &dyn Fn(u32) -> Option<Type>,
    ) -> Result<TestRun, RunnerError> {
        self.run_test_with_host(
            bytes,
            index,
            types,
            heads,
            operand_of,
            Box::new(WorldProvider::new(&HostWorld::default())),
        )
    }

    /// [`Self::run_test`] with `provider` as the test's host, a fresh one for
    /// each test.
    ///
    /// # Errors
    ///
    /// A [`RunnerError`] when the module is not a v1 module or lacks an export.
    pub fn run_test_with_host(
        &self,
        bytes: &[u8],
        index: u32,
        types: &[TypeDefinition],
        heads: &dyn Fn(u32) -> Option<Type>,
        operand_of: &dyn Fn(u32) -> Option<Type>,
        provider: Box<dyn HostProvider>,
    ) -> Result<TestRun, RunnerError> {
        let mut instance = match self.start_with_host(bytes, provider)? {
            Started::MemoryExhausted => return Ok(TestRun::MemoryExhausted),
            Started::Ready(instance) => instance,
        };
        let start = match instance.live_size() {
            Ok(start) => start,
            Err(stop) => return Ok(stopped(stop)),
        };
        match instance.call_test(index)? {
            Outcome::Completed { live_size, .. } => Ok(TestRun::Passed {
                live: LiveSizes {
                    start,
                    with_result: live_size,
                    end: live_size,
                },
                audit_trace: instance.audit_trace().to_vec(),
            }),
            Outcome::AssertionFailed {
                failure,
                origin,
                operands,
            } => {
                let mut run = read_failure(
                    &mut instance,
                    failure,
                    origin,
                    operands,
                    types,
                    heads,
                    operand_of,
                );
                if let TestRun::AssertionFailed { audit_trace, .. } = &mut run {
                    *audit_trace = instance.audit_trace().to_vec();
                }
                Ok(run)
            }
            other => Ok(stopped(other)),
        }
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

fn read_failure(
    instance: &mut Instance,
    failure: Failure,
    origin: Option<u32>,
    operands: Option<(u64, u64)>,
    types: &[TypeDefinition],
    heads: &dyn Fn(u32) -> Option<Type>,
    operand_of: &dyn Fn(u32) -> Option<Type>,
) -> TestRun {
    let defect = |cause: String| TestRun::Defect { cause };
    let bool_text = |value: bool| Value::Bool(value).canonical_vibon();
    match failure {
        // A false `assert.true` required `true` and was given `false`.
        Failure::True => TestRun::AssertionFailed {
            failure,
            origin,
            expected: bool_text(true),
            actual: bool_text(false),
            audit_trace: Vec::new(),
        },
        Failure::False => TestRun::AssertionFailed {
            failure,
            origin,
            expected: bool_text(false),
            actual: bool_text(true),
            audit_trace: Vec::new(),
        },
        Failure::Equal => {
            let (Some((expected, actual)), Some(ordinal)) = (operands, origin) else {
                return defect(
                    "a failed `assert.equal` recorded no operands".to_owned(),
                );
            };
            let Some(ty) = operand_of(ordinal) else {
                return defect(format!(
                    "the origin table records no operand type at the ordinal {ordinal}"
                ));
            };
            let mut read = |bits: u64| {
                instance
                    .observe_with(ResultSlot::from_bits(bits), &ty, types, heads)
                    .map(|value| value.canonical_vibon())
            };
            let expected = match read(expected) {
                Ok(text) => text,
                Err(stop) => return stopped(stop),
            };
            let actual = match read(actual) {
                Ok(text) => text,
                Err(stop) => return stopped(stop),
            };
            TestRun::AssertionFailed {
                failure,
                origin,
                expected,
                actual,
                audit_trace: Vec::new(),
            }
        }
    }
}
