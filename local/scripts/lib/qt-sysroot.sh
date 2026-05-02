#!/usr/bin/env bash

redbear_qt_link_sysroot_dir() {
    local sysroot="$1"
    local dir_name="$2"
    if [ -d "${sysroot}/usr/${dir_name}" ] && [ ! -e "${sysroot}/${dir_name}" ]; then
        ln -s "usr/${dir_name}" "${sysroot}/${dir_name}"
    fi
}

redbear_qt_link_sysroot_dirs() {
    local sysroot="$1"
    shift
    local dir_name
    for dir_name in "$@"; do
        redbear_qt_link_sysroot_dir "$sysroot" "$dir_name"
    done
}

redbear_qt_link_plugins_dir() {
    local sysroot="$1"
    if [ -d "${sysroot}/usr/plugins" ] && [ ! -e "${sysroot}/plugins" ]; then
        ln -s "usr/plugins" "${sysroot}/plugins"
    fi
}

redbear_qt_prepare_common_sysroot() {
    local sysroot="$1"
    redbear_qt_link_sysroot_dirs "$sysroot" plugins mkspecs metatypes modules
    redbear_qt_link_plugins_dir "$sysroot"
}

redbear_qt_reset_cmake_cache_dir() {
    rm -f CMakeCache.txt
    if [ -d CMakeFiles ]; then
        python3 - <<'PY'
from pathlib import Path
import os
import shutil

path = Path("CMakeFiles")
for node in path.rglob('*'):
    try:
        os.chmod(node, 0o700)
    except OSError:
        pass
shutil.rmtree(path, ignore_errors=False)
PY
    fi
}

redbear_qt_rewrite_stage_build_paths() {
    local stage_usr="$1"
    local build_dir="$2"
    find "${stage_usr}/lib/cmake" -name '*.cmake' -exec sed -i \
        "s|${build_dir}|/usr|g" {} + 2>/dev/null || true
}

redbear_qt_copy_common_stage_to_sysroot() {
    local stage_usr="$1"
    local sysroot="$2"
    mkdir -p "${sysroot}/include" "${sysroot}/lib"
    cp -a "${stage_usr}/include/"* "${sysroot}/include/" 2>/dev/null || true
    cp -a "${stage_usr}/lib/libQt6"* "${sysroot}/lib/" 2>/dev/null || true
}

redbear_qt_copy_stage_cmake_subdir_to_sysroot() {
    local stage_usr="$1"
    local sysroot="$2"
    local subdir="$3"
    mkdir -p "${sysroot}/lib/cmake/${subdir}"
    cp -a "${stage_usr}/lib/cmake/${subdir}/"* "${sysroot}/lib/cmake/${subdir}/" 2>/dev/null || true
}

redbear_qt_rewrite_stage_include_paths() {
    local stage_cmake_dir="$1"
    local sysroot="$2"
    find "${stage_cmake_dir}" -name '*.cmake' -exec sed -i \
        "s|/usr/include|${sysroot}/include|g" {} + 2>/dev/null || true
}

redbear_qt_rewrite_stage_lib_paths() {
    local stage_cmake_dir="$1"
    local sysroot="$2"
    find "${stage_cmake_dir}" -name '*.cmake' -exec sed -i \
        "s|/usr/lib|${sysroot}/lib|g" {} + 2>/dev/null || true
}

redbear_qt_rewrite_stage_source_metatype_paths() {
    local stage_cmake_dir="$1"
    local sysroot="$2"
    local source_root="$3"
    find "${stage_cmake_dir}" -name '*.cmake' -exec sed -i \
        "s|/usr/src|${source_root}/src|g" {} + 2>/dev/null || true
    find "${stage_cmake_dir}" -name '*.cmake' -exec sed -i \
        "s|${source_root}/src/.*/meta_types/|${sysroot}/metatypes/|g" {} + 2>/dev/null || true
}

redbear_qt_rewrite_usr_src_metatype_paths() {
    local stage_cmake_dir="$1"
    local sysroot="$2"
    find "${stage_cmake_dir}" -name '*.cmake' -exec sed -i \
        "s|/usr/src/.*/meta_types/|${sysroot}/metatypes/|g" {} + 2>/dev/null || true
}

redbear_qt_rewrite_stage_path_literal() {
    local stage_cmake_dir="$1"
    local from_path="$2"
    local to_path="$3"
    find "${stage_cmake_dir}" -name '*.cmake' -exec sed -i \
        "s|${from_path}|${to_path}|g" {} + 2>/dev/null || true
}

redbear_qt_copy_stage_qt6_cmake_to_sysroot() {
    local stage_usr="$1"
    local sysroot="$2"
    mkdir -p "${sysroot}/lib/cmake"
    cp -a "${stage_usr}/lib/cmake/Qt6"* "${sysroot}/lib/cmake/" 2>/dev/null || true
}

redbear_qt_copy_optional_stage_dir_to_sysroot() {
    local stage_usr="$1"
    local sysroot="$2"
    local dir_name="$3"
    if [ -d "${stage_usr}/${dir_name}" ]; then
        mkdir -p "${sysroot}/${dir_name}"
        cp -a "${stage_usr}/${dir_name}/"* "${sysroot}/${dir_name}/" 2>/dev/null || true
    fi
}
