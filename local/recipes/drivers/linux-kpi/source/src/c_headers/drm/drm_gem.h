#ifndef _DRM_DRM_GEM_H
#define _DRM_DRM_GEM_H

#include <linux/types.h>
#include <stddef.h>

struct drm_device;
struct drm_file;

struct drm_gem_object {
    void *dev;
    u32 handle_count;
    size_t size;
    void *driver_private;
};

struct drm_gem_object_ops {
    void (*free)(struct drm_gem_object *obj);
    int  (*open)(struct drm_gem_object *obj, struct drm_file *file);
    void (*close)(struct drm_gem_object *obj, struct drm_file *file);
    int  (*pin)(struct drm_gem_object *obj);
    void (*unpin)(struct drm_gem_object *obj);
    int  (*get_sg_table)(struct drm_gem_object *obj);
    void *(*vmap)(struct drm_gem_object *obj);
    void (*vunmap)(struct drm_gem_object *obj, void *vaddr);
};

extern int  drm_gem_object_init(struct drm_device *dev,
                                struct drm_gem_object *obj, size_t size);
extern void drm_gem_object_release(struct drm_gem_object *obj);
extern int  drm_gem_handle_create(struct drm_file *file,
                                  struct drm_gem_object *obj,
                                  u32 *handlep);
extern void drm_gem_handle_delete(struct drm_file *file, u32 handle);
extern struct drm_gem_object *drm_gem_object_lookup(struct drm_file *file,
                                                     u32 handle);
extern void drm_gem_object_put(struct drm_gem_object *obj);

#endif
