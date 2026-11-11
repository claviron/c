/* Real host-loop attachment and ownership check, without an Edge or listener. */
#include "orbit.h"
#include <pthread.h>
#include <stdio.h>
#include <string.h>

typedef struct {
    pthread_mutex_t mutex;
    pthread_cond_t ready;
    int signalled;
} host_loop;

static void wake(void *context) {
    host_loop *loop = context;
    pthread_mutex_lock(&loop->mutex);
    loop->signalled = 1;
    pthread_cond_signal(&loop->ready);
    pthread_mutex_unlock(&loop->mutex);
}

static uint32_t request(orbit_request *value, void *context) {
    (void)value; (void)context;
    return ORBIT_CANCELLED;
}

int main(int argc, char **argv) {
    if (argc != 2) { fprintf(stderr, "Usage: %s FLEET\n", argv[0]); return 2; }
    host_loop loop = {PTHREAD_MUTEX_INITIALIZER, PTHREAD_COND_INITIALIZER, 0};
    orbit_runtime_options execution = {.abi_version = ORBIT_ABI_VERSION,
        .execution = ORBIT_CURRENT_THREAD, .native_threads = 1};
    orbit_options options = {.abi_version = ORBIT_ABI_VERSION, .incarnation = 1,
        .max_in_flight = 1, .max_queued = 0, .io_buffer_bytes = 1024};
    uint64_t runtime = 0, application = 0;
    if (orbit_runtime_open(&execution, &runtime) != ORBIT_OK ||
        orbit_open(runtime, (const uint8_t *)argv[1], strlen(argv[1]),
            (const uint8_t *)"loop-check", 10, &options, &application) != ORBIT_OK) return 1;
    orbit_http_handler http = {.request = request};
    orbit_loop hooks = {.abi_version = ORBIT_ABI_VERSION, .struct_size = sizeof hooks,
        .wake = wake, .context = &loop};
    orbit_dispatcher *dispatcher = NULL, *second = NULL;
    if (orbit_dispatcher_open(application, &http, NULL, NULL, &hooks, &dispatcher) != ORBIT_OK) return 1;
    orbit_event event;
    if (orbit_wait(application, &event) != ORBIT_BUSY ||
        orbit_dispatcher_open(application, &http, NULL, NULL, &hooks, &second) != ORBIT_BUSY ||
        second != NULL || orbit_dispatcher_release(dispatcher) != ORBIT_BUSY ||
        orbit_runtime_release(runtime) != ORBIT_BUSY) return 1;
    uint64_t processed = 0;
    uint32_t closed = 0;
    if (orbit_dispatcher_poll(dispatcher, 0, &processed, &closed) != ORBIT_INVALID_ARGUMENT ||
        orbit_withdraw(application) != ORBIT_OK) return 1;
    while (!closed) {
        pthread_mutex_lock(&loop.mutex);
        while (!loop.signalled) pthread_cond_wait(&loop.ready, &loop.mutex);
        loop.signalled = 0;
        pthread_mutex_unlock(&loop.mutex);
        if (orbit_dispatcher_poll(dispatcher, 1, &processed, &closed) != ORBIT_OK) return 1;
    }
    if (orbit_release(application) != ORBIT_BUSY ||
        orbit_dispatcher_release(dispatcher) != ORBIT_OK ||
        orbit_release(application) != ORBIT_OK || orbit_runtime_release(runtime) != ORBIT_OK) return 1;
    pthread_cond_destroy(&loop.ready);
    pthread_mutex_destroy(&loop.mutex);
    puts("Dispatcher: exclusive consumption, budget validation and ordered release passed");
    return 0;
}
