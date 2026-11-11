#define _POSIX_C_SOURCE 200809L
#include "orbit.h"
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>

/* The example declares its budgets; the library invents none. */
enum { EXCHANGES = 32, WINDOW = 8192, MESSAGE_LIMIT = 65536 };

static uint32_t http_request(orbit_request *request, void *context) {
    (void)context;
    uint32_t status = orbit_response_begin(request, 200);
    return status == ORBIT_OK ? orbit_request_read_body(request) : status;
}

static uint32_t http_body(orbit_request *request, const uint8_t *data,
    uint64_t length, uint32_t end, void *context) {
    (void)context;
    return end ? orbit_response_finish(request) : orbit_response_write(request, data, length);
}

static uint32_t socket_open(orbit_request *request, void *context) {
    (void)context;
    return orbit_session_accept(request, MESSAGE_LIMIT);
}

static uint32_t socket_message(orbit_request *request, uint32_t kind,
    uint32_t code, const uint8_t *data, uint64_t length, void *context) {
    (void)context;
    /* Native I/O answers peer close; an application does not send it twice. */
    if (kind == ORBIT_MESSAGE_END || kind == ORBIT_MESSAGE_CLOSE) return ORBIT_OK;
    return orbit_session_send(request, kind, code, data, length);
}

static void *signals(void *value) {
    sigset_t set;
    sigemptyset(&set); sigaddset(&set, SIGINT); sigaddset(&set, SIGTERM); sigaddset(&set, SIGUSR1);
    int received;
    if (sigwait(&set, &received) == 0 && received != SIGUSR1)
        orbit_withdraw(*(const uint64_t *)value);
    return NULL;
}

int main(int argc, char **argv) {
    if (argc != 3) { fprintf(stderr, "Usage: %s FLEET APPLICATION\n", argv[0]); return 2; }
    sigset_t set;
    sigemptyset(&set); sigaddset(&set, SIGINT); sigaddset(&set, SIGTERM); sigaddset(&set, SIGUSR1);
    if (pthread_sigmask(SIG_BLOCK, &set, NULL) != 0) return 1;
    orbit_runtime_options execution = {.abi_version = ORBIT_ABI_VERSION, .execution = ORBIT_MULTI_THREAD, .native_threads = 1};
    orbit_options options = {.abi_version = ORBIT_ABI_VERSION, .incarnation = 1,
        .max_in_flight = EXCHANGES, .max_queued = 0, .io_buffer_bytes = WINDOW};
    uint64_t runtime = 0, app = 0;
    uint32_t status = orbit_runtime_open(&execution, &runtime);
    if (status != ORBIT_OK) return 1;
    status = orbit_open(runtime, (const uint8_t *)argv[1], strlen(argv[1]),
        (const uint8_t *)argv[2], strlen(argv[2]), &options, &app);
    if (status != ORBIT_OK) { orbit_runtime_release(runtime); return 1; }
    pthread_t waiter;
    if (pthread_create(&waiter, NULL, signals, &app) != 0) {
        orbit_cancel(app);
        orbit_event event;
        do { status = orbit_wait(app, &event); } while (status == ORBIT_OK && event.kind != ORBIT_APPLICATION_CLOSED);
        orbit_release(app); orbit_runtime_release(runtime); return 1;
    }
    orbit_http_handler http = {.request = http_request, .body = http_body};
    orbit_session_handler socket = {.open = socket_open, .message = socket_message};
    printf("%s joined fleet %s\n", argv[2], argv[1]); fflush(stdout);
    status = orbit_serve(app, &http, &socket, &socket);
    pthread_kill(waiter, SIGUSR1); pthread_join(waiter, NULL);
    int failed = status != ORBIT_OK;
    if (orbit_release(app) != ORBIT_OK) failed = 1;
    if (orbit_runtime_release(runtime) != ORBIT_OK) failed = 1;
    return failed;
}
