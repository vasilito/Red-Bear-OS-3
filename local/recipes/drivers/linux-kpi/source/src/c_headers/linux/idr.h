#ifndef _LINUX_IDR_H
#define _LINUX_IDR_H

#include <linux/types.h>

struct idr {
    unsigned char __opaque[256];
};

static inline void idr_init(struct idr *idr)
{
    (void)idr;
}

static inline int idr_alloc(struct idr *idr, void *ptr, int start, int end, u32 flags)
{
    (void)idr;
    (void)ptr;
    (void)start;
    (void)end;
    (void)flags;
    return 0;
}

static inline void idr_remove(struct idr *idr, int id)
{
    (void)idr;
    (void)id;
}

static inline void *idr_find(struct idr *idr, int id)
{
    (void)idr;
    (void)id;
    return (void *)0;
}

static inline void idr_destroy(struct idr *idr)
{
    (void)idr;
}

#define idr_for_each_entry(idr, entry, id) \
    for ((id) = 0, (entry) = (void *)0; (entry); (id)++)

#endif
