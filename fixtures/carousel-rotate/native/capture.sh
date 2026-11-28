#!/usr/bin/env bash
set -euo pipefail

exe=$(realpath "${1:?native executable required}")
out=$(realpath -m "${2:?output directory required}")
mkdir -p "$out"
widths=(360 720)
if [[ ${3:-} == --initial-only ]]; then
    widths=(720)
fi

for width in "${widths[@]}"; do
    trace="$out/$width.jsonl"
    "$exe" "$width" 280 --trace >"$trace" 2>"$out/$width.stderr" &
    app=$!
    trap 'kill "$app" 2>/dev/null || true; wait "$app" 2>/dev/null || true' EXIT
    wait_frame() {
        for _ in $(seq 1 150); do
            kill -0 "$app" 2>/dev/null || { cat "$out/$width.stderr" >&2; return 1; }
            if [[ $(wc -l <"$trace") -ge $1 ]]; then
                return
            fi
            sleep 0.05
        done
        echo "timed out waiting for carousel frame $1 at width $width" >&2
        return 1
    }

    wait_frame 1
    wid=$(xdotool search --onlyvisible --pid "$app" | head -1)
    xdotool windowfocus "$wid"
    xdotool mousemove --window "$wid" 2 2
    # The trace comes from GPUI paint, ahead of X11 presentation.
    sleep 0.15
    screenshot() { import -window "$wid" "$out/$width-$1.png"; }
    screenshot initial
    if [[ ${3:-} != --initial-only ]]; then
        click() {
            xdotool mousemove --window "$wid" "$(($1 + width / 2))" 230 click 1
            xdotool mousemove --window "$wid" 2 2
        }
        click 36
        if [[ $width == 720 ]]; then
            sleep 0.22
            screenshot turning
        fi
        wait_frame 2
        sleep 0.08
        screenshot next
        click -24
        wait_frame 3
        sleep 0.08
        screenshot previous
        click -24
        wait_frame 4
        sleep 0.08
        screenshot wrap-previous
    fi
    kill "$app"
    wait "$app" || true
    trap - EXIT
done
