#ifndef _LINUX_BUG_H
#define _LINUX_BUG_H

#include <stdio.h>
#include <stdlib.h>

#define BUG() \
    do { fprintf(stderr, "BUG: %s:%d\n", __FILE__, __LINE__); } while(0)

#define BUG_ON(condition) \
    do { if (unlikely(condition)) { BUG(); } } while(0)

#define WARN(condition, fmt, ...) \
    ({ \
        int __ret = !!(condition); \
        if (__ret) { fprintf(stderr, "WARN: %s:%d: " fmt "\n", \
                             __FILE__, __LINE__, ##__VA_ARGS__); } \
        __ret; \
    })

#define WARN_ON(condition) \
    ({ \
        int __ret = !!(condition); \
        if (__ret) { fprintf(stderr, "WARN: %s:%d\n", __FILE__, __LINE__); } \
        __ret; \
    })

#define WARN_ON_ONCE(condition) WARN_ON(condition)

#define BUILD_BUG_ON(condition) \
    extern char __build_bug_on[(condition) ? -1 : 1] __attribute__((unused))

#endif
