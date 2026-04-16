#ifndef _LINUX_KERNEL_H
#define _LINUX_KERNEL_H

#include "compiler.h"
#include "types.h"
#include <stddef.h>
#include <stdio.h>
#include <unistd.h>

#define min(a, b) \
    ({ typeof(a) _a = (a); typeof(b) _b = (b); _a < _b ? _a : _b; })

#define max(a, b) \
    ({ typeof(a) _a = (a); typeof(b) _b = (b); _a > _b ? _a : _b; })

#define clamp(val, lo, hi) min(max(val, lo), hi)

#define min_t(type, a, b) \
    ((type)(a) < (type)(b) ? (type)(a) : (type)(b))

#define max_t(type, a, b) \
    ((type)(a) > (type)(b) ? (type)(a) : (type)(b))

#define min3(a, b, c) min((a), min((b), (c)))
#define max3(a, b, c) max((a), max((b), (c)))

#define DIV_ROUND_UP(n, d) (((n) + (d) - 1) / (d))
#define DIV_ROUND_DOWN(n, d) ((n) / (d))
#define DIV_ROUND_CLOSEST(n, d) (((n) + (d) / 2) / (d))

#define round_up(x, y) ((((x) + (y) - 1) / (y)) * (y))
#define round_down(x, y) (((x) / (y)) * (y))

#define ALIGN(x, a) (((x) + (a) - 1) & ~((a) - 1))
#define IS_ALIGNED(x, a) (((x) & ((a) - 1)) == 0)

#define swap(a, b) \
    do { typeof(a) __tmp = (a); (a) = (b); (b) = __tmp; } while(0)

static inline void msleep(unsigned int msecs)
{
    usleep(msecs * 1000);
}

static inline void udelay(unsigned long usecs)
{
    usleep(usecs);
}

static inline void mdelay(unsigned long msecs)
{
    usleep(msecs * 1000);
}

#define lower_32_bits(n) ((u32)(n))
#define upper_32_bits(n) ((u32)(((n) >> 16) >> 16))

#define FIELD_SIZEOF(t, f) (sizeof(((t *)0)->f))

#define roundup(x, y) ((((x) + (y) - 1) / (y)) * (y))

#endif
