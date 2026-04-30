# Configuration file with the commands configuration of the Red Bear OS image

$(BUILD)/harddrive.img: $(FSTOOLS) $(REPO_TAG)
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(BUILD)
	$(FUMOUNT) $(MOUNT_DIR) 2>/dev/null || echo "Warning: failed to unmount $(MOUNT_DIR) (may not have been mounted)"
	$(FUMOUNT) /tmp/redox_installer 2>/dev/null || echo "Warning: failed to unmount /tmp/redox_installer (may not have been mounted)"
	rm -rf $@  $@.partial $(MOUNT_DIR)
	FILESYSTEM_SIZE=$(FILESYSTEM_SIZE) && \
	if [ -z "$$FILESYSTEM_SIZE" ] ; then \
	FILESYSTEM_SIZE=$(shell $(INSTALLER) --filesystem-size -c $(FILESYSTEM_CONFIG)); \
	fi && \
	truncate -s "$$FILESYSTEM_SIZE"m $@.partial
	umask 002 && $(INSTALLER) $(INSTALLER_OPTS) --no-mount -c $(FILESYSTEM_CONFIG) $@.partial
	mv $@.partial $@
endif

$(LIVE_IMG): $(FSTOOLS) $(REPO_TAG) sources
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(LIVE_BUILD)
	rm -rf $@ $@.partial
	$(FUMOUNT) /tmp/redox_installer 2>/dev/null || echo "Warning: failed to unmount /tmp/redox_installer (may not have been mounted)"
	FILESYSTEM_SIZE=$(FILESYSTEM_SIZE) && \
	if [ -z "$$FILESYSTEM_SIZE" ] ; then \
		FILESYSTEM_SIZE=$(shell $(INSTALLER) --filesystem-size -c $(FILESYSTEM_CONFIG)); \
	fi && \
	truncate -s "$$FILESYSTEM_SIZE"m $@.partial
	umask 002 && $(INSTALLER) $(INSTALLER_OPTS) --no-mount -c $(FILESYSTEM_CONFIG) --write-bootloader="$(LIVE_BOOTLOADER)" --live $@.partial
	mv $@.partial $@
endif

$(LIVE_ISO): $(LIVE_IMG) redbear.ipxe
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(LIVE_BUILD)
	rm -rf $@ $@.partial
	cp "$(LIVE_IMG)" $@.partial
	mv $@.partial $@
	cp redbear.ipxe $(LIVE_IPXE)
endif

$(BUILD)/filesystem.img: $(FSTOOLS) $(REPO_TAG)
ifeq ($(FSTOOLS_IN_PODMAN),1)
	$(PODMAN_RUN) make $@
else
	mkdir -p $(BUILD)
	$(FUMOUNT) $(MOUNT_DIR) 2>/dev/null || echo "Warning: failed to unmount $(MOUNT_DIR) (may not have been mounted)"
	rm -rf $@  $@.partial $(MOUNT_DIR)
	$(FUMOUNT) /tmp/redox_installer 2>/dev/null || echo "Warning: failed to unmount /tmp/redox_installer (may not have been mounted)"
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
	$(FUMOUNT) $(MOUNT_DIR) 2>/dev/null || echo "Warning: failed to unmount $(MOUNT_DIR) after install"
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
	$(FUMOUNT) $(MOUNT_DIR) 2>/dev/null || echo "Warning: failed to unmount $(MOUNT_DIR)"
	@rm -rf $(MOUNT_DIR)
	@$(FUMOUNT) /tmp/redox_installer 2>/dev/null || echo "Warning: failed to unmount /tmp/redox_installer"
	@echo "\033[1;36;49mFilesystem unmounted\033[0m"
endif
