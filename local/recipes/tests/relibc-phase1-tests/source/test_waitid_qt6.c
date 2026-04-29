#define _GNU_SOURCE 1

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail_step(const char *step) {
    printf("FAIL waitid: %s (errno=%d)\n", step, errno);
    return 1;
}

int main(void) {
    siginfo_t info = {0};
    pid_t child = fork();

    if (child < 0) return fail_step("fork");
    if (child == 0) _Exit(42);
    if (waitid(P_PID, child, &info, WEXITED) < 0) return fail_step("waitid");
    if (info.si_code != CLD_EXITED) {
        printf("FAIL waitid: si_code=%d\n", info.si_code);
        return 1;
    }
    if (info.si_status != 42) {
        printf("FAIL waitid: si_status=%d\n", info.si_status);
        return 1;
    }

    printf("PASS waitid\n");
    return 0;
}
