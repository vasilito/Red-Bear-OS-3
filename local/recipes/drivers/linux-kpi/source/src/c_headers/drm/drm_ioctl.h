#ifndef _DRM_DRM_IOCTL_H
#define _DRM_DRM_IOCTL_H

#include <linux/types.h>

struct drm_file {
    u32 pid;
    u32 uid;
    int authenticated;
    int master;
    void *driver_priv;
};

struct drm_device {
    const char *name;
    const char *desc;
    u32 driver_features;
    void *dev_private;
    void *pdev;
    u32 irq;
    void *mode_config;
    void *primary;
    void *render;
    int unplugged;
};

#define DRIVER_USE_AGP       0x1U
#define DRIVER_REQUIRE_AGP   0x2U
#define DRIVER_GEM           0x8U
#define DRIVER_MODESET       0x10U
#define DRIVER_PRIME         0x20U
#define DRIVER_RENDER        0x40U
#define DRIVER_ATOMIC        0x80U
#define DRIVER_SYNCOBJ       0x100U

struct drm_driver {
    const char *name;
    const char *desc;
    u32 driver_features;
    int (*load)(struct drm_device *dev, unsigned long flags);
    void (*unload)(struct drm_device *dev);
    int (*open)(struct drm_device *dev, struct drm_file *file);
    void (*preclose)(struct drm_device *dev, struct drm_file *file);
    void (*postclose)(struct drm_device *dev, struct drm_file *file);
    void (*lastclose)(struct drm_device *dev);
    int (*dma_ioctl)(struct drm_device *dev, void *data, struct drm_file *file);
    void (*irq_handler)(int irq, void *arg);
};

extern int  drm_dev_register(struct drm_device *dev, unsigned long flags);
extern void drm_dev_unregister(struct drm_device *dev);
extern int  drm_ioctl(struct drm_device *dev, unsigned int cmd, void *data,
                      struct drm_file *file);

#endif
