//! Reserved completions and a detachable host-loop wake callback.
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};

use crate::abi::{Event, Result, Status};

#[derive(Clone, Copy)]
pub(crate) struct Wake {
    pub callback: unsafe extern "C" fn(*mut c_void),
    pub context: *mut c_void,
}

// The host guarantees that wake only signals its loop, from any producer thread.
// Detach holds the same lock as invocation before the context may be released.
unsafe impl Send for Wake {}

pub(crate) struct Completions {
    send: mpsc::SyncSender<Event>,
    receive: Mutex<mpsc::Receiver<Event>>,
    attached: AtomicBool,
    wake: Mutex<Option<Wake>>,
}

impl Completions {
    pub fn new(capacity: usize) -> Self {
        let (send, receive) = mpsc::sync_channel(capacity);
        Self {
            send,
            receive: Mutex::new(receive),
            attached: AtomicBool::new(false),
            wake: Mutex::new(None),
        }
    }

    pub fn push(
        &self,
        event: Event,
    ) {
        self.send.try_send(event).expect("reserved completion space");
        self.signal();
    }

    pub fn wait(&self) -> Result<Event> {
        self.receive
            .lock()
            .map_err(|_| Status::Panic)?
            .recv()
            .map_err(|_| Status::Closed)
    }

    pub fn try_event(&self) -> Result<Option<Event>> {
        match self.receive.lock().map_err(|_| Status::Panic)?.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Status::Closed),
        }
    }

    pub fn attach(
        &self,
        wake: Wake,
    ) -> Result<()> {
        let mut slot = self.wake.lock().map_err(|_| Status::Panic)?;
        if slot.is_some() {
            return Err(Status::Busy);
        }
        *slot = Some(wake);
        self.attached.store(true, Ordering::Release);
        // Also covers completions queued before attachment. The callback only
        // schedules a future dispatch; it must not reenter the binding.
        unsafe { (wake.callback)(wake.context) };
        Ok(())
    }

    pub fn signal(&self) {
        if self.attached.load(Ordering::Acquire) {
            let slot = self.wake.lock().expect("loop wake");
            if let Some(wake) = *slot {
                unsafe { (wake.callback)(wake.context) };
            }
        }
    }

    pub fn detach(&self) {
        let mut slot = self.wake.lock().expect("loop wake");
        self.attached.store(false, Ordering::Release);
        *slot = None;
    }
}

#[cfg(test)]
#[path = "../../tests/application/completions.rs"]
mod tests;
