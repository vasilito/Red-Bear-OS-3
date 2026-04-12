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
