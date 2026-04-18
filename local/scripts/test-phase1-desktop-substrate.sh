#!/usr/bin/env bash
# Validate the Red Bear OS Phase 1 desktop substrate (CONSOLE-TO-KDE-DESKTOP-PLAN v2.0).
#
# Modes:
#   --guest            Run inside a Red Bear OS guest
#   --qemu [CONFIG]    Boot CONFIG in QEMU and run the same checks automatically

set -euo pipefail

find_uefi_firmware() {
    local candidates=(
        "/usr/share/ovmf/x64/OVMF.4m.fd"
        "/usr/share/OVMF/x64/OVMF.4m.fd"
        "/usr/share/ovmf/x64/OVMF_CODE.4m.fd"
        "/usr/share/OVMF/x64/OVMF_CODE.4m.fd"
        "/usr/share/ovmf/OVMF.fd"
        "/usr/share/OVMF/OVMF_CODE.fd"
        "/usr/share/qemu/edk2-x86_64-code.fd"
    )
    local path
    for path in "${candidates[@]}"; do
        if [[ -f "$path" ]]; then
            printf '%s\n' "$path"
            return 0
        fi
    done
    return 1
}

run_guest_checks() {
    echo "=== Red Bear OS Phase 1 Desktop Substrate Test ==="
    echo

    local failures=0

    require_path() {
        local path="$1"
        local message="$2"
        if [ -e "$path" ]; then
            echo "  PASS  $message"
        else
            echo "  FAIL  $message"
            failures=$((failures + 1))
        fi
    }

    require_command() {
        local cmd="$1"
        local message="$2"
        if command -v "$cmd" >/dev/null 2>&1; then
            echo "  PASS  $message"
        else
            echo "  FAIL  $message"
            failures=$((failures + 1))
        fi
    }

    echo "--- relibc POSIX API surface ---"
    require_path /usr/include/sys/signalfd.h "sys/signalfd.h header present"
    require_path /usr/include/sys/timerfd.h "sys/timerfd.h header present"
    require_path /usr/include/sys/eventfd.h "sys/eventfd.h header present"
    require_path /usr/lib/libwayland-client.so "libwayland-client.so present (relibc consumer)"
    require_command wayland-scanner "wayland-scanner is installed"
    echo

    echo "--- evdevd input path ---"
    require_command evdevd "evdevd command is installed"
    require_path /scheme/evdev "/scheme/evdev exists"
    if command -v redbear-phase3-input-check >/dev/null 2>&1; then
        echo "  NOTE  redbear-phase3-input-check available (run manually for full input validation)"
    fi
    echo

    echo "--- udev-shim device enumeration ---"
    require_command udev-shim "udev-shim command is installed"
    require_path /scheme/udev "/scheme/udev exists"
    local libinput_found=false
    for lib in /usr/lib/libinput.so /usr/lib/libinput.so.10 /usr/lib/libinput.so.*; do
        if [ -e "$lib" ]; then
            libinput_found=true
            break
        fi
    done
    if $libinput_found; then
        echo "  PASS  libinput shared library present"
    else
        echo "  FAIL  libinput shared library not found"
        failures=$((failures + 1))
    fi
    echo

    echo "--- firmware-loader ---"
    require_path /scheme/firmware "/scheme/firmware exists"
    require_path /lib/firmware "/lib/firmware directory exists"
    echo

    echo "--- DRM/KMS ---"
    local drm_found=false
    if [ -e /usr/bin/redox-drm ] || command -v redox-drm >/dev/null 2>&1; then
        drm_found=true
    fi
    if $drm_found; then
        echo "  PASS  redox-drm is installed"
    else
        echo "  FAIL  redox-drm not found"
        failures=$((failures + 1))
    fi
    if [ -e /scheme/drm ]; then
        echo "  PASS  /scheme/drm exists"
    else
        echo "  FAIL  /scheme/drm does not exist"
        failures=$((failures + 1))
    fi
    if command -v redbear-drm-display-check >/dev/null 2>&1; then
        echo "  NOTE  redbear-drm-display-check available (run manually for bounded display validation)"
    fi
    echo

    echo "--- health check summary ---"
    if command -v redbear-info >/dev/null 2>&1; then
        local report
        report="$(redbear-info --json 2>/dev/null || true)"
        if [ -n "$report" ]; then
            local net_ok=false
            case "$report" in
                *'"networking"'*|*'"virtio_net_present"'*|*'"ip"'*) net_ok=true ;;
            esac
            if $net_ok; then
                echo "  PASS  networking state reported in redbear-info"
            else
                echo "  FAIL  networking state not reported in redbear-info"
                failures=$((failures + 1))
            fi
            local drm_reported=false
            case "$report" in
                *'scheme drm'*|*'/scheme/drm'*|*'"drm"'*) drm_reported=true ;;
            esac
            if $drm_reported; then
                echo "  PASS  DRM scheme reported in redbear-info"
            else
                echo "  FAIL  DRM scheme not reported in redbear-info"
                failures=$((failures + 1))
            fi
            local fw_reported=false
            case "$report" in
                *'scheme firmware'*|*'/scheme/firmware'*|*'"firmware"'*) fw_reported=true ;;
            esac
            if $fw_reported; then
                echo "  PASS  firmware scheme reported in redbear-info"
            else
                echo "  FAIL  firmware scheme not reported in redbear-info"
                failures=$((failures + 1))
            fi
        else
            echo "  FAIL  redbear-info --json returned empty"
            failures=$((failures + 1))
        fi
    else
        echo "  FAIL  redbear-info is not installed"
        failures=$((failures + 1))
    fi
    echo

    echo "=== Phase 1 Desktop Substrate Test Complete ==="
    if [ "$failures" -gt 0 ]; then
        echo "  $failures check(s) FAILED"
        return 1
    fi
    echo "  All checks PASSED"
    return 0
}

run_qemu_checks() {
    local config="$1"
    local firmware
    firmware="$(find_uefi_firmware)" || {
        echo "ERROR: no usable x86_64 UEFI firmware found" >&2
        exit 1
    }

    local arch image extra
    arch="${ARCH:-$(uname -m)}"
    image="build/$arch/$config/harddrive.img"
    extra="build/$arch/$config/extra.img"

    if [[ ! -f "$image" ]]; then
        echo "ERROR: missing image $image" >&2
        echo "Build it first with: ./local/scripts/build-redbear.sh $config" >&2
        exit 1
    fi

    if [[ ! -f "$extra" ]]; then
        truncate -s 1g "$extra"
    fi

    expect <<EOF
log_user 1
set timeout 300
spawn qemu-system-x86_64 -name {Red Bear OS x86_64} -device qemu-xhci -smp 4 -m 2048 -bios $firmware -chardev stdio,id=debug,signal=off,mux=on -serial chardev:debug -mon chardev=debug -machine q35 -device ich9-intel-hda -device hda-output -device virtio-net,netdev=net0 -netdev user,id=net0 -object filter-dump,id=f1,netdev=net0,file=build/$arch/$config/network.pcap -nographic -vga none -drive file=$image,format=raw,if=none,id=drv0 -device nvme,drive=drv0,serial=NVME_SERIAL -drive file=$extra,format=raw,if=none,id=drv1 -device nvme,drive=drv1,serial=NVME_EXTRA -enable-kvm -cpu host
expect "login:"
send "root\r"
expect "assword:"
send "password\r"
expect "Type 'help' for available commands."
send "echo __READY__\r"
expect "__READY__"
send "test -e /usr/include/sys/signalfd.h && echo __SIGNAFD_OK__ || echo __SIGNAFD_FAIL__\r"
expect {
    "__SIGNAFD_OK__" { }
    "__SIGNAFD_FAIL__" { puts "FAIL: signalfd header missing"; exit 1 }
}
send "test -e /usr/include/sys/timerfd.h && echo __TIMERFD_OK__ || echo __TIMERFD_FAIL__\r"
expect {
    "__TIMERFD_OK__" { }
    "__TIMERFD_FAIL__" { puts "FAIL: timerfd header missing"; exit 1 }
}
send "test -e /usr/include/sys/eventfd.h && echo __EVENTFD_OK__ || echo __EVENTFD_FAIL__\r"
expect {
    "__EVENTFD_OK__" { }
    "__EVENTFD_FAIL__" { puts "FAIL: eventfd header missing"; exit 1 }
}
send "test -e /usr/lib/libwayland-client.so && echo __WAYLAND_LIB_OK__ || echo __WAYLAND_LIB_FAIL__\r"
expect {
    "__WAYLAND_LIB_OK__" { }
    "__WAYLAND_LIB_FAIL__" { puts "FAIL: libwayland-client missing"; exit 1 }
}
send "command -v evdevd && echo __EVDVD_OK__ || echo __EVDVD_FAIL__\r"
expect {
    "__EVDVD_OK__" { }
    "__EVDVD_FAIL__" { puts "FAIL: evdevd missing"; exit 1 }
}
send "test -e /scheme/evdev && echo __EVDEV_SCH_OK__ || echo __EVDEV_SCH_FAIL__\r"
expect {
    "__EVDEV_SCH_OK__" { }
    "__EVDEV_SCH_FAIL__" { puts "FAIL: /scheme/evdev missing"; exit 1 }
}
send "command -v udev-shim && echo __UDEV_OK__ || echo __UDEV_FAIL__\r"
expect {
    "__UDEV_OK__" { }
    "__UDEV_FAIL__" { puts "FAIL: udev-shim missing"; exit 1 }
}
send "test -e /scheme/udev && echo __UDEV_SCH_OK__ || echo __UDEV_SCH_FAIL__\r"
expect {
    "__UDEV_SCH_OK__" { }
    "__UDEV_SCH_FAIL__" { puts "FAIL: /scheme/udev missing"; exit 1 }
}
send "test -e /scheme/firmware && echo __FW_SCH_OK__ || echo __FW_SCH_FAIL__\r"
expect {
    "__FW_SCH_OK__" { }
    "__FW_SCH_FAIL__" { puts "FAIL: /scheme/firmware missing"; exit 1 }
}
send "test -e /lib/firmware && echo __FW_DIR_OK__ || echo __FW_DIR_FAIL__\r"
expect {
    "__FW_DIR_OK__" { }
    "__FW_DIR_FAIL__" { puts "FAIL: /lib/firmware missing"; exit 1 }
}
send "command -v redox-drm && echo __DRM_OK__ || echo __DRM_FAIL__\r"
expect {
    "__DRM_OK__" { }
    "__DRM_FAIL__" { puts "FAIL: redox-drm missing"; exit 1 }
}
send "test -e /scheme/drm && echo __DRM_SCH_OK__ || echo __DRM_SCH_FAIL__\r"
expect {
    "__DRM_SCH_OK__" { }
    "__DRM_SCH_FAIL__" { puts "FAIL: /scheme/drm missing"; exit 1 }
}
send "redbear-info --json\r"
expect "\"virtio_net_present\": true"
expect "scheme firmware is registered"
expect "scheme udev is registered"
send "echo __PHASE1_DONE__\r"
expect "__PHASE1_DONE__"
send "shutdown\r"
expect eof
EOF
}

usage() {
    cat <<'USAGE'
Usage:
  ./local/scripts/test-phase1-desktop-substrate.sh --guest
  ./local/scripts/test-phase1-desktop-substrate.sh --qemu [redbear-wayland]
USAGE
}

case "${1:-}" in
    --guest)
        run_guest_checks
        ;;
    --qemu)
        run_qemu_checks "${2:-redbear-wayland}"
        ;;
    *)
        usage
        exit 1
        ;;
esac
