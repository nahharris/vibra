//! The simulation scheduler: one thread, virtual time, and every decision
//! taken from a recorded list or a seeded generator.
//!
//! A schedule is the sequence of [`Decision`]s taken at kernel actions and
//! nothing else: it holds no quota and no step counts.

use crate::executor::Executor;
use crate::kernel::{Decision, DecisionError, Kernel, StepError, View};

/// A small deterministic generator (SplitMix64). It is not for secrets.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator that produces the same sequence for the same seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..bound`; 0 when `bound` is 0.
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            0
        } else {
            self.next_u64() % bound
        }
    }
}

/// Where the scheduler takes its decisions from.
pub trait DecisionSource {
    /// The next decision, given what the kernel shows; `None` when the source
    /// has no more. Called only while some actor is runnable or some wait has
    /// a deadline.
    fn next(&mut self, view: &View) -> Option<Decision>;

    /// How many decisions the source still holds. A generator holds none.
    fn remaining(&self) -> usize {
        0
    }
}

/// Decisions replayed from a recorded list, in order.
#[derive(Clone, Debug)]
pub struct Recorded {
    decisions: Vec<Decision>,
    next: usize,
}

impl Recorded {
    /// A source that yields `decisions` once each, in order.
    #[must_use]
    pub fn new(decisions: Vec<Decision>) -> Self {
        Self { decisions, next: 0 }
    }
}

impl DecisionSource for Recorded {
    fn next(&mut self, _view: &View) -> Option<Decision> {
        let decision = self.decisions.get(self.next).copied();
        if decision.is_some() {
            self.next += 1;
        }
        decision
    }

    fn remaining(&self) -> usize {
        self.decisions.len().saturating_sub(self.next)
    }
}

/// Decisions drawn from a seeded generator. The same seed against the same
/// kernel gives the same decisions.
#[derive(Clone, Debug)]
pub struct Seeded {
    rng: SplitMix64,
}

impl Seeded {
    /// A generator seeded with `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::new(seed),
        }
    }
}

impl DecisionSource for Seeded {
    fn next(&mut self, view: &View) -> Option<Decision> {
        let gap = view
            .next_deadline
            .map(|deadline| deadline.0.saturating_sub(view.now.0).max(1));
        let advance = match gap {
            Some(gap) if view.runnable.is_empty() => Some(gap),
            Some(gap) if self.rng.below(8) == 0 => Some(gap),
            _ => None,
        };
        if let Some(gap) = advance {
            return Some(Decision::Advance(1 + self.rng.below(gap)));
        }
        let pick = self
            .rng
            .below(u64::try_from(view.runnable.len()).unwrap_or(0));
        let pick = usize::try_from(pick).unwrap_or(0);
        view.runnable.get(pick).copied().map(Decision::Run)
    }
}

/// Why a run ended without error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// Every actor exited.
    Completed,
    /// Nothing can run and no wait has a deadline, but some actor still waits.
    Blocked,
    /// The source had no more decisions while the run could go on.
    ScheduleExhausted,
    /// The decision limit was reached.
    LimitReached,
}

/// What a run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// Why the run ended.
    pub end: RunEnd,
    /// The decisions taken, in order; feeding them to [`Recorded`] against an
    /// identical kernel reproduces the run.
    pub decisions: Vec<Decision>,
}

/// Why a run failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// A decision named something that cannot be done. From a recorded source
    /// this is an invalid replay.
    InvalidDecision {
        /// The position of the decision in the run.
        index: usize,
        /// Why it was refused.
        error: DecisionError,
    },
    /// The run finished but the recorded source still held decisions. From a
    /// recorded source this is an invalid replay.
    SurplusDecisions {
        /// How many were left.
        remaining: usize,
    },
    /// An actor made an invalid request, or the kernel failed.
    Step(StepError),
}

/// Runs `kernel` until it completes, blocks, or the source or limit ends it.
///
/// `limit` caps the number of decisions taken. It is a guard against a
/// program that never ends and is not recorded in the schedule.
pub fn run<E: Executor, S: DecisionSource>(
    kernel: &mut Kernel<E>,
    source: &mut S,
    limit: u64,
) -> Result<Report, RunError> {
    run_observed(kernel, source, limit, &mut |_| {})
}

/// Like [`run`], and calls `observer` after every decision.
pub fn run_observed<E: Executor, S: DecisionSource>(
    kernel: &mut Kernel<E>,
    source: &mut S,
    limit: u64,
    observer: &mut dyn FnMut(&Kernel<E>),
) -> Result<Report, RunError> {
    let mut decisions = Vec::new();
    let end = loop {
        let view = kernel.view();
        if view.runnable.is_empty() && view.next_deadline.is_none() {
            break if kernel.waiting().is_empty() {
                RunEnd::Completed
            } else {
                RunEnd::Blocked
            };
        }
        if u64::try_from(decisions.len()).unwrap_or(u64::MAX) >= limit {
            break RunEnd::LimitReached;
        }
        let Some(decision) = source.next(&view) else {
            break RunEnd::ScheduleExhausted;
        };
        match kernel.step(decision) {
            Ok(()) => {}
            Err(StepError::Decision(error)) => {
                return Err(RunError::InvalidDecision {
                    index: decisions.len(),
                    error,
                });
            }
            Err(other) => return Err(RunError::Step(other)),
        }
        decisions.push(decision);
        observer(kernel);
    };
    if matches!(end, RunEnd::Completed | RunEnd::Blocked) && source.remaining() > 0 {
        // Nothing is left to do, so the next recorded decision either names
        // something that cannot be done, or is surplus.
        if let Some(decision) = source.next(&kernel.view())
            && let Err(error) = kernel.check(decision)
        {
            return Err(RunError::InvalidDecision {
                index: decisions.len(),
                error,
            });
        }
        return Err(RunError::SurplusDecisions {
            remaining: source.remaining().saturating_add(1),
        });
    }
    Ok(Report { end, decisions })
}
