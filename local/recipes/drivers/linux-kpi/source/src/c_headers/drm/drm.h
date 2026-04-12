#ifndef _DRM_DRM_H
#define _DRM_DRM_H

#include <linux/types.h>
#include <stddef.h>

#define DRM_NAME    "drm"
#define DRM_MINORS  256

#define DRM_IOCTL_BASE       'd'
#define DRM_IO(nr)           _IO(DRM_IOCTL_BASE, nr)
#define DRM_IOR(nr,type)     _IOR(DRM_IOCTL_BASE, nr, type)
#define DRM_IOW(nr,type)     _IOW(DRM_IOCTL_BASE, nr, type)
#define DRM_IOWR(nr,type)    _IOWR(DRM_IOCTL_BASE, nr, type)

struct drm_version {
    int version_major;
    int version_minor;
    int version_patchlevel;
    size_t name_len;
    char *name;
    size_t date_len;
    char *date;
    size_t desc_len;
    char *desc;
};

struct drm_unique {
    size_t unique_len;
    char *unique;
};

#define _IO(type, nr)     ((type) << 8 | (nr))
#define _IOR(type, nr, t) ((type) << 8 | (nr))
#define _IOW(type, nr, t) ((type) << 8 | (nr))
#define _IOWR(type, nr, t) ((type) << 8 | (nr))

#endif
