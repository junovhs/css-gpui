"""Generate and build the toolbar from its authored HTML and CSS."""

import fcntl
from pathlib import Path
import shutil
import subprocess
import tempfile


TOOLBAR = Path(__file__).resolve().parent
ROOT = TOOLBAR.parents[1]
CARGO = ("cargo", "+1.95.0")


def build():
    output = ROOT / "target" / "toolbar"
    output.mkdir(parents=True, exist_ok=True)
    with (output / ".build.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        build_locked(output)


def build_locked(output):
    generated = subprocess.check_output(
        [*CARGO, "run", "--locked", "--quiet", "--example", "generate_gpui",
         "--target-dir", output / "compiler", "--", TOOLBAR / "source.html",
         TOOLBAR / "source.css"],
        cwd=ROOT,
    )

    # Stage the host so a build never edits the checked-in native example.
    with tempfile.TemporaryDirectory(prefix="host-", dir=output) as directory:
        host = Path(directory)
        (host / "src").mkdir()
        for file in ("Cargo.toml", "Cargo.lock", "src/main.rs"):
            shutil.copyfile(TOOLBAR / "native" / file, host / file)
        source = host / "src" / "generated.rs"
        source.write_bytes(generated)
        subprocess.run(
            [*CARGO, "build", "--locked", "--manifest-path", host / "Cargo.toml",
             "--target-dir", output],
            cwd=ROOT,
            check=True,
        )
        source.replace(output / "generated.rs")

    print(f"Generated Rust: {output / 'generated.rs'}")
    print(f"Native executable: {output / 'debug' / 'css-gpui-toolbar-host'}")


if __name__ == "__main__":
    try:
        build()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode)
