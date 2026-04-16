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

enum ieee80211_sta_state {
    IEEE80211_STA_NOTEXIST,
    IEEE80211_STA_NONE,
    IEEE80211_STA_AUTH,
    IEEE80211_STA_ASSOC,
    IEEE80211_STA_AUTHORIZED,
};

enum set_key_cmd {
    SET_KEY,
    DISABLE_KEY,
};

struct ieee80211_ops {
    void (*tx)(struct ieee80211_hw *hw, struct sk_buff *skb);
    int  (*start)(struct ieee80211_hw *hw);
    void (*stop)(struct ieee80211_hw *hw);
    int  (*add_interface)(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
    void (*remove_interface)(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
    int  (*config)(struct ieee80211_hw *hw, u32 changed);
    void (*bss_info_changed)(struct ieee80211_hw *hw, struct ieee80211_vif *vif,
                             struct ieee80211_bss_conf *info, u32 changed);
    int  (*sta_state)(struct ieee80211_hw *hw, struct ieee80211_vif *vif,
                      struct ieee80211_sta *sta, enum ieee80211_sta_state old_state,
                      enum ieee80211_sta_state new_state);
    int  (*set_key)(struct ieee80211_hw *hw, enum set_key_cmd cmd,
                    struct ieee80211_vif *vif, struct ieee80211_sta *sta,
                    struct key_params *key);
    void (*sw_scan_start)(struct ieee80211_hw *hw, struct ieee80211_vif *vif, const u8 *mac_addr);
    void (*sw_scan_complete)(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
    int  (*sched_scan_start)(struct ieee80211_hw *hw, struct ieee80211_vif *vif, void *req);
    void (*sched_scan_stop)(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
};

#define BSS_CHANGED_ASSOC        (1U << 0)
#define BSS_CHANGED_BSSID        (1U << 1)
#define BSS_CHANGED_ERP_CTS_PROT (1U << 2)
#define BSS_CHANGED_HT           (1U << 3)
#define BSS_CHANGED_BASIC_RATES  (1U << 4)
#define BSS_CHANGED_BEACON_INT   (1U << 5)
#define BSS_CHANGED_BANDWIDTH    (1U << 6)

extern struct ieee80211_hw *ieee80211_alloc_hw_nm(size_t priv_data_len,
                                                  const void *ops,
                                                  const char *requested_name);
extern void ieee80211_free_hw(struct ieee80211_hw *hw);
extern int ieee80211_register_hw(struct ieee80211_hw *hw);
extern void ieee80211_unregister_hw(struct ieee80211_hw *hw);
extern void ieee80211_queue_work(struct ieee80211_hw *hw, void *work);
extern void ieee80211_scan_completed(struct ieee80211_hw *hw, bool aborted);
extern void ieee80211_connection_loss(struct ieee80211_vif *vif);
extern int  ieee80211_start_tx_ba_session(struct ieee80211_sta *sta, u16 tid, u16 timeout);
extern int  ieee80211_stop_tx_ba_session(struct ieee80211_sta *sta, u16 tid);
extern void ieee80211_beacon_loss(struct ieee80211_vif *vif);

#endif
