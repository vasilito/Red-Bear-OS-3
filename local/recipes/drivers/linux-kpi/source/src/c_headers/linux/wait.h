#ifndef _LINUX_WAIT_H
#define _LINUX_WAIT_H

#include <linux/types.h>
#include <linux/compiler.h>

struct wait_queue_head {
    unsigned char __opaque[128];
};

static inline void init_waitqueue_head(struct wait_queue_head *wq)
{
    (void)wq;
}

#define wait_event(wq, condition) \
    do { while (!(condition)) { __asm__ volatile("pause"); } } while(0)

#define wait_event_timeout(wq, condition, timeout) \
    ({ (void)(wq); (condition) ? 1 : 0; })

#define wait_event_interruptible(wq, condition) \
    ({ (void)(wq); (condition) ? 0 : -512; })

#define wait_event_interruptible_timeout(wq, condition, timeout) \
    ({ (void)(wq); (condition) ? 1 : 0; })

static inline void wake_up(struct wait_queue_head *wq)
{
    (void)wq;
}

static inline void wake_up_interruptible(struct wait_queue_head *wq)
{
    (void)wq;
}

#define DEFINE_WAIT(name) \
    int name = 0

#define finish_wait(wq, wait) \
    do { (void)(wq); (void)(wait); } while(0)

#define prepare_to_wait(wq, wait, state) \
    do { (void)(wq); (void)(wait); (void)(state); } while(0)

#endif
