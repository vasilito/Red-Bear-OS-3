#ifndef _LINUX_PCI_H
#define _LINUX_PCI_H

#include <linux/types.h>
#include <linux/device.h>
#include <linux/io.h>
#include <stddef.h>

#define PCI_VENDOR_ID_AMD    0x1002U
#define PCI_VENDOR_ID_INTEL  0x8086U
#define PCI_VENDOR_ID_NVIDIA 0x10DEU

#define PCI_ANY_ID (~0U)

struct pci_device_id {
    u32 vendor;
    u32 device;
    u32 subvendor;
    u32 subdevice;
    u32 class;
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

extern int  pci_enable_device(struct pci_dev *dev);
extern void pci_disable_device(struct pci_dev *dev);
extern void pci_set_master(struct pci_dev *dev);

extern void *pci_iomap(struct pci_dev *dev, unsigned int bar, size_t max_len);
extern void  pci_iounmap(struct pci_dev *dev, void *addr, size_t size);

extern int pci_read_config_dword(struct pci_dev *dev, unsigned int offset, u32 *val);
extern int pci_write_config_dword(struct pci_dev *dev, unsigned int offset, u32 val);

extern u64 pci_resource_start(struct pci_dev *dev, unsigned int bar);
extern u64 pci_resource_len(struct pci_dev *dev, unsigned int bar);

extern int pci_register_driver(struct pci_driver *drv);
extern void pci_unregister_driver(struct pci_driver *drv);

#define MODULE_DEVICE_TABLE(type, name)

#define PCI_DEVICE(vend, dev) \
    .vendor = (vend), .device = (dev), \
    .subvendor = PCI_ANY_ID, .subdevice = PCI_ANY_ID

#endif
