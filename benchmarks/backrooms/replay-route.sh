#!/usr/bin/env bash
set -euo pipefail

exe=${1:?native executable required}
source=${2:?route bundle or --document required}
width=${3:?width required}
height=${4:?height required}
out=${5:?local capture prefix required}
trace="$out-trace.jsonl"
if [[ $source == --document ]]; then
    args=(--document "${width}x${height}" "${@:6}")
else
    args=(--route "$source")
fi
"$exe" "${args[@]}" >"$trace" &
app=$!
cleanup() {
    kill "$app" 2>/dev/null || true
    wait "$app" 2>/dev/null || true
}
trap cleanup EXIT

capture() {
    # The paint callback precedes X11 presentation.
    sleep 0.25
    import -window "$wid" -crop "${width}x${height}+0+0" +repage "$out-$1.png"
}

wait_for_frame() {
    for _ in $(seq 1 300); do
        kill -0 "$app" 2>/dev/null || break
        if [[ $(wc -l < "$trace") -ge $2 ]] && tail -n 1 "$trace" | grep -q "\"frame\":$1,"; then
            return
        fi
        sleep 0.05
    done
    echo "timed out waiting for native route frame $1" >&2
    cat "$trace" >&2
    return 1
}

records=1
wait_for_frame 0 "$records"
wid=$(xdotool search --sync --onlyvisible --pid "$app" | head -1)
xdotool windowfocus "$wid"
capture 0

# Each line is a native input, the expected frame, and a capture name.
current=0
while read -r input frame shot; do
    if [[ $input == keyboard ]]; then
        xdotool key --delay 150 Tab Tab Return
    elif [[ $input == double-* ]]; then
        xdotool mousemove --window "$wid" "${input#double-}" "$((height + 24))" click --repeat 2 --delay 0 1
    else
        xdotool mousemove --window "$wid" "$input" "$((height + 24))" click 1
    fi
    if [[ $frame != "$current" ]]; then records=$((records + 2)); fi
    wait_for_frame "$frame" "$records"
    capture "$shot"
    current=$frame
done
