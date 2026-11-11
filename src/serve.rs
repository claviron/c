//! Caller-thread callback dispatch over the same bounded native exchanges.
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

use claviron::orbit::link::application::{HttpVersion, Message};

use crate::abi::{EventKind, Result, Status, boundary, bytes};
use crate::application::Exchange;
use crate::registry;

type Callback = unsafe extern "C" fn(*mut Request, *mut c_void) -> u32;
type BodyCallback =
    unsafe extern "C" fn(*mut Request, *const u8, u64, u32, *mut c_void) -> u32;
type MessageCallback = unsafe extern "C" fn(
    *mut Request,
    u32,
    u32,
    *const u8,
    u64,
    *mut c_void,
) -> u32;
type ClosedCallback = unsafe extern "C" fn(*mut Request, u32, *mut c_void);

#[derive(Clone, Copy)]
#[repr(C)]
pub struct HttpHandler {
    pub request: Option<Callback>,
    pub body: Option<BodyCallback>,
    pub writable: Option<Callback>,
    pub closed: Option<ClosedCallback>,
    pub context: *mut c_void,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SessionHandler {
    pub open: Option<Callback>,
    pub message: Option<MessageCallback>,
    pub writable: Option<Callback>,
    pub closed: Option<ClosedCallback>,
    pub context: *mut c_void,
}

#[derive(Clone, Copy)]
enum Handler {
    Http(HttpHandler),
    Session(SessionHandler),
    Unsupported,
}

enum Pending {
    Body(Vec<u8>),
    Message(Message),
}

/// Opaque to C; borrowed only within a dispatcher callback.
pub struct Request {
    id: u64,
    exchange: Arc<Exchange>,
    handler: Handler,
    data: *mut c_void,
    begun: bool,
    head: bool,
    output: bool,
    finishing: bool,
    finish: bool,
    read: bool,
    input: bool,
    ended: bool,
    pending: Option<Pending>,
    complete: Option<u32>,
    error: Option<Status>,
}

impl Request {
    fn flush(&mut self) -> Result<()> {
        if !self.output && !self.finishing {
            if self.head {
                self.exchange.head()?;
                self.head = false;
                self.output = true;
            } else if let Some(pending) = self.pending.take() {
                self.finishing =
                    matches!(&pending, Pending::Message(Message::Close { .. }));
                match pending {
                    Pending::Body(data) => self.exchange.write_owned(data)?,
                    Pending::Message(message) => {
                        self.exchange.write_message(message)?
                    }
                }
                self.output = true;
            } else if self.finish {
                self.exchange.finish_output()?;
                self.finishing = true;
                self.output = true;
            }
        }
        // Backpressure: one input delivery waits for its queued output to drain.
        if self.read
            && !self.ended
            && !self.input
            && !self.output
            && !self.finishing
        {
            if self.exchange.frame.upgrade {
                self.exchange.read_message()?;
            } else {
                self.exchange
                    .read(self.exchange.application.options.io_buffer_bytes)?;
            }
            self.input = true;
        }
        Ok(())
    }

    fn queue(
        &mut self,
        pending: Pending,
    ) -> Result<()> {
        if self.finish || self.finishing {
            return Err(Status::Closed);
        }
        if self.pending.is_some() {
            return Err(Status::Busy);
        }
        self.pending = Some(pending);
        Ok(())
    }

    fn callback(
        &mut self,
        callback: Option<Callback>,
        context: *mut c_void,
    ) -> Result<()> {
        if let Some(callback) = callback {
            check(unsafe { callback(self, context) })?;
        }
        Ok(())
    }

    fn opened(&mut self) -> Result<()> {
        match self.handler {
            Handler::Http(handler) => {
                self.callback(handler.request, handler.context)
            }
            Handler::Session(handler) => {
                self.callback(handler.open, handler.context)
            }
            Handler::Unsupported => {
                self.exchange.begin(501)?;
                self.head = true;
                self.finish = true;
                Ok(())
            }
        }
    }

    fn received(&mut self) -> Result<()> {
        let (data, kind, code, status) = self.exchange.take_input()?;
        self.input = false;
        if status != Status::Ok {
            return Err(status);
        }
        match self.handler {
            Handler::Http(handler) => {
                self.ended = data.is_empty();
                if let Some(callback) = handler.body {
                    check(unsafe {
                        callback(
                            self,
                            data.as_ptr(),
                            data.len() as u64,
                            u32::from(self.ended),
                            handler.context,
                        )
                    })?;
                }
            }
            Handler::Session(handler) => {
                self.ended = kind == 0 || kind == 3;
                // The native writer answers peer CLOSE. EOF alone still needs
                // the application's output to end explicitly.
                if kind == 3 {
                    self.finishing = true;
                }
                if let Some(callback) = handler.message {
                    check(unsafe {
                        callback(
                            self,
                            kind,
                            code,
                            data.as_ptr(),
                            data.len() as u64,
                            handler.context,
                        )
                    })?;
                }
                if kind == 0 {
                    self.finish = true;
                }
            }
            Handler::Unsupported => {}
        }
        self.exchange.recycle_input(data);
        Ok(())
    }

    fn written(&mut self) -> Result<()> {
        let (_, status) = self.exchange.output_result()?;
        self.output = false;
        if status != Status::Ok {
            return Err(status);
        }
        if self.finishing || self.finish || self.pending.is_some() {
            return Ok(());
        }
        match self.handler {
            Handler::Http(handler) => {
                self.callback(handler.writable, handler.context)
            }
            Handler::Session(handler) => {
                self.callback(handler.writable, handler.context)
            }
            Handler::Unsupported => Ok(()),
        }
    }

    fn closed(
        &mut self,
        status: u32,
    ) {
        let (callback, context) = match self.handler {
            Handler::Http(h) => (h.closed, h.context),
            Handler::Session(h) => (h.closed, h.context),
            Handler::Unsupported => return,
        };
        if let Some(callback) = callback {
            unsafe { callback(self, status, context) };
        }
    }
}

fn check(status: u32) -> Result<()> {
    match status {
        0 => Ok(()),
        1 => Err(Status::InvalidArgument),
        2 => Err(Status::InvalidHandle),
        3 => Err(Status::Closed),
        4 => Err(Status::Capacity),
        5 => Err(Status::Unsupported),
        6 => Err(Status::Transport),
        7 => Err(Status::Panic),
        8 => Err(Status::Busy),
        9 => Err(Status::Cancelled),
        10 => Err(Status::Limit),
        _ => Err(Status::InvalidArgument),
    }
}

mod dispatcher;

struct DispatchState {
    requests: HashMap<u64, Request>,
    http: Option<HttpHandler>,
    websocket: Option<SessionHandler>,
    webtransport: Option<SessionHandler>,
    terminal: Option<Status>,
}

impl DispatchState {
    fn new(
        http: Option<HttpHandler>,
        websocket: Option<SessionHandler>,
        webtransport: Option<SessionHandler>,
    ) -> Result<Self> {
        if http.is_some_and(|h| h.request.is_none())
            || websocket.is_some_and(|h| h.open.is_none())
            || webtransport.is_some_and(|h| h.open.is_none())
            || (http.is_none() && websocket.is_none() && webtransport.is_none())
        {
            return Err(Status::InvalidArgument);
        }
        Ok(Self {
            requests: HashMap::new(),
            http,
            websocket,
            webtransport,
            terminal: None,
        })
    }

    fn event(
        &mut self,
        app: u64,
        application: &crate::application::State,
        event: crate::abi::Event,
    ) -> Result<()> {
        if event.kind == EventKind::Closed as u32 {
            let status = check(event.status).err().unwrap_or(Status::Ok);
            self.terminal = Some(status);
            if !self.requests.is_empty() {
                return Err(Status::Busy);
            }
            return check(status as u32);
        }
        if event.kind == EventKind::Lifecycle as u32 {
            application.lifecycle.consume()?;
            return Ok(());
        }
        if event.kind == EventKind::Request as u32 {
            let exchange = registry::exchange(event.exchange)?;
            let handler = if !exchange.frame.upgrade {
                self.http.map(Handler::Http)
            } else if exchange.frame.version == HttpVersion::Http3 {
                self.webtransport.map(Handler::Session)
            } else {
                self.websocket.map(Handler::Session)
            }
            .unwrap_or(Handler::Unsupported);
            self.requests.insert(
                event.exchange,
                Request {
                    id: event.exchange,
                    exchange,
                    handler,
                    data: std::ptr::null_mut(),
                    begun: false,
                    head: false,
                    output: false,
                    finish: false,
                    finishing: false,
                    read: false,
                    input: false,
                    ended: false,
                    pending: None,
                    complete: None,
                    error: None,
                },
            );
        }
        let request = self
            .requests
            .get_mut(&event.exchange)
            .ok_or(Status::InvalidHandle)?;
        let outcome = if event.kind == EventKind::Request as u32 {
            request.opened()
        } else if event.kind == EventKind::Read as u32 {
            request.received()
        } else if event.kind == EventKind::Output as u32 {
            request.written()
        } else if event.kind == EventKind::Complete as u32 {
            request.complete = Some(event.status);
            Ok(())
        } else {
            Ok(())
        };
        let outcome = outcome.and_then(|()| {
            if request.complete.is_none() { request.flush() } else { Ok(()) }
        });
        if let Err(error) = outcome {
            request.error.get_or_insert(error);
            request.exchange.cancel.cancel();
        }
        if let Some(status) = request.complete
            && request.exchange.releasable()
        {
            request.closed(request.error.map_or(status, |error| error as u32));
            registry::remove_exchange(app, event.exchange)?;
            self.requests.remove(&event.exchange);
        }

        Ok(())
    }
}

/// # Safety
/// Handler tables and their contexts live until return. Callbacks run only on
/// this caller's thread. Do not concurrently consume this application's events.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_serve(
    app: u64,
    http: *const HttpHandler,
    websocket: *const SessionHandler,
    webtransport: *const SessionHandler,
) -> u32 {
    boundary(|| {
        let consumer = registry::application(app)?.claim_consumer()?;
        let application = &consumer.0;
        let mut dispatch = DispatchState::new(
            unsafe { http.as_ref().copied() },
            unsafe { websocket.as_ref().copied() },
            unsafe { webtransport.as_ref().copied() },
        )?;
        while dispatch.terminal.is_none() {
            dispatch.event(app, application, application.wait()?)?;
        }
        Ok(())
    })
}

/// # Safety
/// request is borrowed from the current callback, on the dispatcher thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_request_handle(request: *const Request) -> u64 {
    unsafe { request.as_ref() }.map_or(0, |request| request.id)
}

/// # Safety
/// See orbit_request_handle; data is opaque caller-owned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_request_set_data(
    request: *mut Request,
    data: *mut c_void,
) -> u32 {
    boundary(|| {
        unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?.data = data;
        Ok(())
    })
}

/// # Safety
/// See orbit_request_handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_request_data(
    request: *const Request
) -> *mut c_void {
    unsafe { request.as_ref() }
        .map_or(std::ptr::null_mut(), |request| request.data)
}

/// # Safety
/// See orbit_request_handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_response_begin(
    request: *mut Request,
    status: u32,
) -> u32 {
    boundary(|| {
        let request =
            unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?;
        request.exchange.begin(
            u16::try_from(status).map_err(|_| Status::InvalidArgument)?,
        )?;
        request.begun = true;
        request.head = true;
        Ok(())
    })
}

/// # Safety
/// See orbit_request_handle. Name/value are borrowed until this call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_response_header(
    request: *mut Request,
    name: *const u8,
    name_length: u64,
    value: *const u8,
    value_length: u64,
) -> u32 {
    boundary(|| {
        let request =
            unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?;
        request.exchange.header(unsafe { bytes(name, name_length)? }, unsafe {
            bytes(value, value_length)?
        })
    })
}

/// # Safety
/// See orbit_request_handle. Input bytes are borrowed for this call only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_response_write(
    request: *mut Request,
    data: *const u8,
    length: u64,
) -> u32 {
    boundary(|| {
        let request =
            unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?;
        if request.exchange.frame.upgrade {
            return Err(Status::Unsupported);
        }
        if !request.begun {
            return Err(Status::Busy);
        }
        let data = unsafe { bytes(data, length)? };
        if data.len() > request.exchange.application.options.io_buffer_bytes {
            return Err(Status::Capacity);
        }
        request.queue(Pending::Body(data.to_vec()))
    })
}

/// # Safety
/// See orbit_request_handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_response_finish(request: *mut Request) -> u32 {
    boundary(|| {
        let request =
            unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?;
        if !request.begun {
            return Err(Status::Busy);
        }
        request.finish = true;
        Ok(())
    })
}

/// # Safety
/// See orbit_request_handle. A body callback must be registered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_request_read_body(request: *mut Request) -> u32 {
    boundary(|| {
        let request =
            unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?;
        if !matches!(
            request.handler,
            Handler::Http(HttpHandler { body: Some(_), .. })
        ) {
            return Err(Status::Unsupported);
        }
        request.read = true;
        Ok(())
    })
}

/// # Safety
/// See orbit_request_handle. The limit is declared by the application.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_session_accept(
    request: *mut Request,
    max_message_bytes: u32,
) -> u32 {
    boundary(|| {
        let request =
            unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?;
        request.exchange.begin_session(max_message_bytes)?;
        request.begun = true;
        request.head = true;
        request.read = true;
        Ok(())
    })
}

/// # Safety
/// See orbit_request_handle; data is call-scoped text, binary, or close reason.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn orbit_session_send(
    request: *mut Request,
    kind: u32,
    code: u32,
    data: *const u8,
    length: u64,
) -> u32 {
    boundary(|| {
        let request =
            unsafe { request.as_mut() }.ok_or(Status::InvalidArgument)?;
        let data = unsafe { bytes(data, length)? };
        request.exchange.validate_message_size(kind, data.len())?;
        let message = crate::abi::message(kind, code, data)?;
        request.queue(Pending::Message(message))
    })
}
