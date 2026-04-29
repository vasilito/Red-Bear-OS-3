#define _GNU_SOURCE 1

#include <errno.h>
#include <fcntl.h>
#include <semaphore.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail_step(const char *step) {
    printf("FAIL sem_open: %s (errno=%d)\n", step, errno);
    return 1;
}

int main(void) {
    static const char name[] = "/rb_test_sem";
    char go = 'G';
    char ready = 0;
    int child_value = -1;
    int parent_value = -1;
    int parent_to_child[2];
    int value = -1;
    int sync_pipe[2];
    int status;
    sem_t *sem;
    pid_t child;

    errno = 0;
    if (sem_unlink(name) < 0 && errno != ENOENT) return fail_step("sem_unlink pre-clean");
    sem = sem_open(name, O_CREAT, 0666, 0);
    if (sem == SEM_FAILED) return fail_step("sem_open create");
    if (sem_getvalue(sem, &value) < 0 || value != 0) {
        printf("FAIL sem_open: initial value=%d\n", value);
        return 1;
    }
    if (pipe(sync_pipe) < 0) return fail_step("pipe");
    if (pipe(parent_to_child) < 0) return fail_step("pipe parent_to_child");

    child = fork();
    if (child < 0) return fail_step("fork");
    if (child == 0) {
        ready = 'R';
        close(sync_pipe[0]);
        close(parent_to_child[1]);
        sem_t *child_sem = sem_open(name, 0);
        if (child_sem == SEM_FAILED) _Exit(1);
        if (write(sync_pipe[1], &ready, 1) != 1) _Exit(2);
        if (read(parent_to_child[0], &go, 1) != 1) _Exit(3);
        if (sem_wait(child_sem) < 0) _Exit(4);
        if (sem_getvalue(child_sem, &child_value) < 0 || child_value != 0) _Exit(5);
        if (sem_post(child_sem) < 0) _Exit(6);
        if (sem_getvalue(child_sem, &child_value) < 0 || child_value != 1) _Exit(7);
        if (sem_close(child_sem) < 0) _Exit(8);
        close(parent_to_child[0]);
        close(sync_pipe[1]);
        _Exit(0);
    }

    close(sync_pipe[1]);
    close(parent_to_child[0]);
    if (read(sync_pipe[0], &ready, 1) != 1) return fail_step("child ready read");
    if (sem_post(sem) < 0) return fail_step("parent sem_post");
    if (sem_getvalue(sem, &parent_value) < 0 || parent_value != 1) {
        printf("FAIL sem_open: post value=%d\n", parent_value);
        return 1;
    }
    if (write(parent_to_child[1], &go, 1) != 1) return fail_step("release child");
    if (close(parent_to_child[1]) < 0) return fail_step("close parent_to_child");
    if (read(sync_pipe[0], &ready, 1) != 0) return fail_step("child completion pipe");
    if (sem_getvalue(sem, &parent_value) < 0 || parent_value != 1) {
        printf("FAIL sem_open: child post value=%d\n", parent_value);
        return 1;
    }
    close(sync_pipe[0]);
    if (sem_wait(sem) < 0) return fail_step("parent sem_wait");
    if (waitpid(child, &status, 0) < 0) return fail_step("waitpid");
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        printf("FAIL sem_open: child status=%d\n", status);
        return 1;
    }
    if (sem_getvalue(sem, &value) < 0 || value != 0) {
        printf("FAIL sem_open: final value=%d\n", value);
        return 1;
    }
    if (sem_close(sem) < 0 || sem_unlink(name) < 0) return fail_step("cleanup");

    printf("PASS sem_open\n");
    return 0;
}
