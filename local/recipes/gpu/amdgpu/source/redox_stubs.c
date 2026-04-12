#include "redox_glue.h"

#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

unsigned long jiffies;

struct redox_mapped_region {
    void *addr;
    size_t size;
    int fd;
    struct redox_mapped_region *next;
};

static pthread_mutex_t g_region_lock = PTHREAD_MUTEX_INITIALIZER;
static struct redox_mapped_region *g_regions;

static void redox_jiffies_advance(unsigned long delta)
{
    __sync_add_and_fetch(&jiffies, delta);
}

void *kmalloc(size_t size, unsigned int flags)
{
    (void)flags;
    return malloc(size);
}

void *kzalloc(size_t size, unsigned int flags)
{
    (void)flags;
    return calloc(1, size);
}

void kfree(const void *ptr)
{
    free((void *)ptr);
}

void *vmalloc(unsigned long size)
{
    return malloc((size_t)size);
}

void vfree(const void *addr)
{
    free((void *)addr);
}

void *krealloc(const void *ptr, size_t new_size, unsigned int flags)
{
    (void)flags;
    return realloc((void *)ptr, new_size);
}

static void redox_track_region(void *addr, size_t size, int fd)
{
    struct redox_mapped_region *region = malloc(sizeof(*region));
    if (!region) {
        if (fd >= 0) {
            close(fd);
        }
        return;
    }

    region->addr = addr;
    region->size = size;
    region->fd = fd;

    pthread_mutex_lock(&g_region_lock);
    region->next = g_regions;
    g_regions = region;
    pthread_mutex_unlock(&g_region_lock);
}

static struct redox_mapped_region *redox_untrack_region(const void *addr)
{
    struct redox_mapped_region *prev = NULL;
    struct redox_mapped_region *cur;

    pthread_mutex_lock(&g_region_lock);
    cur = g_regions;
    while (cur) {
        if (cur->addr == addr) {
            if (prev) {
                prev->next = cur->next;
            } else {
                g_regions = cur->next;
            }
            pthread_mutex_unlock(&g_region_lock);
            return cur;
        }
        prev = cur;
        cur = cur->next;
    }
    pthread_mutex_unlock(&g_region_lock);
    return NULL;
}

void __iomem *redox_ioremap(phys_addr_t offset, size_t size)
{
    int fd = open("/scheme/memory/physical", O_RDWR);
    void *addr;

    if (fd >= 0) {
        addr = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, (off_t)offset);
        if (addr != MAP_FAILED) {
            redox_track_region(addr, size, fd);
            return addr;
        }
        close(fd);
    }

    addr = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (addr == MAP_FAILED) {
        pr_err("ioremap fallback failed for %#llx (%zu bytes): %s\n",
               (unsigned long long)offset, size, strerror(errno));
        return NULL;
    }

    memset(addr, 0, size);
    redox_track_region(addr, size, -1);
    return addr;
}

void redox_iounmap(void __iomem *addr)
{
    struct redox_mapped_region *region;

    if (!addr) {
        return;
    }

    region = redox_untrack_region(addr);
    if (!region) {
        return;
    }

    munmap(region->addr, region->size);
    if (region->fd >= 0) {
        close(region->fd);
    }
    free(region);
}

void redox_iowrite32(u32 val, void __iomem *addr)
{
    *(volatile u32 *)addr = val;
}

u32 redox_ioread32(const void __iomem *addr)
{
    return *(volatile const u32 *)addr;
}

void redox_iowrite16(u16 val, void __iomem *addr)
{
    *(volatile u16 *)addr = val;
}

u16 redox_ioread16(const void __iomem *addr)
{
    return *(volatile const u16 *)addr;
}

void redox_iowrite8(u8 val, void __iomem *addr)
{
    *(volatile u8 *)addr = val;
}

u8 redox_ioread8(const void __iomem *addr)
{
    return *(volatile const u8 *)addr;
}

void redox_mmio_write32(void *base, u32 offset, u32 val)
{
    if (!base) {
        return;
    }
    *(volatile u32 *)((u8 *)base + offset) = val;
}

u32 redox_mmio_read32(void *base, u32 offset)
{
    if (!base) {
        return 0;
    }
    return *(volatile u32 *)((u8 *)base + offset);
}

void *redox_dma_alloc_coherent(size_t size, dma_addr_t *dma_handle)
{
    void *ptr = NULL;

    if (posix_memalign(&ptr, PAGE_SIZE, PAGE_ALIGN(size)) != 0) {
        return NULL;
    }

    memset(ptr, 0, PAGE_ALIGN(size));
    if (dma_handle) {
        *dma_handle = (dma_addr_t)(uintptr_t)ptr;
    }
    return ptr;
}

void redox_dma_free_coherent(size_t size, void *vaddr, dma_addr_t dma_handle)
{
    (void)size;
    (void)dma_handle;
    free(vaddr);
}

struct pci_dev *redox_pci_find_amd_gpu(void)
{
    static struct pci_dev dev = {
        .vendor = 0x1002,
        .device = 0,
        .revision = 0,
        .irq = 0,
        .resource_start = {0},
        .resource_len = {0},
        .resource_flags = {IORESOURCE_MEM, 0, 0, 0, 0, 0},
        .driver_data = NULL,
        .mmio_base = NULL,
        .is_amdgpu = 1,
    };

    return &dev;
}

void redox_pci_dev_put(struct pci_dev *pdev)
{
    (void)pdev;
}

int redox_pci_enable_device(struct pci_dev *pdev)
{
    return pdev ? 0 : -ENODEV;
}

void redox_pci_set_master(struct pci_dev *pdev)
{
    (void)pdev;
}

int redox_pci_request_regions(struct pci_dev *pdev, const char *name)
{
    (void)name;
    return pdev ? 0 : -ENODEV;
}

void redox_pci_release_regions(struct pci_dev *pdev)
{
    (void)pdev;
}

int redox_request_firmware(const struct firmware **fw, const char *name, void *dev)
{
    char path[512];
    int fd;
    struct stat st;
    struct firmware *image;
    u8 *data;
    ssize_t nread;

    (void)dev;
    if (!fw || !name) {
        return -EINVAL;
    }

    snprintf(path, sizeof(path), "/scheme/firmware/amdgpu/%s", name);
    fd = open(path, O_RDONLY);
    if (fd < 0) {
        return -ENOENT;
    }

    if (fstat(fd, &st) != 0 || st.st_size < 0) {
        close(fd);
        return -EIO;
    }

    image = calloc(1, sizeof(*image));
    data = malloc((size_t)st.st_size);
    if (!image || !data) {
        free(image);
        free(data);
        close(fd);
        return -ENOMEM;
    }

    nread = read(fd, data, (size_t)st.st_size);
    close(fd);
    if (nread != st.st_size) {
        free(image);
        free(data);
        return -EIO;
    }

    image->size = (size_t)st.st_size;
    image->data = data;
    *fw = image;
    return 0;
}

void redox_release_firmware(const struct firmware *fw)
{
    struct firmware *owned = (struct firmware *)fw;

    if (!owned) {
        return;
    }

    free((void *)owned->data);
    free(owned);
}

int redox_request_irq(unsigned int irq, irq_handler_t handler, unsigned long flags, const char *name, void *dev)
{
    char path[128];
    int fd;

    (void)handler;
    (void)flags;
    (void)name;
    (void)dev;

    snprintf(path, sizeof(path), "/scheme/irq/%u", irq);
    fd = open(path, O_RDWR);
    if (fd < 0) {
        return -ENOENT;
    }

    close(fd);
    return 0;
}

void redox_free_irq(unsigned int irq, void *dev_id)
{
    (void)irq;
    (void)dev_id;
}

void msleep(unsigned int msecs)
{
    struct timespec ts;

    ts.tv_sec = msecs / 1000U;
    ts.tv_nsec = (long)(msecs % 1000U) * 1000000L;
    nanosleep(&ts, NULL);
    redox_jiffies_advance(msecs_to_jiffies(msecs));
}

void udelay(unsigned long usecs)
{
    struct timespec ts;

    ts.tv_sec = usecs / 1000000UL;
    ts.tv_nsec = (long)(usecs % 1000000UL) * 1000L;
    nanosleep(&ts, NULL);
    redox_jiffies_advance(usecs_to_jiffies((unsigned int)usecs));
}

void mdelay(unsigned long msecs)
{
    msleep((unsigned int)msecs);
}

unsigned long msecs_to_jiffies(unsigned int msecs)
{
    return (unsigned long)msecs;
}

unsigned long usecs_to_jiffies(unsigned int usecs)
{
    return (unsigned long)DIV_ROUND_UP(usecs, 1000U);
}
