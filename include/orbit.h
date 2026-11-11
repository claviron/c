#ifndef CLAVIRON_ORBIT_H
#define CLAVIRON_ORBIT_H
#include <stdint.h>

#define ORBIT_ABI_VERSION 5u

typedef enum {
    ORBIT_MULTI_THREAD = 0,
    ORBIT_CURRENT_THREAD = 1
} orbit_execution;

typedef enum {
    ORBIT_OK = 0, ORBIT_INVALID_ARGUMENT = 1, ORBIT_INVALID_HANDLE = 2,
    ORBIT_CLOSED = 3, ORBIT_CAPACITY = 4, ORBIT_UNSUPPORTED = 5,
    ORBIT_TRANSPORT = 6, ORBIT_PANIC = 7, ORBIT_BUSY = 8,
    ORBIT_CANCELLED = 9, ORBIT_LIMIT = 10
} orbit_status;

typedef enum {
    ORBIT_REQUEST = 1, ORBIT_CANCEL = 2, ORBIT_COMPLETE = 3,
    ORBIT_APPLICATION_CLOSED = 4, ORBIT_READ = 5, ORBIT_OUTPUT = 6,
    ORBIT_LIFECYCLE = 7
} orbit_event_kind;

typedef enum {
    ORBIT_MESSAGE_END = 0, ORBIT_MESSAGE_TEXT = 1,
    ORBIT_MESSAGE_BINARY = 2, ORBIT_MESSAGE_CLOSE = 3
} orbit_message_kind;
#ifdef __cplusplus
extern "C" {
#endif
/* ABI 5. Byte inputs live only for the call; native never retains their pointers.
 * Outputs must be aligned and writable. Copy-out buffers may not overlap inputs.
 * Status: 0 OK, 1 invalid argument, 2 stale/wrong-kind handle, 3 closed,
 * 4 capacity, 5 unsupported, 6 transport, 7 contained panic, 8 busy, 9 cancelled, 10 body limit.
 * Handles are process-local, never reused. Only release after terminal delivery.
 */
typedef struct {
    uint32_t version, event_size, options_size, info_size;
    uint64_t fleet_capacity, inbox_capacity, inbox_payload_capacity, lane_capacity, buffer_bytes;
} orbit_abi_info;
typedef struct {
    uint32_t abi_version, execution, native_threads;
} orbit_runtime_options;
typedef struct {
    uint32_t abi_version;
    /* 0 < max_in_flight and max_in_flight + max_queued < inbox_capacity.
     * max_in_flight bounds started exchanges through body/session completion
     * and explicit release. max_queued counts only waiting exchanges.
     * Full windows refuse new work without blocking control/event reads. */
    uint64_t incarnation, max_in_flight, max_queued;
    uint64_t max_request_bytes, max_response_bytes, io_buffer_bytes;
} orbit_options;
typedef struct { uint32_t kind, status; uint64_t exchange; } orbit_event;
typedef struct {
    uint32_t version_major, version_minor, header_count, body_stream;
    uint64_t generation;
    uint32_t cancelled, session;
} orbit_request_info_record;
typedef struct { uint64_t generation, revision, running; } orbit_snapshot_record;
uint32_t orbit_abi(orbit_abi_info *out);
/* Runtime creation is explicit. Applications share its native I/O workers.
 * MULTI_THREAD requires native_threads >= 1. CURRENT_THREAD requires exactly 1
 * and drives Tokio on one owned OS thread, even while the C caller is waiting.
 * Both modes are in the same library; neither owns the caller's event loop.
 * Application code runs on the caller's threads. Releasing a runtime with any
 * attached application or concurrent open call returns ORBIT_BUSY.
 * Release all applications before releasing their runtime. */
uint32_t orbit_runtime_open(const orbit_runtime_options *options, uint64_t *out);
uint32_t orbit_runtime_release(uint64_t runtime);
uint32_t orbit_open(uint64_t runtime, const uint8_t *fleet, uint64_t fleet_len, const uint8_t *name, uint64_t name_len, const orbit_options *options, uint64_t *out);
/* Only orbit_wait blocks: one waiter per application, explicitly woken by close.
 * Events: 1 request, 2 cancelled, 3 complete (exactly once), 4 application closed, 5 read result, 6 output result, 7 lifecycle state change (coalesced).
 * Each exchange retains admission/completion credit until exchange_release.
 * Submit success means accepted native work, not response delivery.
 */
uint32_t orbit_wait(uint64_t app, orbit_event *out);
uint32_t orbit_withdraw(uint64_t app);
uint32_t orbit_cancel(uint64_t app);
uint32_t orbit_release(uint64_t app);
uint32_t orbit_lane(uint64_t app, uint64_t *out);
uint32_t orbit_unsupported(uint64_t app, uint64_t *out);
/* Lifecycle actions are application-wide among currently connected same-name
 * C ABI instances: 1 start, 2 stop, 3 restart. Membership is retained. Snapshot is
 * authoritative local state; lifecycle_result consumes the delivered wake and
 * returns the latest state, not an action history. */
uint32_t orbit_lifecycle(uint64_t app, uint32_t action);
uint32_t orbit_snapshot(uint64_t app, orbit_snapshot_record *out);
uint32_t orbit_lifecycle_result(uint64_t app, orbit_snapshot_record *out);
uint32_t orbit_request_info(uint64_t exchange, orbit_request_info_record *out);
/* field: 1 method, 2 scheme, 3 raw path, 4 query, 5 peer, 6 local,
 * 8 header name, 9 header value. index is used for headers only.
 * NULL/0 queries required length; otherwise copy exactly that many bytes.
 * No borrowed native pointer escapes a call. */
uint32_t orbit_request_copy(uint64_t exchange, uint32_t field, uint32_t index, uint8_t *out, uint64_t capacity, uint64_t *length);
uint32_t orbit_reply_begin(uint64_t exchange, uint32_t status);
uint32_t orbit_reply_header(uint64_t exchange, const uint8_t *name, uint64_t name_len, const uint8_t *value, uint64_t value_len);
/* Exactly one pending read and one pending output per exchange. Result calls
 * require delivery of their event and release that direction's credit. Native
 * copies output chunks before return; no caller pointer survives a call. Zero total
 * body limits disable that limit; io_buffer_bytes always bounds each operation. */
uint32_t orbit_head_submit(uint64_t exchange);
uint32_t orbit_output_write(uint64_t exchange, const uint8_t *body, uint64_t length);
uint32_t orbit_output_finish(uint64_t exchange);
uint32_t orbit_output_result(uint64_t exchange, uint64_t *written);
uint32_t orbit_read_start(uint64_t exchange, uint64_t capacity);
uint32_t orbit_read_result(uint64_t exchange, uint8_t *out, uint64_t capacity, uint64_t *length);
uint32_t orbit_read_close(uint64_t exchange);
/* A request with session set is a message session: answer it with
 * orbit_session_begin + orbit_head_submit (the Link's 101, never the wire handshake)
 * or a 3xx-5xx head + orbit_output_finish. Body read/write calls are unsupported on
 * it. Messages share the exchange's one read and one output slot, and complete
 * through events 5 and 6. Read kinds: 0 end, 1 text, 2 binary, 3 close (code set;
 * data is the reason). A peer's close is answered natively and ends the session. */
uint32_t orbit_session_begin(uint64_t exchange, uint32_t max_message_bytes);
uint32_t orbit_message_read_start(uint64_t exchange);
uint32_t orbit_message_read_result(uint64_t exchange, uint32_t *kind, uint32_t *code, uint8_t *out, uint64_t capacity, uint64_t *length, uint32_t consume);
uint32_t orbit_message_write(uint64_t exchange, uint32_t kind, uint32_t code, const uint8_t *data, uint64_t length);
uint32_t orbit_exchange_cancel(uint64_t exchange);
uint32_t orbit_exchange_release(uint64_t app, uint64_t exchange);

/* Callback API. Handler tables are independent; NULL leaves that capability
 * unregistered (requests for it receive 501). Callbacks run on the thread that
 * calls orbit_serve, never on a Tokio worker. Return ORBIT_OK, or a nonzero status
 * to cancel that exchange. Do not throw C++ exceptions across this ABI.
 *
 * Request pointers and body/message bytes are borrowed only for the callback.
 * Use request_data/set_data for caller-owned state; free it in closed. The
 * dispatcher consumes completions and releases exchanges. Do not mix raw I/O,
 * orbit_wait or exchange_release with these managed requests. Metadata calls
 * through orbit_request_handle remain available, including during closed.
 */
typedef struct orbit_request orbit_request;
typedef uint32_t (*orbit_request_fn)(orbit_request *, void *context);
typedef uint32_t (*orbit_body_fn)(orbit_request *, const uint8_t *, uint64_t length, uint32_t end, void *context);
typedef uint32_t (*orbit_message_fn)(orbit_request *, uint32_t kind, uint32_t code, const uint8_t *, uint64_t length, void *context);
typedef void (*orbit_closed_fn)(orbit_request *, uint32_t status, void *context);
typedef struct {
    orbit_request_fn request;
    orbit_body_fn body;
    orbit_request_fn writable;
    orbit_closed_fn closed;
    void *context;
} orbit_http_handler;
typedef struct {
    orbit_request_fn open;
    orbit_message_fn message;
    orbit_request_fn writable;
    orbit_closed_fn closed;
    void *context;
} orbit_session_handler;
/* Blocks until explicit withdraw/cancel has drained this application. It owns
 * the application's single completion consumer. Tables/context live to return.
 * H1/H2 session offers select websocket; H3 selects webtransport's Link message
 * profile. Session acceptance remains the handler's decision. */
uint32_t orbit_serve(uint64_t app, const orbit_http_handler *http, const orbit_session_handler *websocket, const orbit_session_handler *webtransport);

/* Host-loop integration uses the same handlers and native I/O runtime.
 * wake must be thread-safe and nonblocking (for example uv_async_send). It only
 * schedules a later poll: do not call any Orbit API or handlers inside wake.
 * Notifications may coalesce; they are not a completion count. No timer or
 * periodic poll is required. The table is copied; context lives until release.
 */
typedef struct orbit_dispatcher orbit_dispatcher;
typedef struct {
    uint32_t abi_version, struct_size;
    void (*wake)(void *context);
    void *context;
} orbit_loop;
/* Open, poll and release run on the same host-loop thread. Open schedules an
 * initial wake for any already queued work. The dispatcher exclusively owns
 * completion consumption: no orbit_wait or second dispatcher on this app.
 * Runtime I/O remains on Tokio; this API integrates application callbacks,
 * not a replacement I/O reactor. */
uint32_t orbit_dispatcher_open(uint64_t app, const orbit_http_handler *http,
    const orbit_session_handler *websocket, const orbit_session_handler *webtransport,
    const orbit_loop *loop, orbit_dispatcher **out);
/* Nonblocking: process at most max_events (must be positive). processed may be
 * zero. Budget exhaustion schedules another wake. closed=1 means terminal
 * delivery, even if its status reports failure; no further wake callback can
 * run after that return. Reentrant polling returns ORBIT_BUSY.
 * All handlers run on the polling thread. Keep polling after withdraw/cancel
 * until closed; then release the dispatcher, application and runtime in order.
 */
uint32_t orbit_dispatcher_poll(orbit_dispatcher *dispatcher, uint64_t max_events,
    uint64_t *processed, uint32_t *closed);
/* ORBIT_BUSY until terminal delivery; failed release retains ownership. */
uint32_t orbit_dispatcher_release(orbit_dispatcher *dispatcher);
uint64_t orbit_request_handle(const orbit_request *request);
uint32_t orbit_request_set_data(orbit_request *request, void *data);
void *orbit_request_data(const orbit_request *request);
uint32_t orbit_response_begin(orbit_request *request, uint32_t status);
uint32_t orbit_response_header(orbit_request *request, const uint8_t *name, uint64_t name_length, const uint8_t *value, uint64_t value_length);
/* One queued output chunk per callback, at most io_buffer_bytes. Native copies
 * it once before returning, then moves it into the I/O owner. finish is ordered
 * after queued head/body output. writable follows drained head/body output and
 * lets a response producer supply its next chunk without collecting the body. */
uint32_t orbit_response_write(orbit_request *request, const uint8_t *data, uint64_t length);
uint32_t orbit_response_finish(orbit_request *request);
/* Opt into body callbacks; no automatic whole-body buffering. Input advances
 * only when the previous callback's output has drained. end=1 is EOF. */
uint32_t orbit_request_read_body(orbit_request *request);
uint32_t orbit_session_accept(orbit_request *request, uint32_t max_message_bytes);
uint32_t orbit_session_send(orbit_request *request, uint32_t kind, uint32_t code, const uint8_t *data, uint64_t length);
#ifdef __cplusplus
}
#endif
#endif
