#define _GNU_SOURCE 1

#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

#ifdef __redox__
struct signalfd_siginfo {
    uint32_t ssi_signo;
    int32_t ssi_errno;
    int32_t ssi_code;
    uint32_t ssi_pid;
    uint32_t ssi_uid;
    int32_t ssi_fd;
    uint32_t ssi_tid;
    uint32_t ssi_band;
    uint32_t ssi_overrun;
    uint32_t ssi_trapno;
    int32_t ssi_status;
    int32_t ssi_int;
    uint64_t ssi_ptr;
    uint64_t ssi_utime;
    uint64_t ssi_stime;
    uint64_t ssi_addr;
    uint16_t ssi_addr_lsb, __pad2;
    int32_t ssi_syscall;
    uint64_t ssi_call_addr;
    uint32_t ssi_arch;
    unsigned char __pad[28];
};
int signalfd(int fd, const sigset_t *mask, size_t masksize);
_Static_assert(sizeof(struct signalfd_siginfo) == 128, "unexpected signalfd_siginfo size");
#else
#include <sys/signalfd.h>
#endif

static int fail_step(const char *step) {
    printf("FAIL signalfd: %s (errno=%d)\n", step, errno);
    return 1;
}

#ifdef __redox__
#define RB_SIGNALFD(fd, mask) signalfd((fd), (mask), sizeof(*(mask)))
#else
#define RB_SIGNALFD(fd, mask) signalfd((fd), (mask), 0)
#endif

int main(void) {
    sigset_t mask;
    sigset_t oldmask;
    struct signalfd_siginfo info;
    int sfd;
    int status;
    pid_t child;

    if (sigemptyset(&mask) < 0 || sigaddset(&mask, SIGUSR1) < 0) return fail_step("sigset setup");
    if (sigprocmask(SIG_BLOCK, &mask, &oldmask) < 0) return fail_step("sigprocmask block");
    sfd = RB_SIGNALFD(-1, &mask);
    if (sfd < 0) return fail_step("signalfd");

    child = fork();
    if (child < 0) return fail_step("fork");
    if (child == 0) {
        _Exit(kill(getppid(), SIGUSR1) < 0);
    }

    if (read(sfd, &info, sizeof(info)) != (ssize_t)sizeof(info)) return fail_step("read");
    if (waitpid(child, &status, 0) < 0) return fail_step("waitpid");
    if (close(sfd) < 0) return fail_step("close");
    if (sigprocmask(SIG_SETMASK, &oldmask, NULL) < 0) return fail_step("sigprocmask restore");

    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        printf("FAIL signalfd: child status=%d\n", status);
        return 1;
    }
    if (info.ssi_signo != (uint32_t)SIGUSR1) {
        printf("FAIL signalfd: ssi_signo=%u\n", info.ssi_signo);
        return 1;
    }
    if (info.ssi_code != SI_USER) {
        printf("FAIL signalfd: ssi_code=%d\n", info.ssi_code);
        return 1;
    }

    printf("PASS signalfd\n");
    return 0;
}
