#include "orbit.h"
#include <stdio.h>
#include <string.h>

static int join(uint64_t runtime, const char *fleet, const char *name, uint64_t *app) {
    orbit_options options = {.abi_version = ORBIT_ABI_VERSION, .incarnation = 1,
        .max_in_flight = 1, .max_queued = 0, .io_buffer_bytes = 1024};
    return orbit_open(runtime, (const uint8_t *)fleet, strlen(fleet),
        (const uint8_t *)name, strlen(name), &options, app) == ORBIT_OK;
}

static int close_app(uint64_t app) {
    if (orbit_cancel(app) != ORBIT_OK) return 0;
    orbit_event event;
    do {
        if (orbit_wait(app, &event) != ORBIT_OK) return 0;
        if (event.kind == ORBIT_LIFECYCLE) {
            orbit_snapshot_record snapshot;
            if (orbit_lifecycle_result(app, &snapshot) != ORBIT_OK) return 0;
        }
    } while (event.kind != ORBIT_APPLICATION_CLOSED);
    return orbit_release(app) == ORBIT_OK;
}

int main(int argc, char **argv) {
    if (argc != 2) { fprintf(stderr, "Usage: %s FLEET\n", argv[0]); return 2; }
    for (uint32_t mode = ORBIT_MULTI_THREAD; mode <= ORBIT_CURRENT_THREAD; mode++) {
        orbit_runtime_options options = {.abi_version = ORBIT_ABI_VERSION,
            .execution = mode, .native_threads = mode == ORBIT_MULTI_THREAD ? 2 : 1};
        uint64_t runtime = 0, first = 0, second = 0;
        if (orbit_runtime_open(&options, &runtime) != ORBIT_OK) return 1;
        if (!join(runtime, argv[1], "first", &first) || !join(runtime, argv[1], "second", &second)) return 1;
        if (orbit_runtime_release(runtime) != ORBIT_BUSY) return 1;
        if (!close_app(first) || orbit_runtime_release(runtime) != ORBIT_BUSY) return 1;
        if (!close_app(second) || orbit_runtime_release(runtime) != ORBIT_OK) return 1;
        printf("Runtime %s: two applications shared one executor; release stayed BUSY until both closed\n",
            mode == ORBIT_MULTI_THREAD ? "multi" : "current");
    }
    return 0;
}
