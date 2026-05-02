#!/usr/bin/env bash

redbear_project_root() {
    if [ -n "${PROJECT_ROOT:-}" ]; then
        printf '%s\n' "${PROJECT_ROOT}"
        return 0
    fi
    if [ -n "${COOKBOOK_ROOT:-}" ]; then
        printf '%s\n' "${COOKBOOK_ROOT}"
        return 0
    fi
    return 1
}

redbear_choose_toolchain_root() {
    if [ -n "${COOKBOOK_HOST_SYSROOT:-}" ] && [ -d "${COOKBOOK_HOST_SYSROOT}" ]; then
        printf '%s\n' "${COOKBOOK_HOST_SYSROOT}"
        return 0
    fi
    if [ -d "${HOME}/.redoxer/x86_64-unknown-redox/toolchain" ]; then
        printf '%s\n' "${HOME}/.redoxer/x86_64-unknown-redox/toolchain"
        return 0
    fi
    printf '%s\n' "$(redbear_project_root)/prefix/x86_64-unknown-redox/sysroot"
}

redbear_relibc_target_dir() {
    printf '%s\n' "$(redbear_project_root)/recipes/core/relibc/target/x86_64-unknown-redox"
}

redbear_relibc_stage_include_dir() {
    local relibc_target
    relibc_target="$(redbear_relibc_target_dir)"
    printf '%s\n' "${relibc_target}/stage/usr/include"
}

redbear_relibc_stage_lib_dir() {
    local relibc_target
    relibc_target="$(redbear_relibc_target_dir)"
    printf '%s\n' "${relibc_target}/stage/usr/lib"
}

redbear_choose_relibc_stage_include() {
    local relibc_target stage_include tmp_include
    relibc_target="$(redbear_relibc_target_dir)"
    stage_include="${relibc_target}/stage/usr/include"
    tmp_include="${relibc_target}/stage.tmp/usr/include"
    if [ -d "$stage_include" ]; then
        printf '%s\n' "$stage_include"
    elif [ -d "$tmp_include" ]; then
        printf '%s\n' "$tmp_include"
    fi
}

redbear_choose_relibc_stage_lib() {
    local relibc_target stage_lib tmp_lib build_lib candidate
    relibc_target="$(redbear_relibc_target_dir)"
    stage_lib="${relibc_target}/stage/usr/lib"
    tmp_lib="${relibc_target}/stage.tmp/usr/lib"
    build_lib="${relibc_target}/build/target/x86_64-unknown-redox/release"
    for candidate in "$stage_lib" "$tmp_lib" "$build_lib"; do
        if [ -f "$candidate/libc.so" ] && readelf -Ws "$candidate/libc.so" | grep -q '_Z7strtoldPKcPPc'; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    for candidate in "$stage_lib" "$build_lib" "$tmp_lib"; do
        if [ -d "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
}

redbear_copy_relibc_surface_into_sysroot() {
    local destination_sysroot="$1"
    local include_dir lib_dir
    include_dir="$(redbear_choose_relibc_stage_include)"
    lib_dir="$(redbear_choose_relibc_stage_lib)"

    if [ -n "$include_dir" ] && [ -d "$include_dir" ]; then
        mkdir -p "${destination_sysroot}/include"
        cp -a "${include_dir}/." "${destination_sysroot}/include/"
    fi
    if [ -n "$lib_dir" ] && [ -d "$lib_dir" ]; then
        mkdir -p "${destination_sysroot}/lib"
        cp -a "${lib_dir}/." "${destination_sysroot}/lib/"
    fi
}

redbear_relibc_surface_ready() {
    local relibc_target relibc_stage_include relibc_stage_lib
    relibc_target="$(redbear_relibc_target_dir)"
    relibc_stage_include="${relibc_target}/stage/usr/include"
    relibc_stage_lib="${relibc_target}/stage/usr/lib/libc.so"

    [ -f "${relibc_stage_include}/sys/signalfd.h" ] || return 1
    [ -f "${relibc_stage_include}/sys/timerfd.h" ] || return 1
    [ -f "${relibc_stage_include}/sys/eventfd.h" ] || return 1
    [ -f "${relibc_stage_include}/threads.h" ] || return 1
    [ -f "${relibc_stage_lib}" ] || return 1
    readelf -Ws "${relibc_stage_lib}" | grep -q '_Z7strtoldPKcPPc' || return 1
    return 0
}

redbear_sync_relibc_surface_to_toolchain() {
    local relibc_target relibc_stage_include relibc_stage_lib toolchain_sysroot
    relibc_target="$(redbear_relibc_target_dir)"
    relibc_stage_include="${relibc_target}/stage/usr/include"
    relibc_stage_lib="${relibc_target}/stage/usr/lib"
    toolchain_sysroot="$(redbear_choose_toolchain_root)"

    mkdir -p \
        "${toolchain_sysroot}/include" \
        "${toolchain_sysroot}/x86_64-unknown-redox/include" \
        "${toolchain_sysroot}/x86_64-unknown-redox/lib"

    cp -a "${relibc_stage_include}/." "${toolchain_sysroot}/include/"
    cp -a "${relibc_stage_include}/." "${toolchain_sysroot}/x86_64-unknown-redox/include/"
    cp -a "${relibc_stage_lib}/." "${toolchain_sysroot}/x86_64-unknown-redox/lib/"
}

redbear_ensure_relibc_desktop_surface() {
    local relibc_target
    relibc_target="$(redbear_relibc_target_dir)"

    if ! redbear_relibc_surface_ready; then
        echo ">>> Refreshing relibc staged surface for full desktop target..."
        rm -rf \
            "${relibc_target}/build" \
            "${relibc_target}/stage" \
            "${relibc_target}/stage.tmp" \
            "${relibc_target}/sysroot"
        rm -f \
            "${relibc_target}/auto_deps.toml" \
            "${relibc_target}/stage.pkgar" \
            "${relibc_target}/stage.toml"
        REPO_OFFLINE=1 COOKBOOK_OFFLINE=true CI=1 ./target/release/repo cook relibc
        echo ""
    fi

    redbear_sync_relibc_surface_to_toolchain
}
