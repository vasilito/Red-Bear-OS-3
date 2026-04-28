# This file contains the build system commands configuration
# and environment variables
include mk/config.mk

# Build system dependencies
include mk/depends.mk

all: $(BUILD)/harddrive.img

# ── Red Bear OS Build Cache (OBLIGATORY) ─────────────────────────────────
# Cache sync is a mandatory part of every successful build.
# The git-tracked cache survives make clean, make distclean, and git clone.
#
# Flow:
#   make all → cache-restore (if needed) → build → cache-sync → cache-commit
#
# Commands:
#   make cache-sync         Sync built → git cache (manual)
#   make cache-commit       Sync + git commit (manual)
#   make cache-restore      Restore from git cache
#   make cache-status       Compare cache vs build state

CACHE_SYNC  = local/scripts/cache-sync.sh
CACHE_SAVE  = local/scripts/snapshot-cache.sh
CACHE_RESTORE = local/scripts/restore-cache.sh

cache-sync:
	@bash $(CACHE_SYNC)

cache-commit:
	@bash $(CACHE_SYNC) --commit

cache-restore:
	@echo "Red Bear: restoring from git-tracked cache..."
	@bash $(CACHE_SYNC) --restore
	@bash $(CACHE_RESTORE) 2>/dev/null || true

cache-save:
	@bash $(CACHE_SAVE)

cache-save-essential:
	@bash $(CACHE_SAVE) --essential

cache-verify:
	@bash $(CACHE_RESTORE) --verify

cache-list:
	@bash $(CACHE_SAVE) --list

cache-status:
	@bash $(CACHE_SYNC) --status

# Obligatory cache pipeline — runs before AND after every build
cache-auto:
	@# ── BEFORE BUILD: restore cache if target is empty ──
	@if [ ! -f $(BUILD)/repo.tag ]; then \
		if ls local/cache/pkgar/*/stage.pkgar >/dev/null 2>&1; then \
			echo "Red Bear: restoring build cache..."; \
			bash $(CACHE_SYNC) --restore; \
		fi; \
	fi
	@# ── AFTER BUILD: sync cache back to git-tracked storage ──
	@if [ -f $(BUILD)/harddrive.img ]; then \
		echo "Red Bear: syncing build cache..."; \
		bash $(CACHE_SYNC); \
		echo "Red Bear: committing cache to git..."; \
		bash $(CACHE_SYNC) --commit 2>/dev/null || echo "Red Bear: cache commit skipped (no changes or not in git repo)"; \
	fi

$(BUILD)/harddrive.img: cache-auto

live:
	-$(FUMOUNT) $(BUILD)/filesystem/ || true
	-$(FUMOUNT) /tmp/redbear_installer/ || true
	rm -f $(LIVE_ISO) $(LIVE_IMG) $(LIVE_BOOTLOADER) $(LIVE_IPXE)
	$(MAKE) $(LIVE_ISO)

popsicle: $(LIVE_ISO)
	popsicle-gtk $(LIVE_ISO)

image:
	-$(FUMOUNT) $(BUILD)/filesystem/ || true
	-$(FUMOUNT) /tmp/redbear_installer/ || true
	rm -f $(BUILD)/harddrive.img $(LIVE_ISO) $(LIVE_IMG) $(LIVE_BOOTLOADER) $(LIVE_IPXE)
	$(MAKE) all

rebuild:
	-$(FUMOUNT) $(BUILD)/filesystem/ || true
	-$(FUMOUNT) /tmp/redbear_installer/ || true
	rm -rf $(BUILD)/repo.tag $(BUILD)/harddrive.img $(LIVE_ISO) $(LIVE_IMG) $(LIVE_BOOTLOADER) $(LIVE_IPXE)
	$(MAKE) all

# To tell that it's not safe
# to execute the cookbook binary
NOT_ON_PODMAN?=0

clean:
ifeq ($(PODMAN_BUILD),1)
ifneq ("$(wildcard $(CONTAINER_TAG))","")
	$(PODMAN_RUN) make $@
else
	$(info will not run cookbook clean as container is not built)
	$(MAKE) clean PODMAN_BUILD=0 NOT_ON_PODMAN=1 SKIP_CHECK_TOOLS=1
endif # CONTAINER_TAG
else
ifneq ($(NOT_ON_PODMAN),1)
	$(MAKE) repo_clean
	-$(FUMOUNT) $(BUILD)/filesystem/ || true
	-$(FUMOUNT) /tmp/redbear_installer/ || true
endif # NOT_ON_PODMAN
	rm -rf repo
	rm -rf $(BUILD) $(PREFIX)
	$(MAKE) fstools_clean
endif # PODMAN_BUILD

# distclean: removes build artifacts, toolchain, and upstream source trees.
# local/ overlay source trees are PROTECTED — the repo binary refuses to
# unfetch local overlay recipes unless REDBEAR_ALLOW_LOCAL_UNFETCH=1 is set.
# This is the safe default for Red Bear OS. local/ is NEVER deleted.
distclean:
ifeq ($(PODMAN_BUILD),1)
ifneq ("$(wildcard $(CONTAINER_TAG))","")
	$(PODMAN_RUN) make $@
else
	$(info will not run cookbook unfetch as container is not built)
	$(MAKE) distclean PODMAN_BUILD=0 NOT_ON_PODMAN=1 SKIP_CHECK_TOOLS=1
endif # CONTAINER_TAG
else
ifneq ($(NOT_ON_PODMAN),1)
	$(info ==> distclean: cleaning build artifacts and upstream source trees)
	$(info ==> local/ overlay sources are PROTECTED and will NOT be deleted)
	$(MAKE) fetch_clean
endif # NOT_ON_PODMAN
	$(MAKE) clean NOT_ON_PODMAN=1
endif # PODMAN_BUILD

# distclean-nuclear: DESTRUCTIVE — also deletes local/ overlay source trees.
# This is the OLD distclean behavior that can destroy Red Bear work.
# You must set REDBEAR_ALLOW_LOCAL_UNFETCH=1 for this to actually delete
# local overlay sources. Without it, the repo binary still protects them.
# Use ONLY when you are certain you want to discard local overlay source code.
distclean-nuclear:
ifeq ($(PODMAN_BUILD),1)
	$(info distclean-nuclear is not supported in Podman mode; use native build)
else
	$(warning !! distclean-nuclear will attempt to DELETE ALL source trees including local/ overlays)
	$(warning !! This can destroy Red Bear OS work that is not committed to git)
	$(warning !! The repo binary still protects local overlays unless REDBEAR_ALLOW_LOCAL_UNFETCH=1)
	$(MAKE) fetch_clean REDBEAR_ALLOW_LOCAL_UNFETCH=1
	$(MAKE) clean NOT_ON_PODMAN=1
endif # PODMAN_BUILD

pull:
	git pull
	rm -f $(FSTOOLS_TAG)

repo: $(BUILD)/repo.tag

repo_clean: c.--all

fetch_clean: u.--all

# Podman build recipes and vars
include mk/podman.mk

# Disk Imaging and Cookbook tools
include mk/fstools.mk

# Cross compiler recipes
include mk/prefix.mk

# Repository maintenance
include mk/repo.mk

# Disk images
include mk/disk.mk

# Emulation recipes
include mk/qemu.mk
include mk/virtualbox.mk

# CI
include mk/ci.mk

include mk/redbear.mk

# Ensure Red Bear OS integration runs before repo cook and disk image creation
$(BUILD)/harddrive.img: $(REDBEAR_TAG)
$(LIVE_ISO): $(REDBEAR_TAG)
$(REPO_TAG): $(REDBEAR_TAG)

env: prefix FORCE $(CONTAINER_TAG)
ifeq ($(PODMAN_BUILD),1)
	$(PODMAN_RUN) make $@
else
	export PATH="$(PREFIX_PATH):$$PATH" && \
	bash
endif

setenv: FORCE
	@echo export ARCH='$(ARCH)'
	@echo export BOARD='$(BOARD)'
	@echo export CONFIG_NAME='$(CONFIG_NAME)'
	@echo BUILD='$(BUILD)'

export RUST_GDB=gdb-multiarch # Necessary when debugging for another architecture than the host
GDB_KERNEL_FILE=recipes/core/kernel/target/$(TARGET)/build/kernel.sym
gdb: FORCE
	rust-gdb $(GDB_KERNEL_FILE) --eval-command="target remote :1234"

# This target allows debugging a userspace application without requiring gdbserver running inside
# the VM. Because gdb doesn't know when the userspace application is scheduled by the kernel and as
# it stops the entire VM rather than just the userspace application that the user wants to debug,
# connecting to a gdbserver running inside the VM is highly encouraged when possible. This target
# should only be used when the application to debug runs early during boot before the network stack
# has started or you need to debug the interaction between the application and the kernel.
# tl;dr: DO NOT USE THIS TARGET UNLESS YOU HAVE TO
gdb-userspace: FORCE
	rust-gdb $(GDB_APP_FILE) --eval-command="add-symbol-file $(GDB_KERNEL_FILE)" --eval-command="target remote :1234"

# An empty target
FORCE:

# Wireshark
wireshark: FORCE
	wireshark $(BUILD)/network.pcap
packages-sync: ; @bash local/scripts/sync-packages.sh
packages-list: ; @ls -la Packages/*.pkgar 2>/dev/null | wc -l && echo "pkgar files in Packages/"
