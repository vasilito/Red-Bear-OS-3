#ifndef _LINUX_TIMER_H
#define _LINUX_TIMER_H

#include <linux/types.h>
#include <linux/compiler.h>

struct timer_list {
    void (*function)(unsigned long data);
    unsigned long data;
    unsigned long expires;
    unsigned char __opaque[64];
};

extern void setup_timer(struct timer_list *timer,
                        void (*function)(unsigned long),
                        unsigned long data);
extern int mod_timer(struct timer_list *timer, unsigned long expires);
extern int del_timer(struct timer_list *timer);
extern int del_timer_sync(struct timer_list *timer);
extern int timer_pending(const struct timer_list *timer);

#define DEFINE_TIMER(_name, _function, _flags, _data) \
    struct timer_list _name = { .function = (_function), .data = (_data) }

#endif
