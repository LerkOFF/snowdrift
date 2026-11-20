#!/bin/bash
# Runs inside the build image: Snowdrift with the winter decorations on a virtual X display. Frost grows, a garland
# hangs on the window, and a ball is rolled out of the ground snow with the (virtual) mouse.
# Usage: test-festive.sh <snowdrift binary> <out dir>
set -euo pipefail
bin=$1; out=$2; mkdir -p "$out"
export DISPLAY=:99
export SNOWDRIFT_FROST_SECONDS=20
export XDG_CONFIG_HOME=/tmp/cfg
Xvfb :99 -screen 0 1280x800x24 +extension RANDR +extension COMPOSITE >/tmp/xvfb.log 2>&1 &
sleep 1
openbox >/tmp/openbox.log 2>&1 &
xcompmgr >/tmp/xcompmgr.log 2>&1 &
sleep 1
xterm -geometry 80x14+100+160 -bg '#1e2a3a' -fg white -T "window" &
sleep 1
"$bin" run >/tmp/snowdrift.log 2>&1 &
sleep 2
"$bin" set intensity 100
"$bin" set windows off
"$bin" set newyear on
"$bin" status
sleep 50
import -window root "$out/festive-1.png"
# Roll a ball: press in the ground snow and drag along it, trying a few heights.
for y in 782 788 794; do
    xdotool mousemove 300 $y mousedown 1
    for x in 320 360 400 440 480 520 560 600 640 680; do xdotool mousemove $x $y; sleep 0.12; done
    xdotool mouseup 1
    sleep 0.4
done
sleep 1
import -window root "$out/festive-2.png"
"$bin" snowmen clear
sleep 1
import -window root "$out/festive-3.png"
cat /tmp/snowdrift.log
"$bin" stop || true
