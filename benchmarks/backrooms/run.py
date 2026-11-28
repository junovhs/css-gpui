#!/usr/bin/env python3
"""Measure strict GPUI conversion of the browser-served Backrooms Pen."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import re
import subprocess
from tempfile import TemporaryDirectory


REPOSITORY = Path(__file__).resolve().parents[2]
URL = "https://cdpn.io/pen/debug/NPqLLoz"
DOCUMENT_SHA256 = "373084e9cf19f2a62b072eed146a687d2879eac74fa1374b0b3ef865836e1c49"
CSS_SHA256 = "4b51d48812b2482e70e43501ff00811cd92b2a562e7b8bd7c0e839b98c072354"
VIEWPORTS = ("390x844", "1440x900")
SOURCE_LOSS = re.compile(r"^((?:input\.html|linked\.css):\d+:\d+): source loss in ", re.M)
LOSS_MESSAGE = re.compile(
    r": (?:(?P<at_rule>at-rule '.*' was not evaluated)"
    r"|(?P<selector>selector '.*' (?:is outside the static selector subset|"
    r"reaches elements outside the generated interaction subset))"
    r"|property '(?P<property>[\w-]+)' is not supported in v0\.1 and will be dropped"
    r"|(?P<form>unsupported form control)|(?P<value_or_state>[\w-]+): .*?)\Z",
    re.S,
)


def digest(data):
    return sha256(data).hexdigest()


def identify(data, expected, name):
    actual = digest(data)
    if actual != expected:
        raise SystemExit(f"{name} SHA-256 changed. Expected {expected}, received {actual}.")
    return {"bytes": len(data), "sha256": actual}


def output(command, cwd=REPOSITORY):
    return subprocess.check_output(command, cwd=cwd, timeout=120)


def compiler():
    messages = output([
        "cargo", "build", "--quiet", "--locked", "--example", "generate_gpui",
        "--message-format=json",
    ])
    for line in messages.splitlines():
        message = json.loads(line)
        if (message["reason"] == "compiler-artifact" and
                message["target"]["name"] == "generate_gpui" and message["executable"]):
            return message["executable"]
    raise SystemExit("Cargo did not report the generate_gpui executable.")


def capability_for(entry):
    message = LOSS_MESSAGE.search(entry)
    if message is None:
        return "unclassified", "unknown message"
    kind = message.lastgroup
    subject = entry[:message.start()]
    if kind == "at_rule":
        return kind, subject.split()[0]
    if kind == "form":
        return kind, subject
    if kind == "selector":
        if entry.endswith("generated interaction subset"):
            return kind, "generated interaction subset"
        return kind, "static subset with :has()" if ":has(" in subject else "other static subset"
    return kind, message[kind]


def summarize(result, viewport):
    stderr = result.stderr.decode("utf-8")
    diagnostics, *errors = re.split(r"^Error: ", stderr, maxsplit=1, flags=re.M)
    error = errors[0].strip() if errors else None
    groups = {}
    parts = SOURCE_LOSS.split(diagnostics)
    for location, entry in zip(parts[1::2], parts[2::2]):
        kind, capability = capability_for(entry.rstrip())
        group = groups.setdefault(kind, {}).setdefault(capability, {"count": 0, "first": location})
        group["count"] += 1

    target_losses = len(re.findall(r"(?m)^input\.html: GPUI target loss at ", stderr))
    stopped_at = None
    if result.returncode:
        if error == '"GPUI conversion has source losses"':
            stopped_at = "source diagnostics"
        elif error == '"GPUI conversion has target losses"':
            stopped_at = "target diagnostics"
        elif error and "could not parse input.html and linked.css:" in error:
            stopped_at = "source graph"
        else:
            stopped_at = "compiler error"

    return {
        "viewport": viewport,
        "exit_code": result.returncode,
        "stopped_at": stopped_at,
        "first_source_loss": f"{parts[1]}: {parts[2].strip()}" if len(parts) > 1 else None,
        "fatal_error": error or (stderr.strip() if result.returncode else None),
        "source_losses": (len(parts) - 1) // 2,
        "source_loss_groups": groups,
        "target_losses": target_losses if target_losses or not result.returncode else None,
        "renderer_emitted": b"pub fn render_scene" in result.stdout,
        "stdout_bytes": len(result.stdout),
        "stderr_sha256": digest(result.stderr),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--logs", type=Path, help="Keep compiler stderr in this local directory")
    args = parser.parse_args()

    document = output(["curl", "--fail", "--silent", "--show-error", "--location",
                       "--max-time", "60", URL])
    source = {"url": URL, "document": identify(document, DOCUMENT_SHA256, "HTML")}
    stylesheet, = re.findall(rb"<style\b[^>]*>(.*?)</style\s*>", document, re.I | re.S)
    source["inline_css"] = identify(stylesheet, CSS_SHA256, "Inline CSS")
    revision = output(["git", "rev-parse", "HEAD"]).decode().strip()
    executable = compiler()
    if args.logs:
        args.logs.mkdir(parents=True, exist_ok=True)
    results = []
    with TemporaryDirectory(prefix="css-gpui-backrooms-") as folder:
        scratch = Path(folder)
        (scratch / "input.html").write_bytes(document)
        # The Pen has no linked stylesheet. The CLI reads inline CSS from the original HTML.
        (scratch / "linked.css").touch()
        for viewport in VIEWPORTS:
            command = [executable, "--viewport", viewport, "input.html", "linked.css"]
            result = subprocess.run(command, cwd=scratch, capture_output=True, timeout=120)
            results.append(summarize(result, viewport))
            if args.logs:
                (args.logs / f"strict-{viewport}.stderr").write_bytes(result.stderr)
    print(json.dumps({"source": source, "compiler_revision": revision, "runs": results},
                     indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
