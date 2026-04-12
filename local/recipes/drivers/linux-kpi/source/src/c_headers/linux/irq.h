#ifndef _LINUX_IRQ_H
#define _LINUX_IRQ_H

#include <linux/types.h>

typedef unsigned int irqreturn_t;

#define IRQ_NONE       0
#define IRQ_HANDLED    1
#define IRQ_WAKE_THREAD 2

#define IRQF_SHARED         0x0001U
#define IRQF_TRIGGER_RISING 0x0010U
#define IRQF_TRIGGER_FALLING 0x0020U
#define IRQF_TRIGGER_HIGH   0x0040U
#define IRQF_TRIGGER_LOW    0x0080U

typedef irqreturn_t (*irq_handler_t)(int irq, void *dev_id);

extern int  request_irq(unsigned int irq, irq_handler_t handler,
                        unsigned long flags, const char *name, void *dev_id);
extern void free_irq(unsigned int irq, void *dev_id);

#endif
