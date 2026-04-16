#ifndef _LINUX_NETDEVICE_H
#define _LINUX_NETDEVICE_H

#include "device.h"
#include "types.h"
#include <stddef.h>

struct net_device {
    char name[16];
    unsigned char dev_addr[6];
    unsigned char addr_len;
    unsigned int mtu;
    unsigned int flags;
    int carrier;
    void *ml_priv;
    void *ieee80211_ptr;
    void *priv_data;
    int registered;
    size_t __priv_alloc_size;
    size_t __priv_alloc_align;
};

extern struct net_device *alloc_netdev_mqs(size_t sizeof_priv,
                                           const char *name,
                                           unsigned char name_assign_type,
                                           void (*setup)(struct net_device *dev),
                                           unsigned int txqs,
                                           unsigned int rxqs);
extern void free_netdev(struct net_device *dev);
extern int register_netdev(struct net_device *dev);
extern void unregister_netdev(struct net_device *dev);
extern void netif_carrier_on(struct net_device *dev);
extern void netif_carrier_off(struct net_device *dev);
extern int netif_carrier_ok(const struct net_device *dev);

#endif
