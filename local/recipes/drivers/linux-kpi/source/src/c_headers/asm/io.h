#ifndef _ASM_IO_H
#define _ASM_IO_H

#include <linux/types.h>
#include <linux/compiler.h>

static inline unsigned char inb(unsigned short port)
{
    unsigned char val;
    __asm__ __volatile__("inb %1, %0" : "=a"(val) : "Nd"(port));
    return val;
}

static inline unsigned short inw(unsigned short port)
{
    unsigned short val;
    __asm__ __volatile__("inw %1, %0" : "=a"(val) : "Nd"(port));
    return val;
}

static inline unsigned int inl(unsigned short port)
{
    unsigned int val;
    __asm__ __volatile__("inl %1, %0" : "=a"(val) : "Nd"(port));
    return val;
}

static inline void outb(unsigned char val, unsigned short port)
{
    __asm__ __volatile__("outb %0, %1" : : "a"(val), "Nd"(port));
}

static inline void outw(unsigned short val, unsigned short port)
{
    __asm__ __volatile__("outw %0, %1" : : "a"(val), "Nd"(port));
}

static inline void outl(unsigned int val, unsigned short port)
{
    __asm__ __volatile__("outl %0, %1" : : "a"(val), "Nd"(port));
}

static inline void insb(unsigned short port, void *buf, unsigned long count)
{
    __asm__ __volatile__("rep insb" : "+D"(buf), "+c"(count) : "d"(port) : "memory");
}

static inline void insw(unsigned short port, void *buf, unsigned long count)
{
    __asm__ __volatile__("rep insw" : "+D"(buf), "+c"(count) : "d"(port) : "memory");
}

static inline void insl(unsigned short port, void *buf, unsigned long count)
{
    __asm__ __volatile__("rep insl" : "+D"(buf), "+c"(count) : "d"(port) : "memory");
}

static inline void outsb(unsigned short port, const void *buf, unsigned long count)
{
    __asm__ __volatile__("rep outsb" : "+S"(buf), "+c"(count) : "d"(port) : "memory");
}

static inline void outsw(unsigned short port, const void *buf, unsigned long count)
{
    __asm__ __volatile__("rep outsw" : "+S"(buf), "+c"(count) : "d"(port) : "memory");
}

static inline void outsl(unsigned short port, const void *buf, unsigned long count)
{
    __asm__ __volatile__("rep outsl" : "+S"(buf), "+c"(count) : "d"(port) : "memory");
}

#define mb()    __asm__ __volatile__("mfence" : : : "memory")
#define rmb()   __asm__ __volatile__("lfence" : : : "memory")
#define wmb()   __asm__ __volatile__("sfence" : : : "memory")

#endif
