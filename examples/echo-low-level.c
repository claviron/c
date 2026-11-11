#define _POSIX_C_SOURCE 200809L
#include "orbit.h"
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* These are this example's declared budgets, not library defaults. */
enum { EXCHANGES = 32, WINDOW = 8192, MESSAGE_LIMIT = 65536 };
enum { HEAD, STREAM, MESSAGE, FINISH };

typedef struct request {
    uint64_t id;
    int session, phase, complete;
    struct request *next;
} request;

static request *find(request *list, uint64_t id) {
    for (; list; list = list->next) if (list->id == id) return list;
    return NULL;
}

static void submitted(request *r, uint32_t status) {
    if (status != ORBIT_OK) {
        fprintf(stderr, "exchange %llu: status %u\n", (unsigned long long)r->id, status);
        orbit_exchange_cancel(r->id);
    }
}

static void finish(request *r) {
    r->phase = FINISH;
    uint32_t status = orbit_output_finish(r->id);
    if (status != ORBIT_CANCELLED && status != ORBIT_CLOSED) submitted(r, status);
}

static void next_read(request *r) {
    submitted(r, r->session ? orbit_message_read_start(r->id) : orbit_read_start(r->id, WINDOW));
}

static void *signals(void *value) {
    const uint64_t app = *(const uint64_t *)value;
    sigset_t set;
    sigemptyset(&set); sigaddset(&set, SIGINT); sigaddset(&set, SIGTERM); sigaddset(&set, SIGUSR1);
    int signal_number;
    if (sigwait(&set, &signal_number) == 0 && signal_number != SIGUSR1)
        orbit_withdraw(app); /* Stop admission and drain this instance's work. */
    return NULL;
}

int main(int argc, char **argv) {
    if (argc != 3) { fprintf(stderr, "Usage: %s FLEET APPLICATION\n", argv[0]); return 2; }
    sigset_t set;
    sigemptyset(&set); sigaddset(&set, SIGINT); sigaddset(&set, SIGTERM); sigaddset(&set, SIGUSR1);
    if (pthread_sigmask(SIG_BLOCK, &set, NULL) != 0) return 1;
    orbit_runtime_options runtime_options = {.abi_version = ORBIT_ABI_VERSION, .execution = ORBIT_MULTI_THREAD, .native_threads = 1};
    uint64_t runtime = 0;
    uint32_t status = orbit_runtime_open(&runtime_options, &runtime);
    if (status != ORBIT_OK) { fprintf(stderr, "orbit_runtime_open: %u\n", status); return 1; }
    orbit_options options = {
        .abi_version = ORBIT_ABI_VERSION, .incarnation = 1,
        .max_in_flight = EXCHANGES, .max_queued = 0, .io_buffer_bytes = WINDOW
    };
    uint64_t app = 0;
    status = orbit_open(runtime, (const uint8_t *)argv[1], strlen(argv[1]),
        (const uint8_t *)argv[2], strlen(argv[2]), &options, &app);
    if (status != ORBIT_OK) {
        fprintf(stderr, "orbit_open: %u\n", status);
        orbit_runtime_release(runtime); return 1;
    }
    pthread_t signal_thread;
    if (pthread_create(&signal_thread, NULL, signals, &app) != 0) {
        orbit_cancel(app);
        orbit_event event;
        do { status = orbit_wait(app, &event); } while (status == ORBIT_OK && event.kind != ORBIT_APPLICATION_CLOSED);
        orbit_release(app); orbit_runtime_release(runtime); return 1;
    }
    printf("%s joined fleet %s\n", argv[2], argv[1]); fflush(stdout);
    request *requests = NULL;
    int result = 0;
    for (;;) {
        orbit_event event;
        status = orbit_wait(app, &event);
        if (status != ORBIT_OK) { result = 1; break; }
        if (event.kind == ORBIT_APPLICATION_CLOSED) break;
        if (event.kind == ORBIT_LIFECYCLE) {
            orbit_snapshot_record snapshot;
            if (orbit_lifecycle_result(app, &snapshot) != ORBIT_OK) result = 1;
            continue;
        }
        request *r = find(requests, event.exchange);
        if (event.kind == ORBIT_REQUEST) {
            r = calloc(1, sizeof(*r));
            if (!r) { orbit_cancel(app); result = 1; continue; }
            r->id = event.exchange; r->next = requests; requests = r;
            orbit_request_info_record info;
            status = orbit_request_info(r->id, &info);
            if (status != ORBIT_OK) { submitted(r, status); continue; }
            r->session = info.session != 0;
            r->phase = HEAD;
            status = r->session ? orbit_session_begin(r->id, MESSAGE_LIMIT) : orbit_reply_begin(r->id, 200);
            if (status != ORBIT_OK) { submitted(r, status); continue; }
            submitted(r, orbit_head_submit(r->id));
        } else if (!r) {
            /* Terminal notifications for a request allocation failure still own a handle. */
            if (event.kind == ORBIT_COMPLETE) orbit_exchange_release(app, event.exchange);
        } else if (event.kind == ORBIT_READ) {
            uint64_t length = 0;
            uint32_t kind = 0, code = 0;
            if (r->session) {
                status = orbit_message_read_result(r->id, &kind, &code, NULL, 0, &length, 0);
                uint8_t *data = length <= MESSAGE_LIMIT ? malloc(length ? (size_t)length : 1) : NULL;
                if (!data) {
                    orbit_message_read_result(r->id, &kind, &code, NULL, 0, &length, 1);
                    orbit_exchange_cancel(r->id); result = 1;
                } else {
                    status = orbit_message_read_result(r->id, &kind, &code, data, length, &length, 1);
                    if (status == ORBIT_OK && (kind == ORBIT_MESSAGE_TEXT || kind == ORBIT_MESSAGE_BINARY)) {
                        r->phase = MESSAGE;
                        submitted(r, orbit_message_write(r->id, kind, code, data, length));
                    } else if (status == ORBIT_OK) finish(r);
                    else orbit_exchange_cancel(r->id);
                    free(data); /* Writes copy input before returning. */
                }
            } else {
                uint8_t data[WINDOW];
                status = orbit_read_result(r->id, data, sizeof(data), &length);
                if (status != ORBIT_OK) orbit_exchange_cancel(r->id);
                else if (length == 0) finish(r);
                else { r->phase = STREAM; submitted(r, orbit_output_write(r->id, data, length)); }
            }
        } else if (event.kind == ORBIT_OUTPUT) {
            uint64_t written;
            status = orbit_output_result(r->id, &written);
            if (status != ORBIT_OK) orbit_exchange_cancel(r->id);
            else if (r->phase != FINISH) next_read(r);
        } else if (event.kind == ORBIT_COMPLETE) r->complete = 1;
        if (r && r->complete) {
            status = orbit_exchange_release(app, r->id);
            if (status == ORBIT_OK) {
                request **entry = &requests;
                while (*entry != r) entry = &(*entry)->next;
                *entry = r->next; free(r);
            } else if (status != ORBIT_BUSY) result = 1;
        }
    }
    pthread_kill(signal_thread, SIGUSR1); pthread_join(signal_thread, NULL);
    if (requests || orbit_release(app) != ORBIT_OK) result = 1;
    if (orbit_runtime_release(runtime) != ORBIT_OK) result = 1;
    return result;
}
