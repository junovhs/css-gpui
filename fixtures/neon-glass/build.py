"""Generate and build the Neon Glass native menu from the local Pen."""

import argparse
import fcntl
import json
from os import environ
from pathlib import Path
import shutil
import subprocess
import tempfile


FIXTURE = Path(__file__).resolve().parent
ROOT = FIXTURE.parents[1]


def build(playwright_root):
    output = ROOT / "target" / "neon-glass"
    output.mkdir(parents=True, exist_ok=True)
    with (output / ".build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        with tempfile.TemporaryDirectory(prefix="build-", dir=output) as directory:
            build_menu(playwright_root, output, Path(directory))

    print(f"Generated Rust: {output / 'generated'}")
    print(f"Native executable: {output / 'neon-glass'}")


def build_menu(playwright_root, output, staging):
    capture = staging / "capture"
    subprocess.run(
        ["node", FIXTURE / "capture.mjs", playwright_root, capture, "--build"],
        check=True,
    )

    # Cargo reports the exact build-script output and executable it used for
    # this build, even when the target directory has older artifacts.
    cargo = subprocess.run(
        ["cargo", "+1.95.0", "build", "--locked", "--features", "interactive",
         "--example", "menu", "--manifest-path", FIXTURE / "native" / "Cargo.toml",
         "--target-dir", output / "cargo", "--message-format=json-render-diagnostics"],
        cwd=ROOT,
        env={**environ, "NEON_GLASS_REFERENCE": str(capture)},
        stdout=subprocess.PIPE,
        text=True,
        check=True,
    )
    messages = [json.loads(line) for line in cargo.stdout.splitlines()]
    menu = next(
        message for message in messages
        if message["reason"] == "compiler-artifact"
        and message["target"]["name"] == "menu"
        and message["executable"]
    )
    generated = next(
        Path(message["out_dir"]) for message in messages
        if message["reason"] == "build-script-executed"
        and message["package_id"] == menu["package_id"]
    )

    files = [f"{part}-{width}.rs" for part in ("stage", "overlay") for width in (390, 1100)]
    for file in files:
        shutil.copyfile(generated / file, staging / file)
    executable = staging / "neon-glass"
    shutil.copy2(menu["executable"], executable)

    generated_output = output / "generated"
    generated_output.mkdir(exist_ok=True)
    for file in files:
        (staging / file).replace(generated_output / file)
    executable.replace(output / executable.name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("playwright_root", type=Path, help="directory containing the pinned Node dependencies")
    arguments = parser.parse_args()
    try:
        build(arguments.playwright_root.resolve())
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode)
