#ifndef _NET_MAC80211_H
#define _NET_MAC80211_H

#include "../linux/device.h"
#include "../linux/netdevice.h"
#include "../linux/skbuff.h"
#include "../linux/types.h"
#include "cfg80211.h"

struct ieee80211_hw {
    struct wiphy *wiphy;
    void *priv;
    int registered;
    u32 extra_tx_headroom;
    u16 queues;
};

struct ieee80211_vif {
    u8 addr[6];
    void *drv_priv;
    u32 type;
    bool cfg_assoc;
};

struct ieee80211_sta {
    u8 addr[6];
    void *drv_priv;
    u16 aid;
};

struct ieee80211_bss_conf {
    bool assoc;
    u16 aid;
    u16 beacon_int;
};

extern struct ieee80211_hw *ieee80211_alloc_hw_nm(size_t priv_data_len,
                                                  const void *ops,
                                                  const char *requested_name);
extern void ieee80211_free_hw(struct ieee80211_hw *hw);
extern int ieee80211_register_hw(struct ieee80211_hw *hw);
extern void ieee80211_unregister_hw(struct ieee80211_hw *hw);
extern void ieee80211_queue_work(struct ieee80211_hw *hw, void *work);
extern void ieee80211_scan_completed(struct ieee80211_hw *hw, bool aborted);
extern void ieee80211_connection_loss(struct ieee80211_vif *vif);

#endif
