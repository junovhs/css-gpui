"""Capture the generated GPUI client on an isolated X11 display."""

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import subprocess
import time

from PIL import Image, ImageChops

HOST = Path(__file__).resolve().parent / 'native' / 'Cargo.toml'


def capture(executable, width, height, directory, state='baseline', gesture=None):
    with (directory / f'native-{state}.log').open('w') as log:
        process = subprocess.Popen([executable, str(width), str(height)], stdout=log, stderr=log)
        try:
            search = subprocess.run(
                ['xdotool', 'search', '--sync', '--onlyvisible', '--pid', str(process.pid)],
                check=True, capture_output=True, text=True, timeout=10,
            )
            window = search.stdout.splitlines()[0]
            subprocess.run(['xdotool', 'windowfocus', window], check=True, timeout=5)
            subprocess.run(['xdotool', 'mousemove', '0', '0'], check=True, timeout=5)
            time.sleep(2)
            if gesture:
                for arguments in gesture:
                    subprocess.run(['xdotool', *arguments(window)], check=True, timeout=5)
                time.sleep(0.5)
            subprocess.run(['import', '-window', window, directory / f'native-{state}.png'], check=True, timeout=10)
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def compare(directory, width, height, thresholds, state='baseline'):
    with Image.open(directory / f'native-{state}.png') as native, Image.open(directory / f'browser-{state}.png') as browser:
        if native.size != (width, height) or browser.size != (width, height):
            return {'status': 'wrong-frame-size', 'nativeSize': native.size, 'browserSize': browser.size}
        diff = ImageChops.difference(native.convert('RGB'), browser.convert('RGB'))
        diff.save(directory / f'difference-{state}.png')
        histogram = diff.histogram()
        mean = sum(intensity % 256 * count for intensity, count in enumerate(histogram)) / (sum(histogram) * 255)
        channels_over_16 = sum(count for intensity, count in enumerate(histogram) if intensity % 256 > 16) / sum(histogram)
        return {'status': 'captured', 'normalizedRgbMeanAbsoluteError': round(mean, 6),
                'fractionOfRgbChannelsOver16': round(channels_over_16, 6),
                'appearancePass': (mean <= thresholds['maxMeanError'] and
                                   channels_over_16 <= thresholds['maxChannelsOver16'])}


def changed_pixels(directory, renderer, state):
    with Image.open(directory / f'{renderer}-baseline.png') as baseline, Image.open(directory / f'{renderer}-{state}.png') as after:
        return sum(pixel != (0, 0, 0) for pixel in ImageChops.difference(baseline.convert('RGB'), after.convert('RGB')).getdata())


def gesture_for(scenario, reference, width, height):
    action = scenario.get('action')
    if action == 'press':
        return [lambda _: ['key', '--clearmodifiers', scenario['key']]]
    if action not in ('click', 'hover', 'scroll', 'type'):
        return None
    target = reference['baseline'].get(scenario.get('selector'))
    if not target:
        return None
    x, y, w, h = target['bounds']
    center = (round(x + w / 2), round(y + h / 2))
    if not (0 <= center[0] < width and 0 <= center[1] < height):
        return None
    gesture = [lambda window: ['mousemove', '--window', window, *map(str, center)]]
    if action in ('click', 'type'):
        gesture.append(lambda _: ['click', '1'])
    if action == 'type':
        gesture.append(lambda _: ['key', '--clearmodifiers', 'ctrl+a'])
        gesture.append(lambda _: ['type', '--clearmodifiers', '--', scenario['value']])
    if action == 'scroll':
        distance = scenario['distance']
        gesture.append(lambda _: ['click', '--repeat', str(max(1, abs(distance) // 120)), '7' if distance > 0 else '6'])
    return gesture


def replay_gestures(executable, directory, width, height, thresholds, scenarios, reference):
    results = {}
    for scenario in scenarios:
        state = scenario['id']
        replay = reference['states'].get(state)
        if not replay or 'error' in replay:
            results[state] = {'status': 'missing-browser-reference'}
            continue
        gesture = gesture_for(scenario, reference, width, height)
        if not gesture:
            results[state] = {'status': 'unverified-action'}
            continue
        try:
            capture(executable, width, height, directory, state, gesture)
            comparison = compare(directory, width, height, thresholds, state)
            if comparison['status'] == 'captured':
                for renderer in ['browser', 'native']:
                    comparison[renderer + 'ChangedPixels'] = changed_pixels(directory, renderer, state)
            results[state] = comparison
        except (subprocess.SubprocessError, OSError) as error:
            results[state] = {'status': 'capture-failed', 'error': str(error)}
    return results


def build_and_capture(generated, directory, width, height, thresholds, scenarios, reference):
    result = subprocess.run(
        ['cargo', 'build', '--locked', '--manifest-path', HOST, '--message-format=json'],
        env={**os.environ, 'CSS_GPUI_GENERATED': str(generated)}, cwd=HOST.parent,
        capture_output=True, text=True, timeout=600,
    )
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    diagnostics = [message['message']['rendered'] for message in messages
                   if message['reason'] == 'compiler-message' and message['message'].get('rendered')]
    (directory / 'native-build.log').write_text(result.stderr + ''.join(diagnostics))
    if result.returncode:
        return {'status': 'build-failed'}
    executable = next((message['executable'] for message in messages if message['reason'] == 'compiler-artifact'
                       and message['target']['name'] == 'css-gpui-compat-host' and message.get('executable')), None)
    if not executable:
        return {'status': 'missing-executable'}
    try:
        capture(executable, width, height, directory)
        result = compare(directory, width, height, thresholds)
        if scenarios and result['status'] == 'captured':
            result['gestures'] = replay_gestures(executable, directory, width, height, thresholds, scenarios, reference)
        return result
    except (subprocess.SubprocessError, OSError) as error:
        return {'status': 'capture-failed', 'error': str(error)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('results', type=Path, help='directory written by run.mjs')
    args = parser.parse_args()
    directory = args.results.resolve()
    source = json.loads((directory / 'results.json').read_text())
    corpus = (HOST.parent.parent / 'corpus.json').read_bytes()
    if sha256(corpus).hexdigest() != source['corpusSha256']:
        parser.error('results were produced with a different corpus lock')
    manifest = json.loads(corpus)
    thresholds = manifest['appearance']
    samples = {sample['id']: sample for sample in manifest['samples']}
    results = []
    for result in source['results']:
        sample, width, height = (result[key] for key in ('sample', 'width', 'height'))
        folder = directory / sample / f'{width}x{height}'
        tier = 'strict' if result['strict']['clean'] else 'exploratory'
        generated = folder / f'{tier}.rs'
        if generated.is_file():
            scenarios = samples[sample].get('scenarios', []) if tier == 'strict' else []
            comparison = build_and_capture(generated, folder, width, height, thresholds, scenarios, result['reference'])
        else:
            comparison = {'status': 'no-usable-emission'}
        results.append({'sample': sample, 'width': width, 'height': height, 'tier': tier, **comparison})
        print(f'{sample} {width}x{height}: {tier} {comparison["status"]}', flush=True)
    (directory / 'native-results.json').write_text(json.dumps({'corpusSha256': source['corpusSha256'], 'results': results}, indent=2) + '\n')


if __name__ == '__main__':
    main()
