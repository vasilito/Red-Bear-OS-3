#ifndef _LINUX_ERRNO_H
#define _LINUX_ERRNO_H

#define EPERM           1
#define ENOENT          2
#define ESRCH           3
#define EINTR           4
#define EIO             5
#define ENXIO           6
#define E2BIG           7
#define ENOEXEC         8
#define EBADF           9
#define ECHILD         10
#define EAGAIN         11
#define ENOMEM         12
#define EACCES         13
#define EFAULT         14
#define EBUSY          16
#define EEXIST         17
#define ENODEV         19
#define EINVAL         22
#define ENFILE         23
#define EMFILE         24
#define ENOTTY         25
#define EPIPE          32
#define ERANGE         34
#define ENOSYS         38
#define ENODATA        61
#define ENOTSUP        95
#define ETIMEDOUT     110

#define IS_ERR_VALUE(x) unlikely((unsigned long)(void *)(x) >= (unsigned long)-4096)

#endif
