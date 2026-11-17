#!/bin/bash
# Runs inside the build image (see ../build.sh): static builds for x86_64 and aarch64, then tarball, deb and rpm for each.
# Needs /src (the repository), /cache (cargo target dir) and /out (where the files go).
set -euo pipefail
cd /src/linux
VERSION=$(tr -d '[:space:]' < /src/VERSION)
export CARGO_TARGET_DIR=/cache/target RUSTFLAGS="-C linker=rust-lld"
rm -rf /out/* /tmp/pkg && mkdir -p /out /tmp/pkg

# The launcher entry is written by the program itself; the host's own build can run it.
cargo build --release --target aarch64-unknown-linux-musl 2>&1 | tail -1
cargo build --release --target x86_64-unknown-linux-musl 2>&1 | tail -1
entry=$(/cache/target/aarch64-unknown-linux-musl/release/snowdrift desktop-entry snowdrift 2>/dev/null || /cache/target/x86_64-unknown-linux-musl/release/snowdrift desktop-entry snowdrift)

for arch in x86_64 aarch64; do
    bin=/cache/target/$arch-unknown-linux-musl/release/snowdrift
    case $arch in x86_64) deb=amd64 ;; aarch64) deb=arm64 ;; esac
    work=/tmp/pkg/$arch
    mkdir -p "$work"

    # Tarball: the program, its launcher entry and icon; `snowdrift install` does the rest.
    tar_dir=$work/snowdrift-$VERSION-linux-$arch
    mkdir -p "$tar_dir"
    cp "$bin" "$tar_dir/snowdrift"
    cp /src/LICENSE "$tar_dir/"
    cp /src/linux/assets/snowdrift.png "$tar_dir/"
    printf '%s\n' "$entry" > "$tar_dir/snowdrift.desktop"
    tar -C "$work" -czf "/out/Snowdrift-linux-$arch.tar.gz" "snowdrift-$VERSION-linux-$arch"

    # Package root shared by deb and rpm.
    root=$work/root
    mkdir -p "$root/usr/bin" "$root/usr/share/applications" "$root/usr/share/icons/hicolor/256x256/apps" "$root/usr/share/doc/snowdrift"
    install -m 755 "$bin" "$root/usr/bin/snowdrift"
    install -m 644 "$tar_dir/snowdrift.desktop" "$root/usr/share/applications/snowdrift.desktop"
    install -m 644 /src/linux/assets/snowdrift.png "$root/usr/share/icons/hicolor/256x256/apps/snowdrift.png"
    install -m 644 /src/LICENSE "$root/usr/share/doc/snowdrift/copyright"

    # deb
    mkdir -p "$root/DEBIAN"
    size=$(du -sk "$root/usr" | cut -f1)
    cat > "$root/DEBIAN/control" <<CONTROL
Package: snowdrift
Version: $VERSION
Section: x11
Priority: optional
Architecture: $deb
Installed-Size: $size
Recommends: picom | xcompmgr | compton
Maintainer: LerkOFF <noreply@github.com>
Homepage: https://github.com/LerkOFF/snowdrift
Description: Snowfall over all your windows
 Snow falls over the desktop, lies on the top edges of windows, slides off
 when a window is moved, melts, flies apart under the cursor and is cleared
 by a snowplow. Works on X11 (and Wayland through XWayland) and needs a
 compositing manager.
CONTROL
    fakeroot dpkg-deb --build -Zxz "$root" "/out/snowdrift_${VERSION}_$deb.deb" >/dev/null
    rm -rf "$root/DEBIAN"

    # rpm
    rpmtop=$work/rpm
    mkdir -p "$rpmtop"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    cat > "$rpmtop/SPECS/snowdrift.spec" <<SPEC
Name: snowdrift
Version: $VERSION
Release: 1
Summary: Snowfall over all your windows
License: MIT
URL: https://github.com/LerkOFF/snowdrift
AutoReqProv: no
%global debug_package %{nil}
%define __os_install_post %{nil}

%description
Snow falls over the desktop, lies on the top edges of windows, slides off when a window is moved, melts, flies apart
under the cursor and is cleared by a snowplow. Works on X11 (and Wayland through XWayland) and needs a compositing manager.

%install
mkdir -p %{buildroot}
cp -a $root/usr %{buildroot}/usr

%files
/usr/bin/snowdrift
/usr/share/applications/snowdrift.desktop
/usr/share/icons/hicolor/256x256/apps/snowdrift.png
/usr/share/doc/snowdrift/copyright
SPEC
    rpmbuild --define "_topdir $rpmtop" --target "$arch" -bb "$rpmtop/SPECS/snowdrift.spec" >/tmp/rpm-$arch.log 2>&1 || { tail -20 /tmp/rpm-$arch.log; exit 1; }
    cp "$rpmtop"/RPMS/*/snowdrift-*.rpm /out/
done
(cd /out && sha256sum * > SHA256SUMS)
ls -la /out
