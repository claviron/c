//! External event-loop attachment. Orbit I/O keeps its native executor.
use std::cell::RefCell;
use std::ffi::c_void;
use std::thread::ThreadId;

use super::{DispatchState, HttpHandler, SessionHandler};
use crate::abi::{ABI_VERSION, Result, Status, boundary, output};
use crate::application::Consumer;
use crate::application::completions::Wake;
use crate::registry;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Loop {
    pub abi_version: u32,
    pub struct_size: u32,
    pub wake: Option<unsafe extern "C" fn(*mut c_void)>,
    pub context: *mut c_void,
}

pub struct Dispatcher {
    app: u64,
    consumer: Consumer,
    owner: ThreadId,
    state: RefCell<DispatchState>,
}

impl Dispatcher {
    fn check_owner(&self) -> Result<()> {
        if self.owner == std::thread::current().id() {
            Ok(())
        } else {
            Err(Status::InvalidArgument)
        }
    }
}

/// # Safety
/// Handler contexts and the loop wake context remain valid until release.
/// wake is thread-safe, nonblocking and must only signal the host loop; it must
/// not call this ABI. Open, poll and release belong to the same host-loop thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_dispatcher_open(
    app: u64,
    http: *const HttpHandler,
    websocket: *const SessionHandler,
    webtransport: *const SessionHandler,
    event_loop: *const Loop,
    out: *mut *mut Dispatcher,
) -> u32 {
    boundary(|| {
        unsafe { output(out, std::ptr::null_mut())? };
        let event_loop =
            unsafe { event_loop.as_ref() }.ok_or(Status::InvalidArgument)?;
        if event_loop.abi_version != ABI_VERSION
            || event_loop.struct_size as usize != size_of::<Loop>()
        {
            return Err(Status::Unsupported);
        }
        let wake = event_loop.wake.ok_or(Status::InvalidArgument)?;
        let state = DispatchState::new(
            unsafe { http.as_ref().copied() },
            unsafe { websocket.as_ref().copied() },
            unsafe { webtransport.as_ref().copied() },
        )?;
        let consumer = registry::application(app)?.claim_consumer()?;
        let dispatcher = Box::new(Dispatcher {
            app,
            consumer,
            owner: std::thread::current().id(),
            state: RefCell::new(state),
        });
        dispatcher
            .consumer
            .0
            .completions
            .attach(Wake { callback: wake, context: event_loop.context })?;
        unsafe { output(out, Box::into_raw(dispatcher)) }
    })
}

/// # Safety
/// dispatcher is a live pointer returned by open, on its owning thread.
/// processed and closed are writable. A callback may not recursively dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_dispatcher_poll(
    dispatcher: *mut Dispatcher,
    max_events: u64,
    processed: *mut u64,
    closed: *mut u32,
) -> u32 {
    boundary(|| {
        unsafe {
            output(processed, 0)?;
            output(closed, 0)?;
        }
        let dispatcher =
            unsafe { dispatcher.as_ref() }.ok_or(Status::InvalidArgument)?;
        dispatcher.check_owner()?;
        if max_events == 0 {
            return Err(Status::InvalidArgument);
        }
        // The loop owns this dispatcher. A borrow gate rejects callback reentry
        // without a cross-thread lock on every dispatch.
        let mut state =
            dispatcher.state.try_borrow_mut().map_err(|_| Status::Busy)?;
        let application = &dispatcher.consumer.0;
        let mut count = 0;
        let mut outcome = Ok(());
        while count < max_events && state.terminal.is_none() {
            let Some(event) = application.try_event()? else { break };
            count += 1;
            outcome = state.event(dispatcher.app, application, event);
            if outcome.is_err() {
                break;
            }
        }
        let done = state.terminal.is_some();
        unsafe {
            output(processed, count)?;
            output(closed, u32::from(done))?;
        }
        if done {
            // No producer can still enter the foreign wake after this returns.
            application.completions.detach();
        } else if count == max_events {
            // Coalesced wakes do not count completions. Preserve a continuation
            // when this dispatch budget ended before the queue became empty.
            application.completions.signal();
        }
        outcome?;
        if let Some(status) = state.terminal {
            super::check(status as u32)?;
        }
        Ok(())
    })
}

/// # Safety
/// dispatcher is live, owned by this thread, and unused after successful release.
/// Consume the terminal event first. Failed release retains ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_dispatcher_release(
    dispatcher: *mut Dispatcher
) -> u32 {
    boundary(|| {
        let value =
            unsafe { dispatcher.as_ref() }.ok_or(Status::InvalidArgument)?;
        value.check_owner()?;
        {
            let state = value.state.try_borrow().map_err(|_| Status::Busy)?;
            if state.terminal.is_none() || !state.requests.is_empty() {
                return Err(Status::Busy);
            }
        }
        value.consumer.0.completions.detach();
        unsafe { drop(Box::from_raw(dispatcher)) };
        Ok(())
    })
}
