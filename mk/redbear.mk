# Red Bear OS integration — runs before repo cook and disk image creation
# Ensures all custom recipe symlinks, patches, assets, and firmware are staged.

REDBEAR_TAG=$(BUILD)/redbear.tag

$(REDBEAR_TAG): FORCE
ifeq ($(PODMAN_BUILD),1)
	$(PODMAN_RUN) make $@
else
	bash -c 'export REDBEAR_TAG="$$1"; exec bash local/scripts/integrate-redbear.sh' _ '$(REDBEAR_TAG)'
endif

redbear: $(REDBEAR_TAG)

redbear_clean:
	rm -f "$(REDBEAR_TAG)"

# Source archival — exports fully-patched, versioned source archives
# for all recipes with source/ directories to sources/<target>/
# Runs after `make all` and also standalone via `make sources`
SOURCES_DIR=$(BUILD)/../sources/$(TARGET)
SOURCES_TAG=$(SOURCES_DIR)/.sources-tag

# Standalone: archive what's cached (no rebuild needed)
sources:
	@echo "Archiving fully-patched source packages..."
	bash local/scripts/archive-sources.sh --all
	@mkdir -p "$(SOURCES_DIR)"
	@touch "$(SOURCES_TAG)"
	@echo "Sources archived: $$(wc -l < $(SOURCES_DIR)/packages.txt 2>/dev/null || echo 0) packages"

# Hook: run after full build
$(BUILD)/harddrive.img: sources

FORCE:
