#include "orbit.h"
#include <stddef.h>
#include <stdio.h>

_Static_assert(sizeof(orbit_abi_info) == 56, "ABI query layout");
_Static_assert(sizeof(orbit_options) == 56, "options layout");
_Static_assert(offsetof(orbit_options, max_queued) == 24, "queue limit offset");
_Static_assert(sizeof(orbit_runtime_options) == 12, "runtime options layout");
_Static_assert(sizeof(orbit_event) == 16, "event layout");
_Static_assert(sizeof(orbit_request_info_record) == 32, "request info layout");
_Static_assert(offsetof(orbit_request_info_record, generation) == 16, "generation offset");

int main(void) {
    orbit_abi_info abi = {0};
    if (orbit_abi(&abi) != ORBIT_OK || abi.version != ORBIT_ABI_VERSION ||
        abi.event_size != sizeof(orbit_event) ||
        abi.options_size != sizeof(orbit_options) ||
        abi.info_size != sizeof(orbit_request_info_record)) return 1;
    if (orbit_abi(NULL) != ORBIT_INVALID_ARGUMENT ||
        orbit_cancel(UINT64_MAX) != ORBIT_INVALID_HANDLE) return 1;
    printf("Orbit C ABI %u: C/Rust layouts and checked handles agree\n", abi.version);
    return 0;
}
