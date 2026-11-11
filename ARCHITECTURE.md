# Orbit C

## Boundary

A C or C++ executable links `libclaviron_orbit`, joins an application Link,
and dispatches its completion events. Edge owns public HTTP connections and
WS/WT handshakes. The library uses Claviron's existing application protocol,
not a second HTTP listener, interpreter or hosting pipeline.

The initial implementation follows the Go binding's checked-handle and
completion-reservation model. It provides no invoke or general service/event
API. Application lifecycle notifications are not public event subscriptions.

## Files

- `include/orbit.h`: public ABI version, records, status codes and ownership rules.
- `src/abi.rs`: panic containment, input validation and bounded copy-out calls.
- `src/registry.rs`: application/exchange handle ownership and checked release.
- `src/runtime.rs`: explicit native I/O executor shared by applications.
- `src/application/completions.rs`: reserved completions, host-loop wake attachment
  and synchronized callback retirement.
- `src/serve.rs`: caller-thread HTTP and WS/WT callbacks, bounded output ordering,
  input backpressure and automatic completion/release.
- `src/serve/dispatcher.rs`: nonblocking, budgeted dispatch on an external loop,
  using the same request state machine as blocking serve.
- `src/application.rs`: native runtime, Link membership, intake and drain.
- `src/application/io.rs`: independent input/output slots and completion ownership.
- `src/application/lifecycle.rs`: generation state and cancellation.
- `src/application/events.rs`: private notified lifecycle fan-out for C ABI instances.
- `src/application/status.rs`: the existing Claviron application status schema.
- `tests/abi.rs`, `tests/registry.rs`: isolated ABI and checked identities.
- `tests/application.rs`: identity and explicit budget validation.
- `tests/application/io.rs`: output credit/state transitions.
- `tests/application/completions.rs`: queued-before-attach delivery, concurrent
  producers and retirement of an in-flight foreign wake.
- `tests/application/lifecycle.rs`: generation and cancellation transitions.
- `build.rs`: relocatable shared-library identity on Mach-O and ELF targets.
- `Cargo.toml`, `rustfmt.toml`: native build and source formatting.
- `meson.build`, `meson.options`: C builds, install and pkg-config metadata.
- `CMakeLists.txt`, `cmake/OrbitCConfig.cmake.in`, `cmake/orbit-c.pc.in`:
  CMake build/install and the `OrbitC::orbit` consumer target.
- `examples/echo.c`: streaming HTTP and message-session handlers.
- `examples/echo-low-level.c`: explicit completion-loop equivalent.
- `examples/abi-check.c`: C/Rust ABI layout executable.
- `examples/runtime-check.c`: real shared-executor ownership executable.
- `examples/dispatcher-check.c`: real loop attachment and exclusive consumer,
  budget validation and ordered release checks, without public listeners.
- `examples/link-check.rs`: real Link HTTP/message-profile caller, not public QUIC.

## Ownership

Inputs are borrowed for one ABI call. Outputs use caller-owned buffers; Rust
retains no caller pointer. Handles are checked, process-local and never reused.
Each retained exchange reserves its completion credit until explicit release.

Runtime handles own Tokio; applications hold executor leases. Executor tasks
retain application state, never the runtime itself. Release is refused while
any application or open call retains a lease. No runtime is created per
application, and executor shutdown runs outside the handle-registry lock.

The runtime selects multi-thread or current-thread I/O at runtime from one native
library. Multi-thread owns a Tokio worker pool; current-thread owns one OS thread
continuously driving its reactor until explicit release. Application preparation
enters the selected runtime without blocking its driver. Neither mode runs C
handlers on an I/O worker or creates a runtime per application.

External loops own callback dispatch, not Orbit I/O. A dispatcher holds exclusive
completion-consumer ownership until release, keeping application/runtime release
busy. It takes a thread-safe wake callback and processes a declared event budget
without waiting. Reaching the budget schedules continuation, so coalesced host
wakes cannot strand queued completions. A wake only signals the loop; it cannot
reenter the ABI. Terminal delivery detaches it under the invocation lock before
the foreign context can be freed. No polling interval or deadline is introduced.

Callback contexts live for `orbit_serve`; per-request user data stays caller
owned. Request pointers and delivered bytes are borrowed only during a callback.
The dispatcher owns the sole completion consumer. It moves delivered input
into the callback scope, recycles the buffer and advances input after queued
output drains. Output bytes are copied once from C and moved into native I/O.

One read and one output operation may be pending per exchange. Results retain
their direction's credit until the caller consumes the delivered completion.
Native I/O remains bounded independently of optional whole-body limits.

`orbit_withdraw` stops intake and drains admitted work; `orbit_cancel` explicitly
cancels this application's work. Consume terminal exchange/application events
before releasing handles. No implicit deadline or polling loop is introduced.

## Sessions

Session acceptance is explicit. The Link acceptance marker is not the public
H1/H2/H3 wire response. C receives whole text/binary/close messages. The WT path
is the existing reliable-message profile, not a QUIC/session/datagram API.
Generation cancellation retires only the owning application's work.

## Receiver admission

The native application joins with a `max_in_flight + max_queued` window below
`LINK_INBOX_CAPACITY` and publishes `max_in_flight` as its execution limit.
Callers reserve shared target capacity before opening a body stream.
`WorkQueue` holds waiting exchanges until `accept_frame` starts them.
A zero queue limit uses direct intake without a release notification.

Only started work reserves a foreign handle and completion space. The exchange
retains its shared admission and execution place through body/session completion
and the last native/foreign owner. Its destructor releases shared admission
before recording the freed execution place. A count plus `Notify` coalesces
releases without losing capacity; native intake promotes waiting work and skips
withdrawn tickets. Control and event reads never wait for an execution place.
The Edge owns `requests.queue_timeout` and withdraws only still-queued work.
ABI 5 uses `max_queued` for waiting work. Earlier ABIs are rejected before reading the options record.

Request payload location is independent of the duplex cancellation/session
stream: inline payloads are read from their decoded frame slice, streamed payloads
from the input half. Replies declare their own payload location.

Status requests retain the asking Edge's origin: solicited snapshots go to that
lane; lifecycle changes broadcast to connected Edges. Reply heads use the
segment's producer-record API. A ProducerBusy result parks the exchange's output
owner until an Edge frees a record, preserving its admission and completion
credit. Cancellation and an HTTP peer departure interrupt that wait; no retry
timer or intake/event-reader wait is added.

## External process ownership

The executable is owned by its process manager, not a simulated Claviron worker
bootstrap. The manager creates/reaps/restarts processes. C owns its callback or
host-loop runtime, supplies incarnation, and turns a stop signal into withdraw,
continued completion dispatch, then handle release. A generic directory watcher
can observe membership/pressure without advertising an application lane.
Creation/scaling policy and serving-loop heartbeat are separate from the current
ABI; no supervisor spawning API or private WorkerStarted codec is exposed.
