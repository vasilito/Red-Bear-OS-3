#ifndef _LINUX_JIFFIES_H
#define _LINUX_JIFFIES_H

#include "types.h"
#include <time.h>

static inline u64 redox_get_jiffies(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (u64)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

#define jiffies redox_get_jiffies()

#define msecs_to_jiffies(msec)  ((unsigned long)(msec))
#define usecs_to_jiffies(usec)  ((unsigned long)((usec) / 1000))

#define time_after(a, b)   ((long)((b) - (a)) < 0)
#define time_before(a, b)  time_after(b, a)

#define MAX_JIFFY_OFFSET ((unsigned long)(~0UL >> 1))

#endif
