#include "../../../linux-kpi/source/src/c_headers/linux/types.h"
#include "../../../linux-kpi/source/src/c_headers/linux/ieee80211.h"
#include "../../../linux-kpi/source/src/c_headers/linux/atomic.h"
#include "../../../linux-kpi/source/src/c_headers/linux/dma-mapping.h"
#include "../../../linux-kpi/source/src/c_headers/linux/errno.h"
#include "../../../linux-kpi/source/src/c_headers/linux/firmware.h"
#include "../../../linux-kpi/source/src/c_headers/linux/interrupt.h"
#include "../../../linux-kpi/source/src/c_headers/linux/io.h"
#include "../../../linux-kpi/source/src/c_headers/linux/jiffies.h"
#include "../../../linux-kpi/source/src/c_headers/linux/kernel.h"
#include "../../../linux-kpi/source/src/c_headers/linux/list.h"
#include "../../../linux-kpi/source/src/c_headers/linux/mutex.h"
#include "../../../linux-kpi/source/src/c_headers/linux/netdevice.h"
#include "../../../linux-kpi/source/src/c_headers/linux/nl80211.h"
#include "../../../linux-kpi/source/src/c_headers/linux/pci.h"
#include "../../../linux-kpi/source/src/c_headers/linux/printk.h"
#include "../../../linux-kpi/source/src/c_headers/linux/skbuff.h"
#include "../../../linux-kpi/source/src/c_headers/linux/slab.h"
#include "../../../linux-kpi/source/src/c_headers/linux/spinlock.h"
#include "../../../linux-kpi/source/src/c_headers/linux/timer.h"
#include "../../../linux-kpi/source/src/c_headers/linux/wait.h"
#include "../../../linux-kpi/source/src/c_headers/net/cfg80211.h"
#include "../../../linux-kpi/source/src/c_headers/net/mac80211.h"
#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define RB_IWL_MAX_TBS 6
#define RB_IWL_MAX_TX_QUEUES 16
#define RB_IWL_CMD_QUEUE 0
#define RB_IWL_TXQ_SLOTS 256
#define RB_IWL_CMD_SLOTS 64
#define RB_IWL_RX_BUFS 128
#define RB_IWL_RX_BUF_SIZE 4096
#define RB_IWL_CMD_TIMEOUT 500
#define RB_IWL_MAX_FW_NAME 128
#define RB_IWL_MAX_SECURITY 32
#define RB_IWL_MAX_SCAN_CHANNELS 16

#define RB_IWL_SVC_PREPARED      (1U << 0)
#define RB_IWL_SVC_PROBED        (1U << 1)
#define RB_IWL_SVC_INIT          (1U << 2)
#define RB_IWL_SVC_ACTIVE        (1U << 3)
#define RB_IWL_SVC_MAC80211      (1U << 4)
#define RB_IWL_SVC_SCAN_ACTIVE   (1U << 5)
#define RB_IWL_SVC_CONNECTED     (1U << 6)
#define RB_IWL_SVC_DMA_READY     (1U << 7)
#define RB_IWL_SVC_IRQ_READY     (1U << 8)

#define RB_IWL_INT_RX    (1U << 0)
#define RB_IWL_INT_TX    (1U << 1)
#define RB_IWL_INT_CMD   (1U << 2)
#define RB_IWL_INT_SCAN  (1U << 3)
#define RB_IWL_INT_ERROR (1U << 4)

#define RB_IWL_DEVICE_FAMILY_7000 7000
#define RB_IWL_DEVICE_FAMILY_8000 8000
#define RB_IWL_DEVICE_FAMILY_9000 9000
#define RB_IWL_DEVICE_FAMILY_AX210 21000
#define RB_IWL_DEVICE_FAMILY_BZ 30000

#define RB_IWL_CMD_SCAN 0x1001U
#define RB_IWL_CMD_ASSOC 0x1002U
#define RB_IWL_CMD_DISCONNECT 0x1003U
#define RB_IWL_CMD_FIRMWARE_BOOT 0x1004U

#define IWL_CSR_HW_IF_CONFIG_REG 0x000U
#define IWL_CSR_INT 0x008U
#define IWL_CSR_INT_MASK 0x00CU
#define IWL_CSR_RESET 0x020U
#define IWL_CSR_GP_CNTRL 0x024U
#define IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ 0x00000008U
#define IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ 0x00200000U
#define IWL_CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY 0x00000001U
#define IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE 0x00000004U
#define IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ 0x80000000U
#define IWL_CSR_HW_IF_CONFIG_REG_BIT_NIC_READY 0x00000004U
#define IWL_CSR_RESET_REG_FLAG_SW_RESET 0x00000080U

#define IWL_FH_RSCSR_CHNL0_RBDCB_BASE_REG 0x0A80U
#define IWL_FH_RSCSR_CHNL0_STTS_WPTR_REG 0x0A20U
#define IWL_FH_RSCSR_CHNL0_RBDCB_WPTR_REG 0x0A88U
#define IWL_FH_MEM_RCSR_CHNL0_CONFIG_REG 0x0A00U
#define IWL_FH_TCSR_CHNL_TX_CONFIG_REG(_q) (0x0D00U + ((_q) * 0x20U))
#define IWL_HBUS_TARG_WRPTR 0x060U

struct rb_iwl_fw_blob_info {
    u32 magic;
    u32 version;
    u32 build;
    u32 api;
    size_t size;
};

struct rb_iwl_cmd_hdr {
    u32 id;
    u32 len;
    u32 cookie;
    u32 flags;
};

struct rb_iwl_scan_cmd {
    struct rb_iwl_cmd_hdr hdr;
    u32 n_channels;
    u32 passive_dwell;
    u32 active_dwell;
    u32 ssid_len;
    u8 ssid[IEEE80211_MAX_SSID_LEN];
    u16 channels[RB_IWL_MAX_SCAN_CHANNELS];
};

struct rb_iwl_assoc_cmd {
    struct rb_iwl_cmd_hdr hdr;
    u32 ssid_len;
    u32 security_len;
    u32 key_len;
    u8 ssid[IEEE80211_MAX_SSID_LEN];
    char security[RB_IWL_MAX_SECURITY];
    char key[64];
};

struct rb_iwl_disconnect_cmd {
    struct rb_iwl_cmd_hdr hdr;
    u32 reason;
};

struct rb_iwl_fw_boot_cmd {
    struct rb_iwl_cmd_hdr hdr;
    u32 hw_rev;
    u32 fw_version;
    u32 fw_build;
    u32 dma_mask;
    u32 device_family;
};

struct rb_iwl_key {
    u32 cipher;
    u8 key[32];
    u8 key_len;
    u8 key_idx;
    int valid;
};

/* DMA ring descriptor */
struct iwl_tfd {
    u8 num_tbs;
    u8 padding[3];
    u64 tbs[RB_IWL_MAX_TBS];
    u32 status;
};

/* Receive buffer descriptor */
struct iwl_rx_buffer {
    dma_addr_t dma_addr;
    void *addr;
    u32 size;
};

/* TX queue */
struct iwl_tx_queue {
    int id;
    int write_ptr;
    int read_ptr;
    int n_window;
    int n_tfd;
    struct iwl_tfd *tfds;
    dma_addr_t tfds_dma;
    struct sk_buff **skbs;
    struct sk_buff_head overflow_q;
    spinlock_t lock;
    u8 active;
    u8 need_update;
};

/* RX queue */
struct iwl_rx_queue {
    int read_ptr;
    int write_ptr;
    struct iwl_rx_buffer *rx_bufs;
    dma_addr_t buf_dma;
    void *rb_stts;
    dma_addr_t rb_stts_dma;
    u32 n_rb;
    u32 n_rb_in_use;
    spinlock_t lock;
};

/* Command queue entry */
struct iwl_cmd_meta {
    u32 flags;
    void *source;
};

/* The PCIe transport */
struct iwl_trans_pcie {
    struct pci_dev *pci_dev;
    void *mmio_base;
    size_t mmio_size;

    /* TX/RX queues */
    struct iwl_tx_queue *tx_queues;
    int num_tx_queues;
    struct iwl_rx_queue rx_queue;

    /* Command queue (queue 0) */
    struct iwl_cmd_meta *cmd_meta;
    wait_queue_head_t wait_command_queue;
    int cmd_queue_write;
    int cmd_queue_read;

    /* Interrupt state */
    int irq;
    int num_irq_vectors;
    int msix_enabled;

    /* DMA pools */
    struct dma_pool *tfds_pool;
    struct dma_pool *rb_pool;

    /* Driver state */
    u32 hw_rev;
    u32 hw_rf_id;
    u32 svc_flags;
    u32 supported_dma_mask;
    u8 mac_addr[6];

    /* mac80211 integration */
    struct ieee80211_hw *hw;
    struct ieee80211_ops *ops;
    struct ieee80211_vif *vif;
    struct wiphy *wiphy;
    struct net_device *netdev;

    /* Synchronization */
    struct mutex mutex;
    spinlock_t reg_lock;
    int fw_running;
    int command_timeout;

    /* Device family info */
    int device_family;
    const char *fw_name;
    const char *pnvm_name;

    struct list_head link;
    struct wireless_dev wdev;
    struct ieee80211_sta station;
    struct ieee80211_bss_conf bss_conf;
    struct rb_iwl_fw_blob_info fw_info;
    struct rb_iwl_fw_blob_info pnvm_info;
    struct rb_iwl_key keys[4];
    char fw_name_storage[RB_IWL_MAX_FW_NAME];
    char pnvm_name_storage[RB_IWL_MAX_FW_NAME];
    char last_ssid[IEEE80211_MAX_SSID_LEN + 1];
    char last_security[RB_IWL_MAX_SECURITY];
    u8 current_bssid[6];
    u16 vendor_id;
    u16 device_id;
    u8 bus_number;
    u8 dev_number;
    u8 func_number;
    u32 last_interrupt_cause;
    u32 pending_interrupt_cause;
    u32 scan_generation;
    u32 tx_reclaim_count;
    u32 rx_processed_count;
    u32 scan_results_count;
    u32 last_cmd_id;
    u32 last_cmd_cookie;
    int last_cmd_status;
    int command_complete;
    int prepared;
    int transport_probed;
    int transport_inited;
    int nic_active;
    int mac80211_registered;
    int scan_active;
    int scheduled_scan_active;
    int connected;
    int connecting;
    int irq_tested;
    int dma_tested;
};

static DEFINE_MUTEX(rb_iwlwifi_transport_lock);
static LIST_HEAD(rb_iwlwifi_transports);
static atomic_t rb_iwlwifi_cmd_cookie = { .counter = 0 };
static atomic_t rb_iwlwifi_scan_cookie = { .counter = 0 };

static int iwl_pcie_transport_init(struct iwl_trans_pcie *trans);
static int iwl_pcie_tx_alloc(struct iwl_trans_pcie *trans);
static int iwl_pcie_rx_alloc(struct iwl_trans_pcie *trans);
static int iwl_pcie_txq_init(struct iwl_trans_pcie *trans, int queue_id, int slots_num, u32 cmd_queue);
static int iwl_pcie_rxq_init(struct iwl_trans_pcie *trans);
static void iwl_pcie_transport_free(struct iwl_trans_pcie *trans);
static int iwl_pcie_tx_skb(struct iwl_trans_pcie *trans, int queue_id, struct sk_buff *skb);
static void iwl_pcie_txq_reclaim(struct iwl_trans_pcie *trans, int queue_id, int ssn);
static int iwl_pcie_txq_check_stuck(struct iwl_trans_pcie *trans, int queue_id);
static void iwl_pcie_rxq_alloc_rbs(struct iwl_trans_pcie *trans);
static void iwl_pcie_rx_handle(struct iwl_trans_pcie *trans);
static void iwl_pcie_rxq_restock(struct iwl_trans_pcie *trans);
static int iwl_pcie_send_cmd(struct iwl_trans_pcie *trans, void *cmd, int len);
static void iwl_pcie_cmd_response(struct iwl_trans_pcie *trans);
static u32 iwl_pcie_isr(int irq, void *dev_id);
static void iwl_pcie_tasklet(unsigned long data);
static void rb_iwlwifi_release_irqs(struct iwl_trans_pcie *trans);
static void iwl_pcie_tasklet(unsigned long data);
static void iwl_ops_tx(struct ieee80211_hw *hw, struct sk_buff *skb);
static int iwl_ops_start(struct ieee80211_hw *hw);
static void iwl_ops_stop(struct ieee80211_hw *hw);
static int iwl_ops_add_interface(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
static void iwl_ops_remove_interface(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
static int iwl_ops_config(struct ieee80211_hw *hw, u32 changed);
static void iwl_ops_bss_info_changed(struct ieee80211_hw *hw, struct ieee80211_vif *vif,
                                     struct ieee80211_bss_conf *info, u32 changed);
static int iwl_ops_sta_state(struct ieee80211_hw *hw, struct ieee80211_vif *vif,
                             struct ieee80211_sta *sta, enum ieee80211_sta_state old_state,
                             enum ieee80211_sta_state new_state);
static int iwl_ops_set_key(struct ieee80211_hw *hw, enum set_key_cmd cmd,
                           struct ieee80211_vif *vif, struct ieee80211_sta *sta,
                           struct key_params *key);
static void iwl_ops_sw_scan_start(struct ieee80211_hw *hw, struct ieee80211_vif *vif, const u8 *mac_addr);
static void iwl_ops_sw_scan_complete(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
static int iwl_ops_sched_scan_start(struct ieee80211_hw *hw, struct ieee80211_vif *vif, void *req);
static void iwl_ops_sched_scan_stop(struct ieee80211_hw *hw, struct ieee80211_vif *vif);
static int iwl_pci_probe(struct pci_dev *pdev, const struct pci_device_id *ent);
static void iwl_pci_remove(struct pci_dev *pdev);

static struct ieee80211_ops iwl_mac80211_ops = {
    .tx = iwl_ops_tx,
    .start = iwl_ops_start,
    .stop = iwl_ops_stop,
    .add_interface = iwl_ops_add_interface,
    .remove_interface = iwl_ops_remove_interface,
    .config = iwl_ops_config,
    .bss_info_changed = iwl_ops_bss_info_changed,
    .sta_state = iwl_ops_sta_state,
    .set_key = iwl_ops_set_key,
    .sw_scan_start = iwl_ops_sw_scan_start,
    .sw_scan_complete = iwl_ops_sw_scan_complete,
    .sched_scan_start = iwl_ops_sched_scan_start,
    .sched_scan_stop = iwl_ops_sched_scan_stop,
};

static const struct pci_device_id iwl_hw_card_ids[] = {
    { PCI_DEVICE(0x8086, 0x7740) },
    { PCI_DEVICE(0x8086, 0x2725) },
    { PCI_DEVICE(0x8086, 0x7af0) },
    { PCI_DEVICE(0x8086, 0x34f0) },
    { PCI_DEVICE(0x8086, 0x9df0) },
    { PCI_DEVICE(0x8086, 0x2526) },
    { PCI_DEVICE(0x8086, 0x24fd) },
    { 0, }
};

static struct pci_driver iwl_pci_driver = {
    .name = "iwlwifi",
    .id_table = iwl_hw_card_ids,
    .probe = iwl_pci_probe,
    .remove = iwl_pci_remove,
};

static void rb_iwlwifi_default_mac(struct iwl_trans_pcie *trans)
{
    trans->mac_addr[0] = 0x02;
    trans->mac_addr[1] = (u8)(trans->bus_number & 0xFFU);
    trans->mac_addr[2] = (u8)(trans->dev_number & 0xFFU);
    trans->mac_addr[3] = (u8)(trans->func_number & 0xFFU);
    trans->mac_addr[4] = (u8)(trans->device_id & 0xFFU);
    trans->mac_addr[5] = (u8)((trans->device_id >> 8) & 0xFFU);
}

static void rb_iwlwifi_format_out(char *out, unsigned long out_len, const char *fmt, ...)
{
    va_list ap;

    if (!out || out_len == 0)
        return;

    va_start(ap, fmt);
    vsnprintf(out, out_len, fmt, ap);
    va_end(ap);
}

static void rb_iwlwifi_copy_name(char *dst, size_t dst_len, const char *src)
{
    size_t len;

    if (!dst || dst_len == 0)
        return;

    if (!src) {
        dst[0] = '\0';
        return;
    }

    len = strlen(src);
    if (len >= dst_len)
        len = dst_len - 1;
    memcpy(dst, src, len);
    dst[len] = '\0';
}

static inline void rb_iwlwifi_cpu_relax(void)
{
    __asm__ volatile("" ::: "memory");
}

static void rb_iwlwifi_update_fw_info(struct rb_iwl_fw_blob_info *info,
                                      const struct firmware *fw_blob)
{
    const u8 *data;
    u32 magic;

    if (!info || !fw_blob || !fw_blob->data || fw_blob->size < 20)
        return;

    data = fw_blob->data;
    magic = (u32)data[0] | ((u32)data[1] << 8) |
            ((u32)data[2] << 16) | ((u32)data[3] << 24);
    if (magic != 0x0A4F5749U)
        return;

    info->magic = magic;
    info->version = (u32)data[4] | ((u32)data[5] << 8) |
                    ((u32)data[6] << 16) | ((u32)data[7] << 24);
    info->build = (u32)data[8] | ((u32)data[9] << 8) |
                  ((u32)data[10] << 16) | ((u32)data[11] << 24);
    info->api = (info->version >> 8) & 0xFFU;
    info->size = fw_blob->size;
}

static u16 rb_iwlwifi_current_freq(const struct iwl_trans_pcie *trans)
{
    if (!trans)
        return 0;
    if (trans->bss_conf.chandef.center_freq != 0)
        return (u16)trans->bss_conf.chandef.center_freq;
    if (trans->bss_conf.chandef.channel)
        return ((struct ieee80211_channel *)trans->bss_conf.chandef.channel)->center_freq;
    return 2412;
}

static u32 rb_iwlwifi_current_band(const struct iwl_trans_pcie *trans)
{
    if (!trans)
        return NL80211_BAND_2GHZ;
    if (trans->bss_conf.chandef.channel)
        return ((struct ieee80211_channel *)trans->bss_conf.chandef.channel)->band;
    if (trans->bss_conf.chandef.band != 0)
        return (u32)trans->bss_conf.chandef.band;
    return NL80211_BAND_2GHZ;
}

static const char *rb_iwlwifi_family_name(int family)
{
    switch (family) {
    case RB_IWL_DEVICE_FAMILY_7000:
        return "7000";
    case RB_IWL_DEVICE_FAMILY_8000:
        return "8000";
    case RB_IWL_DEVICE_FAMILY_9000:
        return "9000";
    case RB_IWL_DEVICE_FAMILY_AX210:
        return "AX210";
    case RB_IWL_DEVICE_FAMILY_BZ:
        return "BZ";
    default:
        return "unknown";
    }
}

static int rb_iwlwifi_family_from_device(struct pci_dev *dev, int bz_family_hint)
{
    if (bz_family_hint)
        return RB_IWL_DEVICE_FAMILY_BZ;

    switch (dev->device_id) {
    case 0x7740:
        return RB_IWL_DEVICE_FAMILY_BZ;
    case 0x2725:
        return RB_IWL_DEVICE_FAMILY_AX210;
    case 0x7af0:
        return RB_IWL_DEVICE_FAMILY_AX210;
    case 0x34f0:
    case 0x9df0:
    case 0x2526:
        return RB_IWL_DEVICE_FAMILY_9000;
    case 0x24fd:
        return RB_IWL_DEVICE_FAMILY_8000;
    default:
        return RB_IWL_DEVICE_FAMILY_7000;
    }
}

static void rb_iwlwifi_default_fw_names(struct pci_dev *dev, int family,
                                        char *ucode, size_t ucode_len,
                                        char *pnvm, size_t pnvm_len)
{
    const char *ucode_name = "iwlwifi-unknown.ucode";
    const char *pnvm_name = "";

    switch (dev->device_id) {
    case 0x7740:
        ucode_name = "iwlwifi-bz-b0-gf-a0-92.ucode";
        pnvm_name = "iwlwifi-bz-b0-gf-a0.pnvm";
        break;
    case 0x2725:
        ucode_name = "iwlwifi-ty-a0-gf-a0-59.ucode";
        pnvm_name = "iwlwifi-ty-a0-gf-a0.pnvm";
        break;
    case 0x7af0:
        ucode_name = "iwlwifi-so-a0-gf-a0-64.ucode";
        pnvm_name = "iwlwifi-so-a0-gf-a0.pnvm";
        break;
    case 0x34f0:
        ucode_name = "iwlwifi-9000-pu-b0-jf-b0-46.ucode";
        break;
    case 0x9df0:
        ucode_name = "iwlwifi-9260-th-b0-jf-b0-46.ucode";
        break;
    case 0x2526:
        ucode_name = "iwlwifi-9260-th-b0-jf-b0-46.ucode";
        break;
    case 0x24fd:
        ucode_name = "iwlwifi-8265-36.ucode";
        break;
    default:
        if (family == RB_IWL_DEVICE_FAMILY_BZ) {
            ucode_name = "iwlwifi-bz-b0-gf-a0-92.ucode";
            pnvm_name = "iwlwifi-bz-b0-gf-a0.pnvm";
        }
        break;
    }

    rb_iwlwifi_copy_name(ucode, ucode_len, ucode_name);
    rb_iwlwifi_copy_name(pnvm, pnvm_len, pnvm_name);
}

static u64 rb_iwlwifi_pack_tb(dma_addr_t addr, u32 len)
{
    return ((u64)(len & 0xFFFFU) << 48) | (addr & 0x0000FFFFFFFFFFFFULL);
}

static dma_addr_t rb_iwlwifi_unpack_tb_addr(u64 tb)
{
    return tb & 0x0000FFFFFFFFFFFFULL;
}

static u32 rb_iwlwifi_unpack_tb_len(u64 tb)
{
    return (u32)((tb >> 48) & 0xFFFFU);
}

static struct iwl_trans_pcie *rb_iwlwifi_find_transport(struct pci_dev *dev)
{
    struct list_head *pos;

    list_for_each(pos, &rb_iwlwifi_transports) {
        struct iwl_trans_pcie *trans = list_entry(pos, struct iwl_trans_pcie, link);
        if (trans->vendor_id == dev->vendor &&
            trans->device_id == dev->device_id &&
            trans->bus_number == dev->bus_number &&
            trans->dev_number == dev->dev_number &&
            trans->func_number == dev->func_number)
            return trans;
    }

    return NULL;
}

static void rb_iwlwifi_remove_transport(struct iwl_trans_pcie *trans)
{
    if (!trans)
        return;

    list_del(&trans->link);
    iwl_pcie_transport_free(trans);
    kfree(trans);
}

static struct iwl_trans_pcie *rb_iwlwifi_alloc_transport(struct pci_dev *dev)
{
    struct iwl_trans_pcie *trans = kzalloc(sizeof(*trans), GFP_KERNEL);
    if (!trans)
        return NULL;

    INIT_LIST_HEAD(&trans->link);
    mutex_init(&trans->mutex);
    spin_lock_init(&trans->reg_lock);
    spin_lock_init(&trans->rx_queue.lock);
    init_waitqueue_head(&trans->wait_command_queue);
    trans->pci_dev = dev;
    trans->vendor_id = dev->vendor;
    trans->device_id = dev->device_id;
    trans->bus_number = dev->bus_number;
    trans->dev_number = dev->dev_number;
    trans->func_number = dev->func_number;
    trans->irq = -1;
    trans->command_timeout = RB_IWL_CMD_TIMEOUT;
    trans->ops = &iwl_mac80211_ops;
    trans->bss_conf.chandef.center_freq = 2412;
    trans->bss_conf.chandef.band = NL80211_BAND_2GHZ;
    rb_iwlwifi_default_mac(trans);
    list_add_tail(&trans->link, &rb_iwlwifi_transports);
    return trans;
}

static const struct pci_device_id *rb_iwlwifi_lookup_id(struct pci_dev *dev)
{
    const struct pci_device_id *id;

    for (id = iwl_hw_card_ids; id->vendor != 0 || id->device != 0; ++id) {
        if (id->vendor == dev->vendor && id->device == dev->device_id)
            return id;
    }

    return NULL;
}

static int rb_iwlwifi_parse_fw_blob(const struct firmware *fw, struct rb_iwl_fw_blob_info *info)
{
    const u8 *data;

    if (!fw || !info || !fw->data || fw->size < 12)
        return -EINVAL;

    data = fw->data;
    memset(info, 0, sizeof(*info));
    memcpy(&info->magic, data, sizeof(u32));
    memcpy(&info->version, data + 4, sizeof(u32));
    memcpy(&info->build, data + 8, sizeof(u32));
    info->api = (info->version >> 8) & 0xFFU;
    info->size = fw->size;

    /* Intel firmware TLV magic: "IWO\x0a" — exact match required */
    if (info->magic != 0x0A4F5749U)
        return -EINVAL;
    if (info->version == 0)
        return -EINVAL;

    return 0;
}

static u32 iwl_trans_read32(struct iwl_trans_pcie *trans, u32 reg)
{
    if (!trans || !trans->mmio_base || reg + sizeof(u32) > trans->mmio_size)
        return 0;
    return readl((u8 *)trans->mmio_base + reg);
}

static void iwl_trans_write32(struct iwl_trans_pcie *trans, u32 reg, u32 value)
{
    if (!trans || !trans->mmio_base || reg + sizeof(u32) > trans->mmio_size)
        return;
    writel(value, (u8 *)trans->mmio_base + reg);
}

static int rb_iwlwifi_map_bar(struct iwl_trans_pcie *trans, unsigned int bar)
{
    size_t len;

    if (trans->mmio_base)
        return 0;

    len = (size_t)pci_resource_len(trans->pci_dev, bar);
    if (!len)
        return -ENODEV;

    trans->mmio_base = pci_iomap(trans->pci_dev, bar, len);
    if (!trans->mmio_base)
        return -EIO;

    trans->mmio_size = len;
    trans->transport_probed = 1;
    trans->svc_flags |= RB_IWL_SVC_PROBED;
    return 0;
}

static void rb_iwlwifi_unmap_bar(struct iwl_trans_pcie *trans)
{
    if (!trans || !trans->mmio_base)
        return;

    pci_iounmap(trans->pci_dev, trans->mmio_base, trans->mmio_size);
    trans->mmio_base = NULL;
    trans->mmio_size = 0;
}

static int rb_iwlwifi_request_irqs(struct iwl_trans_pcie *trans)
{
    int rc;

    if (trans->num_irq_vectors > 0)
        return 0;

    rc = pci_alloc_irq_vectors(trans->pci_dev, 1, 2,
                               PCI_IRQ_MSIX | PCI_IRQ_MSI | PCI_IRQ_LEGACY | PCI_IRQ_NOLEGACY);
    if (rc > 0) {
        trans->num_irq_vectors = rc;
        trans->msix_enabled = rc > 1 ? 1 : 0;
        trans->irq = pci_irq_vector(trans->pci_dev, 0);
    } else {
        rc = pci_enable_msi(trans->pci_dev);
        if (rc == 0) {
            trans->num_irq_vectors = 1;
            trans->msix_enabled = 0;
            trans->irq = trans->pci_dev->irq ? (int)trans->pci_dev->irq : 0;
        }
    }

    if (trans->irq < 0)
        trans->irq = trans->pci_dev->irq ? (int)trans->pci_dev->irq : 0;

    if (trans->irq <= 0)
        return -ENODEV;

    if (request_irq(trans->irq, iwl_pcie_isr, 0, iwl_pci_driver.name, trans) != 0) {
        if (trans->num_irq_vectors > 0)
            pci_free_irq_vectors(trans->pci_dev);
        else
            pci_disable_msi(trans->pci_dev);
        trans->irq = -1;
        trans->num_irq_vectors = 0;
        trans->msix_enabled = 0;
        return -ENODEV;
    }

    trans->svc_flags |= RB_IWL_SVC_IRQ_READY;
    return 0;
}

static void rb_iwlwifi_release_irqs(struct iwl_trans_pcie *trans)
{
    if (!trans)
        return;

    if (trans->irq > 0)
        free_irq(trans->irq, trans);

    if (trans->num_irq_vectors > 0)
        pci_free_irq_vectors(trans->pci_dev);
    else if (trans->irq > 0)
        pci_disable_msi(trans->pci_dev);

    trans->irq = -1;
    trans->num_irq_vectors = 0;
    trans->msix_enabled = 0;
}

static int rb_iwlwifi_fw_boot(struct iwl_trans_pcie *trans)
{
    struct rb_iwl_fw_boot_cmd cmd;
    int rc;

    if (!trans->prepared)
        return -EINVAL;

    memset(&cmd, 0, sizeof(cmd));
    cmd.hdr.id = RB_IWL_CMD_FIRMWARE_BOOT;
    cmd.hdr.len = sizeof(cmd);
    cmd.hdr.cookie = (u32)atomic_add_return(1, &rb_iwlwifi_cmd_cookie);
    cmd.hw_rev = trans->hw_rev;
    cmd.fw_version = trans->fw_info.version;
    cmd.fw_build = trans->fw_info.build;
    cmd.dma_mask = trans->supported_dma_mask;
    cmd.device_family = (u32)trans->device_family;

    rc = iwl_pcie_send_cmd(trans, &cmd, sizeof(cmd));
    if (rc)
        return rc;

    return 0;
}

static void rb_iwlwifi_start_dma(struct iwl_trans_pcie *trans)
{
    if (!trans || !trans->transport_inited)
        return;

    iwl_trans_write32(trans, IWL_FH_RSCSR_CHNL0_RBDCB_BASE_REG, lower_32_bits(trans->rx_queue.buf_dma));
    iwl_trans_write32(trans, IWL_FH_RSCSR_CHNL0_STTS_WPTR_REG, lower_32_bits(trans->rx_queue.rb_stts_dma));
    iwl_trans_write32(trans, IWL_FH_MEM_RCSR_CHNL0_CONFIG_REG, trans->rx_queue.n_rb);
    iwl_trans_write32(trans, IWL_FH_RSCSR_CHNL0_RBDCB_WPTR_REG,
                      (u32)trans->rx_queue.write_ptr & 0xFFFU);
    iwl_trans_write32(trans, IWL_HBUS_TARG_WRPTR, 0);
    trans->svc_flags |= RB_IWL_SVC_DMA_READY;
}

static void rb_iwlwifi_stop_dma(struct iwl_trans_pcie *trans)
{
    if (!trans || !trans->mmio_base)
        return;

    iwl_trans_write32(trans, IWL_FH_MEM_RCSR_CHNL0_CONFIG_REG, 0);
    trans->svc_flags &= ~RB_IWL_SVC_DMA_READY;
}

static int rb_iwlwifi_register_mac80211_locked(struct iwl_trans_pcie *trans)
{
    if (trans->mac80211_registered)
        return 0;

    trans->hw = ieee80211_alloc_hw_nm(0, trans->ops, iwl_pci_driver.name);
    if (!trans->hw)
        return -ENOMEM;

    trans->hw->priv = trans;
    trans->hw->queues = (u16)max(1, trans->num_tx_queues - 1);
    trans->hw->extra_tx_headroom = 32;
    trans->wiphy = trans->hw->wiphy;
    if (trans->wiphy) {
        trans->wiphy->interface_modes = 1U << NL80211_IFTYPE_STATION;
        trans->wiphy->max_scan_ssids = 4;
        trans->wiphy->max_scan_ie_len = 512;
    }

    if (ieee80211_register_hw(trans->hw) != 0) {
        ieee80211_free_hw(trans->hw);
        trans->hw = NULL;
        return -EIO;
    }

    trans->netdev = alloc_netdev_mqs(0, "wlan%d", 0, NULL, 1, 1);
    if (!trans->netdev) {
        ieee80211_unregister_hw(trans->hw);
        ieee80211_free_hw(trans->hw);
        trans->hw = NULL;
        return -ENOMEM;
    }

    memcpy(trans->netdev->dev_addr, trans->mac_addr, sizeof(trans->mac_addr));
    trans->netdev->addr_len = sizeof(trans->mac_addr);
    trans->netdev->mtu = 1500;
    trans->wdev.wiphy = trans->wiphy;
    trans->wdev.netdev = trans->netdev;
    trans->wdev.iftype = NL80211_IFTYPE_STATION;
    trans->netdev->ieee80211_ptr = &trans->wdev;

    if (register_netdev(trans->netdev) != 0) {
        free_netdev(trans->netdev);
        trans->netdev = NULL;
        ieee80211_unregister_hw(trans->hw);
        ieee80211_free_hw(trans->hw);
        trans->hw = NULL;
        return -EIO;
    }

    trans->vif = kzalloc(sizeof(*trans->vif), GFP_KERNEL);
    if (!trans->vif) {
        unregister_netdev(trans->netdev);
        free_netdev(trans->netdev);
        trans->netdev = NULL;
        ieee80211_unregister_hw(trans->hw);
        ieee80211_free_hw(trans->hw);
        trans->hw = NULL;
        return -ENOMEM;
    }
    memcpy(trans->vif->addr, trans->mac_addr, sizeof(trans->mac_addr));
    trans->vif->type = NL80211_IFTYPE_STATION;

    memset(&trans->station, 0, sizeof(trans->station));
    trans->station.aid = 1;

    netif_carrier_off(trans->netdev);
    trans->mac80211_registered = 1;
    trans->svc_flags |= RB_IWL_SVC_MAC80211;
    return 0;
}

static void rb_iwlwifi_unregister_mac80211_locked(struct iwl_trans_pcie *trans)
{
    if (!trans)
        return;

    if (trans->netdev) {
        if (trans->netdev->registered)
            unregister_netdev(trans->netdev);
        free_netdev(trans->netdev);
        trans->netdev = NULL;
    }

    if (trans->vif) {
        kfree(trans->vif);
        trans->vif = NULL;
    }

    if (trans->hw) {
        if (trans->hw->registered)
            ieee80211_unregister_hw(trans->hw);
        ieee80211_free_hw(trans->hw);
        trans->hw = NULL;
    }

    memset(&trans->wdev, 0, sizeof(trans->wdev));
    trans->wiphy = NULL;
    trans->mac80211_registered = 0;
    trans->svc_flags &= ~RB_IWL_SVC_MAC80211;
}

static int rb_iwlwifi_do_prepare(struct iwl_trans_pcie *trans, const char *ucode, const char *pnvm)
{
    const struct firmware *fw = NULL;
    int rc;

    if (!ucode || !ucode[0])
        return -EINVAL;

    rc = request_firmware_direct(&fw, ucode, &trans->pci_dev->device_obj);
    if (rc)
        return rc;

    rc = rb_iwlwifi_parse_fw_blob(fw, &trans->fw_info);
    if (!rc)
        rb_iwlwifi_update_fw_info(&trans->fw_info, fw);
    release_firmware(fw);
    if (rc)
        return rc;

    rb_iwlwifi_copy_name(trans->fw_name_storage, sizeof(trans->fw_name_storage), ucode);
    trans->fw_name = trans->fw_name_storage;

    if (pnvm && pnvm[0]) {
        fw = NULL;
        rc = request_firmware_direct(&fw, pnvm, &trans->pci_dev->device_obj);
        if (rc)
            return rc;
        rc = rb_iwlwifi_parse_fw_blob(fw, &trans->pnvm_info);
        if (!rc)
            rb_iwlwifi_update_fw_info(&trans->pnvm_info, fw);
        release_firmware(fw);
        if (rc) {
            pr_warn("prepare: PNVM parse failed (rc=%d), proceeding without PNVM\n", rc);
            memset(&trans->pnvm_info, 0, sizeof(trans->pnvm_info));
            trans->pnvm_name_storage[0] = '\0';
            trans->pnvm_name = trans->pnvm_name_storage;
        } else {
            rb_iwlwifi_copy_name(trans->pnvm_name_storage, sizeof(trans->pnvm_name_storage), pnvm);
            trans->pnvm_name = trans->pnvm_name_storage;
        }
    } else {
        memset(&trans->pnvm_info, 0, sizeof(trans->pnvm_info));
        trans->pnvm_name_storage[0] = '\0';
        trans->pnvm_name = trans->pnvm_name_storage;
    }

    trans->prepared = 1;
    trans->svc_flags |= RB_IWL_SVC_PREPARED;
    return 0;
}

static int rb_iwlwifi_probe_transport(struct iwl_trans_pcie *trans, unsigned int bar, int bz_family)
{
    int rc;
    u32 access_req;
    u32 rev = 0;

    if (trans->transport_probed)
        return 0;

    rc = pci_enable_device(trans->pci_dev);
    if (rc)
        return rc;
    pci_set_master(trans->pci_dev);

    rc = rb_iwlwifi_map_bar(trans, bar);
    if (rc) {
        pci_disable_device(trans->pci_dev);
        return rc;
    }

    trans->device_family = rb_iwlwifi_family_from_device(trans->pci_dev, bz_family);
    access_req = trans->device_family == RB_IWL_DEVICE_FAMILY_BZ ?
                 IWL_CSR_GP_CNTRL_REG_FLAG_BZ_MAC_ACCESS_REQ :
                 IWL_CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ;
    trans->hw_rev = iwl_trans_read32(trans, IWL_CSR_HW_IF_CONFIG_REG);
    iwl_trans_write32(trans, IWL_CSR_GP_CNTRL, iwl_trans_read32(trans, IWL_CSR_GP_CNTRL) | access_req);
    pci_read_config_dword(trans->pci_dev, 0x08, &rev);
    trans->hw_rf_id = rev;
    trans->transport_probed = 1;
    trans->svc_flags |= RB_IWL_SVC_PROBED;
    return 0;
}

/* Initialize the PCIe transport */
static int iwl_pcie_transport_init(struct iwl_trans_pcie *trans)
{
    int rc;

    if (trans->transport_inited)
        return 0;

    trans->supported_dma_mask = trans->device_family >= RB_IWL_DEVICE_FAMILY_AX210 ? 64U : 36U;
    rc = dma_set_mask(&trans->pci_dev->device_obj, DMA_BIT_MASK(trans->supported_dma_mask));
    if (rc)
        return rc;
    rc = dma_set_coherent_mask(&trans->pci_dev->device_obj, DMA_BIT_MASK(trans->supported_dma_mask));
    if (rc)
        return rc;

    trans->tfds_pool = dma_pool_create("iwlwifi-tfds", &trans->pci_dev->device_obj,
                                       sizeof(struct iwl_tfd), 64, 0);
    if (!trans->tfds_pool)
        return -ENOMEM;
    trans->rb_pool = dma_pool_create("iwlwifi-rb", &trans->pci_dev->device_obj,
                                     RB_IWL_RX_BUF_SIZE, 64, 0);
    if (!trans->rb_pool) {
        dma_pool_destroy(trans->tfds_pool);
        trans->tfds_pool = NULL;
        return -ENOMEM;
    }

    rc = iwl_pcie_tx_alloc(trans);
    if (rc) {
        iwl_pcie_transport_free(trans);
        return rc;
    }
    rc = iwl_pcie_rx_alloc(trans);
    if (rc) {
        iwl_pcie_transport_free(trans);
        return rc;
    }
    rc = iwl_pcie_rxq_init(trans);
    if (rc) {
        iwl_pcie_transport_free(trans);
        return rc;
    }

    init_waitqueue_head(&trans->wait_command_queue);
    trans->cmd_queue_write = 0;
    trans->cmd_queue_read = 0;
    __atomic_store_n(&trans->command_complete, 1, __ATOMIC_SEQ_CST);
    trans->last_cmd_id = 0;
    trans->last_cmd_cookie = 0;
    trans->last_cmd_status = 0;
    trans->transport_inited = 1;
    trans->svc_flags |= RB_IWL_SVC_INIT | RB_IWL_SVC_DMA_READY;
    return 0;
}

static void iwl_pcie_txq_free(struct iwl_trans_pcie *trans, struct iwl_tx_queue *txq)
{
    int i;

    if (!txq)
        return;

    if (txq->skbs) {
        for (i = 0; i < txq->n_tfd; ++i) {
            if (txq->skbs[i]) {
                dma_unmap_single(&trans->pci_dev->device_obj,
                                 rb_iwlwifi_unpack_tb_addr(txq->tfds[i].tbs[0]),
                                 rb_iwlwifi_unpack_tb_len(txq->tfds[i].tbs[0]),
                                 DMA_TO_DEVICE);
                kfree_skb(txq->skbs[i]);
            }
        }
        kfree(txq->skbs);
        txq->skbs = NULL;
    }

    skb_queue_purge(&txq->overflow_q);

    if (txq->tfds) {
        dma_free_coherent(&trans->pci_dev->device_obj,
                          sizeof(struct iwl_tfd) * (size_t)txq->n_tfd,
                          txq->tfds, txq->tfds_dma);
        txq->tfds = NULL;
        txq->tfds_dma = 0;
    }
}

static void iwl_pcie_rxq_free(struct iwl_trans_pcie *trans)
{
    u32 i;

    if (!trans)
        return;

    if (trans->rx_queue.rx_bufs) {
        for (i = 0; i < trans->rx_queue.n_rb; ++i) {
            if (trans->rx_queue.rx_bufs[i].addr) {
                dma_pool_free(trans->rb_pool,
                              trans->rx_queue.rx_bufs[i].addr,
                              trans->rx_queue.rx_bufs[i].dma_addr);
            }
        }
        kfree(trans->rx_queue.rx_bufs);
        trans->rx_queue.rx_bufs = NULL;
    }

    if (trans->rx_queue.rb_stts) {
        dma_free_coherent(&trans->pci_dev->device_obj, 64,
                          trans->rx_queue.rb_stts, trans->rx_queue.rb_stts_dma);
        trans->rx_queue.rb_stts = NULL;
        trans->rx_queue.rb_stts_dma = 0;
    }
}

/* Free all transport resources */
static void iwl_pcie_transport_free(struct iwl_trans_pcie *trans)
{
    int i;

    if (!trans)
        return;

    rb_iwlwifi_stop_dma(trans);
    rb_iwlwifi_unregister_mac80211_locked(trans);

    if (trans->tx_queues) {
        for (i = 0; i < trans->num_tx_queues; ++i)
            iwl_pcie_txq_free(trans, &trans->tx_queues[i]);
        kfree(trans->tx_queues);
        trans->tx_queues = NULL;
    }

    if (trans->cmd_meta) {
        kfree(trans->cmd_meta);
        trans->cmd_meta = NULL;
    }

    iwl_pcie_rxq_free(trans);

    if (trans->tfds_pool) {
        dma_pool_destroy(trans->tfds_pool);
        trans->tfds_pool = NULL;
    }
    if (trans->rb_pool) {
        dma_pool_destroy(trans->rb_pool);
        trans->rb_pool = NULL;
    }

    rb_iwlwifi_release_irqs(trans);
    rb_iwlwifi_unmap_bar(trans);
    pci_disable_device(trans->pci_dev);

    trans->prepared = 0;
    trans->transport_probed = 0;
    trans->transport_inited = 0;
    trans->nic_active = 0;
    trans->fw_running = 0;
    trans->svc_flags = 0;
}

/* Allocate TX queues */
static int iwl_pcie_tx_alloc(struct iwl_trans_pcie *trans)
{
    int i;

    if (trans->tx_queues)
        return 0;

    switch (trans->device_family) {
    case RB_IWL_DEVICE_FAMILY_BZ:
    case RB_IWL_DEVICE_FAMILY_AX210:
        trans->num_tx_queues = 16;
        break;
    case RB_IWL_DEVICE_FAMILY_9000:
        trans->num_tx_queues = 12;
        break;
    default:
        trans->num_tx_queues = 8;
        break;
    }

    trans->tx_queues = kcalloc((size_t)trans->num_tx_queues, sizeof(*trans->tx_queues), GFP_KERNEL);
    if (!trans->tx_queues)
        return -ENOMEM;

    trans->cmd_meta = kcalloc((size_t)RB_IWL_CMD_SLOTS, sizeof(*trans->cmd_meta), GFP_KERNEL);
    if (!trans->cmd_meta)
        return -ENOMEM;

    for (i = 0; i < trans->num_tx_queues; ++i) {
        int rc = iwl_pcie_txq_init(trans, i, i == RB_IWL_CMD_QUEUE ? RB_IWL_CMD_SLOTS : RB_IWL_TXQ_SLOTS,
                                   i == RB_IWL_CMD_QUEUE ? 1U : 0U);
        if (rc)
            return rc;
    }

    return 0;
}

/* Initialize TX queue ring */
static int iwl_pcie_txq_init(struct iwl_trans_pcie *trans, int queue_id, int slots_num, u32 cmd_queue)
{
    struct iwl_tx_queue *txq = &trans->tx_queues[queue_id];

    txq->id = queue_id;
    txq->n_tfd = slots_num;
    txq->n_window = slots_num - 1;
    txq->write_ptr = 0;
    txq->read_ptr = 0;
    txq->active = 1;
    txq->need_update = cmd_queue ? 1 : 0;
    spin_lock_init(&txq->lock);
    skb_queue_head_init(&txq->overflow_q);

    txq->tfds = dma_alloc_coherent(&trans->pci_dev->device_obj,
                                   sizeof(struct iwl_tfd) * (size_t)slots_num,
                                   &txq->tfds_dma, GFP_KERNEL);
    if (!txq->tfds)
        return -ENOMEM;

    txq->skbs = kcalloc((size_t)slots_num, sizeof(*txq->skbs), GFP_KERNEL);
    if (!txq->skbs) {
        dma_free_coherent(&trans->pci_dev->device_obj,
                          sizeof(struct iwl_tfd) * (size_t)slots_num,
                          txq->tfds, txq->tfds_dma);
        txq->tfds = NULL;
        txq->tfds_dma = 0;
        return -ENOMEM;
    }

    memset(txq->tfds, 0, sizeof(struct iwl_tfd) * (size_t)slots_num);
    return 0;
}

/* Allocate RX queue */
static int iwl_pcie_rx_alloc(struct iwl_trans_pcie *trans)
{
    if (trans->rx_queue.rx_bufs)
        return 0;

    trans->rx_queue.n_rb = RB_IWL_RX_BUFS;
    trans->rx_queue.rx_bufs = kcalloc((size_t)trans->rx_queue.n_rb,
                                      sizeof(*trans->rx_queue.rx_bufs), GFP_KERNEL);
    if (!trans->rx_queue.rx_bufs)
        return -ENOMEM;

    trans->rx_queue.rb_stts = dma_alloc_coherent(&trans->pci_dev->device_obj,
                                                 64, &trans->rx_queue.rb_stts_dma, GFP_KERNEL);
    if (!trans->rx_queue.rb_stts) {
        kfree(trans->rx_queue.rx_bufs);
        trans->rx_queue.rx_bufs = NULL;
        return -ENOMEM;
    }

    trans->rx_queue.read_ptr = 0;
    trans->rx_queue.write_ptr = 0;
    trans->rx_queue.n_rb_in_use = 0;
    return 0;
}

/* Initialize RX queue ring */
static int iwl_pcie_rxq_init(struct iwl_trans_pcie *trans)
{
    if (!trans->rx_queue.rx_bufs)
        return -EINVAL;

    memset(trans->rx_queue.rb_stts, 0, 64);
    iwl_pcie_rxq_alloc_rbs(trans);
    if (trans->rx_queue.n_rb_in_use == 0)
        return -ENOMEM;
    return 0;
}

static int iwl_pcie_txq_space(const struct iwl_tx_queue *txq)
{
    if (txq->write_ptr >= txq->read_ptr)
        return txq->n_tfd - (txq->write_ptr - txq->read_ptr) - 1;
    return txq->read_ptr - txq->write_ptr - 1;
}

/* Map an skb to a TFD and submit to hardware */
static int iwl_pcie_tx_skb(struct iwl_trans_pcie *trans, int queue_id, struct sk_buff *skb)
{
    struct iwl_tx_queue *txq;
    unsigned long flags = 0;
    int index;
    dma_addr_t dma;

    if (!trans || !skb || queue_id < 0 || queue_id >= trans->num_tx_queues)
        return -EINVAL;

    txq = &trans->tx_queues[queue_id];
    spin_lock_irqsave(&txq->lock, &flags);
    if (!txq->active) {
        spin_unlock_irqrestore(&txq->lock, flags);
        return -ENODEV;
    }

    if (iwl_pcie_txq_space(txq) <= 0) {
        if (queue_id == RB_IWL_CMD_QUEUE) {
            spin_unlock_irqrestore(&txq->lock, flags);
            return -EAGAIN;
        }
        skb_queue_tail(&txq->overflow_q, skb);
        txq->need_update = 1;
        spin_unlock_irqrestore(&txq->lock, flags);
        return -EAGAIN;
    }

    index = txq->write_ptr;
    dma = dma_map_single(&trans->pci_dev->device_obj, skb->data, skb->len, DMA_TO_DEVICE);
    if (dma_mapping_error(&trans->pci_dev->device_obj, dma)) {
        spin_unlock_irqrestore(&txq->lock, flags);
        return -EIO;
    }

    memset(&txq->tfds[index], 0, sizeof(txq->tfds[index]));
    txq->tfds[index].num_tbs = 1;
    txq->tfds[index].tbs[0] = rb_iwlwifi_pack_tb(dma, (u32)min_t(unsigned int, skb->len, 0xFFFFU));
    txq->tfds[index].status = 1;
    txq->skbs[index] = skb;
    txq->write_ptr = (txq->write_ptr + 1) % txq->n_tfd;
    txq->need_update = 1;
    wmb();
    if (trans->mmio_base)
        iwl_trans_write32(trans, IWL_HBUS_TARG_WRPTR, ((u32)queue_id << 16) | (u32)txq->write_ptr);
    spin_unlock_irqrestore(&txq->lock, flags);
    return 0;
}

/* Reclaim completed TX frames */
static void iwl_pcie_txq_reclaim(struct iwl_trans_pcie *trans, int queue_id, int ssn)
{
    struct iwl_tx_queue *txq;
    unsigned long flags = 0;

    if (!trans || queue_id < 0 || queue_id >= trans->num_tx_queues)
        return;

    txq = &trans->tx_queues[queue_id];
    spin_lock_irqsave(&txq->lock, &flags);
    while (txq->read_ptr != ssn && txq->read_ptr != txq->write_ptr) {
        int index = txq->read_ptr;
        if (txq->skbs && txq->skbs[index]) {
            if (txq->tfds && txq->tfds[index].num_tbs > 0) {
                dma_unmap_single(&trans->pci_dev->device_obj,
                                 rb_iwlwifi_unpack_tb_addr(txq->tfds[index].tbs[0]),
                                 rb_iwlwifi_unpack_tb_len(txq->tfds[index].tbs[0]),
                                 DMA_TO_DEVICE);
            }
            kfree_skb(txq->skbs[index]);
            txq->skbs[index] = NULL;
        }
        memset(&txq->tfds[index], 0, sizeof(txq->tfds[index]));
        txq->read_ptr = (txq->read_ptr + 1) % txq->n_tfd;
        trans->tx_reclaim_count++;
    }

    while (iwl_pcie_txq_space(txq) > 0 && !skb_queue_empty(&txq->overflow_q)) {
        struct sk_buff *skb = skb_dequeue(&txq->overflow_q);
        if (!skb)
            break;
        spin_unlock_irqrestore(&txq->lock, flags);
        (void)iwl_pcie_tx_skb(trans, queue_id, skb);
        spin_lock_irqsave(&txq->lock, &flags);
    }

    txq->need_update = txq->write_ptr != txq->read_ptr || !skb_queue_empty(&txq->overflow_q);
    spin_unlock_irqrestore(&txq->lock, flags);
}

/* Check if TX queue is stuck */
static int iwl_pcie_txq_check_stuck(struct iwl_trans_pcie *trans, int queue_id)
{
    struct iwl_tx_queue *txq;

    if (!trans || queue_id < 0 || queue_id >= trans->num_tx_queues)
        return 0;

    txq = &trans->tx_queues[queue_id];
    return txq->active && txq->need_update && txq->write_ptr != txq->read_ptr;
}

/* Allocate and post receive buffers to hardware */
static void iwl_pcie_rxq_alloc_rbs(struct iwl_trans_pcie *trans)
{
    struct iwl_rx_queue *rxq = &trans->rx_queue;
    unsigned long flags = 0;

    spin_lock_irqsave(&rxq->lock, &flags);
    while (rxq->n_rb_in_use < rxq->n_rb) {
        struct iwl_rx_buffer *buf = &rxq->rx_bufs[rxq->write_ptr];
        if (!buf->addr) {
            buf->addr = dma_pool_alloc(trans->rb_pool, GFP_KERNEL, &buf->dma_addr);
            if (!buf->addr)
                break;
            buf->size = RB_IWL_RX_BUF_SIZE;
            memset(buf->addr, 0, buf->size);
        }
        rxq->write_ptr = (rxq->write_ptr + 1) % (int)rxq->n_rb;
        rxq->n_rb_in_use++;
        if (rxq->write_ptr == rxq->read_ptr)
            break;
    }
    wmb();
    spin_unlock_irqrestore(&rxq->lock, flags);
}

static void rb_iwlwifi_report_scan_result(struct iwl_trans_pcie *trans)
{
    if (!trans || !trans->wiphy || trans->scan_results_count == 0)
        return;

    if (trans->scheduled_scan_active)
        cfg80211_sched_scan_results(trans->wiphy, trans->scan_generation);
}

/* Handle RX interrupt — process received frames */
static void iwl_pcie_rx_handle(struct iwl_trans_pcie *trans)
{
    struct iwl_rx_queue *rxq = &trans->rx_queue;
    unsigned long flags = 0;

    spin_lock_irqsave(&rxq->lock, &flags);
    while (rxq->read_ptr != rxq->write_ptr && rxq->n_rb_in_use > 0) {
        struct iwl_rx_buffer *buf = &rxq->rx_bufs[rxq->read_ptr];
        if (buf->addr && buf->size >= 24) {
            struct ieee80211_rx_status rx_status;
            u8 *frame;
            u16 frame_control;
            u8 frame_type;
            u8 frame_subtype;
            u8 *addr2;
            u8 *addr3;

            dma_sync_single_for_cpu(&trans->pci_dev->device_obj, buf->dma_addr, buf->size, DMA_FROM_DEVICE);
            rmb();

            frame = (u8 *)buf->addr;
            frame_control = (u16)frame[0] | ((u16)frame[1] << 8);
            frame_type = (u8)((frame_control >> 2) & 0x3U);
            frame_subtype = (u8)((frame_control >> 4) & 0xFU);
            addr2 = &frame[10];
            addr3 = &frame[16];

            memset(&rx_status, 0, sizeof(rx_status));
            rx_status.freq = rb_iwlwifi_current_freq(trans);
            rx_status.band = rb_iwlwifi_current_band(trans);
            rx_status.signal = -42;
            rx_status.rate_idx = 0;

            if (trans->hw) {
                struct sk_buff *rx_skb = dev_alloc_skb(buf->size + sizeof(rx_status) + 2U);
                if (rx_skb) {
                    memcpy(rx_skb->head, &rx_status, sizeof(rx_status));
                    skb_reserve(rx_skb, (unsigned int)sizeof(rx_status) + 2U);
                    memcpy(skb_put(rx_skb, buf->size), buf->addr, buf->size);
                    ieee80211_rx_irqsafe(trans->hw, rx_skb);
                } else {
                    pr_warn("rx_handle: failed to allocate skb, skipping frame\n");
                    dma_sync_single_for_device(&trans->pci_dev->device_obj, buf->dma_addr,
                                               buf->size, DMA_FROM_DEVICE);
                    break;
                }
            }

            if (trans->scan_active && trans->wiphy &&
                frame_type == 0U && (frame_subtype == 8U || frame_subtype == 5U)) {
                u16 beacon_interval = 0;
                u16 capability = 0;
                const u8 *ies = frame;
                size_t ies_len = buf->size;
                struct cfg80211_bss *bss;

                if (buf->size >= 36) {
                    beacon_interval = (u16)frame[32] | ((u16)frame[33] << 8);
                    capability = (u16)frame[34] | ((u16)frame[35] << 8);
                    ies = &frame[36];
                    ies_len = buf->size - 36U;
                }

                bss = cfg80211_inform_bss(trans->wiphy, &trans->wdev,
                                          (u32)rx_status.freq,
                                          addr3, 0, capability, beacon_interval,
                                          ies, ies_len, rx_status.signal,
                                          GFP_KERNEL);
                if (bss) {
                    cfg80211_put_bss(bss);
                    trans->scan_results_count++;
                }
            }

            if (frame_type == 0U && frame_subtype == 1U && trans->connecting) {
                memcpy(trans->current_bssid, addr2, sizeof(trans->current_bssid));
                memcpy(trans->station.addr, addr2, sizeof(trans->station.addr));
                memcpy(trans->bss_conf.bssid, addr2, sizeof(trans->bss_conf.bssid));
            }

            dma_sync_single_for_device(&trans->pci_dev->device_obj, buf->dma_addr, buf->size, DMA_FROM_DEVICE);
        }
        rxq->read_ptr = (rxq->read_ptr + 1) % (int)rxq->n_rb;
        rxq->n_rb_in_use--;
        trans->rx_processed_count++;
    }
    spin_unlock_irqrestore(&rxq->lock, flags);

    if (trans->hw)
        ieee80211_rx_drain(trans->hw);

    if (trans->scan_active)
        iwl_pcie_rxq_restock(trans);
}

/* Replenish RX buffers */
static void iwl_pcie_rxq_restock(struct iwl_trans_pcie *trans)
{
    struct iwl_rx_queue *rxq;
    if (!trans)
        return;
    rxq = &trans->rx_queue;

    if (rxq->n_rb_in_use < rxq->n_rb / 2) {
        iwl_pcie_rxq_alloc_rbs(trans);
        if (trans->mmio_base) {
            iwl_trans_write32(trans, IWL_FH_RSCSR_CHNL0_RBDCB_WPTR_REG,
                              (u32)rxq->write_ptr & 0xFFFU);
        }
    }
}

/* Handle command response */
static void iwl_pcie_cmd_response(struct iwl_trans_pcie *trans)
{
    struct iwl_rx_queue *rxq = &trans->rx_queue;

    if (__atomic_load_n(&trans->last_cmd_id, __ATOMIC_SEQ_CST) == 0xFFFF) {
        pr_warn("cmd_response: discarding stale response for timed-out command");
        if (rxq->rx_bufs && rxq->read_ptr != rxq->write_ptr) {
            struct iwl_rx_buffer *buf = &rxq->rx_bufs[rxq->read_ptr];
            if (buf->addr && buf->dma_addr)
                dma_sync_single_for_device(&trans->pci_dev->device_obj, buf->dma_addr, buf->size, DMA_FROM_DEVICE);
            rxq->read_ptr = (rxq->read_ptr + 1) % (int)rxq->n_rb;
            if (rxq->n_rb_in_use > 0)
                rxq->n_rb_in_use--;
        }
        iwl_pcie_rxq_restock(trans);
        return;
    }

    if (rxq->read_ptr == rxq->write_ptr || !rxq->rx_bufs) {
        __atomic_store_n(&trans->last_cmd_status, -ENOENT, __ATOMIC_RELEASE);
        __atomic_store_n(&trans->command_complete, 1, __ATOMIC_SEQ_CST);
        wake_up(&trans->wait_command_queue);
        return;
    }

    if (rxq->rx_bufs[rxq->read_ptr].addr &&
        rxq->rx_bufs[rxq->read_ptr].size >= sizeof(struct rb_iwl_cmd_hdr)) {
        struct iwl_rx_buffer *buf = &rxq->rx_bufs[rxq->read_ptr];
        struct rb_iwl_cmd_hdr *resp;

        dma_sync_single_for_cpu(&trans->pci_dev->device_obj, buf->dma_addr, buf->size, DMA_FROM_DEVICE);
        resp = (struct rb_iwl_cmd_hdr *)buf->addr;
        if (__atomic_load_n(&trans->last_cmd_id, __ATOMIC_ACQUIRE) == 0xFFFF) {
            pr_warn("iwl_pcie_cmd_response: discarding stale response for timed-out command");
            dma_sync_single_for_device(&trans->pci_dev->device_obj, buf->dma_addr, buf->size, DMA_FROM_DEVICE);
            rxq->read_ptr = (rxq->read_ptr + 1) % (int)rxq->n_rb;
            if (rxq->n_rb_in_use > 0)
                rxq->n_rb_in_use--;
            iwl_pcie_rxq_restock(trans);
            return;
        } else if (resp->id == __atomic_load_n(&trans->last_cmd_id, __ATOMIC_ACQUIRE) &&
            resp->cookie == __atomic_load_n(&trans->last_cmd_cookie, __ATOMIC_ACQUIRE)) {
            __atomic_store_n(&trans->last_cmd_status, (int)resp->flags, __ATOMIC_RELEASE);
            __atomic_store_n(&trans->last_cmd_cookie, resp->cookie, __ATOMIC_RELEASE);
        } else {
            pr_warn("iwl_pcie_cmd_response: response id/cookie mismatch, discarding");
            dma_sync_single_for_device(&trans->pci_dev->device_obj, buf->dma_addr, buf->size, DMA_FROM_DEVICE);
            rxq->read_ptr = (rxq->read_ptr + 1) % (int)rxq->n_rb;
            if (rxq->n_rb_in_use > 0)
                rxq->n_rb_in_use--;
            iwl_pcie_rxq_restock(trans);
            return;
        }
        dma_sync_single_for_device(&trans->pci_dev->device_obj, buf->dma_addr, buf->size, DMA_FROM_DEVICE);
        rxq->read_ptr = (rxq->read_ptr + 1) % (int)rxq->n_rb;
        if (rxq->n_rb_in_use > 0)
            rxq->n_rb_in_use--;
    } else {
        __atomic_store_n(&trans->last_cmd_status, -EIO, __ATOMIC_RELEASE);
    }

    __atomic_store_n(&trans->command_complete, 1, __ATOMIC_SEQ_CST);
    if (trans->cmd_queue_read != trans->cmd_queue_write) {
        trans->cmd_queue_read = (trans->cmd_queue_read + 1) % RB_IWL_CMD_SLOTS;
        iwl_pcie_txq_reclaim(trans, RB_IWL_CMD_QUEUE, trans->cmd_queue_read);
    }
    iwl_pcie_rxq_restock(trans);
    wake_up(&trans->wait_command_queue);
}

/* Tasklet — deferred interrupt processing */
static void iwl_pcie_tasklet(unsigned long data)
{
    struct iwl_trans_pcie *trans = (struct iwl_trans_pcie *)data;
    u32 cause;
    int q;

    if (!trans)
        return;

    cause = trans->pending_interrupt_cause;
    trans->pending_interrupt_cause = 0;
    trans->last_interrupt_cause = cause;

    if (cause & (RB_IWL_INT_RX | RB_IWL_INT_SCAN)) {
        iwl_pcie_rx_handle(trans);
        iwl_pcie_rxq_restock(trans);
    }
    if (cause & RB_IWL_INT_TX) {
        for (q = 0; q < trans->num_tx_queues; ++q)
            iwl_pcie_txq_reclaim(trans, q, trans->tx_queues[q].write_ptr);
    }
    if (cause & RB_IWL_INT_CMD)
        iwl_pcie_cmd_response(trans);
    if (cause & RB_IWL_INT_ERROR) {
        trans->nic_active = 0;
        trans->fw_running = 0;
        if (trans->netdev)
            netif_carrier_off(trans->netdev);
    }

    trans->irq_tested = 1;
}

/* ISR — read interrupt cause, schedule processing */
static u32 iwl_pcie_isr(int irq, void *dev_id)
{
    struct iwl_trans_pcie *trans = (struct iwl_trans_pcie *)dev_id;
    u32 inta;
    u32 inta_mask;

    (void)irq;
    if (!trans || !trans->mmio_base)
        return 0;

    inta = iwl_trans_read32(trans, IWL_CSR_INT);
    inta_mask = iwl_trans_read32(trans, IWL_CSR_INT_MASK);
    inta &= inta_mask;
    if (!inta)
        return 0;

    iwl_trans_write32(trans, IWL_CSR_INT, inta);
    trans->pending_interrupt_cause = inta;
    iwl_pcie_tasklet((unsigned long)trans);
    return inta;
}

/* Send a firmware command and wait for response */
static int iwl_pcie_send_cmd(struct iwl_trans_pcie *trans, void *cmd, int len)
{
    struct rb_iwl_cmd_hdr *hdr = cmd;
    struct sk_buff *skb;
    int rc;
    u64 deadline;

    if (!trans || !cmd || len <= 0)
        return -EINVAL;
    if (!trans->transport_inited)
        return -EINVAL;
    if (__atomic_load_n(&trans->command_complete, __ATOMIC_SEQ_CST) == 0 &&
        __atomic_load_n(&trans->last_cmd_id, __ATOMIC_SEQ_CST) != 0) {
        pr_warn("iwl_pcie_send_cmd: command %d still pending, rejecting new cmd %d",
                (int)__atomic_load_n(&trans->last_cmd_id, __ATOMIC_SEQ_CST), ((struct rb_iwl_cmd_hdr *)cmd)->id);
        return -EBUSY;
    }

    skb = alloc_skb((unsigned int)len + 64U, GFP_KERNEL);
    if (!skb)
        return -ENOMEM;

    skb_reserve(skb, 32U);
    memcpy(skb_put(skb, (unsigned int)len), cmd, (size_t)len);
    trans->cmd_meta[trans->cmd_queue_write].flags = hdr->flags;
    trans->cmd_meta[trans->cmd_queue_write].source = cmd;

    __atomic_store_n(&trans->last_cmd_id, hdr->id, __ATOMIC_SEQ_CST);
    __atomic_store_n(&trans->last_cmd_cookie, hdr->cookie, __ATOMIC_SEQ_CST);
    __atomic_store_n(&trans->last_cmd_status, -1, __ATOMIC_SEQ_CST);
    __atomic_store_n(&trans->command_complete, 0, __ATOMIC_SEQ_CST);

    iwl_pcie_txq_reclaim(trans, RB_IWL_CMD_QUEUE, trans->cmd_queue_write);

    rc = iwl_pcie_tx_skb(trans, RB_IWL_CMD_QUEUE, skb);
    if (rc) {
        kfree_skb(skb);
        __atomic_store_n(&trans->command_complete, 1, __ATOMIC_SEQ_CST);
        __atomic_store_n(&trans->last_cmd_id, 0, __ATOMIC_SEQ_CST);
        return rc;
    }

    trans->cmd_queue_write = (trans->cmd_queue_write + 1) % RB_IWL_CMD_SLOTS;

    deadline = jiffies + msecs_to_jiffies((unsigned long)trans->command_timeout);
    wait_event_timeout(trans->wait_command_queue,
        __atomic_load_n(&trans->command_complete, __ATOMIC_SEQ_CST),
        deadline - jiffies);

    if (!__atomic_load_n(&trans->command_complete, __ATOMIC_SEQ_CST)) {
        __atomic_store_n(&trans->last_cmd_status, -ETIMEDOUT, __ATOMIC_SEQ_CST);
        pr_warn("iwl_pcie_send_cmd: command 0x%02x timed out after %lu ms",
                __atomic_load_n(&trans->last_cmd_id, __ATOMIC_SEQ_CST), (unsigned long)trans->command_timeout);
        __atomic_store_n(&trans->last_cmd_cookie, 0, __ATOMIC_SEQ_CST);
        __atomic_store_n(&trans->last_cmd_id, 0xFFFF, __ATOMIC_SEQ_CST);
        __atomic_store_n(&trans->command_complete, 1, __ATOMIC_SEQ_CST);
        return -ETIMEDOUT;
    }

    return __atomic_load_n(&trans->last_cmd_status, __ATOMIC_SEQ_CST);
}

static struct iwl_trans_pcie *iwl_hw_to_trans(struct ieee80211_hw *hw)
{
    return hw ? hw->priv : NULL;
}

static int rb_iwlwifi_choose_txq(struct iwl_trans_pcie *trans)
{
    int q;

    for (q = 1; q < trans->num_tx_queues; ++q) {
        if (trans->tx_queues[q].active)
            return q;
    }
    return -1;
}

static void iwl_ops_tx(struct ieee80211_hw *hw, struct sk_buff *skb)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    int txq;
    if (!trans || !skb)
        return;
    txq = rb_iwlwifi_choose_txq(trans);
    if (txq < 0) {
        pr_warn("iwl_ops_tx: no active data TX queue, dropping frame");
        kfree_skb(skb);
        return;
    }
    {
        int rc = iwl_pcie_tx_skb(trans, txq, skb);
        if (rc == -EAGAIN) {
            pr_debug("iwl_ops_tx: queue %d full, skb queued to overflow", txq);
        } else if (rc) {
            pr_warn("iwl_ops_tx: TX failed on queue %d (rc=%d)", txq, rc);
            kfree_skb(skb);
        }
    }
}

static int iwl_ops_start(struct ieee80211_hw *hw)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    if (!trans)
        return -ENODEV;
    trans->fw_running = trans->nic_active ? 1 : 0;
    return 0;
}

static void iwl_ops_stop(struct ieee80211_hw *hw)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    if (!trans)
        return;
    trans->nic_active = 0;
    trans->fw_running = 0;
    if (trans->netdev)
        netif_carrier_off(trans->netdev);
}

static int iwl_ops_add_interface(struct ieee80211_hw *hw, struct ieee80211_vif *vif)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    if (!trans || !vif)
        return -EINVAL;
    trans->vif = vif;
    memcpy(vif->addr, trans->mac_addr, sizeof(trans->mac_addr));
    vif->type = NL80211_IFTYPE_STATION;
    return 0;
}

static void iwl_ops_remove_interface(struct ieee80211_hw *hw, struct ieee80211_vif *vif)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    if (!trans)
        return;
    if (trans->vif == vif)
        trans->vif = NULL;
}

static int iwl_ops_config(struct ieee80211_hw *hw, u32 changed)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    if (!trans)
        return -ENODEV;
    (void)changed;
    return 0;
}

static void iwl_ops_bss_info_changed(struct ieee80211_hw *hw, struct ieee80211_vif *vif,
                                     struct ieee80211_bss_conf *info, u32 changed)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    (void)vif;
    if (!trans || !info)
        return;

    if (changed & BSS_CHANGED_ASSOC) {
        trans->bss_conf.assoc = info->assoc;
        trans->bss_conf.aid = info->aid;
        trans->connected = info->assoc ? 1 : 0;
        trans->wdev.connected = info->assoc;
        if (info->assoc)
            trans->svc_flags |= RB_IWL_SVC_CONNECTED;
        else
            trans->svc_flags &= ~RB_IWL_SVC_CONNECTED;
    }
    if (changed & BSS_CHANGED_BSSID) {
        memcpy(trans->current_bssid, info->bssid, sizeof(trans->current_bssid));
        memcpy(trans->station.addr, info->bssid, sizeof(trans->station.addr));
        memcpy(trans->bss_conf.bssid, info->bssid, sizeof(trans->bss_conf.bssid));
        memcpy(trans->wdev.last_bssid, info->bssid, sizeof(trans->wdev.last_bssid));
        trans->wdev.has_bssid = true;
    }
    if (changed & BSS_CHANGED_BEACON_INT)
        trans->bss_conf.beacon_int = info->beacon_int;
    if (changed & BSS_CHANGED_BASIC_RATES)
        trans->bss_conf.basic_rates = info->basic_rates;
    if (changed & BSS_CHANGED_BANDWIDTH)
        trans->bss_conf.bandwidth = info->bandwidth;

    trans->bss_conf.chandef.center_freq = info->chandef.center_freq;
    trans->bss_conf.chandef.band = info->chandef.band;
    trans->bss_conf.chandef.channel = info->chandef.channel;
}

static int iwl_ops_sta_state(struct ieee80211_hw *hw, struct ieee80211_vif *vif,
                             struct ieee80211_sta *sta, enum ieee80211_sta_state old_state,
                             enum ieee80211_sta_state new_state)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);

    (void)vif;
    if (!trans || !sta)
        return -EINVAL;
    if ((u32)new_state < IEEE80211_STA_NOTEXIST || (u32)new_state > IEEE80211_STA_AUTHORIZED)
        return -EINVAL;
    if ((u32)old_state > (u32)new_state && new_state != IEEE80211_STA_NOTEXIST)
        return -EINVAL;

    if (new_state == IEEE80211_STA_AUTHORIZED) {
        memcpy(trans->station.addr, sta->addr, sizeof(trans->station.addr));
        trans->station.aid = sta->aid;
        trans->connected = 1;
        if (trans->netdev) {
            struct station_parameters params;

            memset(&params, 0, sizeof(params));
            cfg80211_new_sta(trans->netdev, sta->addr, &params, GFP_KERNEL);
        }
    } else if (new_state == IEEE80211_STA_NOTEXIST) {
        memset(&trans->station, 0, sizeof(trans->station));
        trans->connected = 0;
    }

    return 0;
}

static int iwl_ops_set_key(struct ieee80211_hw *hw, enum set_key_cmd cmd,
                           struct ieee80211_vif *vif, struct ieee80211_sta *sta,
                           struct key_params *key)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    (void)vif;
    (void)sta;
    if (!trans || !key)
        return -EINVAL;
    if (key->key_idx >= 4)
        return -ENOSPC;

    if (cmd == SET_KEY) {
        trans->keys[key->key_idx].cipher = key->cipher;
        trans->keys[key->key_idx].key_len = (u8)min_t(u32, key->key_len, 32U);
        if (key->key)
            memcpy(trans->keys[key->key_idx].key,
                   key->key,
                   trans->keys[key->key_idx].key_len);
        trans->keys[key->key_idx].key_idx = key->key_idx;
        trans->keys[key->key_idx].valid = 1;
        rb_iwlwifi_copy_name(trans->last_security,
                             sizeof(trans->last_security),
                             key->cipher ? "wpa2-psk" : "open");
    } else {
        memset(&trans->keys[key->key_idx], 0, sizeof(trans->keys[key->key_idx]));
        trans->last_security[0] = '\0';
    }

    return 0;
}

static void iwl_ops_sw_scan_start(struct ieee80211_hw *hw, struct ieee80211_vif *vif, const u8 *mac_addr)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    (void)vif;
    (void)mac_addr;
    if (!trans)
        return;
    trans->scan_results_count = 0;
    trans->scan_active = 1;
    trans->svc_flags |= RB_IWL_SVC_SCAN_ACTIVE;
}

static void iwl_ops_sw_scan_complete(struct ieee80211_hw *hw, struct ieee80211_vif *vif)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    (void)vif;
    if (!trans)
        return;
    rb_iwlwifi_report_scan_result(trans);
    trans->scan_active = 0;
    trans->svc_flags &= ~RB_IWL_SVC_SCAN_ACTIVE;
}

static int iwl_ops_sched_scan_start(struct ieee80211_hw *hw, struct ieee80211_vif *vif, void *req)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    (void)vif;
    (void)req;
    if (!trans)
        return -ENODEV;
    trans->scheduled_scan_active = 1;
    return 0;
}

static void iwl_ops_sched_scan_stop(struct ieee80211_hw *hw, struct ieee80211_vif *vif)
{
    struct iwl_trans_pcie *trans = iwl_hw_to_trans(hw);
    (void)vif;
    if (!trans)
        return;
    trans->scheduled_scan_active = 0;
}

static int rb_iwlwifi_activate_locked(struct iwl_trans_pcie *trans)
{
    int rc;

    if (trans->nic_active)
        return 0;
    if (!trans->transport_inited)
        return -EINVAL;

    if (trans->device_family == RB_IWL_DEVICE_FAMILY_BZ) {
        u32 gp = iwl_trans_read32(trans, IWL_CSR_GP_CNTRL);
        iwl_trans_write32(trans, IWL_CSR_GP_CNTRL, gp | IWL_CSR_GP_CNTRL_REG_FLAG_SW_RESET_BZ |
                          IWL_CSR_GP_CNTRL_REG_FLAG_INIT_DONE);
    } else {
        u32 reset = iwl_trans_read32(trans, IWL_CSR_RESET);
        iwl_trans_write32(trans, IWL_CSR_RESET, reset | IWL_CSR_RESET_REG_FLAG_SW_RESET);
    }

    rc = rb_iwlwifi_request_irqs(trans);
    if (rc)
        return rc;

    rc = rb_iwlwifi_fw_boot(trans);
    if (rc) {
        rb_iwlwifi_release_irqs(trans);
        return rc;
    }

    rb_iwlwifi_start_dma(trans);
    iwl_trans_write32(trans, IWL_CSR_INT_MASK, RB_IWL_INT_RX | RB_IWL_INT_TX | RB_IWL_INT_CMD | RB_IWL_INT_SCAN);
    trans->fw_running = 1;
    trans->nic_active = 1;
    trans->svc_flags |= RB_IWL_SVC_ACTIVE;
    return 0;
}

static int rb_iwlwifi_full_init_locked(struct iwl_trans_pcie *trans, unsigned int bar,
                                       int bz_family, const char *ucode, const char *pnvm)
{
    int rc;
    char auto_ucode[RB_IWL_MAX_FW_NAME];
    char auto_pnvm[RB_IWL_MAX_FW_NAME];

    if (!ucode || !ucode[0]) {
        rb_iwlwifi_default_fw_names(trans->pci_dev, trans->device_family, auto_ucode, sizeof(auto_ucode),
                                    auto_pnvm, sizeof(auto_pnvm));
        ucode = auto_ucode;
        pnvm = auto_pnvm[0] ? auto_pnvm : NULL;
    }

    if (!trans->prepared) {
        rc = rb_iwlwifi_do_prepare(trans, ucode, pnvm);
        if (rc)
            return rc;
    }

    rc = rb_iwlwifi_probe_transport(trans, bar, bz_family);
    if (rc)
        return rc;

    rc = iwl_pcie_transport_init(trans);
    if (rc)
        return rc;

    rc = rb_iwlwifi_activate_locked(trans);
    if (rc) {
        trans->nic_active = 0;
        trans->fw_running = 0;
        trans->svc_flags &= ~RB_IWL_SVC_ACTIVE;
        iwl_pcie_transport_free(trans);
        return rc;
    }

    rc = rb_iwlwifi_register_mac80211_locked(trans);
    if (rc) {
        rb_iwlwifi_stop_dma(trans);
        rb_iwlwifi_release_irqs(trans);
        trans->nic_active = 0;
        trans->fw_running = 0;
        iwl_pcie_transport_free(trans);
        return rc;
    }

    return 0;
}

/* PCI probe — full device initialization */
static int iwl_pci_probe(struct pci_dev *pdev, const struct pci_device_id *ent)
{
    struct iwl_trans_pcie *trans = rb_iwlwifi_find_transport(pdev);
    int rc;

    (void)ent;
    if (trans)
        return 0;

    trans = rb_iwlwifi_alloc_transport(pdev);
    if (!trans)
        return -ENOMEM;

    rc = pci_enable_device(pdev);
    if (rc) {
        rb_iwlwifi_remove_transport(trans);
        return rc;
    }
    pci_set_master(pdev);
    trans->device_family = rb_iwlwifi_family_from_device(pdev, 0);
    pdev->driver_data = trans;
    return 0;
}

/* PCI remove — full device cleanup */
static void iwl_pci_remove(struct pci_dev *pdev)
{
    struct iwl_trans_pcie *trans = rb_iwlwifi_find_transport(pdev);
    if (!trans)
        return;
    pdev->driver_data = NULL;
    rb_iwlwifi_remove_transport(trans);
}

static int rb_iwlwifi_require_transport(struct pci_dev *dev, struct iwl_trans_pcie **out_trans)
{
    struct iwl_trans_pcie *trans;
    const struct pci_device_id *id;
    int rc;

    if (!dev || !out_trans)
        return -EINVAL;

    trans = rb_iwlwifi_find_transport(dev);
    if (!trans) {
        id = rb_iwlwifi_lookup_id(dev);
        if (!id)
            return -ENODEV;
        rc = iwl_pci_probe(dev, id);
        if (rc)
            return rc;
        trans = rb_iwlwifi_find_transport(dev);
    }

    if (!trans)
        return -ENODEV;

    *out_trans = trans;
    return 0;
}

static void rb_iwlwifi_status_line(struct iwl_trans_pcie *trans, char *out, unsigned long out_len)
{
    rb_iwlwifi_format_out(
        out, out_len,
        "linux_kpi_status=ok family=%s prepared=%d probed=%d init=%d active=%d fw_running=%d mac80211=%d irq=%d vectors=%d msix=%d tx_queues=%d rx_in_use=%u scan_results=%u connected=%d ssid=%s",
        rb_iwlwifi_family_name(trans->device_family),
        trans->prepared,
        trans->transport_probed,
        trans->transport_inited,
        trans->nic_active,
        trans->fw_running,
        trans->mac80211_registered,
        trans->irq,
        trans->num_irq_vectors,
        trans->msix_enabled,
        trans->num_tx_queues,
        trans->rx_queue.n_rb_in_use,
        trans->scan_results_count,
        trans->connected,
        trans->last_ssid[0] ? trans->last_ssid : "none");
}

int rb_iwlwifi_linux_prepare(struct pci_dev *dev, const char *ucode, const char *pnvm,
                             char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    int rc;

    if (!dev || !ucode || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_do_prepare(trans, ucode, pnvm);
    if (!rc) {
        rb_iwlwifi_format_out(out, out_len,
                              "linux_kpi_prepare=ok fw=%s size=%zu magic=0x%08x version=%u pnvm=%s",
                              trans->fw_name ? trans->fw_name : "none",
                              trans->fw_info.size,
                              trans->fw_info.magic,
                              trans->fw_info.version,
                              trans->pnvm_name && trans->pnvm_name[0] ? trans->pnvm_name : "none");
    }
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_linux_transport_probe(struct pci_dev *dev, unsigned int bar, char *out,
                                     unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    int rc;

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_probe_transport(trans, bar, 0);
    if (!rc) {
        rb_iwlwifi_format_out(out, out_len,
                              "linux_kpi_transport_probe=ok driver=%s bar=%u mmio_size=0x%lx hw_rev=0x%08x rf_id=0x%08x family=%s",
                              iwl_pci_driver.name,
                              bar,
                              (unsigned long)trans->mmio_size,
                              trans->hw_rev,
                              trans->hw_rf_id,
                              rb_iwlwifi_family_name(trans->device_family));
    }
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_linux_init_transport(struct pci_dev *dev, unsigned int bar, int bz_family,
                                    char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    int rc;

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_probe_transport(trans, bar, bz_family);
    if (!rc)
        rc = iwl_pcie_transport_init(trans);
    if (!rc) {
        rb_iwlwifi_format_out(out, out_len,
                              "linux_kpi_transport_init=ok tx_queues=%d cmd_slots=%d rx_bufs=%u dma_mask=%u stuck_cmdq=%d",
                              trans->num_tx_queues,
                              RB_IWL_CMD_SLOTS,
                              trans->rx_queue.n_rb,
                              trans->supported_dma_mask,
                              iwl_pcie_txq_check_stuck(trans, RB_IWL_CMD_QUEUE));
    }
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_linux_activate_nic(struct pci_dev *dev, unsigned int bar, int bz_family,
                                  char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    int rc;
    char auto_ucode[RB_IWL_MAX_FW_NAME];
    char auto_pnvm[RB_IWL_MAX_FW_NAME];

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc && !trans->prepared) {
        rb_iwlwifi_default_fw_names(dev, rb_iwlwifi_family_from_device(dev, bz_family),
                                    auto_ucode, sizeof(auto_ucode), auto_pnvm, sizeof(auto_pnvm));
        rc = rb_iwlwifi_do_prepare(trans, auto_ucode, auto_pnvm[0] ? auto_pnvm : NULL);
    }
    if (!rc)
        rc = rb_iwlwifi_probe_transport(trans, bar, bz_family);
    if (!rc)
        rc = iwl_pcie_transport_init(trans);
    if (!rc) {
        rc = rb_iwlwifi_activate_locked(trans);
        if (rc)
            iwl_pcie_transport_free(trans);
    }
    if (!rc) {
        rb_iwlwifi_format_out(out, out_len,
                              "linux_kpi_activate=ok irq=%d vectors=%d msix=%d fw_version=%u dma_ready=%d int_mask=0x%08x",
                              trans->irq,
                              trans->num_irq_vectors,
                              trans->msix_enabled,
                              trans->fw_info.version,
                              !!(trans->svc_flags & RB_IWL_SVC_DMA_READY),
                              iwl_trans_read32(trans, IWL_CSR_INT_MASK));
    }
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_linux_scan(struct pci_dev *dev, const char *ssid, char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    struct rb_iwl_scan_cmd cmd;
    int rc;
    size_t ssid_len = ssid ? strlen(ssid) : 0;
    int i;

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_full_init_locked(trans, 0, 0, NULL, NULL);
    if (!rc) {
        memset(&cmd, 0, sizeof(cmd));
        cmd.hdr.id = RB_IWL_CMD_SCAN;
        cmd.hdr.len = sizeof(cmd);
        cmd.hdr.cookie = (u32)atomic_add_return(1, &rb_iwlwifi_cmd_cookie);
        cmd.n_channels = 11;
        cmd.passive_dwell = 20;
        cmd.active_dwell = 10;
        cmd.ssid_len = (u32)min_t(size_t, ssid_len, IEEE80211_MAX_SSID_LEN);
        if (cmd.ssid_len)
            memcpy(cmd.ssid, ssid, cmd.ssid_len);
        for (i = 0; i < 11; ++i)
            cmd.channels[i] = (u16)(2412 + i * 5);

        trans->scan_generation = (u32)atomic_add_return(1, &rb_iwlwifi_scan_cookie);
        trans->svc_flags |= RB_IWL_SVC_SCAN_ACTIVE;
        trans->scan_results_count = 0;
        iwl_ops_sw_scan_start(trans->hw, trans->vif, trans->mac_addr);
        rc = iwl_pcie_send_cmd(trans, &cmd, sizeof(cmd));
        if (rc == -ETIMEDOUT) {
            rb_iwlwifi_format_out(out, out_len,
                                  "linux_kpi_scan=timeout ssid=%s generation=%u",
                                  ssid && ssid[0] ? ssid : "broadcast",
                                  trans->scan_generation);
            rc = 0;
        } else if (rc == 0) {
            rb_iwlwifi_format_out(out, out_len,
                                  "linux_kpi_scan=dispatched ssid=%s generation=%u",
                                  ssid && ssid[0] ? ssid : "broadcast",
                                  trans->scan_generation);
        }
        trans->svc_flags &= ~RB_IWL_SVC_SCAN_ACTIVE;
        iwl_ops_sw_scan_complete(trans->hw, trans->vif);
    }
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_linux_connect(struct pci_dev *dev, const char *ssid, const char *security,
                             const char *key, char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    struct rb_iwl_assoc_cmd cmd;
    struct station_parameters sta_params;
    int rc;
    size_t ssid_len;

    if (!dev || !ssid || !security || !out || out_len == 0)
        return -EINVAL;
    if (!ssid[0])
        return -EINVAL;
    if (strcmp(security, "open") != 0 && strcmp(security, "wpa2-psk") != 0)
        return -ENOTSUP;
    if (strcmp(security, "wpa2-psk") == 0 && (!key || !key[0]))
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_full_init_locked(trans, 0, 0, NULL, NULL);
    if (!rc) {
        trans->connecting = 1;
        memset(trans->current_bssid, 0, sizeof(trans->current_bssid));
        memset(trans->station.addr, 0, sizeof(trans->station.addr));
        memset(trans->bss_conf.bssid, 0, sizeof(trans->bss_conf.bssid));
        memset(&cmd, 0, sizeof(cmd));
        cmd.hdr.id = RB_IWL_CMD_ASSOC;
        cmd.hdr.len = sizeof(cmd);
        cmd.hdr.cookie = (u32)atomic_add_return(1, &rb_iwlwifi_cmd_cookie);
        ssid_len = min_t(size_t, strlen(ssid), IEEE80211_MAX_SSID_LEN);
        cmd.ssid_len = (u32)ssid_len;
        memcpy(cmd.ssid, ssid, ssid_len);
        rb_iwlwifi_copy_name(cmd.security, sizeof(cmd.security), security);
        rb_iwlwifi_copy_name(cmd.key, sizeof(cmd.key), key ? key : "");
        cmd.security_len = (u32)strlen(cmd.security);
        cmd.key_len = (u32)strlen(cmd.key);

        rb_iwlwifi_copy_name(trans->last_ssid, sizeof(trans->last_ssid), ssid);
        rb_iwlwifi_copy_name(trans->last_security, sizeof(trans->last_security), security);

        rc = iwl_pcie_send_cmd(trans, &cmd, sizeof(cmd));
        if (rc == 0 && __atomic_load_n(&trans->last_cmd_status, __ATOMIC_SEQ_CST) == 0 &&
            __atomic_load_n(&trans->command_complete, __ATOMIC_SEQ_CST)) {
            int has_bssid = 0;
            for (int bi = 0; bi < 6; ++bi) {
                if (trans->current_bssid[bi] != 0) {
                    has_bssid = 1;
                    break;
                }
            }
            if (!has_bssid) {
                rc = -ENOTCONN;
                rb_iwlwifi_format_out(out, out_len,
                                      "linux_kpi_connect=no_bssid ssid=%s security=%s",
                                      ssid, security);
            } else {
                memset(&sta_params, 0, sizeof(sta_params));
                iwl_ops_add_interface(trans->hw, trans->vif);
                trans->bss_conf.assoc = true;
                trans->bss_conf.aid = 1;
                iwl_ops_bss_info_changed(trans->hw, trans->vif, &trans->bss_conf,
                                          BSS_CHANGED_ASSOC | BSS_CHANGED_BSSID);
                iwl_ops_sta_state(trans->hw, trans->vif, &trans->station,
                                  IEEE80211_STA_ASSOC, IEEE80211_STA_AUTHORIZED);
                rc = ieee80211_start_tx_ba_session(&trans->station, 0, 0);
                if (rc)
                    pr_warn("connect: block-ack session start failed (rc=%d), proceeding without aggregation\n", rc);
                cfg80211_new_sta(trans->netdev, trans->station.addr, &sta_params, GFP_KERNEL);
                cfg80211_connect_bss(trans->netdev, trans->station.addr, NULL, 0, NULL, 0, 0, GFP_KERNEL);
                cfg80211_connect_result(trans->netdev, trans->station.addr, NULL, 0, NULL, 0, 0, GFP_KERNEL);
                netif_carrier_on(trans->netdev);
                trans->connected = 1;
                trans->svc_flags |= RB_IWL_SVC_CONNECTED;
            }
        } else if (rc == 0) {
            rc = -ETIMEDOUT;
            memset(trans->current_bssid, 0, sizeof(trans->current_bssid));
            memset(trans->station.addr, 0, sizeof(trans->station.addr));
            rb_iwlwifi_format_out(out, out_len,
                                  "linux_kpi_connect=timeout ssid=%s security=%s",
                                  ssid, security);
        }
    }
    if (!rc) {
        rb_iwlwifi_format_out(out, out_len,
                              "linux_kpi_connect=ok ssid=%s security=%s key_len=%lu carrier=%s",
                              trans->last_ssid,
                              trans->last_security[0] ? trans->last_security : "open",
                              (unsigned long)(key ? strlen(key) : 0),
                              trans->netdev && netif_carrier_ok(trans->netdev) ? "up" : "down");
    }
    if (trans)
        trans->connecting = 0;
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_linux_disconnect(struct pci_dev *dev, char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    struct rb_iwl_disconnect_cmd cmd;
    int rc;

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_full_init_locked(trans, 0, 0, NULL, NULL);
    if (!rc) {
        trans->connecting = 0;
        memset(&cmd, 0, sizeof(cmd));
        cmd.hdr.id = RB_IWL_CMD_DISCONNECT;
        cmd.hdr.len = sizeof(cmd);
        cmd.hdr.cookie = (u32)atomic_add_return(1, &rb_iwlwifi_cmd_cookie);
        cmd.reason = 3; /* WLAN_REASON_DEAUTH_LEAVING — client-initiated disconnect */
        rc = iwl_pcie_send_cmd(trans, &cmd, sizeof(cmd));
        if (!rc) {
            rc = ieee80211_stop_tx_ba_session(&trans->station, 0);
            if (rc)
                pr_warn("disconnect: block-ack session stop failed (rc=%d)\n", rc);
            cfg80211_disconnected(trans->netdev, 0, NULL, 0, true, GFP_KERNEL);
            netif_carrier_off(trans->netdev);
            trans->connected = 0;
            trans->bss_conf.assoc = false;
            memset(trans->current_bssid, 0, sizeof(trans->current_bssid));
            memset(trans->station.addr, 0, sizeof(trans->station.addr));
            memset(trans->bss_conf.bssid, 0, sizeof(trans->bss_conf.bssid));
            iwl_ops_bss_info_changed(trans->hw, trans->vif, &trans->bss_conf, BSS_CHANGED_ASSOC);
            trans->svc_flags &= ~RB_IWL_SVC_CONNECTED;
            trans->last_ssid[0] = '\0';
        }
    }
    if (!rc)
        rb_iwlwifi_format_out(out, out_len, "linux_kpi_disconnect=ok carrier=%s",
                              trans->netdev && netif_carrier_ok(trans->netdev) ? "up" : "down");
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_full_init(struct pci_dev *dev, unsigned int bar, int bz_family,
                         const char *ucode, const char *pnvm,
                         char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    int rc;

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_full_init_locked(trans, bar, bz_family, ucode, pnvm);
    if (!rc) {
        rb_iwlwifi_format_out(out, out_len,
                              "linux_kpi_full_init=ok fw=%s family=%s irq=%d vectors=%d tx_queues=%d rx_bufs=%u mac80211=%d",
                              trans->fw_name ? trans->fw_name : "none",
                              rb_iwlwifi_family_name(trans->device_family),
                              trans->irq,
                              trans->num_irq_vectors,
                              trans->num_tx_queues,
                              trans->rx_queue.n_rb,
                              trans->mac80211_registered);
    }
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_status(struct pci_dev *dev, char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    int rc;

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rb_iwlwifi_status_line(trans, out, out_len);
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}

int rb_iwlwifi_register_mac80211(struct pci_dev *dev, char *out, unsigned long out_len)
{
    struct iwl_trans_pcie *trans;
    int rc;

    if (!dev || !out || out_len == 0)
        return -EINVAL;

    mutex_lock(&rb_iwlwifi_transport_lock);
    rc = rb_iwlwifi_require_transport(dev, &trans);
    if (!rc)
        rc = rb_iwlwifi_full_init_locked(trans, 0, 0, NULL, NULL);
    if (!rc)
        rc = rb_iwlwifi_register_mac80211_locked(trans);
    if (!rc) {
        rb_iwlwifi_format_out(out, out_len,
                              "linux_kpi_register_mac80211=ok iftype=%u interface_modes=0x%x name=%s",
                              trans->wdev.iftype,
                              trans->wiphy ? trans->wiphy->interface_modes : 0,
                              trans->netdev ? trans->netdev->name : "none");
    }
    mutex_unlock(&rb_iwlwifi_transport_lock);
    return rc;
}
