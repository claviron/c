# Orbit C

Write C or C++ applications over Orbit Link. Register HTTP, WebSocket or
WebTransport message handlers; the library handles native I/O, completions
and application lifecycle.

Orbit C `0.1.1` uses C ABI 5 and targets Claviron `0.2.1`.

## Handlers

```c
static uint32_t hello(orbit_request *request, void *context) {
    (void)context;
    uint32_t status = orbit_response_begin(request, 200);
    if (status != ORBIT_OK) return status;
    status = orbit_response_write(request, (const uint8_t *)"Hello!\n", 7);
    return status == ORBIT_OK ? orbit_response_finish(request) : status;
}

/* After opening the runtime and application: */
orbit_http_handler http = {.request = hello};
orbit_serve(app, &http, NULL, NULL);
```

HTTP, WS and WT handlers are independent. A null handler leaves that capability
unregistered. Body callbacks stream bounded chunks; `writable` callbacks drive
response streams. WS/WT handlers explicitly accept sessions and receive whole
text, binary and close messages. WT supports the reliable-message profile,
without exposing a QUIC transport API.

`orbit_serve` runs callbacks on its calling thread. It consumes completion
events and releases exchanges. Callbacks must return promptly; they do not run
on Tokio workers. The same handlers can attach to an existing event loop through
`orbit_dispatcher_open` and `orbit_dispatcher_poll`. The lower-level `orbit_wait`
API remains available; see [echo-low-level.c](examples/echo-low-level.c).

## Runtime

Open one native I/O runtime with `orbit_runtime_open`, then attach applications
with `orbit_open`. Applications can share that runtime. Both execution choices
are available in the same shared library:

```c
orbit_runtime_options options = {
    .abi_version = ORBIT_ABI_VERSION,
    .execution = ORBIT_CURRENT_THREAD,
    .native_threads = 1,
};
orbit_runtime_open(&options, &runtime);
```

`ORBIT_MULTI_THREAD` accepts one or more native I/O workers. `ORBIT_CURRENT_THREAD`
requires exactly one and continuously drives Tokio on one owned OS thread.
Exchange capacity, I/O window and optional body limits are explicit caller options.

ABI 5 separates running exchanges (`max_in_flight`) from waiting exchanges
(`max_queued`). For example, 128 and 64 allow 128 running plus 64 waiting.
The running limit must be positive and the sum must be below the Link inbox
capacity. A queue limit of zero uses direct intake. A full window refuses new
work immediately.

Queued requests stay in the native Link `WorkQueue`; no foreign handle or
completion credit is allocated until they start. Running exchanges retain
capacity through body/session completion and explicit foreign-handle release.
Release wakes the queue; event/control reads continue independently. Edge's
`requests.queue_timeout` may withdraw a request that has not started.
Read `orbit_abi` and use matching header/library versions; earlier records
are rejected before their full layout is read.

Edge owns public connections and hosting policy. The C executable owns its
process and handlers. Tokio drives Orbit I/O inside the native library.
The binding exposes no general task or timer API. Both execution modes and
event-loop attachment are available. `inherit`, running Orbit I/O on the
host's own loop, is reserved and unavailable.

## Process management

An external process manager starts and restarts the executable. The binding
does not fork workers or join a Claviron master/worker process family. The
application keeps ownership of its host event loop and signal handling.
For graceful process shutdown, stop intake with `orbit_withdraw`, keep serving
completions until closed, then release application and runtime handles. Use
`orbit_cancel` only when explicitly cancelling admitted work.

Each process supplies its incarnation in `orbit_options`. Application-wide
`orbit_lifecycle` operations change application generations, not process count.
Serving-loop heartbeat and an external manager's pressure-driven process scaling
are not implemented by this ABI update.

## Existing event loops

Create an `orbit_loop` with a thread-safe `wake(context)` function that only
signals the application's loop. For libuv, this can call `uv_async_send` on an
already initialized handle. Open a dispatcher on the loop thread, then call
`orbit_dispatcher_poll` from that handle's callback. Give each poll a positive
completion budget; budget exhaustion schedules another wake. Notifications may
coalesce, so they are not a count of pending completions. No periodic polling is
needed.

The dispatcher uses the same HTTP and session handlers as `orbit_serve`. All
handlers run on the polling thread. Orbit I/O still uses the selected Tokio
runtime: attaching libuv does not replace the native reactor or remove Tokio.
Open, poll and release the dispatcher on the same thread. Do not consume the
application's completions through any other API while it is attached.

After withdraw or cancel, keep dispatching until `closed` is set, including when
the terminal status is an error. That return detaches the wake callback and waits
for any invocation already in progress. Release the dispatcher before releasing
the application and runtime; the host can then close its notification handle.

`orbit_withdraw` drains admitted work; `orbit_cancel` cancels it. After
`orbit_serve` returns, release the application, then its runtime. Runtime release
returns `ORBIT_BUSY` while applications remain attached.

## Install

Download the archive for your platform from [GitHub Releases](https://github.com/claviron/c/releases).
The archive includes the header, shared and static libraries, and CMake and
pkg-config metadata. Linux archives require glibc 2.28 on x86-64 and glibc 2.34
on arm64. The FreeBSD archives were built against FreeBSD 14.4 on x86-64 and
15.1 on arm64.

Extract the archive and pass its directory to CMake in `CMAKE_PREFIX_PATH`.
Use `find_package(OrbitC CONFIG REQUIRED)` and link to `OrbitC::orbit`.
For pkg-config, add the archive's `lib/pkgconfig` directory to
`PKG_CONFIG_PATH` and use `pkg-config --cflags --libs orbit-c`.
Use `--static` with pkg-config when linking the static library.

The repository also ships a vcpkg overlay port:

```sh
vcpkg install orbit-c --overlay-ports=packaging/vcpkg
```

## Build from source

Rust builds the shared library and static archive. Meson or CMake builds the C
examples and installs the header, native artifacts and consumer metadata.

```sh
cargo build --release
meson setup build -Dnative_dir=target/release --prefix=/usr/local
meson compile -C build
meson install -C build
```

Or:

```sh
cmake -S . -B build-cmake -DORBIT_NATIVE_DIR="$PWD/target/release"
cmake --build build-cmake
cmake --install build-cmake
```

Consumers use `pkg-config --cflags --libs orbit-c`, or CMake's
`find_package(OrbitC CONFIG REQUIRED)` and `OrbitC::orbit`. Static consumers also
need their target's native link flags, reported by
`cargo rustc --release --lib -- --print=native-static-libs`.

## Examples

```sh
./build/orbit-echo demo echo
```

[echo.c](examples/echo.c) registers streaming HTTP and message-session echo
handlers. It declares its own budgets and signal handling. An Edge can route
to application `echo` on fleet `demo`.

`orbit-abi-check` checks C/Rust layouts. `orbit-runtime-check FLEET` exercises
shared-runtime ownership in both execution modes. `orbit-dispatcher-check FLEET`
checks exclusive host-loop consumption and release ordering. The Rust `link-check` example checks HTTP and message
profiles against the running echo application; it does not establish a public
QUIC/WebTransport connection.

Release builds cover macOS arm64 and Linux and FreeBSD on x86-64 and arm64.
Invoke, general service and public event APIs are outside this initial scope.

`cargo run --example admission-check -- FLEET` runs an explicit native-ABI
acceptance check with one running and one waiting place. Supply a fresh fleet.
It verifies refusal at the full window, control intake while full, queued
withdrawal, and promotion on foreign-handle release. It removes its inbox after
draining; no server or listener is started. After the process exits, remove the
remaining test-fleet resources with
`orbit clear FLEET --all --yes`.
