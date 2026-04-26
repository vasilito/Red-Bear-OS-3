#define _POSIX_C_SOURCE 200809L

#include "libudev.h"

#include <errno.h>
#include <fcntl.h>
#include <fnmatch.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

struct udev_list_entry {
    char *name;
    char *value;
    struct udev_list_entry *next;
};

struct udev {
    int refcount;
};

struct udev_match_property {
    char *key;
    char *value;
    struct udev_match_property *next;
};

struct udev_device {
    int refcount;
    struct udev *udev;
    char *syspath;
    char *devpath;
    char *devnode;
    char *subsystem;
    char *devtype;
    char *sysname;
    char *sysnum;
    char *driver;
    char *action;
    dev_t devnum;
    struct udev_list_entry *properties;
    struct udev_list_entry *devlinks;
    struct udev_list_entry *sysattrs;
    struct udev_device *parent;
};

struct udev_enumerate {
    int refcount;
    struct udev *udev;
    char *match_subsystem;
    char *match_sysname;
    struct udev_match_property *match_properties;
    struct udev_list_entry *list;
};

struct udev_monitor_filter {
    char *subsystem;
    char *devtype;
    struct udev_monitor_filter *next;
};

struct udev_monitor {
    int refcount;
    struct udev *udev;
    int read_fd;
    int write_fd;
    bool enabled;
    struct udev_monitor_filter *filters;
    struct udev_monitor_event *pending_head;
    struct udev_monitor_event *pending_tail;
};

struct udev_monitor_event {
    struct udev_device *device;
    struct udev_monitor_event *next;
};

static char *xstrdup(const char *value)
{
    if (!value) {
        return NULL;
    }

    size_t len = strlen(value) + 1;
    char *copy = malloc(len);
    if (!copy) {
        return NULL;
    }

    memcpy(copy, value, len);
    return copy;
}

static void free_list_entries(struct udev_list_entry *entry)
{
    while (entry) {
        struct udev_list_entry *next = entry->next;
        free(entry->name);
        free(entry->value);
        free(entry);
        entry = next;
    }
}

static void free_match_properties(struct udev_match_property *entry)
{
    while (entry) {
        struct udev_match_property *next = entry->next;
        free(entry->key);
        free(entry->value);
        free(entry);
        entry = next;
    }
}

static void free_monitor_filters(struct udev_monitor_filter *entry)
{
    while (entry) {
        struct udev_monitor_filter *next = entry->next;
        free(entry->subsystem);
        free(entry->devtype);
        free(entry);
        entry = next;
    }
}

static void free_monitor_events(struct udev_monitor_event *entry)
{
    while (entry) {
        struct udev_monitor_event *next = entry->next;
        udev_device_unref(entry->device);
        free(entry);
        entry = next;
    }
}

static int list_entry_append(struct udev_list_entry **head, const char *name, const char *value)
{
    struct udev_list_entry *entry = calloc(1, sizeof(*entry));
    if (!entry) {
        return -1;
    }

    entry->name = xstrdup(name);
    entry->value = xstrdup(value);
    if ((name && !entry->name) || (value && !entry->value)) {
        free(entry->name);
        free(entry->value);
        free(entry);
        return -1;
    }

    if (!*head) {
        *head = entry;
        return 0;
    }

    struct udev_list_entry *tail = *head;
    while (tail->next) {
        tail = tail->next;
    }
    tail->next = entry;
    return 0;
}

static const char *list_entry_find_value(const struct udev_list_entry *entry, const char *name)
{
    while (entry) {
        if (entry->name && name && strcmp(entry->name, name) == 0) {
            return entry->value;
        }
        entry = entry->next;
    }
    return NULL;
}

static bool list_entry_has_name(const struct udev_list_entry *entry, const char *name)
{
    while (entry) {
        if (entry->name && name && strcmp(entry->name, name) == 0) {
            return true;
        }
        entry = entry->next;
    }
    return false;
}

static bool string_matches(const char *left, const char *right)
{
    return left && right && strcmp(left, right) == 0;
}

static char *read_text_file(const char *path)
{
    FILE *file = fopen(path, "r");
    if (!file) {
        return NULL;
    }

    size_t cap = 1024;
    size_t len = 0;
    char *buffer = malloc(cap);
    if (!buffer) {
        fclose(file);
        return NULL;
    }

    for (;;) {
        if (len + 512 >= cap) {
            size_t next_cap = cap * 2;
            char *next = realloc(buffer, next_cap);
            if (!next) {
                free(buffer);
                fclose(file);
                return NULL;
            }
            buffer = next;
            cap = next_cap;
        }

        size_t read_bytes = fread(buffer + len, 1, cap - len - 1, file);
        len += read_bytes;
        if (read_bytes == 0) {
            if (ferror(file)) {
                free(buffer);
                fclose(file);
                return NULL;
            }
            break;
        }
    }

    buffer[len] = '\0';
    fclose(file);
    return buffer;
}

static char *dup_basename(const char *path)
{
    if (!path) {
        return NULL;
    }

    const char *base = strrchr(path, '/');
    base = base ? base + 1 : path;
    return xstrdup(base);
}

static char *dup_sysnum(const char *sysname)
{
    if (!sysname) {
        return NULL;
    }

    const char *end = sysname + strlen(sysname);
    const char *start = end;
    while (start > sysname && start[-1] >= '0' && start[-1] <= '9') {
        start--;
    }

    if (start == end) {
        return NULL;
    }

    size_t len = (size_t)(end - start);
    char *copy = malloc(len + 1);
    if (!copy) {
        return NULL;
    }

    memcpy(copy, start, len);
    copy[len] = '\0';
    return copy;
}

static int replace_string(char **slot, const char *value)
{
    char *copy = xstrdup(value);
    if (value && !copy) {
        return -1;
    }

    free(*slot);
    *slot = copy;
    return 0;
}

static dev_t devnum_from_node(const char *devnode)
{
    if (!devnode) {
        return 0;
    }

    struct stat st;
    if (stat(devnode, &st) != 0) {
        return 0;
    }

    return st.st_rdev;
}

static void udev_device_destroy(struct udev_device *device)
{
    if (!device) {
        return;
    }

    if (device->parent) {
        udev_device_destroy(device->parent);
    }

    udev_unref(device->udev);
    free(device->syspath);
    free(device->devpath);
    free(device->devnode);
    free(device->subsystem);
    free(device->devtype);
    free(device->sysname);
    free(device->sysnum);
    free(device->driver);
    free(device->action);
    free_list_entries(device->properties);
    free_list_entries(device->devlinks);
    free_list_entries(device->sysattrs);
    free(device);
}

static struct udev_device *udev_device_alloc(struct udev *udev)
{
    struct udev_device *device = calloc(1, sizeof(*device));
    if (!device) {
        return NULL;
    }

    device->refcount = 1;
    device->udev = udev_ref(udev);
    if (!device->udev) {
        free(device);
        return NULL;
    }

    return device;
}

static bool is_primary_drm_device(const struct udev_device *device)
{
    return device && device->subsystem && strcmp(device->subsystem, "drm") == 0 && device->sysname && strcmp(device->sysname, "card0") == 0;
}

static struct udev_device *make_pci_parent(struct udev_device *child)
{
    if (!child || !child->syspath || strncmp(child->syspath, "/devices/pci/", 13) != 0) {
        return NULL;
    }
    if (child->subsystem && strcmp(child->subsystem, "pci") == 0) {
        return NULL;
    }

    struct udev_device *parent = udev_device_alloc(child->udev);
    if (!parent) {
        return NULL;
    }

    if (replace_string(&parent->syspath, child->syspath) != 0 || replace_string(&parent->devpath, child->devpath ? child->devpath : child->syspath) != 0 || replace_string(&parent->subsystem, "pci") != 0 || !(parent->sysname = dup_basename(child->syspath))) {
        udev_device_destroy(parent);
        return NULL;
    }

    parent->sysnum = dup_sysnum(parent->sysname);
    if (list_entry_append(&parent->properties, "DEVPATH", parent->devpath) != 0 || list_entry_append(&parent->properties, "SUBSYSTEM", "pci") != 0) {
        udev_device_destroy(parent);
        return NULL;
    }

    const char *vendor_id = list_entry_find_value(child->properties, "PCI_VENDOR_ID");
    const char *device_id = list_entry_find_value(child->properties, "PCI_DEVICE_ID");
    const char *pci_class = list_entry_find_value(child->properties, "PCI_CLASS");

    if ((vendor_id && list_entry_append(&parent->properties, "PCI_VENDOR_ID", vendor_id) != 0) || (device_id && list_entry_append(&parent->properties, "PCI_DEVICE_ID", device_id) != 0) || (pci_class && list_entry_append(&parent->properties, "PCI_CLASS", pci_class) != 0) || list_entry_append(&parent->sysattrs, "boot_vga", is_primary_drm_device(child) ? "1" : "0") != 0) {
        udev_device_destroy(parent);
        return NULL;
    }

    return parent;
}

static struct udev_device *parse_device_record(struct udev *udev, const char *content, const char *action)
{
    char *buffer = xstrdup(content);
    if (!buffer) {
        return NULL;
    }

    struct udev_device *device = udev_device_alloc(udev);
    if (!device) {
        free(buffer);
        return NULL;
    }

    char *save = NULL;
    for (char *line = strtok_r(buffer, "\n", &save); line; line = strtok_r(NULL, "\n", &save)) {
        if (strncmp(line, "P=", 2) == 0) {
            if (replace_string(&device->syspath, line + 2) != 0 || replace_string(&device->devpath, line + 2) != 0) {
                udev_device_destroy(device);
                free(buffer);
                return NULL;
            }
        } else if (strncmp(line, "E=", 2) == 0) {
            char *payload = line + 2;
            char *separator = strchr(payload, '=');
            if (!separator) {
                continue;
            }

            *separator = '\0';
            const char *key = payload;
            const char *value = separator + 1;
            if (list_entry_append(&device->properties, key, value) != 0) {
                udev_device_destroy(device);
                free(buffer);
                return NULL;
            }

            if (strcmp(key, "DEVPATH") == 0) {
                if (replace_string(&device->devpath, value) != 0) {
                    udev_device_destroy(device);
                    free(buffer);
                    return NULL;
                }
            } else if (strcmp(key, "SUBSYSTEM") == 0) {
                if (replace_string(&device->subsystem, value) != 0) {
                    udev_device_destroy(device);
                    free(buffer);
                    return NULL;
                }
            } else if (strcmp(key, "DEVNAME") == 0) {
                if (replace_string(&device->devnode, value) != 0) {
                    udev_device_destroy(device);
                    free(buffer);
                    return NULL;
                }
            }
        } else if (strncmp(line, "S=", 2) == 0) {
            const char *value = line + 2;
            if (*value == '\0') {
                continue;
            }

            char *devlink = NULL;
            if (value[0] == '/') {
                devlink = xstrdup(value);
            } else {
                size_t len = strlen(value) + 2;
                devlink = malloc(len);
                if (devlink) {
                    snprintf(devlink, len, "/%s", value);
                }
            }

            if (!devlink || list_entry_append(&device->devlinks, devlink, NULL) != 0) {
                free(devlink);
                udev_device_destroy(device);
                free(buffer);
                return NULL;
            }

            free(devlink);
        }
    }

    if (!device->syspath && device->devpath && replace_string(&device->syspath, device->devpath) != 0) {
        udev_device_destroy(device);
        free(buffer);
        return NULL;
    }

    if (!device->sysname) {
        device->sysname = dup_basename(device->devnode ? device->devnode : device->syspath);
    }
    if (device->sysname && !device->sysnum) {
        device->sysnum = dup_sysnum(device->sysname);
    }
    if (device->devnode) {
        device->devnum = devnum_from_node(device->devnode);
    }
    if (action && replace_string(&device->action, action) != 0) {
        udev_device_destroy(device);
        free(buffer);
        return NULL;
    }
    if (!device->subsystem && replace_string(&device->subsystem, "unknown") != 0) {
        udev_device_destroy(device);
        free(buffer);
        return NULL;
    }

    device->parent = make_pci_parent(device);
    free(buffer);
    return device;
}

typedef int (*device_callback_fn)(struct udev_device *device, void *ctx);

static int scan_scheme_devices(struct udev *udev, device_callback_fn callback, void *ctx)
{
    char *listing = read_text_file("/scheme/udev/devices");
    if (!listing) {
        return 0;
    }

    int result = 0;
    char *save = NULL;
    for (char *line = strtok_r(listing, "\n", &save); line; line = strtok_r(NULL, "\n", &save)) {
        if (*line == '\0') {
            continue;
        }

        char path[256];
        snprintf(path, sizeof(path), "/scheme/udev/devices/%s", line);

        char *content = read_text_file(path);
        if (!content) {
            continue;
        }

        struct udev_device *device = parse_device_record(udev, content, NULL);
        free(content);
        if (!device) {
            continue;
        }

        result = callback(device, ctx);
        udev_device_unref(device);
        if (result != 0) {
            break;
        }
    }

    free(listing);
    return result;
}

struct find_by_syspath_ctx {
    struct udev_device *result;
    const char *target;
};

static int find_by_syspath_cb(struct udev_device *device, void *ctx)
{
    struct find_by_syspath_ctx *state = ctx;
    if (string_matches(device->syspath, state->target) || string_matches(device->devnode, state->target) || list_entry_has_name(device->devlinks, state->target)) {
        state->result = udev_device_ref(device);
        return 1;
    }
    return 0;
}

struct find_by_devnum_ctx {
    struct udev_device *result;
    dev_t target;
};

static int find_by_devnum_cb(struct udev_device *device, void *ctx)
{
    struct find_by_devnum_ctx *state = ctx;
    if (device->devnum != 0 && device->devnum == state->target) {
        state->result = udev_device_ref(device);
        return 1;
    }
    return 0;
}

struct find_by_subsystem_sysname_ctx {
    struct udev_device *result;
    const char *subsystem;
    const char *sysname;
};

static int find_by_subsystem_sysname_cb(struct udev_device *device, void *ctx)
{
    struct find_by_subsystem_sysname_ctx *state = ctx;
    if (string_matches(device->subsystem, state->subsystem) && string_matches(device->sysname, state->sysname)) {
        state->result = udev_device_ref(device);
        return 1;
    }
    return 0;
}

static bool device_matches_enumerate(const struct udev_enumerate *enumerate, const struct udev_device *device)
{
    if (enumerate->match_subsystem && !string_matches(enumerate->match_subsystem, device->subsystem)) {
        return false;
    }

    if (enumerate->match_sysname && (!device->sysname || fnmatch(enumerate->match_sysname, device->sysname, 0) != 0)) {
        return false;
    }

    for (const struct udev_match_property *entry = enumerate->match_properties; entry; entry = entry->next) {
        const char *value = list_entry_find_value(device->properties, entry->key);
        if (!value) {
            return false;
        }
        if (entry->value && strcmp(value, entry->value) != 0) {
            return false;
        }
    }

    return true;
}

static bool device_matches_monitor_filters(const struct udev_monitor *monitor, const struct udev_device *device)
{
    const struct udev_monitor_filter *filter;

    if (!monitor->filters) {
        return true;
    }

    for (filter = monitor->filters; filter; filter = filter->next) {
        if (!string_matches(filter->subsystem, device->subsystem)) {
            continue;
        }
        if (filter->devtype && !string_matches(filter->devtype, device->devtype)) {
            continue;
        }
        return true;
    }

    return false;
}

static int monitor_queue_device(struct udev_monitor *monitor, struct udev_device *device, const char *action)
{
    struct udev_monitor_event *event;
    struct udev_device *copy;

    if (!monitor || !device || !device_matches_monitor_filters(monitor, device)) {
        return 0;
    }

    copy = udev_device_ref(device);
    if (!copy) {
        errno = ENOMEM;
        return -1;
    }

    free(copy->action);
    copy->action = xstrdup(action ? action : "change");
    if (!copy->action) {
        udev_device_unref(copy);
        errno = ENOMEM;
        return -1;
    }

    event = calloc(1, sizeof(*event));
    if (!event) {
        udev_device_unref(copy);
        errno = ENOMEM;
        return -1;
    }

    event->device = copy;
    if (monitor->pending_tail) {
        monitor->pending_tail->next = event;
    } else {
        monitor->pending_head = event;
    }
    monitor->pending_tail = event;
    return 0;
}

static int monitor_emit_pending(struct udev_monitor *monitor)
{
    struct udev_monitor_event *event;

    if (!monitor || monitor->write_fd < 0) {
        return 0;
    }

    for (event = monitor->pending_head; event; event = event->next) {
        if (write(monitor->write_fd, "u", 1) < 0) {
            return -1;
        }
    }

    return 0;
}

static int monitor_seed_existing_devices_cb(struct udev_device *device, void *ctx)
{
    return monitor_queue_device(ctx, device, "change");
}

static void clear_enumerate_list(struct udev_enumerate *enumerate)
{
    free_list_entries(enumerate->list);
    enumerate->list = NULL;
}

struct enumerate_devices_ctx {
    struct udev_enumerate *enumerate;
    int failed;
};

static int enumerate_devices_cb(struct udev_device *device, void *ctx)
{
    struct enumerate_devices_ctx *state = ctx;
    if (!device_matches_enumerate(state->enumerate, device)) {
        return 0;
    }

    if (list_entry_append(&state->enumerate->list, device->syspath, NULL) != 0) {
        state->failed = 1;
        return 1;
    }

    return 0;
}

struct enumerate_subsystems_ctx {
    struct udev_enumerate *enumerate;
    int failed;
};

static int enumerate_subsystems_cb(struct udev_device *device, void *ctx)
{
    struct enumerate_subsystems_ctx *state = ctx;
    if (!device->subsystem || list_entry_has_name(state->enumerate->list, device->subsystem)) {
        return 0;
    }

    if (list_entry_append(&state->enumerate->list, device->subsystem, NULL) != 0) {
        state->failed = 1;
        return 1;
    }

    return 0;
}

struct udev *udev_new(void)
{
    struct udev *udev = calloc(1, sizeof(*udev));
    if (!udev) {
        return NULL;
    }

    udev->refcount = 1;
    return udev;
}

struct udev *udev_ref(struct udev *udev)
{
    if (udev) {
        udev->refcount++;
    }
    return udev;
}

struct udev *udev_unref(struct udev *udev)
{
    if (!udev) {
        return NULL;
    }

    udev->refcount--;
    if (udev->refcount <= 0) {
        free(udev);
        return NULL;
    }

    return udev;
}

struct udev_enumerate *udev_enumerate_new(struct udev *udev)
{
    if (!udev) {
        errno = EINVAL;
        return NULL;
    }

    struct udev_enumerate *enumerate = calloc(1, sizeof(*enumerate));
    if (!enumerate) {
        return NULL;
    }

    enumerate->refcount = 1;
    enumerate->udev = udev_ref(udev);
    return enumerate;
}

struct udev_enumerate *udev_enumerate_ref(struct udev_enumerate *udev_enumerate)
{
    if (udev_enumerate) {
        udev_enumerate->refcount++;
    }
    return udev_enumerate;
}

struct udev_enumerate *udev_enumerate_unref(struct udev_enumerate *udev_enumerate)
{
    if (!udev_enumerate) {
        return NULL;
    }

    udev_enumerate->refcount--;
    if (udev_enumerate->refcount <= 0) {
        udev_unref(udev_enumerate->udev);
        free(udev_enumerate->match_subsystem);
        free(udev_enumerate->match_sysname);
        free_match_properties(udev_enumerate->match_properties);
        clear_enumerate_list(udev_enumerate);
        free(udev_enumerate);
        return NULL;
    }

    return udev_enumerate;
}

struct udev *udev_enumerate_get_udev(struct udev_enumerate *udev_enumerate)
{
    return udev_enumerate ? udev_enumerate->udev : NULL;
}

int udev_enumerate_add_match_subsystem(struct udev_enumerate *udev_enumerate, const char *subsystem)
{
    if (!udev_enumerate || !subsystem) {
        errno = EINVAL;
        return -1;
    }

    return replace_string(&udev_enumerate->match_subsystem, subsystem);
}

int udev_enumerate_add_match_sysname(struct udev_enumerate *udev_enumerate, const char *sysname)
{
    if (!udev_enumerate || !sysname) {
        errno = EINVAL;
        return -1;
    }

    return replace_string(&udev_enumerate->match_sysname, sysname);
}

int udev_enumerate_add_match_property(struct udev_enumerate *udev_enumerate, const char *property, const char *value)
{
    if (!udev_enumerate || !property) {
        errno = EINVAL;
        return -1;
    }

    struct udev_match_property *entry = calloc(1, sizeof(*entry));
    if (!entry) {
        return -1;
    }

    entry->key = xstrdup(property);
    entry->value = xstrdup(value);
    if (!entry->key || (value && !entry->value)) {
        free(entry->key);
        free(entry->value);
        free(entry);
        return -1;
    }

    if (!udev_enumerate->match_properties) {
        udev_enumerate->match_properties = entry;
        return 0;
    }

    struct udev_match_property *tail = udev_enumerate->match_properties;
    while (tail->next) {
        tail = tail->next;
    }
    tail->next = entry;
    return 0;
}

int udev_enumerate_scan_devices(struct udev_enumerate *udev_enumerate)
{
    if (!udev_enumerate) {
        errno = EINVAL;
        return -1;
    }

    clear_enumerate_list(udev_enumerate);
    struct enumerate_devices_ctx ctx = {
        .enumerate = udev_enumerate,
        .failed = 0,
    };
    scan_scheme_devices(udev_enumerate->udev, enumerate_devices_cb, &ctx);
    if (ctx.failed) {
        errno = ENOMEM;
        return -1;
    }
    return 0;
}

int udev_enumerate_scan_subsystems(struct udev_enumerate *udev_enumerate)
{
    if (!udev_enumerate) {
        errno = EINVAL;
        return -1;
    }

    clear_enumerate_list(udev_enumerate);
    struct enumerate_subsystems_ctx ctx = {
        .enumerate = udev_enumerate,
        .failed = 0,
    };
    scan_scheme_devices(udev_enumerate->udev, enumerate_subsystems_cb, &ctx);
    if (ctx.failed) {
        errno = ENOMEM;
        return -1;
    }
    return 0;
}

struct udev_list_entry *udev_enumerate_get_list_entry(struct udev_enumerate *udev_enumerate)
{
    return udev_enumerate ? udev_enumerate->list : NULL;
}

struct udev_list_entry *udev_list_entry_get_next(struct udev_list_entry *list_entry)
{
    return list_entry ? list_entry->next : NULL;
}

const char *udev_list_entry_get_name(struct udev_list_entry *list_entry)
{
    return list_entry ? list_entry->name : NULL;
}

const char *udev_list_entry_get_value(struct udev_list_entry *list_entry)
{
    return list_entry ? list_entry->value : NULL;
}

struct udev_device *udev_device_ref(struct udev_device *udev_device)
{
    if (udev_device) {
        udev_device->refcount++;
    }
    return udev_device;
}

struct udev_device *udev_device_unref(struct udev_device *udev_device)
{
    if (!udev_device) {
        return NULL;
    }

    udev_device->refcount--;
    if (udev_device->refcount <= 0) {
        udev_device_destroy(udev_device);
        return NULL;
    }

    return udev_device;
}

struct udev_device *udev_device_new_from_syspath(struct udev *udev, const char *syspath)
{
    if (!udev || !syspath) {
        errno = EINVAL;
        return NULL;
    }

    struct find_by_syspath_ctx ctx = {
        .result = NULL,
        .target = syspath,
    };
    scan_scheme_devices(udev, find_by_syspath_cb, &ctx);
    if (!ctx.result) {
        errno = ENOENT;
    }
    return ctx.result;
}

struct udev_device *udev_device_new_from_devnum(struct udev *udev, char type, dev_t devnum)
{
    (void)type;
    if (!udev) {
        errno = EINVAL;
        return NULL;
    }

    struct find_by_devnum_ctx ctx = {
        .result = NULL,
        .target = devnum,
    };
    scan_scheme_devices(udev, find_by_devnum_cb, &ctx);
    if (!ctx.result) {
        errno = ENOENT;
    }
    return ctx.result;
}

struct udev_device *udev_device_new_from_subsystem_sysname(struct udev *udev, const char *subsystem, const char *sysname)
{
    if (!udev || !subsystem || !sysname) {
        errno = EINVAL;
        return NULL;
    }

    struct find_by_subsystem_sysname_ctx ctx = {
        .result = NULL,
        .subsystem = subsystem,
        .sysname = sysname,
    };
    scan_scheme_devices(udev, find_by_subsystem_sysname_cb, &ctx);
    if (!ctx.result) {
        errno = ENOENT;
    }
    return ctx.result;
}

struct udev *udev_device_get_udev(struct udev_device *udev_device)
{
    return udev_device ? udev_device->udev : NULL;
}

const char *udev_device_get_devnode(struct udev_device *udev_device)
{
    return udev_device ? udev_device->devnode : NULL;
}

dev_t udev_device_get_devnum(struct udev_device *udev_device)
{
    return udev_device ? udev_device->devnum : 0;
}

const char *udev_device_get_action(struct udev_device *udev_device)
{
    return udev_device ? udev_device->action : NULL;
}

const char *udev_device_get_property_value(struct udev_device *udev_device, const char *key)
{
    return udev_device ? list_entry_find_value(udev_device->properties, key) : NULL;
}

struct udev_list_entry *udev_device_get_properties_list_entry(struct udev_device *udev_device)
{
    return udev_device ? udev_device->properties : NULL;
}

struct udev_list_entry *udev_device_get_devlinks_list_entry(struct udev_device *udev_device)
{
    return udev_device ? udev_device->devlinks : NULL;
}

struct udev_list_entry *udev_device_get_sysattr_list_entry(struct udev_device *udev_device)
{
    return udev_device ? udev_device->sysattrs : NULL;
}

struct udev_device *udev_device_get_parent(struct udev_device *udev_device)
{
    return udev_device ? udev_device->parent : NULL;
}

struct udev_device *udev_device_get_parent_with_subsystem_devtype(struct udev_device *udev_device, const char *subsystem, const char *devtype)
{
    if (!udev_device || !subsystem) {
        return NULL;
    }

    if (string_matches(udev_device->subsystem, subsystem) && (!devtype || string_matches(udev_device->devtype, devtype))) {
        return udev_device;
    }

    if (udev_device->parent && string_matches(udev_device->parent->subsystem, subsystem) && (!devtype || string_matches(udev_device->parent->devtype, devtype))) {
        return udev_device->parent;
    }

    return NULL;
}

const char *udev_device_get_sysattr_value(struct udev_device *udev_device, const char *sysattr)
{
    return udev_device ? list_entry_find_value(udev_device->sysattrs, sysattr) : NULL;
}

const char *udev_device_get_devpath(struct udev_device *udev_device)
{
    return udev_device ? udev_device->devpath : NULL;
}

const char *udev_device_get_syspath(struct udev_device *udev_device)
{
    return udev_device ? udev_device->syspath : NULL;
}

const char *udev_device_get_subsystem(struct udev_device *udev_device)
{
    return udev_device ? udev_device->subsystem : NULL;
}

const char *udev_device_get_devtype(struct udev_device *udev_device)
{
    return udev_device ? udev_device->devtype : NULL;
}

const char *udev_device_get_sysname(struct udev_device *udev_device)
{
    return udev_device ? udev_device->sysname : NULL;
}

const char *udev_device_get_sysnum(struct udev_device *udev_device)
{
    return udev_device ? udev_device->sysnum : NULL;
}

const char *udev_device_get_driver(struct udev_device *udev_device)
{
    return udev_device ? udev_device->driver : NULL;
}

int udev_device_get_is_initialized(struct udev_device *udev_device)
{
    return udev_device ? 1 : 0;
}

int udev_device_has_tag(struct udev_device *udev_device, const char *tag)
{
    const char *tags = udev_device_get_property_value(udev_device, "TAGS");
    if (!tags || !tag || *tag == '\0') {
        return 0;
    }

    size_t tag_len = strlen(tag);
    const char *cursor = tags;
    while (*cursor) {
        while (*cursor == ':' || *cursor == ' ') {
            cursor++;
        }

        const char *end = cursor;
        while (*end && *end != ':') {
            end++;
        }

        if ((size_t)(end - cursor) == tag_len && strncmp(cursor, tag, tag_len) == 0) {
            return 1;
        }

        cursor = end;
    }

    return 0;
}

struct udev_monitor *udev_monitor_new_from_netlink(struct udev *udev, const char *name)
{
    int pipe_fds[2];

    if (!udev || !name || strcmp(name, "udev") != 0) {
        errno = EINVAL;
        return NULL;
    }

    struct udev_monitor *monitor = calloc(1, sizeof(*monitor));
    if (!monitor) {
        return NULL;
    }

    if (pipe(pipe_fds) != 0) {
        free(monitor);
        return NULL;
    }

    int flags = fcntl(pipe_fds[0], F_GETFL);
    if (flags >= 0) {
        (void)fcntl(pipe_fds[0], F_SETFL, flags | O_NONBLOCK);
    }

    monitor->refcount = 1;
    monitor->udev = udev_ref(udev);
    monitor->read_fd = pipe_fds[0];
    monitor->write_fd = pipe_fds[1];
    return monitor;
}

struct udev_monitor *udev_monitor_ref(struct udev_monitor *udev_monitor)
{
    if (udev_monitor) {
        udev_monitor->refcount++;
    }
    return udev_monitor;
}

struct udev_monitor *udev_monitor_unref(struct udev_monitor *udev_monitor)
{
    if (!udev_monitor) {
        return NULL;
    }

    udev_monitor->refcount--;
    if (udev_monitor->refcount <= 0) {
        if (udev_monitor->read_fd >= 0) {
            close(udev_monitor->read_fd);
        }
        if (udev_monitor->write_fd >= 0) {
            close(udev_monitor->write_fd);
        }
        free_monitor_filters(udev_monitor->filters);
        free_monitor_events(udev_monitor->pending_head);
        udev_unref(udev_monitor->udev);
        free(udev_monitor);
        return NULL;
    }

    return udev_monitor;
}

int udev_monitor_filter_add_match_subsystem_devtype(struct udev_monitor *udev_monitor, const char *subsystem, const char *devtype)
{
    if (!udev_monitor || !subsystem) {
        errno = EINVAL;
        return -1;
    }

    struct udev_monitor_filter *filter = calloc(1, sizeof(*filter));
    if (!filter) {
        return -1;
    }

    filter->subsystem = xstrdup(subsystem);
    filter->devtype = xstrdup(devtype);
    if (!filter->subsystem || (devtype && !filter->devtype)) {
        free(filter->subsystem);
        free(filter->devtype);
        free(filter);
        return -1;
    }

    if (!udev_monitor->filters) {
        udev_monitor->filters = filter;
        return 0;
    }

    struct udev_monitor_filter *tail = udev_monitor->filters;
    while (tail->next) {
        tail = tail->next;
    }
    tail->next = filter;
    return 0;
}

int udev_monitor_enable_receiving(struct udev_monitor *udev_monitor)
{
    if (!udev_monitor) {
        errno = EINVAL;
        return -1;
    }

    if (udev_monitor->enabled) {
        return 0;
    }

    if (udev_monitor->read_fd < 0 || udev_monitor->write_fd < 0) {
        errno = EINVAL;
        return -1;
    }

    if (scan_scheme_devices(udev_monitor->udev, monitor_seed_existing_devices_cb, udev_monitor) != 0) {
        return -1;
    }

    if (monitor_emit_pending(udev_monitor) != 0) {
        return -1;
    }

    udev_monitor->enabled = true;
    return 0;
}

int udev_monitor_get_fd(struct udev_monitor *udev_monitor)
{
    return udev_monitor ? udev_monitor->read_fd : -1;
}

struct udev_device *udev_monitor_receive_device(struct udev_monitor *udev_monitor)
{
    struct udev_monitor_event *event;

    if (!udev_monitor || udev_monitor->read_fd < 0) {
        return NULL;
    }

    char byte;
    ssize_t read_bytes = read(udev_monitor->read_fd, &byte, sizeof(byte));
    if (read_bytes <= 0) {
        return NULL;
    }

    event = udev_monitor->pending_head;
    if (!event) {
        return NULL;
    }

    udev_monitor->pending_head = event->next;
    if (!udev_monitor->pending_head) {
        udev_monitor->pending_tail = NULL;
    }

    struct udev_device *device = event->device;
    free(event);
    return device;
}
