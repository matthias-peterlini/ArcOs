#!/bin/sh
# Builds a small Linux program and a small Windows program, puts them on a
# copy of the image together with the store programs, boots arcOS and checks
# that they run: the Linux ABI, the Windows loader (winrun), `install` and the
# BusyBox applets.
#
# Needs musl-gcc (musl-tools) and x86_64-w64-mingw32-gcc (mingw-w64). If either
# is missing, that half is skipped (and noted), so the script still checks what
# it can.
#
# Usage: scripts/test-compat.sh <image>
# Prints the lines missing from the session and exits non-zero if any;
# VERBOSE=1 prints the whole session.
set -u
IMAGE=${1:?usage: test-compat.sh <image>}
HERE=$(dirname "$0")
ARCTOOL=${ARCTOOL:-$HERE/../tools/target/release/arctool}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cp --sparse=always "$IMAGE" "$WORK/arcos.img" || exit 2
"$ARCTOOL" fs "$WORK/arcos.img" mkdir /home/compat >/dev/null 2>&1

have_linux=0
have_windows=0

if command -v musl-gcc >/dev/null 2>&1; then
    cat > "$WORK/hello.c" <<'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main(int argc, char **argv) {
    printf("linux: argc=%d\n", argc);
    for (int i = 0; i < argc; i++) printf("linux: argv[%d]=%s\n", i, argv[i]);
    char *m = malloc(32);
    strcpy(m, "linux: heap ok");
    puts(m);
    free(m);
    printf("linux: float=%.3f\n", 3.14159);
    FILE *f = fopen("out.txt", "w");
    fputs("written by a linux program\n", f);
    fclose(f);
    f = fopen("out.txt", "r");
    char buf[64];
    if (fgets(buf, sizeof buf, f)) printf("linux: read back: %s", buf);
    fclose(f);
    return 0;
}
EOF
    if musl-gcc -static -Os -o "$WORK/lhello" "$WORK/hello.c" 2>/dev/null; then
        strip "$WORK/lhello" 2>/dev/null
        "$ARCTOOL" fs "$WORK/arcos.img" put "$WORK/lhello" /home/compat/lhello >/dev/null && have_linux=1
    fi
fi
[ $have_linux = 0 ] && echo "  note: musl-gcc not available, skipping the Linux program"

if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    cat > "$WORK/whello.c" <<'EOF'
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) {
    printf("windows: argc=%d\n", argc);
    for (int i = 0; i < argc; i++) printf("windows: argv[%d]=%s\n", i, argv[i]);
    int *m = malloc(4 * sizeof(int));
    for (int i = 0; i < 4; i++) m[i] = i * i;
    printf("windows: squares %d %d %d %d\n", m[0], m[1], m[2], m[3]);
    free(m);
    printf("windows: float=%.3f\n", 2.71828);
    return 0;
}
EOF
    if x86_64-w64-mingw32-gcc -O2 -o "$WORK/whello.exe" "$WORK/whello.c" 2>/dev/null; then
        "$ARCTOOL" fs "$WORK/arcos.img" put "$WORK/whello.exe" /home/compat/whello.exe >/dev/null && have_windows=1
    fi
fi
[ $have_windows = 0 ] && echo "  note: x86_64-w64-mingw32-gcc not available, skipping the Windows program"

cat > "$WORK/t.lua" <<'EOF'
print("lua: " .. _VERSION)
local s = 0
for i = 1, 100 do s = s + i end
print("lua: sum 1..100 = " .. s)
print(string.format("lua: float=%.3f", 1.0 / 3.0))
EOF
"$ARCTOOL" fs "$WORK/arcos.img" put "$WORK/t.lua" /home/compat/t.lua >/dev/null

# Build the session depending on what is available.
{
    echo '@sleep 1'
    echo 'cd sys:/home/compat'
    echo 'install busybox'
    echo 'install lua'
    echo 'echo busybox-echo-works'
    echo 'seq 1 3'
    echo 'grep linux t.lua'
    [ $have_linux = 1 ] && echo './lhello one two'
    [ $have_windows = 1 ] && echo 'whello.exe alpha beta'
    echo 'lua t.lua'
    echo '@sleep 2'
    echo 'shutdown'
} > "$WORK/session.txt"

{
    echo 'busybox-echo-works'
    echo 'lua: sum 1..100 = 5050'
    echo 'lua: float=0.333'
    [ $have_linux = 1 ] && { echo 'linux: heap ok'; echo 'linux: float=3.142'; echo 'linux: read back: written by a linux program'; }
    [ $have_windows = 1 ] && { echo 'windows: squares 0 1 4 9'; echo 'windows: float=2.718'; }
} > "$WORK/expect.txt"

STEP_WAIT=${STEP_WAIT:-2} "$HERE/test.sh" "$WORK/arcos.img" "$WORK/session.txt" > "$WORK/out.txt" 2>&1
STATUS=$?
FAILED=0
while IFS= read -r pattern; do
    [ -z "$pattern" ] && continue
    if ! grep -Fq -- "$pattern" "$WORK/out.txt"; then
        printf '  missing: %s\n' "$pattern"
        FAILED=1
    fi
done < "$WORK/expect.txt"
[ $STATUS -ne 0 ] && { echo "  the kernel stopped"; FAILED=1; }
[ "${VERBOSE:-0}" = 1 ] && cat "$WORK/out.txt"
if [ $FAILED = 0 ]; then
    echo "  ok    compat ($(grep -c . "$WORK/expect.txt") checks, linux=$have_linux windows=$have_windows)"
fi
exit $FAILED
