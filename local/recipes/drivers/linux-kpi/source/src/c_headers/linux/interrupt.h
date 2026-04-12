#ifndef _LINUX_INTERRUPT_H
#define _LINUX_INTERRUPT_H

#include <linux/types.h>
#include <linux/irq.h>

static inline int in_interrupt(void)
{
    return 0;
}

static inline int in_irq(void)
{
    return 0;
}

static inline void local_irq_save(unsigned long *flags)
{
    (void)flags;
}

static inline void local_irq_restore(unsigned long flags)
{
    (void)flags;
}

static inline void local_irq_disable(void) {}
static inline void local_irq_enable(void) {}

#define disable_irq_nosync(irq) ((void)(irq))
#define enable_irq(irq)         ((void)(irq))

#define IRQF_NO_SUSPEND     0x0000U
#define IRQF_FORCE_RESUME   0x0000U
#define IRQF_NO_THREAD      0x0000U
#define IRQF_EARLY_RESUME   0x0000U

#endif
