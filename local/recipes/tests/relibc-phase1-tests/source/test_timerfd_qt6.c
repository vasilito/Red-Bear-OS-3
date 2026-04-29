#define _GNU_SOURCE 1

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>
#include <unistd.h>

#ifdef __redox__
#define TFD_NONBLOCK 0x800
int timerfd_create(clockid_t clockid, int flags);
int timerfd_settime(int fd, int flags, const struct itimerspec *new_value, struct itimerspec *old_value);
#else
#include <sys/timerfd.h>
#endif

static int fail_step(const char *step) {
    printf("FAIL timerfd: %s (errno=%d)\n", step, errno);
    return 1;
}

int main(void) {
    const struct timespec pause = {.tv_sec = 0, .tv_nsec = 20000000};
    struct itimerspec spec = {{0, 0}, {0, 100000000}};
    uint64_t expirations = 0;
    int tfd = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK);

    if (tfd < 0) return fail_step("timerfd_create");
    if (timerfd_settime(tfd, 0, &spec, NULL) < 0) return fail_step("timerfd_settime");

    for (int i = 0; i < 50; ++i) {
        ssize_t n = read(tfd, &expirations, sizeof(expirations));
        if (n == (ssize_t)sizeof(expirations)) {
            if (expirations >= 1) {
                if (close(tfd) < 0) return fail_step("close");
                printf("PASS timerfd\n");
                return 0;
            }
            printf("FAIL timerfd: expirations=%llu\n", (unsigned long long)expirations);
            return 1;
        }
        if (n >= 0 || (errno != EAGAIN && errno != EWOULDBLOCK)) return fail_step("read");
        nanosleep(&pause, NULL);
    }

    printf("FAIL timerfd: timeout waiting for expiration\n");
    return 1;
}
