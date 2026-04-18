#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "include/libdisplay-info/info.h"

#define EDID_BLOCK_SIZE 128
#define EDID_DESCRIPTOR_COUNT 4
#define EDID_DESCRIPTOR_OFFSET 54

struct di_edid {
    struct di_edid_vendor_product vendor_product;
    struct di_edid_chromaticity_coords chromaticity;
    struct di_edid_screen_size screen_size;
    struct di_edid_misc_features misc_features;
    bool has_chromaticity;
    struct di_edid_detailed_timing_def *detailed_timing_storage;
    const struct di_edid_detailed_timing_def **detailed_timings;
    const struct di_edid_ext **extensions;
};

struct di_info {
    struct di_edid edid;
    char *model;
    char *serial;
};

static const uint8_t edid_header[8] = { 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00 };
static const struct di_edid_detailed_timing_def *empty_detailed_timings[] = { NULL };
static const struct di_edid_ext *empty_edid_exts[] = { NULL };
static const struct di_cta_data_block *empty_cta_blocks[] = { NULL };
static const struct di_displayid_data_block *empty_displayid_blocks[] = { NULL };
static const struct di_displayid_type_i_ii_vii_timing *empty_timings[] = { NULL };
static const struct di_edid_screen_size empty_screen_size = { 0, 0 };

static char *dup_string(const char *value)
{
    size_t len;
    char *copy;

    if (!value) {
        value = "";
    }

    len = strlen(value) + 1;
    copy = malloc(len);
    if (!copy) {
        return NULL;
    }

    memcpy(copy, value, len);
    return copy;
}

static bool has_nonzero_bytes(const uint8_t *data, size_t len)
{
    size_t i;

    for (i = 0; i < len; i++) {
        if (data[i] != 0) {
            return true;
        }
    }

    return false;
}

static bool validate_edid_block(const uint8_t *block)
{
    uint8_t checksum = 0;
    size_t i;

    for (i = 0; i < EDID_BLOCK_SIZE; i++) {
        checksum = (uint8_t)(checksum + block[i]);
    }

    return checksum == 0;
}

static uint16_t read_le_u16(const uint8_t *data)
{
    return (uint16_t)data[0] | ((uint16_t)data[1] << 8);
}

static uint32_t read_le_u32(const uint8_t *data)
{
    return (uint32_t)data[0] | ((uint32_t)data[1] << 8) | ((uint32_t)data[2] << 16) | ((uint32_t)data[3] << 24);
}

static float decode_chromaticity_value(uint8_t high_bits, uint8_t low_bits)
{
    return (float)(((unsigned int)high_bits << 2) | low_bits) / 1024.0f;
}

static void decode_manufacturer_id(const uint8_t *base, char manufacturer[4])
{
    manufacturer[0] = (char)('A' + ((base[8] >> 2) & 0x1f) - 1);
    manufacturer[1] = (char)('A' + (((base[8] & 0x03) << 3) | ((base[9] >> 5) & 0x07)) - 1);
    manufacturer[2] = (char)('A' + (base[9] & 0x1f) - 1);
    manufacturer[3] = '\0';

    if (!isupper((unsigned char)manufacturer[0]) || !isupper((unsigned char)manufacturer[1]) || !isupper((unsigned char)manufacturer[2])) {
        memcpy(manufacturer, "???", 4);
    }
}

static char *parse_descriptor_string(const uint8_t *descriptor)
{
    char buffer[14];
    size_t out_len = 0;
    size_t i;

    for (i = 5; i < 18; i++) {
        const uint8_t value = descriptor[i];

        if (value == 0x00 || value == 0x0a || value == 0x0d) {
            break;
        }
        if (value < 0x20 || value > 0x7e) {
            continue;
        }

        buffer[out_len++] = (char)value;
    }

    while (out_len > 0 && isspace((unsigned char)buffer[out_len - 1])) {
        out_len--;
    }
    buffer[out_len] = '\0';

    return dup_string(buffer);
}

static bool parse_detailed_timing(const uint8_t *descriptor, struct di_edid_detailed_timing_def *timing)
{
    const uint16_t pixel_clock = read_le_u16(descriptor);

    if (pixel_clock == 0) {
        return false;
    }

    timing->horiz_video = (int)(descriptor[2] | ((descriptor[4] & 0xf0) << 4));
    timing->vert_video = (int)(descriptor[5] | ((descriptor[7] & 0xf0) << 4));
    timing->horiz_image_mm = (int)(descriptor[12] | ((descriptor[14] & 0xf0) << 4));
    timing->vert_image_mm = (int)(descriptor[13] | ((descriptor[14] & 0x0f) << 8));

    return timing->horiz_video > 0 && timing->vert_video > 0;
}

static bool populate_detailed_timings(struct di_edid *edid, const uint8_t *base)
{
    struct di_edid_detailed_timing_def parsed[EDID_DESCRIPTOR_COUNT];
    size_t count = 0;
    size_t i;

    edid->detailed_timings = empty_detailed_timings;

    for (i = 0; i < EDID_DESCRIPTOR_COUNT; i++) {
        const uint8_t *descriptor = base + EDID_DESCRIPTOR_OFFSET + (i * 18);

        if (parse_detailed_timing(descriptor, &parsed[count])) {
            count++;
        }
    }

    if (count == 0) {
        return true;
    }

    edid->detailed_timing_storage = calloc(count, sizeof(*edid->detailed_timing_storage));
    edid->detailed_timings = calloc(count + 1, sizeof(*edid->detailed_timings));
    if (!edid->detailed_timing_storage || !edid->detailed_timings) {
        return false;
    }

    for (i = 0; i < count; i++) {
        edid->detailed_timing_storage[i] = parsed[i];
        edid->detailed_timings[i] = &edid->detailed_timing_storage[i];
    }

    edid->detailed_timings[count] = NULL;
    return true;
}

static void populate_vendor_product(struct di_edid *edid, const uint8_t *base)
{
    const int manufacture_year = (int)base[17] + 1990;
    const bool has_model_year = base[16] == 0xff;

    decode_manufacturer_id(base, edid->vendor_product.manufacturer);
    edid->vendor_product.product = (int)read_le_u16(base + 10);
    edid->vendor_product.serial = (int)read_le_u32(base + 12);
    edid->vendor_product.manufacture_week = has_model_year ? 0 : (int)base[16];
    edid->vendor_product.manufacture_year = has_model_year ? 0 : manufacture_year;
    edid->vendor_product.model_year = has_model_year ? manufacture_year : 0;
}

static void populate_chromaticity(struct di_edid *edid, const uint8_t *base)
{
    if (!has_nonzero_bytes(base + 25, 10)) {
        edid->has_chromaticity = false;
        return;
    }

    edid->chromaticity.red_x = decode_chromaticity_value(base[27], (base[25] >> 6) & 0x03);
    edid->chromaticity.red_y = decode_chromaticity_value(base[28], (base[25] >> 4) & 0x03);
    edid->chromaticity.green_x = decode_chromaticity_value(base[29], (base[25] >> 2) & 0x03);
    edid->chromaticity.green_y = decode_chromaticity_value(base[30], base[25] & 0x03);
    edid->chromaticity.blue_x = decode_chromaticity_value(base[31], (base[26] >> 6) & 0x03);
    edid->chromaticity.blue_y = decode_chromaticity_value(base[32], (base[26] >> 4) & 0x03);
    edid->chromaticity.white_x = decode_chromaticity_value(base[33], (base[26] >> 2) & 0x03);
    edid->chromaticity.white_y = decode_chromaticity_value(base[34], base[26] & 0x03);
    edid->has_chromaticity = true;
}

static bool populate_strings(struct di_info *info, const uint8_t *base)
{
    size_t i;

    for (i = 0; i < EDID_DESCRIPTOR_COUNT; i++) {
        const uint8_t *descriptor = base + EDID_DESCRIPTOR_OFFSET + (i * 18);

        if (descriptor[0] != 0x00 || descriptor[1] != 0x00 || descriptor[2] != 0x00) {
            continue;
        }

        if (descriptor[3] == 0xfc && !info->model) {
            info->model = parse_descriptor_string(descriptor);
        } else if (descriptor[3] == 0xff && !info->serial) {
            info->serial = parse_descriptor_string(descriptor);
        }
    }

    if (!info->model) {
        info->model = dup_string("");
    }
    if (!info->serial) {
        if (info->edid.vendor_product.serial != 0) {
            char serial_buffer[32];

            snprintf(serial_buffer, sizeof(serial_buffer), "%u", (unsigned int)info->edid.vendor_product.serial);
            info->serial = dup_string(serial_buffer);
        } else {
            info->serial = dup_string("");
        }
    }

    return info->model && info->serial;
}

const struct di_info *di_info_parse_edid(const void *data, size_t size)
{
    const uint8_t *base = data;
    struct di_info *info;

    if (!base || size < EDID_BLOCK_SIZE) {
        return NULL;
    }
    if (memcmp(base, edid_header, sizeof(edid_header)) != 0) {
        return NULL;
    }
    if (!validate_edid_block(base)) {
        return NULL;
    }

    info = calloc(1, sizeof(*info));
    if (!info) {
        return NULL;
    }

    info->edid.screen_size.width_cm = (int)base[21];
    info->edid.screen_size.height_cm = (int)base[22];
    info->edid.misc_features.preferred_timing_is_native = (base[24] & 0x02) != 0;
    info->edid.extensions = empty_edid_exts;

    populate_vendor_product(&info->edid, base);
    populate_chromaticity(&info->edid, base);
    if (!populate_detailed_timings(&info->edid, base) || !populate_strings(info, base)) {
        di_info_destroy(info);
        return NULL;
    }

    return info;
}

void di_info_destroy(const struct di_info *info)
{
    struct di_info *mutable_info = (struct di_info *)info;

    if (!mutable_info) {
        return;
    }

    free(mutable_info->model);
    free(mutable_info->serial);
    if (mutable_info->edid.detailed_timings != empty_detailed_timings) {
        free((void *)mutable_info->edid.detailed_timings);
    }
    free(mutable_info->edid.detailed_timing_storage);
    free(mutable_info);
}

const struct di_edid *di_info_get_edid(const struct di_info *info)
{
    return info ? &info->edid : NULL;
}

char *di_info_get_model(const struct di_info *info)
{
    return dup_string(info ? info->model : "");
}

char *di_info_get_serial(const struct di_info *info)
{
    return dup_string(info ? info->serial : "");
}

const struct di_edid_detailed_timing_def *const *di_edid_get_detailed_timing_defs(const struct di_edid *edid)
{
    return edid && edid->detailed_timings ? edid->detailed_timings : empty_detailed_timings;
}

const struct di_edid_screen_size *di_edid_get_screen_size(const struct di_edid *edid)
{
    return edid ? &edid->screen_size : &empty_screen_size;
}

const struct di_edid_vendor_product *di_edid_get_vendor_product(const struct di_edid *edid)
{
    return edid ? &edid->vendor_product : NULL;
}

const struct di_edid_chromaticity_coords *di_edid_get_chromaticity_coords(const struct di_edid *edid)
{
    return edid && edid->has_chromaticity ? &edid->chromaticity : NULL;
}

const struct di_edid_ext *const *di_edid_get_extensions(const struct di_edid *edid)
{
    return edid && edid->extensions ? edid->extensions : empty_edid_exts;
}

const struct di_edid_misc_features *di_edid_get_misc_features(const struct di_edid *edid)
{
    return edid ? &edid->misc_features : NULL;
}

const struct di_edid_cta *di_edid_ext_get_cta(const struct di_edid_ext *ext)
{
    (void)ext;
    return NULL;
}

const struct di_displayid *di_edid_ext_get_displayid(const struct di_edid_ext *ext)
{
    (void)ext;
    return NULL;
}

const struct di_cta_data_block *const *di_edid_cta_get_data_blocks(const struct di_edid_cta *cta)
{
    (void)cta;
    return empty_cta_blocks;
}

const struct di_cta_hdr_static_metadata_block *di_cta_data_block_get_hdr_static_metadata(const struct di_cta_data_block *block)
{
    (void)block;
    return NULL;
}

const struct di_cta_colorimetry_block *di_cta_data_block_get_colorimetry(const struct di_cta_data_block *block)
{
    (void)block;
    return NULL;
}

const struct di_displayid_data_block *const *di_displayid_get_data_blocks(const struct di_displayid *displayid)
{
    (void)displayid;
    return empty_displayid_blocks;
}

const struct di_displayid_display_params *di_displayid_data_block_get_display_params(const struct di_displayid_data_block *block)
{
    (void)block;
    return NULL;
}

const struct di_displayid_type_i_ii_vii_timing *const *di_displayid_data_block_get_type_i_timings(const struct di_displayid_data_block *block)
{
    (void)block;
    return empty_timings;
}

const struct di_displayid_type_i_ii_vii_timing *const *di_displayid_data_block_get_type_ii_timings(const struct di_displayid_data_block *block)
{
    (void)block;
    return empty_timings;
}
