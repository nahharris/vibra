//! The dispatcher and the standard set of handlers.

use std::collections::BTreeMap;
use std::fmt;

use crate::corpus::Case;
use crate::profile::{
    CaseObservation, ConformanceProfile, HandlerError, ProfileHandler,
};
use crate::reader::ReaderV2Handler;

/// Selects the closest registered handler that can run each case.
#[derive(Default)]
pub struct ProfileDispatcher {
    handlers: BTreeMap<ConformanceProfile, Vec<Box<dyn ProfileHandler>>>,
}

type HandlerCandidate<'a> = (ConformanceProfile, &'a dyn ProfileHandler);
type HandlerSelection<'a> =
    Result<Option<HandlerCandidate<'a>>, (ConformanceProfile, String)>;

impl fmt::Debug for ProfileDispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileDispatcher")
            .field("profiles", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ProfileDispatcher {
    /// A dispatcher with no handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a handler under `profile`, beside the handlers already there.
    pub fn register<H>(&mut self, profile: ConformanceProfile, handler: H)
    where
        H: ProfileHandler + 'static,
    {
        self.handlers
            .entry(profile)
            .or_default()
            .push(Box::new(handler));
    }

    /// Builder form of [`Self::register`].
    #[must_use]
    pub fn with_handler<H>(mut self, profile: ConformanceProfile, handler: H) -> Self
    where
        H: ProfileHandler + 'static,
    {
        self.register(profile, handler);
        self
    }

    /// Dispatches one case to the closest handler that can run it.
    ///
    /// The candidates are the handlers whose profile includes the case's and
    /// that claim the case; the closest is the one whose profile is the
    /// fewest levels above the case's. Two closest candidates are a failure,
    /// never a choice.
    #[must_use]
    pub fn dispatch(&self, case: &Case) -> DispatchResult {
        let required = case.manifest().profile;
        let selection = match self.best_handler(required, case) {
            Ok(selection) => selection,
            Err((provided, reason)) => {
                return DispatchResult::Failed {
                    required,
                    provided,
                    error: HandlerError::new(reason),
                };
            }
        };
        let Some((provided, handler)) = selection else {
            return DispatchResult::Unavailable {
                required,
                reason: format!(
                    "no handler runs operation `{}` under {required}",
                    case.manifest().operation
                ),
            };
        };
        match handler.run(case) {
            Ok(observation) => DispatchResult::Executed {
                required,
                provided,
                observation: Box::new(observation),
            },
            Err(error) => DispatchResult::Failed {
                required,
                provided,
                error,
            },
        }
    }

    fn best_handler(
        &self,
        required: ConformanceProfile,
        case: &Case,
    ) -> HandlerSelection<'_> {
        let candidates = self
            .handlers
            .iter()
            .filter(|(profile, _)| profile.supports(required))
            .flat_map(|(profile, handlers)| {
                handlers
                    .iter()
                    .filter(|handler| handler.can_run(case))
                    .map(move |handler| (*profile, handler.as_ref()))
            })
            .collect::<Vec<_>>();
        let distance = |profile: ConformanceProfile| {
            profile.depth().saturating_sub(required.depth())
        };
        let Some(closest) = candidates
            .iter()
            .map(|(profile, _)| distance(*profile))
            .min()
        else {
            return Ok(None);
        };
        let mut matching = candidates
            .into_iter()
            .filter(|(profile, _)| distance(*profile) == closest);
        let Some(first) = matching.next() else {
            return Ok(None);
        };
        if matching.next().is_some() {
            return Err((
                first.0,
                format!(
                    "several handlers claim operation `{}` at profile distance {closest}",
                    case.manifest().operation
                ),
            ));
        }
        Ok(Some(first))
    }
}

/// The result of dispatching one case, before comparison.
#[derive(Debug)]
pub enum DispatchResult {
    /// A handler returned observations.
    Executed {
        /// The case's profile.
        required: ConformanceProfile,
        /// The profile of the handler that ran it.
        provided: ConformanceProfile,
        /// The observations.
        observation: Box<CaseObservation>,
    },
    /// No registered handler runs the case.
    Unavailable {
        /// The case's profile.
        required: ConformanceProfile,
        /// Why no handler was selected.
        reason: String,
    },
    /// The selected handler failed, or the selection was ambiguous.
    Failed {
        /// The case's profile.
        required: ConformanceProfile,
        /// The profile of the handler that failed.
        provided: ConformanceProfile,
        /// The failure.
        error: HandlerError,
    },
}

/// Every handler the workspace has, registered under its profile.
///
/// The reader runs under `readerV2`; every other profile has no handler yet.
/// The binary and the host tests build this one dispatcher, so the two cannot
/// disagree about which handlers exist.
#[must_use]
pub fn standard_dispatcher() -> ProfileDispatcher {
    ProfileDispatcher::new().with_handler(ConformanceProfile::ReaderV2, ReaderV2Handler)
}
