#ifndef _LINUX_SLAB_H
#define _LINUX_SLAB_H

#include <linux/types.h>
#include <stddef.h>

#define GFP_KERNEL  0U
#define GFP_ATOMIC  1U
#define GFP_DMA32   2U
#define GFP_HIGHUSER 3U
#define GFP_NOWAIT  4U
#define GFP_DMA     5U

#define __GFP_NOWARN  0U
#define __GFP_ZERO    0U

extern void *kmalloc(size_t size, gfp_t flags);
extern void *kzalloc(size_t size, gfp_t flags);
extern void  kfree(const void *ptr);

#define kmalloc_array(n, size, flags) \
    kmalloc((n) * (size), flags)

#define kcalloc(n, size, flags) \
    kzalloc((n) * (size), flags)

#define kmemdup(src, len, flags) ({          \
    void *__p = kmalloc(len, flags);         \
    if (__p) __builtin_memcpy(__p, src, len); \
    __p;                                      \
})

#endif
