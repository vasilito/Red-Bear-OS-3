#ifndef _LINUX_COMPILER_H
#define _LINUX_COMPILER_H

#define __init
#define __exit
#define __devinit
#define __devexit

#define likely(x)   __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)

#define __read_mostly
#define __aligned(x) __attribute__((aligned(x)))
#define __packed     __attribute__((packed))
#define __cold       __attribute__((cold))
#define __hot        __attribute__((hot))

#define barrier() __asm__ __volatile__("" : : : "memory")

#define WRITE_ONCE(var, val) \
    (*((volatile typeof(var) *)&(var)) = (val))

#define READ_ONCE(var) \
    (*((volatile typeof(var) *)&(var)))

#define offsetof(TYPE, MEMBER) __builtin_offsetof(TYPE, MEMBER)

#define container_of(ptr, type, member) \
    ((type *)((char *)(ptr) - offsetof(type, member)))

#define ARRAY_SIZE(arr) (sizeof(arr) / sizeof((arr)[0]))

#define __same_type(a, b) __builtin_types_compatible_p(typeof(a), typeof(b))

#endif
