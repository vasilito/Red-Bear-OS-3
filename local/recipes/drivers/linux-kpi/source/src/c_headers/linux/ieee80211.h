#ifndef _LINUX_IEEE80211_H
#define _LINUX_IEEE80211_H

#include "types.h"

#define IEEE80211_MAX_SSID_LEN 32
#define IEEE80211_NUM_ACS 4

struct ieee80211_channel {
    u16 center_freq;
    u16 hw_value;
    u32 flags;
};

struct ieee80211_rate {
    u16 bitrate;
    u16 hw_value;
};

#endif
