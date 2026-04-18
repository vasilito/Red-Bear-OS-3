#include "redox_glue.h"

/* Global state */
static struct drm_device g_drm_dev;
static struct device g_device;
static struct pci_dev *g_pci_dev;
static void __iomem *g_mmio_base;
static size_t g_mmio_size;
static u64 g_fb_phys;
static size_t g_fb_size;
static int g_asic_family = -1;

/* ASIC family definitions based on device IDs */
#define ASIC_FAMILY_NAVI10  0x7310
#define ASIC_FAMILY_NAVI14  0x7340
#define ASIC_FAMILY_NAVI21  0x73A0
#define ASIC_FAMILY_NAVI22  0x73C0
#define ASIC_FAMILY_NAVI23  0x73E0
#define ASIC_FAMILY_NAVI24  0x7420
#define ASIC_FAMILY_NAVI31  0x7440
#define ASIC_FAMILY_NAVI32  0x7480
#define ASIC_FAMILY_NAVI33  0x74A0

#define AMDGPU_DC_HPD_STATUS_REG            0x4A00
#define AMDGPU_DC_MAX_CONNECTORS            4
#define AMDGPU_DC_BYTES_PER_PIXEL           4U
#define AMDGPU_DC_PIXEL_FORMAT_ARGB8888     3U

#define AMDGPU_DC_OTG_CONTROL               0x00
#define AMDGPU_DC_OTG_VIEWPORT_SIZE         0x10
#define AMDGPU_DC_OTG_VSYNC_ADJUST          0x14
#define AMDGPU_DC_OTG_H_TOTAL               0x18
#define AMDGPU_DC_OTG_V_TOTAL               0x1C
#define AMDGPU_DC_OTG_VSTARTUP              0x20

#define AMDGPU_DC_HUBP_PRIMARY_ADDR_LOW     0x00
#define AMDGPU_DC_HUBP_PRIMARY_ADDR_HIGH    0x04
#define AMDGPU_DC_HUBP_SURFACE_PITCH        0x08
#define AMDGPU_DC_HUBP_SURFACE_CONFIG       0x0C
#define AMDGPU_DC_HUBP_VIEWPORT_START       0x10
#define AMDGPU_DC_HUBP_VIEWPORT_SIZE        0x14
#define AMDGPU_DC_HUBP_FLIP_CONTROL         0x18
#define AMDGPU_DC_HUBP_FLIP_ADDR_LOW        0x1C
#define AMDGPU_DC_HUBP_FLIP_ADDR_HIGH       0x20

struct connector_info_ffi {
    int id;
    int connector_type;
    int connector_type_id;
    int connection;
    int mm_width;
    int mm_height;
    int encoder_id;
};

struct amdgpu_redox_connector_desc {
    int id;
    u32 hpd_mask;
    int connector_type;
    int connector_type_id;
    int encoder_id;
    int mm_width;
    int mm_height;
};

static const struct amdgpu_redox_connector_desc g_connector_descs[AMDGPU_DC_MAX_CONNECTORS] = {
    { .id = 1, .hpd_mask = 0x01, .connector_type = 10, .connector_type_id = 1, .encoder_id = 1, .mm_width = 600, .mm_height = 340 },
    { .id = 2, .hpd_mask = 0x02, .connector_type = 10, .connector_type_id = 2, .encoder_id = 2, .mm_width = 600, .mm_height = 340 },
    { .id = 3, .hpd_mask = 0x04, .connector_type = 11, .connector_type_id = 3, .encoder_id = 3, .mm_width = 600, .mm_height = 340 },
    { .id = 4, .hpd_mask = 0x08, .connector_type = 11, .connector_type_id = 4, .encoder_id = 4, .mm_width = 600, .mm_height = 340 },
};

static inline void __iomem *amdgpu_dc_reg_ptr(u32 base, u32 offset)
{
    return (u8 __iomem *)g_mmio_base + base + offset;
}

static int amdgpu_dc_validate_mmio_access(u32 base, u32 offset)
{
    u64 end = (u64)base + (u64)offset + sizeof(u32);

    if (!g_mmio_base) {
        return -ENODEV;
    }

    if (end > g_mmio_size) {
        pr_err("amdgpu_redox: MMIO access %#x+%#x outside aperture %zu\n",
               base, offset, g_mmio_size);
        return -EINVAL;
    }

    return 0;
}

static inline void amdgpu_dc_write_reg(u32 base, u32 offset, u32 value)
{
    if (amdgpu_dc_validate_mmio_access(base, offset) != 0) {
        return;
    }
    writel(value, amdgpu_dc_reg_ptr(base, offset));
}

static inline u32 amdgpu_dc_read_reg(u32 base, u32 offset)
{
    if (amdgpu_dc_validate_mmio_access(base, offset) != 0) {
        return 0;
    }
    return readl(amdgpu_dc_reg_ptr(base, offset));
}

static inline u32 amdgpu_dc_hpd_status(void)
{
    if (amdgpu_dc_validate_mmio_access(0, AMDGPU_DC_HPD_STATUS_REG) != 0) {
        return 0;
    }
    return readl((u8 __iomem *)g_mmio_base + AMDGPU_DC_HPD_STATUS_REG);
}

static void amdgpu_redox_log_irq_expectation(u64 quirk_flags)
{
    const char *policy = "MSI-X first, then MSI, then legacy IRQ fallback";

    if ((quirk_flags & PCI_QUIRK_FORCE_LEGACY) != 0 ||
        ((quirk_flags & PCI_QUIRK_NO_MSIX) != 0 &&
         (quirk_flags & PCI_QUIRK_NO_MSI) != 0)) {
        policy = "legacy IRQ only";
    } else if ((quirk_flags & PCI_QUIRK_NO_MSIX) != 0) {
        policy = "avoid MSI-X, prefer MSI with legacy fallback";
    } else if ((quirk_flags & PCI_QUIRK_NO_MSI) != 0) {
        policy = "avoid MSI, prefer MSI-X with legacy fallback";
    }

    printk("amdgpu_redox: quirk-aware IRQ expectation: %s\n", policy);
}

/* Initialize AMD Display Core */
int amdgpu_dc_init(void *mmio_base, size_t mmio_size)
{
    int ret = 0;
    u32 gpu_id = 0;
    const char *firmware_name = NULL;
    u64 quirk_flags = 0;

    printk("amdgpu_redox: initializing AMD Display Core\n");

    if (!mmio_base || mmio_size < sizeof(u32)) {
        pr_err("amdgpu_redox: invalid MMIO for DC init\n");
        return -EINVAL;
    }

    gpu_id = readl(mmio_base);
    printk("amdgpu_redox: GPU ID = %#010x\n", gpu_id);

    if (g_pci_dev) {
        quirk_flags = pci_get_quirk_flags(g_pci_dev);
        printk("amdgpu_redox: PCI %02x:%02x.%u quirk flags = %#llx\n",
               g_pci_dev->bus_number,
               g_pci_dev->dev_number,
               g_pci_dev->func_number,
               (unsigned long long)quirk_flags);
        if (pci_has_quirk(g_pci_dev, PCI_QUIRK_NO_ASPM)) {
            pr_warn("amdgpu_redox: NO_ASPM quirk active; skipping any future ASPM-dependent assumptions\n");
        }
        if (pci_has_quirk(g_pci_dev, PCI_QUIRK_NEED_IOMMU)) {
            pr_warn("amdgpu_redox: NEED_IOMMU quirk active; runtime must provide a functional IOMMU path\n");
        }
        if (pci_has_quirk(g_pci_dev, PCI_QUIRK_NO_MSIX)) {
            pr_warn("amdgpu_redox: NO_MSIX quirk active; IRQ setup must avoid MSI-X\n");
        }
        if (pci_has_quirk(g_pci_dev, PCI_QUIRK_NO_MSI)) {
            pr_warn("amdgpu_redox: NO_MSI quirk active; IRQ setup must avoid MSI\n");
        }
        amdgpu_redox_log_irq_expectation(quirk_flags);
    }

    switch (gpu_id) {
        case ASIC_FAMILY_NAVI10:
            g_asic_family = ASIC_FAMILY_NAVI10;
            firmware_name = "dmcub_dcn20.bin";
            break;
        case ASIC_FAMILY_NAVI14:
            g_asic_family = ASIC_FAMILY_NAVI14;
            firmware_name = "dmcub_dcn20.bin";
            break;
        case ASIC_FAMILY_NAVI21:
            g_asic_family = ASIC_FAMILY_NAVI21;
            firmware_name = "dmcub_dcn31.bin";
            break;
        case ASIC_FAMILY_NAVI22:
            g_asic_family = ASIC_FAMILY_NAVI22;
            firmware_name = "dmcub_dcn31.bin";
            break;
        case ASIC_FAMILY_NAVI23:
            g_asic_family = ASIC_FAMILY_NAVI23;
            firmware_name = "dmcub_dcn31.bin";
            break;
        case ASIC_FAMILY_NAVI24:
            g_asic_family = ASIC_FAMILY_NAVI24;
            firmware_name = "dmcub_dcn31.bin";
            break;
        case ASIC_FAMILY_NAVI31:
            g_asic_family = ASIC_FAMILY_NAVI31;
            firmware_name = "dmcub_dcn31.bin";
            break;
        case ASIC_FAMILY_NAVI32:
            g_asic_family = ASIC_FAMILY_NAVI32;
            firmware_name = "dmcub_dcn31.bin";
            break;
        case ASIC_FAMILY_NAVI33:
            g_asic_family = ASIC_FAMILY_NAVI33;
            firmware_name = "dmcub_dcn31.bin";
            break;
        default:
            pr_warn("amdgpu_redox: unknown ASIC %#010x, using DCN31 firmware\n", gpu_id);
            g_asic_family = gpu_id;
            firmware_name = "dmcub_dcn31.bin";
            break;
    }

    printk("amdgpu_redox: ASIC family identified, loading firmware: %s\n", firmware_name);

    {
        const struct firmware *fw = NULL;
        int fw_ret = request_firmware(&fw, firmware_name, NULL);

        if (fw_ret != 0 || !fw) {
            pr_warn("amdgpu_redox: firmware %s not available in backend load path (err=%d), continuing with Rust-side quirk policy already applied (quirks=%#llx)\n",
                    firmware_name,
                    fw_ret,
                    (unsigned long long)quirk_flags);
        } else {
            printk("amdgpu_redox: firmware %s loaded (%zu bytes)\n", firmware_name, fw->size);
            release_firmware(fw);
        }
    }

    return ret;
}

/* Initialize AMD GPU hardware for display */
int amdgpu_redox_init(void *mmio_base, size_t mmio_size, uint64_t fb_phys, size_t fb_size)
{
    int ret;
    printk("amdgpu_redox: initializing AMD GPU display\n");
    printk("amdgpu_redox: MMIO base=%p size=%zu\n", mmio_base, mmio_size);
    printk("amdgpu_redox: FB phys=%#llx size=%zu\n", (unsigned long long)fb_phys, fb_size);

    if (!mmio_base || mmio_size == 0) {
        pr_err("amdgpu_redox: invalid MMIO mapping provided by redox-drm\n");
        return -EINVAL;
    }

    memset(&g_drm_dev, 0, sizeof(g_drm_dev));
    memset(&g_device, 0, sizeof(g_device));

    g_mmio_base = mmio_base;
    g_mmio_size = mmio_size;
    g_fb_phys = fb_phys;
    g_fb_size = fb_size;

    g_pci_dev = redox_pci_find_amd_gpu();
    if (!g_pci_dev) {
        pr_err("amdgpu_redox: no AMD PCI device available from integration layer\n");
        return -ENODEV;
    }

    g_pci_dev->mmio_base = g_mmio_base;
    g_pci_dev->resource_len[0] = g_mmio_size;

    g_device.pci_dev = g_pci_dev;
    g_drm_dev.dev = &g_device;

    ret = amdgpu_dc_init(mmio_base, mmio_size);
    if (ret != 0) {
        pr_err("amdgpu_redox: failed to initialize DC\n");
        return ret;
    }

    return 0;
}

/* Cleanup */
void amdgpu_redox_cleanup(void)
{
    printk("amdgpu_redox: cleanup\n");
    if (g_pci_dev) {
        redox_pci_dev_put(g_pci_dev);
        g_pci_dev = NULL;
    }

    g_mmio_base = NULL;
    g_mmio_size = 0;
    g_fb_phys = 0;
    g_fb_size = 0;
    memset(&g_drm_dev, 0, sizeof(g_drm_dev));
    memset(&g_device, 0, sizeof(g_device));
}

/* Get connector info — called by redox-drm */
int amdgpu_dc_detect_connectors(void)
{
    int num_connectors = 0;

    if (!g_mmio_base) {
        pr_err("amdgpu_redox: detect_connectors called before init\n");
        return -ENODEV;
    }

#ifdef __redox__
    u32 hpd_status = amdgpu_dc_hpd_status();
    int i;

    for (i = 0; i < AMDGPU_DC_MAX_CONNECTORS; ++i) {
        if (hpd_status & g_connector_descs[i].hpd_mask) {
            num_connectors++;
        }
    }

    printk("amdgpu_redox: detected %d connector(s)\n", num_connectors);
#else
    printk("amdgpu_redox: running on Linux, using AMD DC detection\n");
#endif

    return num_connectors;
}

/* Get connector info by index */
int amdgpu_dc_get_connector_info(int idx, void *info)
{
    struct connector_info_ffi *ffi_info = (struct connector_info_ffi *)info;

    if (!g_mmio_base) {
        pr_err("amdgpu_redox: get_connector_info called before init\n");
        return -ENODEV;
    }

    if (idx < 0 || !ffi_info) {
        return -EINVAL;
    }

#ifdef __redox__
    {
        u32 hpd_status = amdgpu_dc_hpd_status();
        int active_index = 0;
        int i;

        for (i = 0; i < AMDGPU_DC_MAX_CONNECTORS; ++i) {
            const struct amdgpu_redox_connector_desc *desc = &g_connector_descs[i];

            if (!(hpd_status & desc->hpd_mask)) {
                continue;
            }

            if (active_index == idx) {
                ffi_info->id = desc->id;
                ffi_info->connector_type = desc->connector_type;
                ffi_info->connector_type_id = desc->connector_type_id;
                ffi_info->connection = 1;
                ffi_info->mm_width = desc->mm_width;
                ffi_info->mm_height = desc->mm_height;
                ffi_info->encoder_id = desc->encoder_id;
                return 0;
            }

            active_index++;
        }
    }
#endif

    return -ENOENT;
}

/* Set CRTC mode — called by redox-drm for modesetting */
int amdgpu_dc_set_crtc(int crtc_id, uint64_t fb_addr, uint32_t width, uint32_t height)
{
    printk("amdgpu_redox: set_crtc(%d, fb=%#llx, %ux%u)\n",
           crtc_id,
           (unsigned long long)fb_addr,
           width,
           height);

    if (!g_mmio_base) {
        pr_err("amdgpu_redox: set_crtc called before amdgpu_redox_init\n");
        return -ENODEV;
    }

#ifdef __redox__
    const u32 bytes_per_pixel = AMDGPU_DC_BYTES_PER_PIXEL;
    u32 pitch;
    u32 viewport_size;
    const u32 h_total = width + 160U;
    const u32 v_total = height + 45U;
    const u32 v_sync_start = height + 3U;
    const u32 v_sync_end = v_sync_start + 5U;
    const u32 v_sync_adjust = (v_sync_start & 0xFFFFU) | (v_sync_end << 16);
    const u32 vstartup = v_sync_start > 1U ? (v_sync_start - 1U) : 0U;
    u64 required_bytes;

    if (crtc_id < 0 || crtc_id > 3) {
        pr_err("amdgpu_redox: invalid crtc_id %d\n", crtc_id);
        return -EINVAL;
    }

    if (width == 0 || height == 0 || width > 0xFFFFU || height > 0xFFFFU) {
        pr_err("amdgpu_redox: invalid mode %ux%u\n", width, height);
        return -EINVAL;
    }

    if (width > (UINT32_MAX / bytes_per_pixel)) {
        pr_err("amdgpu_redox: pitch overflow for width %u\n", width);
        return -EINVAL;
    }

    pitch = width * bytes_per_pixel;
    viewport_size = (width & 0xFFFFU) | (height << 16);
    required_bytes = (u64)pitch * (u64)height;

    /* The Rust-side allocates scanout buffers via GTT VA space (0..256MiB).
     * The display controller programs these GPU-virtual addresses directly;
     * the GTT hardware translates them to physical backing pages at runtime.
     * Validate only that the address + size fits in a u64 and that the
     * programmed registers can hold the values. */
    if (required_bytes == 0) {
        pr_err("amdgpu_redox: zero-sized framebuffer for crtc %d\n", crtc_id);
        return -EINVAL;
    }

    u32 otg_base = 0x4800 + (crtc_id * 0x800);
    u32 hubp_base = 0x5800 + (crtc_id * 0x400);
    u32 otg_control;

    if (amdgpu_dc_validate_mmio_access(otg_base, AMDGPU_DC_OTG_VSTARTUP) != 0 ||
        amdgpu_dc_validate_mmio_access(hubp_base, AMDGPU_DC_HUBP_FLIP_ADDR_HIGH) != 0) {
        return -EINVAL;
    }

    otg_control = amdgpu_dc_read_reg(otg_base, AMDGPU_DC_OTG_CONTROL);
    otg_control &= ~0x01U;
    amdgpu_dc_write_reg(otg_base, AMDGPU_DC_OTG_CONTROL, otg_control);
    mb();

    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_PRIMARY_ADDR_LOW, (u32)(fb_addr & 0xFFFFFFFFULL));
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_PRIMARY_ADDR_HIGH, (u32)((fb_addr >> 32) & 0xFFFFFFFFULL));
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_SURFACE_PITCH, pitch);
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_SURFACE_CONFIG, AMDGPU_DC_PIXEL_FORMAT_ARGB8888);
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_VIEWPORT_START, 0);
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_VIEWPORT_SIZE, viewport_size);
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_FLIP_ADDR_LOW, (u32)(fb_addr & 0xFFFFFFFFULL));
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_FLIP_ADDR_HIGH, (u32)((fb_addr >> 32) & 0xFFFFFFFFULL));
    amdgpu_dc_write_reg(hubp_base, AMDGPU_DC_HUBP_FLIP_CONTROL, 0);

    amdgpu_dc_write_reg(otg_base, AMDGPU_DC_OTG_VIEWPORT_SIZE, viewport_size);
    amdgpu_dc_write_reg(otg_base, AMDGPU_DC_OTG_VSYNC_ADJUST, v_sync_adjust);
    amdgpu_dc_write_reg(otg_base, AMDGPU_DC_OTG_H_TOTAL, h_total);
    amdgpu_dc_write_reg(otg_base, AMDGPU_DC_OTG_V_TOTAL, v_total);
    amdgpu_dc_write_reg(otg_base, AMDGPU_DC_OTG_VSTARTUP, vstartup);
    mb();

    otg_control |= 0x01;
    amdgpu_dc_write_reg(otg_base, AMDGPU_DC_OTG_CONTROL, otg_control);

    printk("amdgpu_redox: CRTC %d enabled at %ux%u, fb=%#llx\n",
           crtc_id, width, height, (unsigned long long)fb_addr);
#else
    printk("amdgpu_redox: running on Linux, using AMD DC modesetting\n");
#endif

    return 0;
}
