#ifndef _LINUX_REFCOUNT_H
#define _LINUX_REFCOUNT_H

#include <linux/atomic.h>
#include <linux/mutex.h>
#include <linux/spinlock.h>

typedef struct {
    atomic_t refs;
} refcount_t;

#define REFCOUNT_INIT(value) { .refs = { .counter = (value) } }

static inline unsigned int refcount_read(const refcount_t *r)
{
    return (unsigned int)atomic_read(&r->refs);
}

static inline void refcount_set(refcount_t *r, int n)
{
    atomic_set(&r->refs, n);
}

static inline void refcount_inc(refcount_t *r)
{
    atomic_inc(&r->refs);
}

static inline int refcount_inc_not_zero(refcount_t *r)
{
    return atomic_inc_not_zero(&r->refs);
}

static inline int refcount_dec_and_test(refcount_t *r)
{
    return atomic_dec_and_test(&r->refs);
}

static inline int refcount_dec_not_one(refcount_t *r)
{
    int current;

    do {
        current = atomic_read(&r->refs);
        if (current == 1) {
            return 0;
        }
    } while (atomic_cmpxchg(&r->refs, current, current - 1) != current);

    return 1;
}

static inline int refcount_dec_and_mutex_lock(refcount_t *r, struct mutex *lock)
{
    if (!refcount_dec_and_test(r)) {
        return 0;
    }

    mutex_lock(lock);
    return 1;
}

static inline int refcount_dec_and_lock(refcount_t *r, spinlock_t *lock)
{
    if (!refcount_dec_and_test(r)) {
        return 0;
    }

    spin_lock(lock);
    return 1;
}

#endif /* _LINUX_REFCOUNT_H */
