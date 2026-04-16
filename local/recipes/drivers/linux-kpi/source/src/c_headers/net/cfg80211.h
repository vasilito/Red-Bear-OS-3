#ifndef _NET_CFG80211_H
#define _NET_CFG80211_H

#include "../linux/device.h"
#include "../linux/ieee80211.h"
#include "../linux/netdevice.h"
#include "../linux/types.h"
#include <stddef.h>
#include <stdbool.h>

struct wiphy {
    void *privid;
    int registered;
    u32 interface_modes;
    int max_scan_ssids;
    int max_scan_ie_len;
};

struct wireless_dev {
    struct wiphy *wiphy;
    struct net_device *netdev;
    u32 iftype;
    bool scan_in_flight;
    bool scan_aborted;
    bool connecting;
    bool connected;
    bool locally_generated;
    u16 last_status;
    u16 last_reason;
    bool has_bssid;
    u8 last_bssid[6];
};

struct cfg80211_scan_info {
    bool aborted;
};

struct cfg80211_scan_request {
    struct wiphy *wiphy;
    struct wireless_dev *wdev;
    u32 n_ssids;
    u32 n_channels;
};

struct cfg80211_ssid {
    u8 ssid[IEEE80211_MAX_SSID_LEN];
    u8 ssid_len;
};

struct key_params {
    const u8 *key;
    u8 key_len;
    u32 cipher;
};

struct cfg80211_connect_params {
    const u8 *ssid;
    size_t ssid_len;
    const u8 *bssid;
    const u8 *ie;
    size_t ie_len;
    struct key_params key;
};

struct station_parameters {
    const u8 *supported_rates;
    size_t supported_rates_len;
    u32 sta_flags_mask;
    u32 sta_flags_set;
};

extern struct wiphy *wiphy_new_nm(const void *ops, size_t sizeof_priv, const char *requested_name);
extern void wiphy_free(struct wiphy *wiphy);
extern int wiphy_register(struct wiphy *wiphy);
extern void wiphy_unregister(struct wiphy *wiphy);

extern void cfg80211_scan_done(struct cfg80211_scan_request *request,
                               const struct cfg80211_scan_info *info);
extern void cfg80211_connect_result(struct net_device *dev,
                                    const u8 *bssid,
                                    const u8 *req_ie,
                                    size_t req_ie_len,
                                    const u8 *resp_ie,
                                    size_t resp_ie_len,
                                    u16 status,
                                    gfp_t gfp);
extern void cfg80211_disconnected(struct net_device *dev,
                                  u16 reason,
                                  const u8 *ie,
                                  size_t ie_len,
                                  bool locally_generated,
                                  gfp_t gfp);
extern void cfg80211_connect_bss(struct net_device *dev,
                                 const u8 *bssid,
                                 const u8 *req_ie,
                                 size_t req_ie_len,
                                 const u8 *resp_ie,
                                 size_t resp_ie_len,
                                 u16 status,
                                 gfp_t gfp);
extern void cfg80211_ready_on_channel(struct wireless_dev *wdev,
                                      u64 cookie,
                                      struct ieee80211_channel *chan,
                                      u32 chan_type,
                                      u32 duration,
                                      gfp_t gfp);

#endif
