#ifndef _LINUX_ATOMIC_H
#define _LINUX_ATOMIC_H

#include <linux/types.h>

typedef struct {
    volatile int counter;
} atomic_t;

typedef struct {
    volatile long counter;
} atomic_long_t;

static inline int atomic_read(const atomic_t *v)
{
    return __sync_fetch_and_add((volatile int *)&v->counter, 0) + v->counter;
}

static inline void atomic_set(atomic_t *v, int i)
{
    v->counter = i;
    __sync_synchronize();
}

static inline void atomic_inc(atomic_t *v)
{
    __sync_fetch_and_add(&v->counter, 1);
}

static inline void atomic_dec(atomic_t *v)
{
    __sync_fetch_and_sub(&v->counter, 1);
}

static inline void atomic_add(int i, atomic_t *v)
{
    __sync_fetch_and_add(&v->counter, i);
}

static inline void atomic_sub(int i, atomic_t *v)
{
    __sync_fetch_and_sub(&v->counter, i);
}

static inline int atomic_inc_and_test(atomic_t *v)
{
    return __sync_add_and_fetch(&v->counter, 1) == 0;
}

static inline int atomic_dec_and_test(atomic_t *v)
{
    return __sync_sub_and_fetch(&v->counter, 1) == 0;
}

static inline int atomic_add_return(int i, atomic_t *v)
{
    return __sync_add_and_fetch(&v->counter, i);
}

static inline int atomic_sub_return(int i, atomic_t *v)
{
    return __sync_sub_and_fetch(&v->counter, i);
}

static inline int atomic_inc_return(atomic_t *v)
{
    return __sync_add_and_fetch(&v->counter, 1);
}

static inline int atomic_dec_return(atomic_t *v)
{
    return __sync_sub_and_fetch(&v->counter, 1);
}

static inline int atomic_xchg(atomic_t *v, int new_val)
{
    return __sync_lock_test_and_set(&v->counter, new_val);
}

static inline int atomic_cmpxchg(atomic_t *v, int old_val, int new_val)
{
    return __sync_val_compare_and_swap(&v->counter, old_val, new_val);
}

static inline int atomic_add_unless(atomic_t *v, int a, int u)
{
    int c = v->counter;
    while (c != u && !__sync_bool_compare_and_swap(&v->counter, c, c + a))
        c = v->counter;
    return c != u;
}

#define atomic_inc_not_zero(v) atomic_add_unless((v), 1, 0)

#define smp_mb()  __sync_synchronize()
#define smp_rmb() __sync_synchronize()
#define smp_wmb() __sync_synchronize()

#endif
