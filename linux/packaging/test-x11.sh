#!/bin/bash
# Runs inside the build image: a virtual X server with a window manager and a compositor, some terminals,
# Snowdrift for a while, then a screenshot. Usage: test-x11.sh <snowdrift binary> <out dir>
set -euo pipefail
bin=$1; out=$2; mkdir -p "$out"
export DISPLAY=:99
Xvfb :99 -screen 0 1280x800x24 +extension RANDR +extension COMPOSITE >/tmp/xvfb.log 2>&1 &
sleep 1
openbox >/tmp/openbox.log 2>&1 &
xcompmgr >/tmp/xcompmgr.log 2>&1 &
sleep 1
xterm -geometry 70x20+80+260 -bg '#1e2a3a' -fg white -T "back window" &
xterm -geometry 70x18+420+180 -bg '#3a2a1e' -fg white -T "front window" &
sleep 1
"$bin" run >/tmp/snowdrift.log 2>&1 &
sleep "${3:-25}"
import -window root "$out/snow.png"
"$bin" status || true
"$bin" plow || true
sleep 6
import -window root "$out/plow.png"
cat /tmp/snowdrift.log
"$bin" stop || true
