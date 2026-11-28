#!/usr/bin/env bash
set -euo pipefail

exe=${1:?native executable required}
width=${2:?width required}
height=${3:?height required}
frame=${4:?RGBA frame required}
output=${5:?output PNG path required}
trace=$(mktemp)
"$exe" "$width" "$height" "$frame" >"$trace" &
app=$!
cleanup() {
    kill "$app" 2>/dev/null || true
    wait "$app" 2>/dev/null || true
    rm -f "$trace"
}
trap cleanup EXIT
for _ in $(seq 1 100); do
    kill -0 "$app" 2>/dev/null || exit 1
    if [[ -s $trace ]] && wid=$(xdotool search --onlyvisible --pid "$app" | head -1); then
        xdotool windowfocus "$wid"
        xdotool mousemove --window "$wid" 2 2
        # X11 window focus schedules a GPUI redraw. The paint callback
        # precedes presentation by the compositor.
        sleep 0.25
        import -window "$wid" "$output"
        exit
    fi
    sleep 0.05
done
echo 'timed out waiting for the GPUI X11 window' >&2
exit 1
