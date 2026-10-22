#!/bin/sh
# Boots an arcOS image headless and types a list of commands on the serial
# console. Usage: scripts/test.sh <image> [commands-file] [qemu options...]
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
    -drive file="$IMAGE",format=raw,if=ide "$@" > "$OUT" 2>&1 || true
# Strip colours for readability.
sed 's/\x1b\[[0-9;]*[A-Za-z]//g; s/\r//g' "$OUT"
if grep -q "arcOS stopped" "$OUT"; then
    rm -f "$OUT"
    exit 1
fi
rm -f "$OUT"
