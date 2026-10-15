//! Acts on a running session while `Sandbox::wait()` holds it mutably.

use std::sync::Arc;

use crate::error::{SandboxRuntimeError, SandlockError};
use crate::result::ExitStatus;

#[derive(Clone)]
pub struct Controller {
    pub(crate) groups: Arc<crate::pgroup::ProcessGroups>,
    pub(crate) stop: tokio_util::sync::CancellationToken,
    pub(crate) main_exit: tokio::sync::watch::Receiver<Option<ExitStatus>>,
}

impl Controller {
    pub fn kill(&self) -> Result<(), SandlockError> {
        self.signal(libc::SIGKILL).map(|_| ())
    }

    pub fn pause(&self) -> Result<(), SandlockError> {
        self.signal_live(libc::SIGSTOP)
    }

    pub fn resume(&self) -> Result<(), SandlockError> {
        self.signal_live(libc::SIGCONT)
    }

    /// Sorted pgids that still have members; may change as soon as it returns.
    pub fn process_groups(&self) -> Result<Vec<i32>, SandlockError> {
        Ok(self.groups.live().map_err(SandboxRuntimeError::Io)?)
    }

    pub fn main_exited(&self) -> bool {
        self.main_exit.borrow().is_some()
    }

    /// Signal every group, returning how many had members left.
    pub fn signal(&self, sig: i32) -> Result<usize, SandlockError> {
        if sig == libc::SIGKILL {
            self.stop.cancel();
        }
        Ok(self.groups.signal(sig).map_err(SandboxRuntimeError::Io)?)
    }

    fn signal_live(&self, sig: i32) -> Result<(), SandlockError> {
        if self.signal(sig)? == 0 {
            return Err(SandboxRuntimeError::Io(std::io::Error::from_raw_os_error(libc::ESRCH)).into());
        }
        Ok(())
    }
}
