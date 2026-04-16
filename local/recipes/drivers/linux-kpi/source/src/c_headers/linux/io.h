#ifndef _LINUX_IO_H
#define _LINUX_IO_H

#include "types.h"
#include <stddef.h>

extern void *ioremap(phys_addr_t phys_addr, size_t size);
extern void  iounmap(void *addr, size_t size);

extern u32 readl(const void *addr);
extern void writel(u32 val, void *addr);
extern u64 readq(const void *addr);
extern void writeq(u64 val, void *addr);
extern u8  readb(const void *addr);
extern void writeb(u8 val, void *addr);
extern u16 readw(const void *addr);
extern void writew(u16 val, void *addr);

static inline void memcpy_toio(void *dst, const void *src, size_t count)
{
    __builtin_memcpy(dst, src, count);
}

static inline void memcpy_fromio(void *dst, const void *src, size_t count)
{
    __builtin_memcpy(dst, src, count);
}

static inline void memset_io(void *dst, int c, size_t count)
{
    __builtin_memset(dst, c, count);
}

static inline void mb(void)
{
    __sync_synchronize();
}

static inline void rmb(void)
{
    __sync_synchronize();
}

static inline void wmb(void)
{
    __sync_synchronize();
}

#define ioread8(addr)    readb(addr)
#define ioread16(addr)   readw(addr)
#define ioread32(addr)   readl(addr)
#define iowrite8(v, a)   writeb(v, a)
#define iowrite16(v, a)  writew(v, a)
#define iowrite32(v, a)  writel(v, a)

#endif
