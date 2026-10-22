# arcOS build.
#
#   make            build the bootable disk image (build/arcos.img)
#   make run        start it in QEMU (PC with IDE disk)
#   make run-q35    start it in QEMU (modern PC: AHCI/SATA disk)
#   make run-nvme   start it in QEMU with the system disk on NVMe
#   make test       boot headless and run a scripted session
#   make check      unit tests of the shared libraries and the host tool

PROFILE  ?= release
CARGO    ?= cargo
QEMU     ?= qemu-system-x86_64
MEMORY   ?= 512M
SIZE     ?= 128M

BUILD    := build
TARGET   := system/target/x86_64-unknown-none/$(PROFILE)
ARCTOOL  := tools/target/release/arctool
DRIVERS  := $(notdir $(wildcard system/drivers/*))
PACKAGES := $(DRIVERS:%=$(BUILD)/drivers/%.adp)
APPS     := $(notdir $(wildcard system/apps/*))
ROOT     := $(BUILD)/root
IMAGE    := $(BUILD)/arcos.img

ifeq ($(PROFILE),release)
CARGO_PROFILE := --release
endif

.PHONY: all system tools root image run run-q35 run-nvme test check clean $(PACKAGES)

all: image

$(BUILD)/stage1.bin: boot/bios/stage1.asm
	@mkdir -p $(BUILD)
	nasm -f bin -o $@ $<

$(BUILD)/stage2.bin: boot/bios/stage2.asm
	@mkdir -p $(BUILD)
	nasm -f bin -o $@ $<

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

image: $(BUILD)/stage1.bin $(BUILD)/stage2.bin system tools $(PACKAGES) root
	$(ARCTOOL) image $(IMAGE) --size $(SIZE) \
		--stage1 $(BUILD)/stage1.bin --stage2 $(BUILD)/stage2.bin \
		--kernel $(TARGET)/arc-kernel --drivers $(BUILD)/drivers --files $(ROOT)

QEMU_COMMON := -m $(MEMORY) -serial stdio -no-reboot -rtc base=localtime

run: image
	$(QEMU) $(QEMU_COMMON) -drive file=$(IMAGE),format=raw,if=ide

run-q35: image
	$(QEMU) $(QEMU_COMMON) -machine q35 -drive file=$(IMAGE),format=raw,if=none,id=sys -device ahci,id=ahci -device ide-hd,drive=sys,bus=ahci.0

run-nvme: image
	$(QEMU) $(QEMU_COMMON) -machine q35 -drive file=$(IMAGE),format=raw,if=none,id=sys -device nvme,drive=sys,serial=arc0001

test: image
	scripts/test.sh $(IMAGE)

check:
	cd system && $(CARGO) test --target x86_64-unknown-linux-gnu -p arcdk -p arcfs
	cd tools && $(CARGO) test --release

clean:
	rm -rf $(BUILD)
	cd system && $(CARGO) clean
	cd tools && $(CARGO) clean
