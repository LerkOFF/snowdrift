#!/bin/zsh
# Builds every Linux package into dist/linux (tarballs, deb, rpm for x86_64 and aarch64) inside Docker.
set -euo pipefail
cd "$(dirname "$0")/.."
out=${1:-dist/linux}; mkdir -p "$out"; out=${out:A}
docker build -q -t snowdrift-linux-build -f linux/packaging/Dockerfile.build linux/packaging >/dev/null
docker run --rm -v "$PWD":/src:ro -v snowdrift-cargo:/usr/local/cargo/registry -v snowdrift-target:/cache -v "$out":/out \
    snowdrift-linux-build bash /src/linux/packaging/package.sh
