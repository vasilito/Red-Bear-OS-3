#ifndef _LINUX_IEEE80211_H
#define _LINUX_IEEE80211_H

#include "types.h"

#define IEEE80211_MAX_SSID_LEN 32
#define IEEE80211_NUM_ACS 4

struct ieee80211_channel {
    u32 band;
    u16 center_freq;
    u16 hw_value;
    u32 flags;
    s8 max_power;
    s8 max_reg_power;
    s8 max_antenna_gain;
    bool beacon_found;
};

struct ieee80211_rate {
    u32 flags;
    u16 bitrate;
    u16 hw_value;
    u16 hw_value_short;
};

#endif
