#ifndef _LINUX_PCI_H
#define _LINUX_PCI_H

#include "types.h"
#include "device.h"
#include "io.h"
#include <stddef.h>

#define PCI_VENDOR_ID_AMD    0x1002U
#define PCI_VENDOR_ID_INTEL  0x8086U
#define PCI_VENDOR_ID_NVIDIA 0x10DEU

#define PCI_ANY_ID (~0U)

/* MSI/MSI-X support */
#define PCI_IRQ_MSI      1U
#define PCI_IRQ_MSIX     2U
#define PCI_IRQ_LEGACY   4U
#define PCI_IRQ_NOLEGACY 8U

struct pci_device_id {
    u32 vendor;
    u32 device;
    u32 subvendor;
    u32 subdevice;
    u32 class_code;
    u32 class_mask;
    unsigned long driver_data;
};

struct pci_dev {
    u16 vendor;
    u16 device_id;
    u8  bus_number;
    u8  dev_number;
    u8  func_number;
    u8  revision;
    u32 irq;
    u64 resource_start[6];
    u64 resource_len[6];
    void *driver_data;
    struct device device_obj;
};

struct pci_driver {
    const char *name;
    const struct pci_device_id *id_table;
    int  (*probe)(struct pci_dev *dev, const struct pci_device_id *id);
    void (*remove)(struct pci_dev *dev);
    int  (*suspend)(struct pci_dev *dev, u32 state);
    int  (*resume)(struct pci_dev *dev);
    void (*shutdown)(struct pci_dev *dev);
};

struct msix_entry {
    u32 vector;
    u16 entry;
    u16 _pad;
};

extern int  pci_enable_device(struct pci_dev *dev);
extern void pci_disable_device(struct pci_dev *dev);
extern void pci_set_master(struct pci_dev *dev);
extern int  pci_alloc_irq_vectors(struct pci_dev *dev, int min_vecs, int max_vecs, unsigned int flags);
extern void pci_free_irq_vectors(struct pci_dev *dev);
extern int  pci_irq_vector(struct pci_dev *dev, unsigned int nr);
extern int  pci_enable_msi(struct pci_dev *dev);
extern void pci_disable_msi(struct pci_dev *dev);
extern int  pci_enable_msix_range(struct pci_dev *dev, struct msix_entry *entries, int minvec, int maxvec);
extern void pci_disable_msix(struct pci_dev *dev);

extern void *pci_iomap(struct pci_dev *dev, unsigned int bar, size_t max_len);
extern void  pci_iounmap(struct pci_dev *dev, void *addr, size_t size);

extern int pci_read_config_dword(struct pci_dev *dev, unsigned int offset, u32 *val);
extern int pci_write_config_dword(struct pci_dev *dev, unsigned int offset, u32 val);

extern u64 pci_resource_start(struct pci_dev *dev, unsigned int bar);
extern u64 pci_resource_len(struct pci_dev *dev, unsigned int bar);

extern u64 pci_get_quirk_flags(struct pci_dev *dev);
extern bool pci_has_quirk(struct pci_dev *dev, u64 flag);

#define PCI_QUIRK_NO_MSI         (1ULL << 0)
#define PCI_QUIRK_NO_MSIX        (1ULL << 1)
#define PCI_QUIRK_FORCE_LEGACY   (1ULL << 2)
#define PCI_QUIRK_NO_PM          (1ULL << 3)
#define PCI_QUIRK_NO_D3COLD      (1ULL << 4)
#define PCI_QUIRK_NO_ASPM        (1ULL << 5)
#define PCI_QUIRK_NEED_IOMMU     (1ULL << 6)
#define PCI_QUIRK_DMA_32BIT_ONLY (1ULL << 8)
#define PCI_QUIRK_NEED_FIRMWARE  (1ULL << 11)
#define PCI_QUIRK_DISABLE_ACCEL  (1ULL << 12)

extern int pci_register_driver(struct pci_driver *drv);
extern void pci_unregister_driver(struct pci_driver *drv);

#define MODULE_DEVICE_TABLE(type, name)

#define PCI_DEVICE(vend, dev) \
    .vendor = (vend), .device = (dev), \
    .subvendor = PCI_ANY_ID, .subdevice = PCI_ANY_ID

#endif
