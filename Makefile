# arcOS build.
#
#   make            build the bootable disk image (build/arcos.img)
#   make iso        build a live CD (build/arcos.iso), BIOS and UEFI
#   make run-iso    start the live CD in QEMU, with no hard disk
#   make run        start it in QEMU (PC with IDE disk)
#   make run-q35    start it in QEMU (modern PC: AHCI/SATA disk)
#   make run-nvme   start it in QEMU with the system disk on NVMe
#   make run-uefi   start it in QEMU with UEFI firmware (OVMF) instead of a BIOS
#   make test       boot headless and run a scripted session
#   make test-uefi  the same, started by UEFI firmware
#   make test-iso   the same session on the live CD, with BIOS and with UEFI
#   make test-drivers  boot once per emulated disk, network card and USB
#                   controller and check that its driver works
#   make test-arcscript  run ArcScript programs on arcOS and check them
#   make test-compat   run a Linux and a Windows program on arcOS and check them
#   make shots      boot headless, drive the GUI and save screenshots (build/shots)
#   make assets     regenerate fonts, icons and wallpapers (Python, Pillow, numpy)
#   make check      unit tests of the shared libraries and the host tool

PROFILE  ?= release
CARGO    ?= cargo
QEMU     ?= qemu-system-x86_64
MEMORY   ?= 512M
SIZE     ?= 128M
# UEFI firmware for QEMU: code (read-only) and a template of its variables.
OVMF_CODE ?= /usr/share/OVMF/OVMF_CODE_4M.fd
OVMF_VARS ?= /usr/share/OVMF/OVMF_VARS_4M.fd

BUILD    := build
TARGET   := system/target/x86_64-unknown-none/$(PROFILE)
ARCTOOL  := tools/target/release/arctool
EFI_APP  := boot/uefi/target/x86_64-unknown-uefi/release/BOOTX64.efi
DRIVERS  := $(notdir $(wildcard system/drivers/*))
PACKAGES := $(DRIVERS:%=$(BUILD)/drivers/%.adp)
APPS     := $(notdir $(wildcard system/apps/*))
ROOT     := $(BUILD)/root
IMAGE    := $(BUILD)/arcos.img
ISO      := $(BUILD)/arcos.iso
# Size of the live CD's system volume (it lives in memory).
LIVE_SIZE ?= 16M

ifeq ($(PROFILE),release)
CARGO_PROFILE := --release
endif

.PHONY: all uefi system tools root image iso run-iso test-iso run run-q35 run-nvme run-uefi test test-uefi test-drivers test-arcscript test-compat shots assets check clean $(PACKAGES)

all: image

$(BUILD)/stage1.bin: boot/bios/stage1.asm
	@mkdir -p $(BUILD)
	nasm -f bin -o $@ $<

$(BUILD)/stage2.bin: boot/bios/stage2.asm
	@mkdir -p $(BUILD)
	nasm -f bin -o $@ $<

uefi:
	cd boot/uefi && $(CARGO) build --release

system:
	cd system && $(CARGO) build $(CARGO_PROFILE) --workspace

tools:
	cd tools && $(CARGO) build --release

$(PACKAGES): $(BUILD)/drivers/%.adp: system tools
	@mkdir -p $(BUILD)/drivers
	$(ARCTOOL) pack system/drivers/$*/driver.manifest $(TARGET)/$* $@

# The files of the system volume: rootfs/ plus the programs.
root: system
	rm -rf $(ROOT)
	mkdir -p $(ROOT)/apps
	cp -r rootfs/. $(ROOT)/
	for app in $(APPS); do cp $(TARGET)/$$app $(ROOT)/apps/$$app.app; done

image: $(BUILD)/stage1.bin $(BUILD)/stage2.bin uefi system tools $(PACKAGES) root
	$(ARCTOOL) image $(IMAGE) --size $(SIZE) \
		--stage1 $(BUILD)/stage1.bin --stage2 $(BUILD)/stage2.bin --efi $(EFI_APP) \
		--kernel $(TARGET)/arc-kernel --drivers $(BUILD)/drivers --files $(ROOT)

iso: $(BUILD)/stage1.bin $(BUILD)/stage2.bin uefi system tools $(PACKAGES) root
	$(ARCTOOL) iso $(ISO) --size $(LIVE_SIZE) \
		--stage1 $(BUILD)/stage1.bin --stage2 $(BUILD)/stage2.bin --efi $(EFI_APP) \
		--kernel $(TARGET)/arc-kernel --drivers $(BUILD)/drivers --files $(ROOT)

QEMU_COMMON := -m $(MEMORY) -serial stdio -no-reboot -rtc base=localtime

run: image
	$(QEMU) $(QEMU_COMMON) -drive file=$(IMAGE),format=raw,if=ide

run-q35: image
	$(QEMU) $(QEMU_COMMON) -machine q35 -drive file=$(IMAGE),format=raw,if=none,id=sys -device ahci,id=ahci -device ide-hd,drive=sys,bus=ahci.0

run-nvme: image
	$(QEMU) $(QEMU_COMMON) -machine q35 -drive file=$(IMAGE),format=raw,if=none,id=sys -device nvme,drive=sys,serial=arc0001

# The firmware keeps its settings (boot entries) in build/ovmf-vars.fd.
run-iso: iso
	$(QEMU) $(QEMU_COMMON) -cdrom $(ISO)

run-uefi: image
	@test -f $(BUILD)/ovmf-vars.fd || cp $(OVMF_VARS) $(BUILD)/ovmf-vars.fd
	$(QEMU) $(QEMU_COMMON) -drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE) \
		-drive if=pflash,format=raw,file=$(BUILD)/ovmf-vars.fd -drive file=$(IMAGE),format=raw,if=ide

test: image
	scripts/test.sh $(IMAGE)

test-uefi: image
	UEFI=1 OVMF_CODE=$(OVMF_CODE) OVMF_VARS=$(OVMF_VARS) scripts/test.sh $(IMAGE)

test-drivers: image
	scripts/test-drivers.sh $(IMAGE)

test-iso: iso
	DISK=cdrom scripts/test.sh $(ISO)
	UEFI=1 DISK=cdrom OVMF_CODE=$(OVMF_CODE) OVMF_VARS=$(OVMF_VARS) scripts/test.sh $(ISO)

test-arcscript: image
	scripts/test-arcscript.sh $(IMAGE)

test-compat: image
	scripts/test-compat.sh $(IMAGE)

shots: image
	scripts/gui-shots.py $(IMAGE) scripts/gui/tour.txt $(BUILD)/shots

assets:
	tools/assets/gen_fonts.py system/kernel/assets/fonts.bin
	tools/assets/gen_icons.py rootfs/system/gui
	tools/assets/gen_wallpapers.py rootfs/system/gui/wallpapers

check:
	cd system && $(CARGO) test --target x86_64-unknown-linux-gnu -p arcdk -p arcfs -p arcc-core -p arc-vm
	cd tools && $(CARGO) test --release

clean:
	rm -rf $(BUILD)
	cd system && $(CARGO) clean
	cd tools && $(CARGO) clean
	cd boot/uefi && $(CARGO) clean
