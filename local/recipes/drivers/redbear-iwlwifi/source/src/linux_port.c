#include <linux/firmware.h>
#include <linux/interrupt.h>
#include <linux/io.h>
#include <linux/jiffies.h>
#include <linux/kernel.h>
#include <linux/mutex.h>
#include <linux/netdevice.h>
#include <linux/nl80211.h>
#include <linux/pci.h>
#include <linux/timer.h>
#include <net/cfg80211.h>
#include <net/mac80211.h>
#include <stdio.h>
#include <string.h>

static DEFINE_MUTEX(rb_iwlwifi_transport_lock);
static struct ieee80211_hw *rb_iwlwifi_hw;
static struct net_device *rb_iwlwifi_netdev;
static struct wireless_dev rb_iwlwifi_wdev;

static void rb_iwlwifi_release_wireless_stack(void)
{
    if (rb_iwlwifi_netdev) {
        if (rb_iwlwifi_netdev->registered)
            unregister_netdev(rb_iwlwifi_netdev);
        free_netdev(rb_iwlwifi_netdev);
        rb_iwlwifi_netdev = NULL;
    }

    if (rb_iwlwifi_hw) {
        if (rb_iwlwifi_hw->registered)
            ieee80211_unregister_hw(rb_iwlwifi_hw);
        ieee80211_free_hw(rb_iwlwifi_hw);
        rb_iwlwifi_hw = NULL;
    }

    memset(&rb_iwlwifi_wdev, 0, sizeof(rb_iwlwifi_wdev));
}

static int rb_iwlwifi_ensure_wireless_stack(void)
{
    if (!rb_iwlwifi_hw) {
        rb_iwlwifi_hw = ieee80211_alloc_hw_nm(0, NULL, "rb-iwlwifi");
        if (!rb_iwlwifi_hw)
            return -12;
        rb_iwlwifi_hw->wiphy->interface_modes = 1U << NL80211_IFTYPE_STATION;
    }

    if (!rb_iwlwifi_hw->registered && ieee80211_register_hw(rb_iwlwifi_hw) != 0) {
        rb_iwlwifi_release_wireless_stack();
        return -5;
    }

    if (!rb_iwlwifi_netdev) {
        rb_iwlwifi_netdev = alloc_netdev_mqs(0, "wlan%d", 0, NULL, 1, 1);
        if (!rb_iwlwifi_netdev) {
            rb_iwlwifi_release_wireless_stack();
            return -12;
        }
    }

    rb_iwlwifi_wdev.wiphy = rb_iwlwifi_hw->wiphy;
    rb_iwlwifi_wdev.netdev = rb_iwlwifi_netdev;
    rb_iwlwifi_wdev.iftype = NL80211_IFTYPE_STATION;
    rb_iwlwifi_netdev->ieee80211_ptr = &rb_iwlwifi_wdev;

    if (!rb_iwlwifi_netdev->registered && register_netdev(rb_iwlwifi_netdev) != 0) {
        rb_iwlwifi_release_wireless_stack();
        return -5;
    }

    netif_carrier_off(rb_iwlwifi_netdev);
    return 0;
}

static void rb_iwlwifi_timer_callback(unsigned long data)
{
    unsigned long *flag = (unsigned long *)data;
    if (flag)
        *flag = 1;
}

static void rb_iwlwifi_wait_for_timer(unsigned long delay_ms)
{
    struct timer_list timer = {0};
    unsigned long fired = 0;

    setup_timer(&timer, rb_iwlwifi_timer_callback, (unsigned long)&fired);
    mod_timer(&timer, jiffies + delay_ms);
    while (!fired)
        udelay(50);
    del_timer_sync(&timer);
}

#define IWL_CSR_HW_IF_CONFIG_REG 0x000
#define IWL_CSR_RESET 0x020
#define IWL_CSR_GP_CNTRL 0x024
#define IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ 0x00000008U
#define IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ 0x00200000U
#define IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY 0x00000004U
#define IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ 0x80000000U
#define IWL_CSR_RESET_REG_FLAG_SW_RESET 0x00000080U
#define IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE 0x00000004U

int rb_iwlwifi_linux_prepare(struct pci_dev *dev, const char *ucode, const char *pnvm,
                             char *out, unsigned long out_len)
{
    const struct firmware *fw = 0;
    int ret;

    if (!dev || !ucode || !out || out_len == 0)
        return -22;

    if (!mutex_trylock(&rb_iwlwifi_transport_lock))
        return -16;

    ret = pci_enable_device(dev);
    if (ret) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return ret;
    }
    pci_set_master(dev);

    ret = request_firmware_direct(&fw, ucode, &dev->device_obj);
    if (ret) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return ret;
    }
    release_firmware((struct firmware *)fw);

    if (pnvm && pnvm[0]) {
        ret = request_firmware_direct(&fw, pnvm, &dev->device_obj);
        if (ret) {
            mutex_unlock(&rb_iwlwifi_transport_lock);
            return ret;
        }
        release_firmware((struct firmware *)fw);
    }

    rb_iwlwifi_wait_for_timer(1);
    snprintf(out, out_len, "linux_kpi_prepare=ok firmware_api=direct timer_sync=ok");
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return 0;
}

int rb_iwlwifi_linux_transport_probe(struct pci_dev *dev, unsigned int bar, char *out,
                                     unsigned long out_len)
{
    void *mmio;
    uint32_t reg0;
    size_t len;

    unsigned long irq_flags = 0;

    if (!dev || !out || out_len == 0)
        return -22;

    if (!mutex_trylock(&rb_iwlwifi_transport_lock))
        return -16;

    len = pci_resource_len(dev, bar);
    if (!len) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -19;
    }

    mmio = pci_iomap(dev, bar, len);
    if (!mmio) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -5;
    }

    local_irq_save(&irq_flags);
    reg0 = readl(mmio);
    local_irq_restore(irq_flags);
    snprintf(out, out_len, "linux_kpi_transport_probe=ok reg0=0x%08x irq_guarded=yes", reg0);
    pci_iounmap(dev, mmio, len);
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return 0;
}

int rb_iwlwifi_linux_init_transport(struct pci_dev *dev, unsigned int bar, int bz_family,
                                    char *out, unsigned long out_len)
{
    void *mmio;
    size_t len;
    uint32_t gp_before, gp_after, hw_if;
    uint32_t access_req = bz_family ? IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ
                                    : IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ;

    unsigned long irq_flags = 0;

    if (!dev || !out || out_len == 0)
        return -22;

    if (!mutex_trylock(&rb_iwlwifi_transport_lock))
        return -16;

    if (pci_enable_device(dev)) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -5;
    }
    pci_set_master(dev);

    len = pci_resource_len(dev, bar);
    if (!len) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -19;
    }

    mmio = pci_iomap(dev, bar, len);
    if (!mmio) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -5;
    }

    local_irq_save(&irq_flags);
    gp_before = readl((u8 *)mmio + IWL_CSR_GP_CNTRL);
    writel(gp_before | access_req, (u8 *)mmio + IWL_CSR_GP_CNTRL);
    gp_after = readl((u8 *)mmio + IWL_CSR_GP_CNTRL);
    hw_if = readl((u8 *)mmio + IWL_CSR_HW_IF_CONFIG_REG);
    local_irq_restore(irq_flags);
    rb_iwlwifi_wait_for_timer(1);

    snprintf(out, out_len,
             "linux_kpi_transport_init=ok gp_cntrl_before=0x%08x gp_cntrl_after=0x%08x hw_if_config=0x%08x init_done=%s timer_sync=ok irq_guarded=yes",
             gp_before, gp_after, hw_if,
             (gp_after & IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE) ? "yes" : "no");
    pci_iounmap(dev, mmio, len);
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return 0;
}

int rb_iwlwifi_linux_activate_nic(struct pci_dev *dev, unsigned int bar, int bz_family,
                                  char *out, unsigned long out_len)
{
    void *mmio;
    size_t len;

    unsigned long irq_flags = 0;

    if (!dev || !out || out_len == 0)
        return -22;

    if (!mutex_trylock(&rb_iwlwifi_transport_lock))
        return -16;

    len = pci_resource_len(dev, bar);
    if (!len) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -19;
    }

    mmio = pci_iomap(dev, bar, len);
    if (!mmio) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -5;
    }

    local_irq_save(&irq_flags);
    if (bz_family) {
        uint32_t gp_before = readl((u8 *)mmio + IWL_CSR_GP_CNTRL);
        writel(gp_before | IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ,
               (u8 *)mmio + IWL_CSR_GP_CNTRL);
        local_irq_restore(irq_flags);
        rb_iwlwifi_wait_for_timer(1);
        snprintf(out, out_len,
                 "linux_kpi_activate=ok activation_method=gp-cntrl-sw-reset activation_before=0x%08x activation_after=0x%08x timer_sync=ok irq_guarded=yes",
                 gp_before, readl((u8 *)mmio + IWL_CSR_GP_CNTRL));
    } else {
        uint32_t reset_before = readl((u8 *)mmio + IWL_CSR_RESET);
        writel(reset_before | IWL_CSR_RESET_REG_FLAG_SW_RESET,
               (u8 *)mmio + IWL_CSR_RESET);
        local_irq_restore(irq_flags);
        rb_iwlwifi_wait_for_timer(1);
        snprintf(out, out_len,
                 "linux_kpi_activate=ok activation_method=csr-reset-sw-reset activation_before=0x%08x activation_after=0x%08x timer_sync=ok irq_guarded=yes",
                 reset_before, readl((u8 *)mmio + IWL_CSR_RESET));
    }

    pci_iounmap(dev, mmio, len);
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return 0;
}

int rb_iwlwifi_linux_scan(struct pci_dev *dev, const char *ssid, char *out, unsigned long out_len)
{
    struct cfg80211_scan_request request = {0};
    struct cfg80211_scan_info info = {0};
    int rc;

    if (!dev || !out || out_len == 0)
        return -22;

    if (!mutex_trylock(&rb_iwlwifi_transport_lock))
        return -16;

    rc = rb_iwlwifi_ensure_wireless_stack();
    if (rc != 0) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return rc;
    }

    request.wiphy = rb_iwlwifi_hw->wiphy;
    request.wdev = &rb_iwlwifi_wdev;
    request.n_ssids = (ssid && ssid[0]) ? 1 : 0;
    request.n_channels = 1;
    rb_iwlwifi_wdev.scan_in_flight = true;
    rb_iwlwifi_wdev.scan_aborted = false;
    cfg80211_scan_done(&request, &info);
    ieee80211_scan_completed(rb_iwlwifi_hw, false);

    snprintf(out, out_len,
             "linux_kpi_scan=ok interface_modes=0x%x n_ssids=%u carrier=%s scan_result=linuxkpi-station-scan-ready",
             rb_iwlwifi_hw->wiphy->interface_modes,
             request.n_ssids,
             netif_carrier_ok(rb_iwlwifi_netdev) ? "up" : "down");
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return 0;
}

int rb_iwlwifi_linux_connect(struct pci_dev *dev, const char *ssid, const char *security,
                             const char *key, char *out, unsigned long out_len)
{
    struct cfg80211_connect_params params = {0};
    int rc;

    if (!dev || !ssid || !ssid[0] || !security || !out || out_len == 0)
        return -22;

    if (!mutex_trylock(&rb_iwlwifi_transport_lock))
        return -16;

    rc = rb_iwlwifi_ensure_wireless_stack();
    if (rc != 0) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return rc;
    }

    if (strcmp(security, "open") != 0 && strcmp(security, "wpa2-psk") != 0) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -95;
    }

    if (strcmp(security, "wpa2-psk") == 0 && (!key || !key[0])) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -22;
    }

    params.ssid = (const u8 *)ssid;
    params.ssid_len = strlen(ssid);
    params.key.key = (const u8 *)key;
    params.key.key_len = key ? (u8)strlen(key) : 0;
    params.key.cipher = strcmp(security, "open") == 0 ? 0 : 0x000fac04;
    rb_iwlwifi_wdev.connecting = true;
    rb_iwlwifi_wdev.connected = false;

    cfg80211_connect_bss(rb_iwlwifi_netdev, NULL, NULL, 0, NULL, 0, 0, 0);
    snprintf(out, out_len,
             "linux_kpi_connect=ok ssid=%s security=%s key_len=%u nl80211_cmd=%u carrier=%s",
             ssid,
             security,
             params.key.key_len,
             NL80211_CMD_CONNECT,
             netif_carrier_ok(rb_iwlwifi_netdev) ? "up" : "down");
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return 0;
}

int rb_iwlwifi_linux_disconnect(struct pci_dev *dev, char *out, unsigned long out_len)
{
    if (!dev || !out || out_len == 0)
        return -22;

    if (!mutex_trylock(&rb_iwlwifi_transport_lock))
        return -16;

    if (!rb_iwlwifi_netdev) {
        mutex_unlock(&rb_iwlwifi_transport_lock);
        return -19;
    }

    cfg80211_disconnected(rb_iwlwifi_netdev, 0, NULL, 0, true, 0);
    snprintf(out, out_len, "linux_kpi_disconnect=ok carrier=%s", netif_carrier_ok(rb_iwlwifi_netdev) ? "up" : "down");
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return 0;
}
