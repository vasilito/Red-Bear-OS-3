#ifndef _LINUX_SPINLOCK_H
#define _LINUX_SPINLOCK_H

#include <linux/types.h>

typedef struct spinlock {
    volatile unsigned char __locked;
} spinlock_t;

extern void spin_lock_init(spinlock_t *lock);
extern void spin_lock(spinlock_t *lock);
extern void spin_unlock(spinlock_t *lock);
extern unsigned long spin_lock_irqsave(spinlock_t *lock, unsigned long *flags);
extern void spin_unlock_irqrestore(spinlock_t *lock, unsigned long flags);

static inline void spin_lock_irq(spinlock_t *lock)
{
    spin_lock(lock);
}

static inline void spin_unlock_irq(spinlock_t *lock)
{
    spin_unlock(lock);
}

#define DEFINE_SPINLOCK(name) spinlock_t name = { .__locked = 0 }

#endif
