#define _GNU_SOURCE 1

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>

#ifdef __redox__
#define EFD_CLOEXEC 0x80000
#define EFD_NONBLOCK 0x800
int eventfd(unsigned int initval, int flags);
#else
#include <sys/eventfd.h>
#endif

static int fail_step(const char *step) {
    printf("FAIL eventfd: %s (errno=%d)\n", step, errno);
    return 1;
}

int main(void) {
    uint64_t expected = 42;
    uint64_t observed = 0;
    int efd = eventfd(0, EFD_NONBLOCK | EFD_CLOEXEC);

    if (efd < 0) return fail_step("eventfd");
    if (write(efd, &expected, sizeof(expected)) != (ssize_t)sizeof(expected)) return fail_step("write first");
    if (read(efd, &observed, sizeof(observed)) != (ssize_t)sizeof(observed)) return fail_step("read first");
    if (observed != expected) {
        printf("FAIL eventfd: first read=%llu\n", (unsigned long long)observed);
        return 1;
    }

    expected = 7;
    if (write(efd, &expected, sizeof(expected)) != (ssize_t)sizeof(expected)) return fail_step("write second");
    if (read(efd, &observed, sizeof(observed)) != (ssize_t)sizeof(observed)) return fail_step("read second");
    if (observed != expected) {
        printf("FAIL eventfd: second read=%llu\n", (unsigned long long)observed);
        return 1;
    }
    if (close(efd) < 0) return fail_step("close");

    printf("PASS eventfd\n");
    return 0;
}
