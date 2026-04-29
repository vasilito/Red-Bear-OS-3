#!/bin/bash

RED='\033[1;38;5;196m'
GREEN='\033[1;38;5;46m'
NC='\033[0m'

show_help() {
    echo "Usage: $(basename "$0") [OPTIONS]"
    echo ""
    echo "Description:"
    echo "  Wrapper for redoxer to run checks or tests on Redox OS targets."
    echo ""
    echo "Options:"
    echo "  --test              Run 'cargo test' instead of 'cargo check'"
    echo "  --all-target        Run the command on all supported Redox architectures"
    echo "  --target=<target>   Override the target architecture (e.g., i586-unknown-redox)"
    echo "  --arch=<arch>       Override the target architecture using arch (e.g., i586)"
    echo "  --help              Show this help message"
    echo ""
    echo "Supported Targets:"
    for t in "${SUPPORTED_TARGETS[@]}"; do
        echo "  - $t"
    done
    echo ""
    echo "Environment:"
    echo "  TARGET              Sets the default target (overridden by --target)"
}

if ! command -v redoxer &> /dev/null; then
    echo "Error: 'redoxer' CLI not found."
    echo "Please install it: cargo install redoxer"
    exit 1
fi

SUPPORTED_TARGETS=(
    "x86_64-unknown-redox"
    "i586-unknown-redox"
    "aarch64-unknown-redox"
    "riscv64gc-unknown-redox"
)

CURRENT_TARGET="${TARGET:-x86_64-unknown-redox}"
CHECK_ALL=false
CMD_ACTION="all"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --all-target)
            CHECK_ALL=true
            ;;
        --test)
            CMD_ACTION="test"
            ;;
        --target=*)
            CURRENT_TARGET="${1#*=}"
            ;;
        --arch=*)
            CURRENT_TARGET="${1#*=}-unknown-redox"
            ;;
        --help)
            show_help
            exit 0
            ;;
        *)
            echo -e "${RED}Error: Unknown option '$1'${NC}"
            show_help
            exit 1
            ;;
    esac
    shift
done

run_redoxer() {
    export TARGET=$1
    redoxer toolchain || { echo -e "${RED}Fail: redoxer toolchain for: $target.${NC}" && exit 1; }

    echo "----------------------------------------"
    echo "Running make $CMD_ACTION for: $TARGET"

    if make "$CMD_ACTION"; then
        return 0
    else
        echo -e "${RED}Fail: $CMD_ACTION $TARGET failed.${NC}"
        return 1
    fi
}

if [ "$CHECK_ALL" = true ]; then
    echo "Running $CMD_ACTION for all supported Redox targets..."

    has_error=false

    for target in "${SUPPORTED_TARGETS[@]}"; do
        if ! run_redoxer "$target"; then
            has_error=true
        fi
    done

    echo "----------------------------------------"
    if [ "$has_error" = true ]; then
        echo -e "${RED}Summary: One or more targets failed.${NC}"
        exit 1
    else
        echo -e "${GREEN}Summary: All targets passed!${NC}"
        exit 0
    fi
else
    if run_redoxer "$CURRENT_TARGET"; then
        echo -e "${GREEN}Success: $CMD_ACTION $CURRENT_TARGET passed.${NC}"
        exit 0
    else
        exit 1
    fi
fi
