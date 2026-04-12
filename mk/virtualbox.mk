# Configuration file for VirtualBox, it creates a VirtualBox virtual machine

virtualbox: $(BUILD)/harddrive.img
	echo "Delete VM"
	-$(VBM) unregistervm RedBearOS --delete; \
	if [ $$? -ne 0 ]; \
	then \
		if [ -d "$$HOME/VirtualBox VMs/RedBearOS" ]; \
		then \
			echo "RedBearOS directory exists, deleting..."; \
			$(RM) -rf "$$HOME/VirtualBox VMs/RedBearOS"; \
		fi \
	fi
	echo "Delete Disk"
	-$(RM) harddrive.vdi
	echo "Create VM"
	$(VBM) createvm --name RedBearOS --register
	echo "Set Configuration"
	$(VBM) modifyvm RedBearOS --memory 2048
	$(VBM) modifyvm RedBearOS --vram 32
	if [ "$(net)" != "no" ]; \
	then \
		$(VBM) modifyvm RedBearOS --nic1 nat; \
		$(VBM) modifyvm RedBearOS --nictype1 82540EM; \
		$(VBM) modifyvm RedBearOS --cableconnected1 on; \
		$(VBM) modifyvm RedBearOS --nictrace1 on; \
		$(VBM) modifyvm RedBearOS --nictracefile1 "$(ROOT)/$(BUILD)/network.pcap"; \
	fi
	$(VBM) modifyvm RedBearOS --uart1 0x3F8 4
	$(VBM) modifyvm RedBearOS --uartmode1 file "$(ROOT)/$(BUILD)/serial.log"
	$(VBM) modifyvm RedBearOS --usb off # on
	$(VBM) modifyvm RedBearOS --keyboard ps2
	$(VBM) modifyvm RedBearOS --mouse ps2
	$(VBM) modifyvm RedBearOS --audio-driver $(VB_AUDIO)
	$(VBM) modifyvm RedBearOS --audiocontroller hda
	$(VBM) modifyvm RedBearOS --audioout on
	$(VBM) modifyvm RedBearOS --nestedpaging on
	echo "Create Disk"
	$(VBM) convertfromraw $< $(BUILD)/harddrive.vdi
	echo "Attach Disk"
	$(VBM) storagectl RedBearOS --name ATA --add sata --controller IntelAHCI --bootable on --portcount 1
	$(VBM) storageattach RedBearOS --storagectl ATA --port 0 --device 0 --type hdd --medium $(BUILD)/harddrive.vdi
	echo "Run VM"
	$(VBM) startvm RedBearOS
