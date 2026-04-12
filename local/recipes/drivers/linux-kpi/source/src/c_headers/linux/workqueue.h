#ifndef _LINUX_WORKQUEUE_H
#define _LINUX_WORKQUEUE_H

#include <linux/types.h>

struct work_struct {
    void (*func)(struct work_struct *work);
    unsigned char __opaque[64];
};

struct delayed_work {
    struct work_struct work;
    unsigned char __timer_opaque[64];
};

struct workqueue_struct {
    unsigned char __opaque[128];
};

typedef void (*work_func_t)(struct work_struct *work);

extern struct workqueue_struct *alloc_workqueue(const char *name,
                                                 unsigned int flags,
                                                 int max_active);
extern void destroy_workqueue(struct workqueue_struct *wq);
extern int  queue_work(struct workqueue_struct *wq, struct work_struct *work);
extern void flush_workqueue(struct workqueue_struct *wq);

#define INIT_WORK(_work, _func) \
    do { (_work)->func = (_func); } while(0)

#define INIT_DELAYED_WORK(_work, _func) \
    do { (_work)->work.func = (_func); } while(0)

extern int schedule_work(struct work_struct *work);
extern int schedule_delayed_work(struct delayed_work *dwork, unsigned long delay);
extern void flush_scheduled_work(void);

#define create_singlethread_workqueue(name) alloc_workqueue(name, 0, 1)
#define create_workqueue(name) alloc_workqueue(name, 0, 0)

#endif
