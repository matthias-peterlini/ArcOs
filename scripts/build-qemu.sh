#!/bin/sh
# Builds a QEMU with the extra device models in tools/qemu (the Realtek
# RTL8169, RTL8168 and RTL8125, which stock QEMU lacks), so that their
# drivers can be tested: scripts/test-drivers.sh runs the rtl8169, rtl8168
# and rtl8125 cases with it.
#
# Usage: scripts/build-qemu.sh [directory]
# The default directory is build/qemu; the binary ends up in
# <directory>/build/qemu-system-x86_64. Needs git, a C compiler, ninja,
# meson's Python (python3-venv), glib, pixman, libslirp and libfdt headers.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
DIR=${1:-$HERE/../build/qemu}
VERSION=v8.2.2

if [ ! -d "$DIR/.git" ]; then
    git clone --depth 1 --branch $VERSION https://github.com/qemu/qemu.git "$DIR"
fi
cd "$DIR"

# The subprojects QEMU fetches by itself come from gitlab.com; take them
# from the GitHub mirrors instead, at the revision the .wrap file names
# (or the mirror's default branch when it does not have that one).
for p in keycodemapdb berkeley-softfloat-3 berkeley-testfloat-3; do
    [ -d subprojects/$p ] && continue
    git clone -q https://github.com/qemu/$p.git subprojects/$p
    rev=$(sed -n 's/^revision = //p' subprojects/$p.wrap)
    git -C subprojects/$p checkout -q "$rev" 2>/dev/null || true
    [ -d subprojects/packagefiles/$p ] && cp -r subprojects/packagefiles/$p/. subprojects/$p/
done

cp "$HERE/../tools/qemu/rtl8169.c" hw/net/
grep -q rtl8169.c hw/net/meson.build ||
    sed -i "s/files('rtl8139.c')/files('rtl8139.c', 'rtl8169.c')/" hw/net/meson.build

mkdir -p build
cd build
[ -f build.ninja ] || ../configure --target-list=x86_64-softmmu --without-default-features \
    --enable-slirp --enable-tcg --enable-kvm --disable-docs --disable-tools \
    --disable-download --disable-werror
ninja qemu-system-x86_64
echo "built $PWD/qemu-system-x86_64"
