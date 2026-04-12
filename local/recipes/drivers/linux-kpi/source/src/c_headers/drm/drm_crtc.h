#ifndef _DRM_DRM_CRTC_H
#define _DRM_DRM_CRTC_H

#include <linux/types.h>
#include <stddef.h>

struct drm_crtc {
    void *dev;
    void *primary;
    void *cursor;
    u32 index;
    char name[32];
    bool enabled;
    int x;
    int y;
    u32 width;
    u32 height;
};

struct drm_connector {
    void *dev;
    u32 connector_type;
    u32 connector_type_id;
    int status;
    char name[32];
};

struct drm_encoder {
    void *dev;
    u32 encoder_type;
    u32 possible_crtcs;
    u32 possible_clones;
};

struct drm_display_mode {
    u32 clock;
    u16 hdisplay;
    u16 hsync_start;
    u16 hsync_end;
    u16 htotal;
    u16 hskew;
    u16 vdisplay;
    u16 vsync_start;
    u16 vsync_end;
    u16 vtotal;
    u16 vscan;
    u32 flags;
    u32 type;
    char name[32];
};

struct drm_mode_fb_cmd {
    u32 fb_id;
    u32 width;
    u32 height;
    u32 pitch;
    u32 bpp;
    u32 depth;
    u32 handle;
};

#define DRM_MODE_TYPE_BUILTIN   (1 << 0)
#define DRM_MODE_TYPE_CLOCK_C   ((1 << 1) | (1 << 2))
#define DRM_MODE_TYPE_CRTC_C    ((1 << 3) | (1 << 4))

#define DRM_MODE_FLAG_PHSYNC    (1 << 0)
#define DRM_MODE_FLAG_NHSYNC    (1 << 1)
#define DRM_MODE_FLAG_PVSYNC    (1 << 2)
#define DRM_MODE_FLAG_NVSYNC    (1 << 3)

#define DRM_CONNECTOR_STATUS_UNKNOWN  0
#define DRM_CONNECTOR_STATUS_CONNECTED 1
#define DRM_CONNECTOR_STATUS_DISCONNECTED 2

#endif
