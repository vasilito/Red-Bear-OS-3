#ifndef _LINUX_INTERRUPT_H
#define _LINUX_INTERRUPT_H

#include "types.h"
#include "irq.h"
#include "spinlock.h"

extern void local_irq_save(unsigned long *flags);
extern void local_irq_restore(unsigned long flags);
extern void local_irq_disable(void);
extern void local_irq_enable(void);
extern int irqs_disabled(void);

static inline int in_interrupt(void) { return irqs_disabled(); }
static inline int in_irq(void) { return irqs_disabled(); }

#define disable_irq_nosync(irq) ((void)(irq))
#define enable_irq(irq)         ((void)(irq))

#define IRQF_NO_SUSPEND     0x0000U
#define IRQF_FORCE_RESUME   0x0000U
#define IRQF_NO_THREAD      0x0000U
#define IRQF_EARLY_RESUME   0x0000U

#endif
