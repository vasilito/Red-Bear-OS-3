#ifndef _LINUX_DEVICE_H
#define _LINUX_DEVICE_H

#include <linux/types.h>
#include <stddef.h>

struct device_driver {
    const char *name;
    void *owner;
};

struct device {
    struct device_driver *driver;
    void *driver_data;
    void *platform_data;
    void *of_node;
    u64 dma_mask;
};

static inline void *dev_get_drvdata(const struct device *dev)
{
    return dev->driver_data;
}

static inline void dev_set_drvdata(struct device *dev, void *data)
{
    dev->driver_data = data;
}

struct class {
    const char *name;
};

extern struct device *devm_kzalloc(struct device *dev, size_t size, gfp_t flags);
extern void devm_kfree(struct device *dev, void *ptr);

#endif
