/* Redox POSIX compat: waitid() stub for relibc.
   relibc has waitpid() but waitid() is unimplemented (commented out).
   Qt's forkfd.c calls waitid(P_PID, ...) for child readiness checks.
   Returning -1 (no children ready) is safe — fork() still works for process spawning. */
#include <signal.h>

int waitid(int idtype, int id, siginfo_t *info, int options) {
    (void)idtype;
    (void)id;
    (void)info;
    (void)options;
    return -1;
}
