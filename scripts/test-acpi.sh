#!/bin/sh
# Boots an arcOS image on a classic PC (PIIX4) and on a modern one (Q35),
# presses the power button through QMP and checks that arcOS turns itself
# off cleanly.
#
# Usage: scripts/test-acpi.sh <image> [case...]   (cases: pc, q35)
set -u
IMAGE=${1:?usage: test-acpi.sh <image> [case...]}
shift
HERE=$(dirname "$0")
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
CASES=${*:-pc q35}

printf '@sleep 3\ndev\n@sleep 8\necho still-on\n' > "$WORK/cmds.txt"

# Presses the power button through QMP after $2 seconds.
cat > "$WORK/press.py" <<'EOT'
import json, socket, sys, time
time.sleep(float(sys.argv[2]))
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
f = s.makefile("rw")
f.readline()
for cmd in ({"execute": "qmp_capabilities"}, {"execute": "system_powerdown"}):
    f.write(json.dumps(cmd) + "\n")
    f.flush()
    while "event" in json.loads(f.readline()):
        pass
EOT

FAILED=0
for c in $CASES; do
    case $c in
        pc) ARGS= ;;
        q35) ARGS="-machine q35" ;;
        *) echo "unknown case: $c" >&2; exit 2 ;;
    esac
    python3 "$WORK/press.py" "$WORK/qmp.sock" 9 &
    PRESS=$!
    DISK=$([ $c = q35 ] && echo ahci || echo ide) "$HERE/test.sh" "$IMAGE" "$WORK/cmds.txt" $ARGS \
        -qmp "unix:$WORK/qmp.sock,server,nowait" > "$WORK/out.txt" 2>&1
    STATUS=$?
    wait $PRESS
    problems=
    [ $STATUS -ne 0 ] && problems="$problems [kernel stopped]"
    grep -q "power button (ACPI)" "$WORK/out.txt" || problems="$problems [no power button device]"
    grep -q "power button pressed" "$WORK/out.txt" || problems="$problems [the press was not seen]"
    # The press comes before the last command: a machine that turned off
    # never runs it.
    grep -q "^still-on" "$WORK/out.txt" && problems="$problems [the machine did not turn off]"
    if [ -z "$problems" ]; then
        printf '  ok    %s\n' "$c"
    else
        printf '  FAIL  %s:%s\n' "$c" "$problems"
        [ "${VERBOSE:-0}" = 1 ] && cat "$WORK/out.txt"
        FAILED=1
    fi
done
exit $FAILED
