#ifndef _REDOX_GLUE_H
#define _REDOX_GLUE_H

/*
 * Redox-specific Linux compatibility surface for the AMDGPU display port.
 * The real build enables this via -D__redox__, but the declarations stay
 * visible unconditionally so editor/LSP diagnostics can parse the sources.
 */

/* ---- Standard types ---- */
#include <errno.h>
#include <pthread.h>
#include <stddef.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#ifndef __iomem
#define __iomem
#endif

#ifndef __user
#define __user
#endif

#ifndef __force
#define __force
#endif

#ifndef __must_check
#define __must_check
#endif

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef uint64_t u64;
typedef int8_t s8;
typedef int16_t s16;
typedef int32_t s32;
typedef int64_t s64;

typedef unsigned long ulong;
typedef unsigned long long ullong;
typedef unsigned int uint;
typedef size_t phys_addr_t;
typedef u64 dma_addr_t;
typedef u32 __be32;
typedef u16 __be16;
typedef u32 __le32;
typedef u16 __le16;
typedef unsigned int gfp_t;

/* ---- Kernel replacements ---- */
#define GFP_KERNEL 0U
#define GFP_ATOMIC 1U
#define GFP_DMA32 2U
#define GFP_NOWAIT 3U
#define GFP_KERNEL_ACCOUNT 0U

extern void *kmalloc(size_t size, unsigned int flags);
extern void *kzalloc(size_t size, unsigned int flags);
extern void kfree(const void *ptr);
extern void *vmalloc(unsigned long size);
extern void vfree(const void *addr);
extern void *krealloc(const void *ptr, size_t new_size, unsigned int flags);

/* printk → stderr */
#define printk(fmt, ...) fprintf(stderr, "[amdgpu] " fmt, ##__VA_ARGS__)
#define pr_err(fmt, ...) fprintf(stderr, "[amdgpu ERR] " fmt, ##__VA_ARGS__)
#define pr_warn(fmt, ...) fprintf(stderr, "[amdgpu WARN] " fmt, ##__VA_ARGS__)
#define pr_info(fmt, ...) fprintf(stderr, "[amdgpu INFO] " fmt, ##__VA_ARGS__)
#define pr_debug(fmt, ...) fprintf(stderr, "[amdgpu DBG] " fmt, ##__VA_ARGS__)
#define dev_err(dev, fmt, ...) fprintf(stderr, "[amdgpu ERR] " fmt, ##__VA_ARGS__)
#define dev_warn(dev, fmt, ...) fprintf(stderr, "[amdgpu WARN] " fmt, ##__VA_ARGS__)
#define dev_info(dev, fmt, ...) fprintf(stderr, "[amdgpu INFO] " fmt, ##__VA_ARGS__)
#define dev_dbg(dev, fmt, ...) fprintf(stderr, "[amdgpu DBG] " fmt, ##__VA_ARGS__)

/* ---- Module system replacement ---- */
#define module_init(fn) /* noop */
#define module_exit(fn) /* noop */
#define module_param(name, type, perm) /* noop */
#define MODULE_PARM_DESC(name, desc) /* noop */
#define MODULE_LICENSE(license) /* noop */
#define MODULE_AUTHOR(author) /* noop */
#define MODULE_DESCRIPTION(desc) /* noop */
#define MODULE_DEVICE_TABLE(type, table) /* noop */
#define EXPORT_SYMBOL(sym) /* noop */
#define EXPORT_SYMBOL_GPL(sym) /* noop */
#define MODULE_FIRMWARE(fw) /* noop */
#define THIS_MODULE NULL

/* ---- Atomic operations ---- */
typedef struct {
    volatile int counter;
} atomic_t;

typedef struct {
    volatile long counter;
} atomic_long_t;

typedef struct {
    volatile u64 counter;
} atomic64_t;

#define atomic_read(v) ((v)->counter)
#define atomic_set(v, i) ((v)->counter = (i))
#define atomic_inc(v) __sync_add_and_fetch(&(v)->counter, 1)
#define atomic_dec(v) __sync_sub_and_fetch(&(v)->counter, 1)
#define atomic_add(i, v) __sync_add_and_fetch(&(v)->counter, (i))
#define atomic_sub(i, v) __sync_sub_and_fetch(&(v)->counter, (i))
#define atomic_inc_return(v) __sync_add_and_fetch(&(v)->counter, 1)
#define atomic_dec_return(v) __sync_sub_and_fetch(&(v)->counter, 1)
#define atomic_cmpxchg(v, oldv, newv) __sync_val_compare_and_swap(&(v)->counter, (oldv), (newv))

/* ---- Locking ---- */
typedef pthread_mutex_t mutex_t;
#define DEFINE_MUTEX(name) pthread_mutex_t name = PTHREAD_MUTEX_INITIALIZER
#define mutex_init(m) pthread_mutex_init((m), NULL)
#define mutex_lock(m) pthread_mutex_lock((m))
#define mutex_unlock(m) pthread_mutex_unlock((m))
#define mutex_destroy(m) pthread_mutex_destroy((m))
#define mutex_is_locked(m) (pthread_mutex_trylock((m)) != 0)

typedef struct {
    volatile int lock;
} spinlock_t;

#define spin_lock_init(l) ((l)->lock = 0)
#define spin_lock(l) while (__sync_lock_test_and_set(&(l)->lock, 1)) {}
#define spin_unlock(l) __sync_lock_release(&(l)->lock)
#define spin_lock_irqsave(l, flags) do { (flags) = 0; spin_lock((l)); } while (0)
#define spin_unlock_irqrestore(l, flags) do { (void)(flags); spin_unlock((l)); } while (0)
#define spin_lock_irq(l) spin_lock((l))
#define spin_unlock_irq(l) spin_unlock((l))

/* ---- Power management stubs ---- */
#define pm_runtime_get_sync(dev) 0
#define pm_runtime_put_autosuspend(dev) 0
#define pm_runtime_allow(dev) 0
#define pm_runtime_forbid(dev) 0
#define pm_runtime_set_active(dev) 0
#define pm_runtime_enable(dev) 0
#define pm_runtime_disable(dev) 0
#define pm_runtime_idle(dev) 0
#define pm_runtime_put_noidle(dev) 0
#define pm_runtime_get_noresume(dev) 0
#define pm_suspend_ignore_children(dev, enable) /* noop */

/* ---- I/O memory — maps to redox-driver-sys MmioRegion ---- */
extern void __iomem *redox_ioremap(phys_addr_t offset, size_t size);
extern void redox_iounmap(void __iomem *addr);
extern void redox_iowrite32(u32 val, void __iomem *addr);
extern u32 redox_ioread32(const void __iomem *addr);
extern void redox_iowrite16(u16 val, void __iomem *addr);
extern u16 redox_ioread16(const void __iomem *addr);
extern void redox_iowrite8(u8 val, void __iomem *addr);
extern u8 redox_ioread8(const void __iomem *addr);
extern void redox_mmio_write32(void *base, u32 offset, u32 val);
extern u32 redox_mmio_read32(void *base, u32 offset);

#define ioremap(offset, size) redox_ioremap((offset), (size))
#define ioremap_wc(offset, size) redox_ioremap((offset), (size))
#define ioremap_np(offset, size) redox_ioremap((offset), (size))
#define iounmap(addr) redox_iounmap((addr))
#define iowrite32(val, addr) redox_iowrite32((val), (addr))
#define ioread32(addr) redox_ioread32((addr))
#define iowrite16(val, addr) redox_iowrite16((val), (addr))
#define ioread16(addr) redox_ioread16((addr))
#define iowrite8(val, addr) redox_iowrite8((val), (addr))
#define ioread8(addr) redox_ioread8((addr))

#define writel(val, addr) (*(volatile u32 *)(addr) = (val))
#define readl(addr) (*(volatile const u32 *)(addr))
#define writew(val, addr) (*(volatile u16 *)(addr) = (val))
#define readw(addr) (*(volatile const u16 *)(addr))
#define writeb(val, addr) (*(volatile u8 *)(addr) = (val))
#define readb(addr) (*(volatile const u8 *)(addr))
#define writeq(val, addr) (*(volatile u64 *)(addr) = (val))
#define readq(addr) (*(volatile const u64 *)(addr))

/* ---- Memory barriers ---- */
#define mb() __sync_synchronize()
#define rmb() __sync_synchronize()
#define wmb() __sync_synchronize()
#define smp_mb() __sync_synchronize()
#define smp_rmb() __sync_synchronize()
#define smp_wmb() __sync_synchronize()
#define barrier() __asm__ __volatile__("" : : : "memory")

/* ---- DMA mapping — maps to redox-driver-sys DmaBuffer ---- */
extern void *redox_dma_alloc_coherent(size_t size, dma_addr_t *dma_handle);
extern void redox_dma_free_coherent(size_t size, void *vaddr, dma_addr_t dma_handle);

#define dma_alloc_coherent(dev, size, dma_handle, flags) redox_dma_alloc_coherent((size), (dma_handle))
#define dma_free_coherent(dev, size, vaddr, dma_handle) redox_dma_free_coherent((size), (vaddr), (dma_handle))
#define dma_map_page(dev, page, offset, size, dir) ((dma_addr_t)0)
#define dma_unmap_page(dev, addr, size, dir) /* noop */
#define dma_map_single(dev, ptr, size, dir) ((dma_addr_t)(uintptr_t)(ptr))
#define dma_unmap_single(dev, addr, size, dir) /* noop */
#define dma_mapping_error(dev, addr) 0

/* ---- PCI — maps to redox-driver-sys PCI ---- */
struct pci_dev {
    u16 vendor;
    u16 device;
    u8 revision;
    u8 irq;
    phys_addr_t resource_start[6];
    u64 resource_len[6];
    u32 resource_flags[6];
    void *driver_data;
    void __iomem *mmio_base;
    int is_amdgpu;
};

extern struct pci_dev *redox_pci_find_amd_gpu(void);
extern void redox_pci_set_device_info(u16 vendor, u16 device, u8 revision,
                                       u8 irq, u64 bar0_addr, u64 bar0_size,
                                       u64 bar2_addr, u64 bar2_size);
extern void redox_pci_dev_put(struct pci_dev *pdev);
extern int redox_pci_enable_device(struct pci_dev *pdev);
extern void redox_pci_set_master(struct pci_dev *pdev);
extern int redox_pci_request_regions(struct pci_dev *pdev, const char *name);
extern void redox_pci_release_regions(struct pci_dev *pdev);

#define pci_get_device(vendor, device, from) redox_pci_find_amd_gpu()
#define pci_dev_put(pdev) redox_pci_dev_put((pdev))
#define pci_enable_device(pdev) redox_pci_enable_device((pdev))
#define pci_set_master(pdev) redox_pci_set_master((pdev))
#define pci_request_regions(pdev, name) redox_pci_request_regions((pdev), (name))
#define pci_release_regions(pdev) redox_pci_release_regions((pdev))
#define pci_resource_start(pdev, bar) ((pdev)->resource_start[(bar)])
#define pci_resource_len(pdev, bar) ((pdev)->resource_len[(bar)])
#define pci_resource_flags(pdev, bar) ((pdev)->resource_flags[(bar)])
#define pci_resource_end(pdev, bar) ((pdev)->resource_start[(bar)] + (pdev)->resource_len[(bar)] - 1)

#define IORESOURCE_MEM 0x00000200U
#define IORESOURCE_IO 0x00000100U
#define IORESOURCE_MEM_64 0x00040000U
#define IORESOURCE_PREFETCH 0x00001000U

/* ---- Firmware loading — maps to scheme:firmware ---- */
struct firmware {
    size_t size;
    const u8 *data;
};

extern int redox_request_firmware(const struct firmware **fw, const char *name, void *dev);
extern void redox_release_firmware(const struct firmware *fw);

#define request_firmware(fw, name, dev) redox_request_firmware((fw), (name), (dev))
#define release_firmware(fw) redox_release_firmware((fw))

/* ---- Device model ---- */
struct device {
    void *driver_data;
    struct pci_dev *pci_dev;
};

#define dev_get_drvdata(dev) ((dev)->driver_data)
#define dev_set_drvdata(dev, data) ((dev)->driver_data = (data))

/* ---- Interrupts ---- */
typedef int (*irq_handler_t)(int irq, void *dev_id);
extern int redox_request_irq(unsigned int irq, irq_handler_t handler, unsigned long flags, const char *name, void *dev);
extern void redox_free_irq(unsigned int irq, void *dev_id);

#define IRQF_SHARED 0x00000080UL
#define IRQF_TRIGGER_FALLING 0x00000002UL

/* ---- Workqueue ---- */
struct work_struct {
    void (*func)(struct work_struct *work);
};

struct delayed_work {
    struct work_struct work;
    unsigned long delay;
};

#define INIT_WORK(w, fn) ((w)->func = (fn))
#define INIT_DELAYED_WORK(w, fn) INIT_WORK(&(w)->work, (fn))
#define schedule_work(w) do { if ((w)->func) { (w)->func((w)); } } while (0)
#define schedule_delayed_work(w, delayv) do { (void)(delayv); if ((w)->work.func) { (w)->work.func(&(w)->work); } } while (0)
#define cancel_work_sync(w) /* noop */
#define cancel_delayed_work_sync(w) /* noop */
#define flush_workqueue(wq) /* noop */
#define flush_scheduled_work() /* noop */

/* ---- Completion ---- */
struct completion {
    volatile int done;
    pthread_mutex_t mutex;
    pthread_cond_t cond;
};

#define init_completion(c) do { \
    (c)->done = 0; \
    pthread_mutex_init(&(c)->mutex, NULL); \
    pthread_cond_init(&(c)->cond, NULL); \
} while (0)
#define reinit_completion(c) do { (c)->done = 0; } while (0)
#define complete(c) do { \
    pthread_mutex_lock(&(c)->mutex); \
    (c)->done = 1; \
    pthread_cond_broadcast(&(c)->cond); \
    pthread_mutex_unlock(&(c)->mutex); \
} while (0)
#define wait_for_completion(c) do { \
    pthread_mutex_lock(&(c)->mutex); \
    while (!(c)->done) { \
        pthread_cond_wait(&(c)->cond, &(c)->mutex); \
    } \
    pthread_mutex_unlock(&(c)->mutex); \
} while (0)
#define wait_for_completion_timeout(c, timeout) ({ (void)(timeout); wait_for_completion((c)); 1UL; })

/* ---- Error helpers ---- */
#ifndef EOPNOTSUPP
#define EOPNOTSUPP 95
#endif

#define IS_ERR(ptr) ((unsigned long)(uintptr_t)(ptr) >= (unsigned long)-4095)
#define PTR_ERR(ptr) ((long)(intptr_t)(ptr))
#define ERR_PTR(err) ((void *)(intptr_t)(err))
#define IS_ERR_OR_NULL(ptr) (!(ptr) || IS_ERR(ptr))

/* ---- Min/Max ---- */
#define min(a, b) ((a) < (b) ? (a) : (b))
#define max(a, b) ((a) > (b) ? (a) : (b))
#define min_t(type, a, b) ((type)(a) < (type)(b) ? (type)(a) : (type)(b))
#define max_t(type, a, b) ((type)(a) > (type)(b) ? (type)(a) : (type)(b))
#define clamp(val, lo, hi) min(max((val), (lo)), (hi))
#define clamp_t(type, val, lo, hi) ((type)clamp((val), (lo), (hi)))
#define clamp_val(val, lo, hi) clamp((val), (lo), (hi))
#define swap(a, b) do { typeof(a) __tmp = (a); (a) = (b); (b) = __tmp; } while (0)

/* ---- DIV_ROUND_UP, alignment ---- */
#define DIV_ROUND_UP(n, d) (((n) + (d) - 1) / (d))
#define DIV_ROUND_UP_ULL(n, d) DIV_ROUND_UP((n), (d))
#define DIV_ROUND_CLOSEST(n, d) (((n) + ((d) / 2)) / (d))
#define ALIGN(x, a) (((x) + (a) - 1) & ~((a) - 1))
#define IS_ALIGNED(x, a) (((x) & ((a) - 1)) == 0)
#define PAGE_SHIFT 12
#define PAGE_SIZE 4096UL
#define PAGE_MASK (~(PAGE_SIZE - 1))
#define PAGE_ALIGN(x) ALIGN((x), PAGE_SIZE)

/* ---- msleep, udelay — implemented in redox_stubs.c ---- */
extern void msleep(unsigned int msecs);
extern void udelay(unsigned long usecs);
extern void mdelay(unsigned long msecs);
extern unsigned long jiffies;
extern unsigned long msecs_to_jiffies(unsigned int msecs);
extern unsigned long usecs_to_jiffies(unsigned int usecs);

/* ---- Kconfig macros ---- */
#define IS_ENABLED(option) 0
#define IS_REACHABLE(option) 0
#ifndef CONFIG_DRM_AMDGPU
#define CONFIG_DRM_AMDGPU 1
#endif
#ifndef CONFIG_DRM_AMD_DC
#define CONFIG_DRM_AMD_DC 1
#endif
#ifndef CONFIG_DRM_AMD_DC_FP
#define CONFIG_DRM_AMD_DC_FP 1
#endif
#ifndef CONFIG_DRM_AMD_ACP
#define CONFIG_DRM_AMD_ACP 0
#endif
#ifndef CONFIG_DRM_AMD_SECURE_DISPLAY
#define CONFIG_DRM_AMD_SECURE_DISPLAY 0
#endif
#ifndef CONFIG_DRM_AMDGPU_SI
#define CONFIG_DRM_AMDGPU_SI 0
#endif
#ifndef CONFIG_DRM_AMDGPU_CIK
#define CONFIG_DRM_AMDGPU_CIK 0
#endif
#ifndef CONFIG_DEBUG_FS
#define CONFIG_DEBUG_FS 0
#endif
#ifndef CONFIG_FAULT_INJECTION
#define CONFIG_FAULT_INJECTION 0
#endif
#ifndef CONFIG_ACPI
#define CONFIG_ACPI 0
#endif
#ifndef CONFIG_HWMON
#define CONFIG_HWMON 0
#endif
#ifndef CONFIG_PM
#define CONFIG_PM 0
#endif
#ifndef CONFIG_SLEEP
#define CONFIG_SLEEP 0
#endif
#ifndef CONFIG_BACKLIGHT_CLASS_DEVICE
#define CONFIG_BACKLIGHT_CLASS_DEVICE 0
#endif
#ifndef CONFIG_BACKLIGHT_LCD_SUPPORT
#define CONFIG_BACKLIGHT_LCD_SUPPORT 0
#endif
#ifndef CONFIG_DRM_AMD_DC_HDCP
#define CONFIG_DRM_AMD_DC_HDCP 0
#endif
#ifndef CONFIG_DRM_AMD_DC_DSC
#define CONFIG_DRM_AMD_DC_DSC 1
#endif
#ifndef CONFIG_DRM_AMD_DC_DCN
#define CONFIG_DRM_AMD_DC_DCN 1
#endif
#ifndef CONFIG_DRM_AMD_DC_DML2
#define CONFIG_DRM_AMD_DC_DML2 0
#endif
#ifndef CONFIG_DRM_AMD_DC_SMU
#define CONFIG_DRM_AMD_DC_SMU 0
#endif

/* ---- Linked list ---- */
struct list_head {
    struct list_head *next;
    struct list_head *prev;
};

#define LIST_HEAD_INIT(name) { &(name), &(name) }
#define LIST_HEAD(name) struct list_head name = LIST_HEAD_INIT(name)

static inline void INIT_LIST_HEAD(struct list_head *list) {
    list->next = list;
    list->prev = list;
}

static inline void list_add(struct list_head *new_entry, struct list_head *head) {
    head->next->prev = new_entry;
    new_entry->next = head->next;
    new_entry->prev = head;
    head->next = new_entry;
}

static inline void list_add_tail(struct list_head *new_entry, struct list_head *head) {
    head->prev->next = new_entry;
    new_entry->prev = head->prev;
    new_entry->next = head;
    head->prev = new_entry;
}

static inline void list_del(struct list_head *entry) {
    entry->next->prev = entry->prev;
    entry->prev->next = entry->next;
    entry->next = (struct list_head *)(uintptr_t)0xDEADBEEF;
    entry->prev = (struct list_head *)(uintptr_t)0xDEADBEEF;
}

static inline int list_empty(const struct list_head *head) {
    return head->next == head;
}

#define list_entry(ptr, type, member) ((type *)((char *)(ptr) - offsetof(type, member)))
#define list_for_each(pos, head) for ((pos) = (head)->next; (pos) != (head); (pos) = (pos)->next)
#define list_for_each_safe(pos, n, head) for ((pos) = (head)->next, (n) = (pos)->next; (pos) != (head); (pos) = (n), (n) = (pos)->next)
#define list_for_each_entry(pos, head, member) \
    for ((pos) = list_entry((head)->next, typeof(*(pos)), member); \
         &(pos)->member != (head); \
         (pos) = list_entry((pos)->member.next, typeof(*(pos)), member))

/* ---- IDR ---- */
struct idr {
    int next_id;
};

#define DEFINE_IDR(name) struct idr name = { .next_id = 1 }

static inline int idr_alloc(struct idr *idr, void *ptr, int start, int end, int flags) {
    (void)ptr;
    (void)start;
    (void)end;
    (void)flags;
    return idr->next_id++;
}

static inline void *idr_find(struct idr *idr, int id) {
    (void)idr;
    (void)id;
    return NULL;
}

static inline void idr_remove(struct idr *idr, int id) {
    (void)idr;
    (void)id;
}

static inline void idr_destroy(struct idr *idr) {
    (void)idr;
}

#define idr_for_each_entry(idr, entry, id) for ((id) = 0; ((entry) = idr_find((idr), (id))) != NULL; (id)++)

/* ---- Misc ---- */
#define ARRAY_SIZE(arr) (sizeof(arr) / sizeof((arr)[0]))
#define BITS_PER_LONG (sizeof(long) * 8)
#define BIT(n) (1UL << (n))
#define GENMASK(h, l) (((~0UL) >> (BITS_PER_LONG - 1 - (h))) & (~0UL << (l)))
#define GENMASK_ULL(h, l) (((~0ULL) >> (63 - (h))) & (~0ULL << (l)))
#define container_of(ptr, type, member) ((type *)((char *)(ptr) - offsetof(type, member)))
#define likely(x) __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#define WARN_ON(condition) ({ int __ret = !!(condition); if (__ret) fprintf(stderr, "WARN_ON: %s at %s:%d\n", #condition, __FILE__, __LINE__); __ret; })
#define WARN_ON_ONCE(condition) WARN_ON(condition)
#define BUG_ON(condition) do { if (condition) { fprintf(stderr, "BUG: %s at %s:%d\n", #condition, __FILE__, __LINE__); abort(); } } while (0)
#define BUILD_BUG_ON(condition) ((void)sizeof(char[1 - 2 * !!(condition)]))

/* ---- Enum constants ---- */
#define DRM_MODE_DPMS_ON 0
#define DRM_MODE_DPMS_STANDBY 1
#define DRM_MODE_DPMS_SUSPEND 2
#define DRM_MODE_DPMS_OFF 3

#define DRM_CONNECTOR_POLL_HPD (1 << 0)
#define DRM_CONNECTOR_POLL_CONNECT (1 << 1)
#define DRM_CONNECTOR_POLL_DISCONNECT (1 << 2)

/* ---- Minimal DRM structures ---- */
struct drm_device {
    void *dev_private;
    struct device *dev;
};

struct drm_file {
    int filp;
};

struct drm_mode_object {
    int id;
    int type;
};

/* ---- DRM logging helpers ---- */
#define drm_dbg_core(dev, fmt, ...) /* noop */
#define drm_dbg_kms(dev, fmt, ...) /* noop */
#define drm_err(dev, fmt, ...) fprintf(stderr, "[drm ERR] " fmt, ##__VA_ARGS__)
#define drm_info(dev, fmt, ...) fprintf(stderr, "[drm INFO] " fmt, ##__VA_ARGS__)
#define drm_warn(dev, fmt, ...) fprintf(stderr, "[drm WARN] " fmt, ##__VA_ARGS__)
#define drm_dbg(dev, fmt, ...) /* noop */

#endif /* _REDOX_GLUE_H */
