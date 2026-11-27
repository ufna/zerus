/* Delay only the original private worker, before it can adopt its reservation. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

__attribute__((constructor))
static void hold_worker(int argc, char **argv, char **envp) {
    (void)envp;
    const char *log = getenv("AGY_TEST_WORKER_LOG");
    const char *release = getenv("AGY_TEST_WORKER_RELEASE");
    if (argc != 5 || strcmp(argv[1], "__state") || strcmp(argv[2], "agy-worker") ||
        !log || !release) {
        return;
    }
    if (!freopen(log, "w", stderr)) {
        _exit(91);
    }
    for (int i = 0; i < 2000; ++i) {
        if (access(release, F_OK) == 0) {
            return;
        }
        usleep(10000);
    }
    _exit(92);
}
