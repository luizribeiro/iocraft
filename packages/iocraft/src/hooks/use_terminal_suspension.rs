use super::terminal_suspensions::TerminalSuspensions;
use crate::{hooks::UseContext, Hooks};
use std::sync::{Arc, Weak};

mod private {
    pub trait Sealed {}
    impl Sealed for crate::Hooks<'_, '_> {}
}

/// Lets components temporarily give another program control of the terminal.
pub trait UseTerminalSuspension: private::Sealed {
    /// Gets a handle that can request terminal suspension from an event handler.
    fn use_terminal_suspension(&self) -> TerminalSuspensionHandle;
}

impl UseTerminalSuspension for Hooks<'_, '_> {
    fn use_terminal_suspension(&self) -> TerminalSuspensionHandle {
        TerminalSuspensionHandle(
            self.try_use_context::<Arc<TerminalSuspensions>>()
                .map(|queue| Arc::downgrade(&queue))
                .unwrap_or_default(),
        )
    }
}

/// Requests terminal suspension from a running terminal render loop.
#[derive(Clone)]
pub struct TerminalSuspensionHandle(Weak<TerminalSuspensions>);

impl TerminalSuspensionHandle {
    /// Queues a callback that runs with terminal input and rendering paused.
    ///
    /// The built-in terminal backend releases the terminal before running the callback and
    /// reclaims it for a full redraw afterward. A custom backend must implement
    /// [`TerminalBackend::suspend`](crate::backend::TerminalBackend::suspend) and
    /// [`TerminalBackend::resume`](crate::backend::TerminalBackend::resume) to release and
    /// reclaim the terminal modes it manages.
    ///
    /// Returns `true` if the callback was queued, or `false` if the loop is unavailable or
    /// already has a pending callback. The render loop discards pending callbacks on exit.
    /// Once started, the callback blocks the executor thread until it returns. Errors releasing
    /// or reclaiming the terminal are returned by the render loop. If release fails, the callback
    /// is skipped.
    pub fn suspend_terminal(&self, callback: impl FnOnce() + Send + 'static) -> bool {
        let Some(suspensions) = self.0.upgrade() else {
            return false;
        };
        suspensions.request(callback)
    }
}
