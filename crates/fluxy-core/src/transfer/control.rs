//! Shared pause/cancel control for one transfer.
//!
//! The transfer loops poll this handle on every tick, so a pause or cancel is
//! observed within one `TRANSFER_TICK` (100 ms). The handle is cloneable and
//! shares one lock-protected state; it has no async runtime dependency, so the
//! core stays usable from the GUI thread.

use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

/// User-requested control state for a running transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlState {
    Running,
    Paused,
    Cancelled,
}

#[derive(Debug)]
struct ControlInner {
    state: Mutex<ControlState>,
}

/// Cloneable handle used by the UI to pause, resume, or cancel a transfer.
#[derive(Debug, Clone)]
pub struct TransferControl {
    inner: Arc<ControlInner>,
}

impl Default for TransferControl {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferControl {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ControlInner {
                state: Mutex::new(ControlState::Running),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ControlState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn state(&self) -> ControlState {
        *self.lock()
    }

    pub fn is_running(&self) -> bool {
        self.state() == ControlState::Running
    }

    pub fn is_paused(&self) -> bool {
        self.state() == ControlState::Paused
    }

    pub fn is_cancelled(&self) -> bool {
        self.state() == ControlState::Cancelled
    }

    /// Pause a running transfer. Returns false when it is already paused or
    /// cancelled, so a repeated click is a no-op.
    pub fn pause(&self) -> bool {
        let mut state = self.lock();
        if *state == ControlState::Running {
            *state = ControlState::Paused;
            true
        } else {
            false
        }
    }

    /// Resume a paused transfer. Returns false when it is not paused.
    pub fn resume(&self) -> bool {
        let mut state = self.lock();
        if *state == ControlState::Paused {
            *state = ControlState::Running;
            true
        } else {
            false
        }
    }

    /// Cancel from any state; cancel always wins over a pause. Returns false
    /// only when the transfer was already cancelled.
    pub fn cancel(&self) -> bool {
        let mut state = self.lock();
        if *state == ControlState::Cancelled {
            false
        } else {
            *state = ControlState::Cancelled;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_and_resume_are_reversible() {
        let control = TransferControl::new();
        assert!(control.is_running());
        assert!(control.pause());
        assert!(control.is_paused());
        assert!(!control.pause());
        assert!(control.resume());
        assert!(control.is_running());
        assert!(!control.resume());
    }

    #[test]
    fn cancel_wins_over_pause_and_is_idempotent() {
        let control = TransferControl::new();
        assert!(control.pause());
        assert!(control.cancel());
        assert!(control.is_cancelled());
        assert!(!control.cancel());
        assert!(!control.resume());
        assert!(!control.pause());
    }

    #[test]
    fn clones_share_one_state() {
        let control = TransferControl::new();
        let clone = control.clone();
        assert!(clone.pause());
        assert!(control.is_paused());
        assert!(control.cancel());
        assert!(clone.is_cancelled());
        assert_eq!(clone.state(), ControlState::Cancelled);
    }
}
