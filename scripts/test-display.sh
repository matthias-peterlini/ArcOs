#!/bin/sh
# Boots an arcOS image once per display controller QEMU emulates, switches
# the resolution from the shell and checks the screen QEMU shows: it must
# have the new size and show the desktop (not a black or stale picture).
#
# Usage: scripts/test-display.sh <image> [case...]
# Cases: std (QEMU VGA), bochs (bochs-display), vmware, virtio.
set -u
IMAGE=${1:?usage: test-display.sh <image> [case...]}
shift
HERE=$(dirname "$0")
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
CASES=${*:-std bochs vmware virtio}
MODE=${MODE:-1280x720}

cat > "$WORK/cmds.txt" <<EOT
@sleep 4
display
display $MODE
@sleep 6
display
@sleep 2
shutdown
EOT

# Takes a screenshot through QMP after $2 seconds.
cat > "$WORK/shot.py" <<'EOT'
import json, socket, sys, time
sock, delay, out = sys.argv[1], float(sys.argv[2]), sys.argv[3]
time.sleep(delay)
s = socket.socket(socket.AF_UNIX)
s.connect(sock)
f = s.makefile("rw")
f.readline()
for cmd in ({"execute": "qmp_capabilities"}, {"execute": "screendump", "arguments": {"filename": out}}):
    f.write(json.dumps(cmd) + "\n")
    f.flush()
    while "event" in json.loads(f.readline()):
        pass
EOT

# Width, height and number of distinct colours of a PPM screenshot.
cat > "$WORK/look.py" <<'EOT'
import sys
data = open(sys.argv[1], "rb").read()
parts = data.split(maxsplit=4)
w, h = int(parts[1]), int(parts[2])
px = parts[4]
colours = len({px[i:i + 3] for i in range(0, len(px), 3 * 97)})
print(w, h, colours)
EOT

FAILED=0
for c in $CASES; do
    case $c in
        std) ARGS="-vga std" ;;
        bochs) ARGS="-vga none -device bochs-display" ;;
        vmware) ARGS="-vga vmware" ;;
        virtio) ARGS="-vga virtio" ;;
        *) echo "unknown case: $c" >&2; exit 2 ;;
    esac
    rm -f "$WORK/shot.ppm"
    python3 "$WORK/shot.py" "$WORK/qmp.sock" 16 "$WORK/shot.ppm" &
    SHOT=$!
    "$HERE/test.sh" "$IMAGE" "$WORK/cmds.txt" $ARGS -qmp "unix:$WORK/qmp.sock,server,nowait" > "$WORK/out.txt" 2>&1
    STATUS=$?
    wait $SHOT
    problems=
    [ $STATUS -ne 0 ] && problems="$problems [kernel stopped]"
    grep -q "^screen $MODE" "$WORK/out.txt" || problems="$problems [no \"screen $MODE\"]"
    if [ -f "$WORK/shot.ppm" ]; then
        set -- $(python3 "$WORK/look.py" "$WORK/shot.ppm")
        [ "${1}x${2}" = "$MODE" ] || problems="$problems [screen is ${1}x${2}]"
        [ "$3" -gt 50 ] || problems="$problems [blank screen: $3 colours]"
    else
        problems="$problems [no screenshot]"
    fi
    if [ -z "$problems" ]; then
        printf '  ok    %s\n' "$c"
    else
        printf '  FAIL  %s:%s\n' "$c" "$problems"
        [ "${VERBOSE:-0}" = 1 ] && cat "$WORK/out.txt"
        FAILED=1
    fi
    [ -n "${SHOTS:-}" ] && cp "$WORK/shot.ppm" "$SHOTS/$c.ppm" 2>/dev/null
done
exit $FAILED
