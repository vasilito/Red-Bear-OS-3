#ifndef _LINUX_MM_H
#define _LINUX_MM_H

#include <linux/types.h>
#include <linux/slab.h>
#include <stddef.h>

struct page {
    unsigned char __opaque[64];
};

#define __get_free_pages(flags, order) \
    ((unsigned long)kmalloc(4096 << (order), (flags)))

#define free_pages(addr, order) \
    kfree((const void *)(addr))

static inline void *vmalloc(unsigned long size)
{
    return kmalloc(size, 0);
}

static inline void vfree(const void *addr)
{
    kfree(addr);
}

static inline unsigned long get_zeroed_page(unsigned int flags)
{
    void *p = kzalloc(4096, flags);
    return (unsigned long)p;
}

#define PageReserved(page) (0)

#endif
