# Configuration file with the commands configuration of the Red Bear OS image

$(BUILD)/harddrive.img: $(FSTOOLS) $(REPO_TAG)
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(BUILD)
	-$(FUMOUNT) $(MOUNT_DIR) || true
	-$(FUMOUNT) /tmp/redox_installer || true
	rm -rf $@  $@.partial $(MOUNT_DIR)
	FILESYSTEM_SIZE=$(FILESYSTEM_SIZE) && \
	if [ -z "$$FILESYSTEM_SIZE" ] ; then \
	FILESYSTEM_SIZE=$(shell $(INSTALLER) --filesystem-size -c $(FILESYSTEM_CONFIG)); \
	fi && \
	truncate -s "$$FILESYSTEM_SIZE"m $@.partial
	umask 002 && $(INSTALLER) $(INSTALLER_OPTS) -c $(FILESYSTEM_CONFIG) $@.partial
	mv $@.partial $@
endif

$(LIVE_IMG): $(FSTOOLS) $(REPO_TAG)
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(LIVE_BUILD)
	rm -rf $@  $@.partial
	-$(FUMOUNT) /tmp/redox_installer || true
	FILESYSTEM_SIZE=$(FILESYSTEM_SIZE) && \
	if [ -z "$$FILESYSTEM_SIZE" ] ; then \
		FILESYSTEM_SIZE=$(shell $(INSTALLER) --filesystem-size -c $(FILESYSTEM_CONFIG)); \
	fi && \
	truncate -s "$$FILESYSTEM_SIZE"m $@.partial
	umask 002 && $(INSTALLER) $(INSTALLER_OPTS) -c $(FILESYSTEM_CONFIG) --write-bootloader="$(LIVE_BOOTLOADER)" --live $@.partial
	mv $@.partial $@
endif

$(LIVE_ISO): $(LIVE_IMG) redbear.ipxe
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(LIVE_BUILD)
	rm -rf $@ $@.partial
	tmpdir="$$(mktemp -d)"; \
	esp_img="$$tmpdir/efiboot.img"; \
	trap 'rm -rf "$$tmpdir"' EXIT; \
	mkdir -p "$$tmpdir/EFI/BOOT"; \
	BOOTLOADER_LIVE_BIOS=""; \
	for path in recipes/core/bootloader/target/*/stage/usr/lib/boot/bootloader-live.bios repo/*/*/bootloader/*/usr/lib/boot/bootloader-live.bios; do \
		if [ -f "$$path" ]; then \
			BOOTLOADER_LIVE_BIOS="$$path"; \
			break; \
		fi; \
	done; \
	live_size="$$(stat -c%s "$(LIVE_IMG)")"; \
	esp_size="$$((live_size + 64 * 1024 * 1024))"; \
	truncate -s "$$esp_size" "$$esp_img"; \
	mkfs.fat -F 32 "$$esp_img" >/dev/null; \
	python3 local/scripts/fat_tool.py mkdir "$$esp_img" 0 EFI; \
	python3 local/scripts/fat_tool.py mkdir "$$esp_img" 0 EFI/BOOT; \
	python3 local/scripts/fat_tool.py cp-in "$$esp_img" 0 "$(LIVE_BOOTLOADER)" EFI/BOOT/BOOTX64.EFI; \
	python3 local/scripts/fat_tool.py cp-in "$$esp_img" 0 "$(LIVE_IMG)" redox-live.iso; \
	cp "$(LIVE_BOOTLOADER)" "$$tmpdir/EFI/BOOT/BOOTX64.EFI"; \
	cp redbear.ipxe "$$tmpdir/redbear.ipxe"; \
	if [ -n "$$BOOTLOADER_LIVE_BIOS" ]; then \
		cp "$$BOOTLOADER_LIVE_BIOS" "$$tmpdir/bootloader-live.bios"; \
		xorriso -as mkisofs -R -J -V "REDBEARLIVE" -o $@.partial \
			-b bootloader-live.bios -no-emul-boot \
			-eltorito-alt-boot -e efiboot.img -no-emul-boot \
			"$$tmpdir" >/dev/null; \
	else \
		xorriso -as mkisofs -R -J -V "REDBEARLIVE" -o $@.partial \
			-eltorito-alt-boot -e efiboot.img -no-emul-boot \
			"$$tmpdir" >/dev/null; \
	fi
	mv $@.partial $@
	cp redbear.ipxe $(LIVE_IPXE)
endif

$(BUILD)/filesystem.img: $(FSTOOLS) $(REPO_TAG)
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(BUILD)
	-$(FUMOUNT) $(MOUNT_DIR) || true
	rm -rf $@  $@.partial $(MOUNT_DIR)
	-$(FUMOUNT) /tmp/redox_installer || true
	FILESYSTEM_SIZE=$(FILESYSTEM_SIZE) && \
	if [ -z "$$FILESYSTEM_SIZE" ] ; then \
	FILESYSTEM_SIZE=$(shell $(INSTALLER) --filesystem-size -c $(FILESYSTEM_CONFIG)); \
	fi && \
	truncate -s "$$FILESYSTEM_SIZE"m $@.partial
	$(REDOXFS_MKFS) $(REDOXFS_MKFS_FLAGS) $@.partial
	mkdir -p $(MOUNT_DIR)
	$(REDOXFS) $@.partial $(MOUNT_DIR)
	sleep 1
	pgrep redoxfs
	umask 002 && $(INSTALLER) $(INSTALLER_OPTS) -c $(FILESYSTEM_CONFIG) $(MOUNT_DIR)
	sync
	-$(FUMOUNT) $(MOUNT_DIR) || true
	rm -rf $(MOUNT_DIR)
	mv $@.partial $@
endif

mount: $(FSTOOLS) FORCE
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	@mkdir -p $(MOUNT_DIR)
	$(REDOXFS) $(BUILD)/harddrive.img $(MOUNT_DIR)
	@sleep 2
	@echo "\033[1;36;49mharddrive.img mounted ($$(pgrep redoxfs))\033[0m"
endif

mount_extra: $(FSTOOLS) FORCE
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	@mkdir -p $(MOUNT_DIR)
	$(REDOXFS) $(BUILD)/extra.img $(MOUNT_DIR)
	@sleep 2
	@echo "\033[1;36;49mextra.img mounted ($$(pgrep redoxfs))\033[0m"
endif

mount_live: $(FSTOOLS) FORCE
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	@mkdir -p $(MOUNT_DIR)
	$(REDOXFS) $(LIVE_IMG) $(MOUNT_DIR)
	@sleep 2
	@echo "\033[1;36;49m$(notdir $(LIVE_IMG)) mounted ($$(pgrep redoxfs))\033[0m"
endif

unmount: FORCE
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	@sync
	-$(FUMOUNT) $(MOUNT_DIR)
	@rm -rf $(MOUNT_DIR)
	@-$(FUMOUNT) /tmp/redox_installer 2>/dev/null || true
	@echo "\033[1;36;49mFilesystem unmounted\033[0m"
endif
