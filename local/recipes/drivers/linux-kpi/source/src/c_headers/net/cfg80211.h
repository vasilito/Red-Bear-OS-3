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
    u8 key_idx;
};

struct cfg80211_bss {
    u8 bssid[6];
    struct ieee80211_channel *channel;
    s16 signal;
    u16 capability;
    u16 beacon_interval;
    const u8 *ies;
    size_t ies_len;
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
extern void cfg80211_new_sta(struct net_device *dev, const u8 *mac_addr,
                              struct station_parameters *params, gfp_t gfp);
extern void cfg80211_rx_mgmt(struct wireless_dev *wdev, u32 freq, int sig_dbm,
                              const u8 *buf, size_t len, gfp_t gfp);
extern void cfg80211_mgmt_tx_status(struct wireless_dev *wdev, u64 cookie,
                                    const u8 *buf, size_t len, bool ack, gfp_t gfp);
extern void cfg80211_sched_scan_results(struct wiphy *wiphy, u64 reqid);
extern void cfg80211_ready_on_channel(struct wireless_dev *wdev,
                                      u64 cookie,
                                      struct ieee80211_channel *chan,
                                      u32 chan_type,
                                      u32 duration,
                                      gfp_t gfp);
extern u32 ieee80211_channel_to_frequency(u32 chan, u32 band);
extern u32 ieee80211_frequency_to_channel(u32 freq);
extern struct cfg80211_bss *cfg80211_inform_bss(struct wiphy *wiphy,
                                                struct wireless_dev *wdev,
                                                u32 freq,
                                                const u8 *bssid,
                                                u64 tsf,
                                                u16 capability,
                                                u16 beacon_interval,
                                                const u8 *ie,
                                                size_t ielen,
                                                int signal,
                                                gfp_t gfp);
extern void cfg80211_put_bss(struct cfg80211_bss *bss);

#endif
