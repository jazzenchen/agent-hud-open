//! While Windows ends the session, background work starts no more helper
//! programs.
use std::{
    io,
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, PoisonError,
    },
};

struct SpawnGate {
    ending: AtomicBool,
    launching: Mutex<()>,
}

impl SpawnGate {
    const fn new() -> Self {
        Self {
            ending: AtomicBool::new(false),
            launching: Mutex::new(()),
        }
    }

    fn set_ending(&self, ending: bool) {
        let _launching = self
            .launching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.ending.store(ending, Ordering::Release);
    }

    fn run<T>(
        &self,
        system_ending: impl FnOnce() -> bool,
        launch: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        // The last check and the spawn hold the same lock; checking only while
        // the command is configured would leave a gap.
        let _launching = self
            .launching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.ending.load(Ordering::Acquire) || system_ending() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Windows is ending the session; the helper program was not started",
            ));
        }
        launch()
    }
}

static GATE: SpawnGate = SpawnGate::new();

/// Pauses when Windows asks to end the session and resumes when the shutdown
/// is cancelled; reading is never switched off for good.
pub fn set_ending(ending: bool) {
    GATE.set_ending(ending);
}

/// The system's own flag backs this up, and covers command-line runs that make
/// no desktop window.
pub fn is_ending() -> bool {
    GATE.ending.load(Ordering::Acquire) || system_is_ending()
}

/// A command that is already configured still passes the shutdown check right
/// before its process is created.
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    GATE.run(system_is_ending, || command.spawn())
}

fn system_is_ending() -> bool {
    // SAFETY: GetSystemMetrics takes no pointers; SM_SHUTTINGDOWN only reads
    // the session's state.
    unsafe { GetSystemMetrics(0x2000) != 0 }
}

#[link(name = "user32")]
extern "system" {
    fn GetSystemMetrics(index: i32) -> i32;
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
