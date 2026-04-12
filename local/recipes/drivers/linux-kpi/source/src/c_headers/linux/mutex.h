#ifndef _LINUX_MUTEX_H
#define _LINUX_MUTEX_H

#include <linux/types.h>

struct mutex {
    unsigned char __opaque[64];
};

extern void mutex_init(struct mutex *lock);
extern void mutex_lock(struct mutex *lock);
extern void mutex_unlock(struct mutex *lock);
extern int  mutex_is_locked(struct mutex *lock);

static inline int mutex_trylock(struct mutex *lock)
{
    (void)lock;
    return 1;
}

#define DEFINE_MUTEX(name) struct mutex name = { .__opaque = {0} }

#endif
