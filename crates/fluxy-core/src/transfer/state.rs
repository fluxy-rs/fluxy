//! Transfer lifecycle state machine (DESIGN §8.1).
//!
//! `TransferState` is the durable subset shared with the checkpoint file;
//! this module owns the legal transitions, including pause, cancel, and a
//! user-triggered retry. GUI code never rewrites a state directly: it asks the
//! core to make a transition and an illegal request is rejected instead of
//! silently changing history.

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode};

/// Lifecycle states for one transfer attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Preparing,
    Transferring,
    Paused,
    Interrupted,
    Resuming,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

impl TransferState {
    /// A completed or cancelled transfer never transitions again.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    /// States from which a user-triggered retry can restart the transfer.
    pub const fn can_retry(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Interrupted | Self::Cancelled | Self::Paused
        )
    }

    /// States in which a transfer is still owned by the engine.
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Transferring
                | Self::Paused
                | Self::Interrupted
                | Self::Resuming
                | Self::Verifying
        )
    }

    /// States that may be cancelled. A cancelled transfer is already clean.
    pub const fn can_cancel(self) -> bool {
        !matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// A rejected transition, with both states for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    #[error("cannot move a transfer from {from:?} to {to:?}")]
    IllegalTransition {
        from: TransferState,
        to: TransferState,
    },
}

impl StateError {
    pub const fn code(&self) -> ErrorCode {
        ErrorCode::Transfer
    }
}

impl From<StateError> for Error {
    fn from(value: StateError) -> Self {
        Error::new(value.code(), value.to_string(), false)
    }
}

/// Enforces the DESIGN §8.1 transitions for one transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateMachine {
    state: TransferState,
}

impl StateMachine {
    pub const fn new(state: TransferState) -> Self {
        Self { state }
    }

    pub const fn state(&self) -> TransferState {
        self.state
    }

    pub const fn can_transition(&self, next: TransferState) -> bool {
        transitions(self.state, next)
    }

    pub fn transition(&mut self, next: TransferState) -> Result<TransferState, StateError> {
        let from = self.state;
        let result = self.try_transition(next);
        // Every state change is part of a transfer's story; without this a run
        // that ends in the wrong state cannot be explained after the fact.
        match &result {
            Ok(_) => tracing::debug!(?from, ?next, "transfer state"),
            Err(error) => tracing::warn!(?from, ?next, %error, "transfer state rejected"),
        }
        result
    }

    fn try_transition(&mut self, next: TransferState) -> Result<TransferState, StateError> {
        if !self.can_transition(next) {
            return Err(StateError::IllegalTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(self.state)
    }

    /// `Transferring -> Paused`.
    pub fn pause(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Paused).map(|_| ())
    }

    /// `Paused -> Transferring`.
    pub fn resume(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Transferring).map(|_| ())
    }

    /// Mark an unexpected stop that a later resume can continue from.
    pub fn interrupt(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Interrupted).map(|_| ())
    }

    /// `Interrupted -> Resuming`.
    pub fn begin_resume(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Resuming).map(|_| ())
    }

    /// `Transferring -> Verifying`; the root-hash check runs in M2-06.
    pub fn begin_verify(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Verifying).map(|_| ())
    }

    /// `Verifying -> Completed`.
    pub fn complete(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Completed).map(|_| ())
    }

    /// `-> Failed` for a non-recoverable attempt.
    pub fn fail(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Failed).map(|_| ())
    }

    /// Cancel from any non-completed state. Cancelling an already cancelled
    /// transfer is an idempotent success so cleanup code can run twice.
    pub fn cancel(&mut self) -> Result<(), StateError> {
        if self.state == TransferState::Cancelled {
            return Ok(());
        }
        self.transition(TransferState::Cancelled).map(|_| ())
    }

    /// User-triggered retry; restarts an interrupted, failed, paused, or
    /// cancelled transfer from its checkpoint.
    pub fn retry(&mut self) -> Result<(), StateError> {
        self.transition(TransferState::Transferring).map(|_| ())
    }
}

/// Transition table for the transfer-phase subset of DESIGN §8.1.
const fn transitions(from: TransferState, to: TransferState) -> bool {
    use TransferState::*;
    if matches!(from, Completed) {
        return false;
    }
    match from {
        Preparing => matches!(to, Transferring | Cancelled),
        Transferring => matches!(to, Paused | Interrupted | Verifying | Failed | Cancelled),
        Paused => matches!(to, Transferring | Interrupted | Failed | Cancelled),
        Interrupted => matches!(to, Resuming | Transferring | Failed | Cancelled),
        Resuming => matches!(to, Transferring | Interrupted | Failed | Cancelled),
        Verifying => matches!(to, Completed | Failed | Cancelled),
        Failed => matches!(to, Transferring | Cancelled),
        Cancelled => matches!(to, Transferring),
        Completed => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_reaches_completed() {
        let mut machine = StateMachine::new(TransferState::Preparing);
        machine.transition(TransferState::Transferring).unwrap();
        machine.pause().unwrap();
        machine.resume().unwrap();
        machine.interrupt().unwrap();
        machine.begin_resume().unwrap();
        machine.transition(TransferState::Transferring).unwrap();
        machine.begin_verify().unwrap();
        machine.complete().unwrap();
        assert_eq!(machine.state(), TransferState::Completed);
        assert!(machine.state().is_terminal());
    }

    #[test]
    fn rejects_illegal_transitions() {
        let mut machine = StateMachine::new(TransferState::Transferring);
        assert!(matches!(
            machine.transition(TransferState::Completed),
            Err(StateError::IllegalTransition { .. })
        ));
        let mut paused = StateMachine::new(TransferState::Paused);
        assert!(paused.begin_verify().is_err());
        let mut completed = StateMachine::new(TransferState::Completed);
        assert!(completed.retry().is_err());
        assert!(completed.cancel().is_err());
        assert!(!completed.can_transition(TransferState::Transferring));
    }

    #[test]
    fn every_active_state_can_be_cancelled() {
        for state in [
            TransferState::Preparing,
            TransferState::Transferring,
            TransferState::Paused,
            TransferState::Interrupted,
            TransferState::Resuming,
            TransferState::Verifying,
            TransferState::Failed,
        ] {
            assert!(state.can_cancel(), "{state:?} must be cancellable");
            let mut machine = StateMachine::new(state);
            machine.cancel().unwrap();
            assert_eq!(machine.state(), TransferState::Cancelled);
            // Cancelling twice is idempotent so cleanup can run again.
            machine.cancel().unwrap();
            assert_eq!(machine.state(), TransferState::Cancelled);
        }
    }

    #[test]
    fn retry_restarts_failed_interrupted_and_cancelled() {
        for state in [
            TransferState::Failed,
            TransferState::Interrupted,
            TransferState::Cancelled,
            TransferState::Paused,
        ] {
            assert!(state.can_retry(), "{state:?} must be retryable");
            let mut machine = StateMachine::new(state);
            machine.retry().unwrap();
            assert_eq!(machine.state(), TransferState::Transferring);
        }
        assert!(!TransferState::Completed.can_retry());
        assert!(!TransferState::Preparing.can_retry());
    }

    #[test]
    fn state_error_maps_to_a_stable_code() {
        let error = StateError::IllegalTransition {
            from: TransferState::Completed,
            to: TransferState::Transferring,
        };
        assert_eq!(error.code(), ErrorCode::Transfer);
        let converted: Error = error.into();
        assert_eq!(converted.code_number, 3001);
        assert!(!converted.retryable);
    }
}
