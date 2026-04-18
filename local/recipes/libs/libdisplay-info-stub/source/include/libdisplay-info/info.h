#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

struct di_info;
struct di_edid;
struct di_edid_ext;
struct di_edid_cta;
struct di_displayid;
struct di_cta_data_block;
struct di_displayid_data_block;

struct di_edid_vendor_product {
    char manufacturer[4];
    int product;
    int serial;
    int manufacture_week;
    int manufacture_year;
    int model_year;
};

struct di_edid_chromaticity_coords {
    float red_x;
    float red_y;
    float green_x;
    float green_y;
    float blue_x;
    float blue_y;
    float white_x;
    float white_y;
};

struct di_cta_hdr_static_metadata_eotfs {
    bool pq;
};

struct di_cta_hdr_static_metadata_block {
    int desired_content_min_luminance;
    int desired_content_max_luminance;
    int desired_content_max_frame_avg_luminance;
    const struct di_cta_hdr_static_metadata_eotfs *eotfs;
};

struct di_cta_colorimetry_block {
    bool bt2020_rgb;
};

struct di_displayid_display_params {
    int horiz_pixels;
    int vert_pixels;
};

struct di_displayid_type_i_ii_vii_timing {
    bool preferred;
    int horiz_active;
    int vert_active;
};

struct di_edid_misc_features {
    bool preferred_timing_is_native;
};

struct di_edid_detailed_timing_def {
    int horiz_image_mm;
    int vert_image_mm;
    int horiz_video;
    int vert_video;
};

struct di_edid_screen_size {
    int width_cm;
    int height_cm;
};

const struct di_info *di_info_parse_edid(const void *data, size_t size);
void di_info_destroy(const struct di_info *info);
const struct di_edid *di_info_get_edid(const struct di_info *info);
char *di_info_get_model(const struct di_info *info);
char *di_info_get_serial(const struct di_info *info);

const struct di_edid_detailed_timing_def *const *di_edid_get_detailed_timing_defs(const struct di_edid *edid);
const struct di_edid_screen_size *di_edid_get_screen_size(const struct di_edid *edid);
const struct di_edid_vendor_product *di_edid_get_vendor_product(const struct di_edid *edid);
const struct di_edid_chromaticity_coords *di_edid_get_chromaticity_coords(const struct di_edid *edid);
const struct di_edid_ext *const *di_edid_get_extensions(const struct di_edid *edid);
const struct di_edid_misc_features *di_edid_get_misc_features(const struct di_edid *edid);

const struct di_edid_cta *di_edid_ext_get_cta(const struct di_edid_ext *ext);
const struct di_displayid *di_edid_ext_get_displayid(const struct di_edid_ext *ext);

const struct di_cta_data_block *const *di_edid_cta_get_data_blocks(const struct di_edid_cta *cta);
const struct di_cta_hdr_static_metadata_block *di_cta_data_block_get_hdr_static_metadata(const struct di_cta_data_block *block);
const struct di_cta_colorimetry_block *di_cta_data_block_get_colorimetry(const struct di_cta_data_block *block);

const struct di_displayid_data_block *const *di_displayid_get_data_blocks(const struct di_displayid *displayid);
const struct di_displayid_display_params *di_displayid_data_block_get_display_params(const struct di_displayid_data_block *block);
const struct di_displayid_type_i_ii_vii_timing *const *di_displayid_data_block_get_type_i_timings(const struct di_displayid_data_block *block);
const struct di_displayid_type_i_ii_vii_timing *const *di_displayid_data_block_get_type_ii_timings(const struct di_displayid_data_block *block);

#ifdef __cplusplus
}
#endif
