#define _GNU_SOURCE 1

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

static int fail_step(const char *step) {
    printf("FAIL shm_open: %s (errno=%d)\n", step, errno);
    return 1;
}

int main(void) {
    static const char name[] = "/rb_test_shm";
    uint32_t *first;
    uint32_t *second;
    int fd;
    int second_fd;

    errno = 0;
    if (shm_unlink(name) < 0 && errno != ENOENT) return fail_step("shm_unlink pre-clean");
    fd = shm_open(name, O_CREAT | O_RDWR, 0666);
    if (fd < 0) return fail_step("shm_open");
    if (ftruncate(fd, 4096) < 0) return fail_step("ftruncate");
    second_fd = shm_open(name, O_RDWR, 0666);
    if (second_fd < 0) return fail_step("shm_open reopen");

    first = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (first == MAP_FAILED) return fail_step("mmap first");
    second = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, second_fd, 0);
    if (second == MAP_FAILED) return fail_step("mmap second");
    *first = 0xDEADBEEFU;
    if (*second != 0xDEADBEEFU) {
        printf("FAIL shm_open: observed=0x%08X\n", *second);
        return 1;
    }
    if (munmap(second, 4096) < 0) return fail_step("munmap second");
    if (munmap(first, 4096) < 0) return fail_step("munmap first");
    if (close(second_fd) < 0) return fail_step("close second");
    if (close(fd) < 0) return fail_step("close");
    if (shm_unlink(name) < 0) return fail_step("shm_unlink");

    printf("PASS shm_open\n");
    return 0;
}
