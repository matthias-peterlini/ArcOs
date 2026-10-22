#!/bin/sh
# Boots an arcOS image headless and types a list of commands on the serial
# console. Usage: scripts/test.sh <image> [commands-file] [qemu options...]
#
# DISK selects how the image is attached: ide (default), ahci, nvme, virtio.
#
# Prints the whole session; exits non-zero if the kernel stopped.
set -eu
IMAGE=${1:?usage: test.sh <image> [commands-file] [qemu options...]}
shift
COMMANDS=${1:-scripts/session.txt}
[ $# -gt 0 ] && shift
QEMU=${QEMU:-qemu-system-x86_64}
BOOT_WAIT=${BOOT_WAIT:-4}
STEP_WAIT=${STEP_WAIT:-1}
case "${DISK:-ide}" in
    ide)    DRIVE="-drive file=$IMAGE,format=raw,if=ide" ;;
    ahci)   DRIVE="-machine q35 -drive file=$IMAGE,format=raw,if=none,id=sys -device ahci,id=ahci -device ide-hd,drive=sys,bus=ahci.0" ;;
    nvme)   DRIVE="-machine q35 -drive file=$IMAGE,format=raw,if=none,id=sys -device nvme,drive=sys,serial=arc0001" ;;
    virtio) DRIVE="-drive file=$IMAGE,format=raw,if=none,id=sys -device virtio-blk-pci,drive=sys" ;;
    *) echo "unknown DISK=$DISK" >&2; exit 2 ;;
esac
OUT=$(mktemp)
{
    sleep "$BOOT_WAIT"
    while IFS= read -r line; do
        case "$line" in
            '#'*|'') continue ;;
            '@sleep '*) sleep "${line#@sleep }"; continue ;;
        esac
        printf '%s\r' "$line"
        sleep "$STEP_WAIT"
    done < "$COMMANDS"
    sleep 1
} | timeout 120 "$QEMU" -m 512M -display none -no-reboot -monitor none -serial stdio \
    $DRIVE "$@" > "$OUT" 2>&1 || true
# Strip colours for readability.
sed 's/\x1b\[[0-9;]*[A-Za-z]//g; s/\r//g' "$OUT"
if grep -q "arcOS stopped" "$OUT"; then
    rm -f "$OUT"
    exit 1
fi
rm -f "$OUT"
