#ifndef _LINUX_FIRMWARE_H
#define _LINUX_FIRMWARE_H

#include <stddef.h>
#include "types.h"

struct firmware {
    size_t size;
    const u8 *data;
    void *priv;
};

struct device;

extern int  request_firmware(const struct firmware **fw, const char *name,
                             struct device *dev);
extern void release_firmware(const struct firmware *fw);

extern int request_firmware_nowait(
    struct device *dev, int uevent,
    const char *name, void *context,
    void (*cont)(const struct firmware *fw, void *context));

extern int request_firmware_direct(const struct firmware **fw,
                                   const char *name, struct device *dev);

#define FW_ACTION_HOTPLUG 0

#endif
