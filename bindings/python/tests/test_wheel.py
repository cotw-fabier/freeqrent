"""A release wheel installs into a fresh virtual environment and works.

The wheel is ``$FREEQRENT_WHEEL`` if set (CI hands in the one it built),
otherwise this builds one with ``maturin build --release``.
"""

import os
import re
import shutil
import subprocess
import sys
import venv
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent

SMOKE = """
import freeqrent as fq
svg = fq.render("https://example.com", fq.Style(preset="ink", modules=fq.Dots(size=0.9)))
assert svg.startswith("<svg"), svg[:40]
assert fq.check("hi").ok
assert fq.__version__ == fq.version()
assert fq.__file__ and "site-packages" in fq.__file__, fq.__file__
print(fq.__version__)
"""


def maturin() -> str:
    exe = Path(sys.executable).parent / ("maturin.exe" if os.name == "nt" else "maturin")
    if exe.exists():
        return str(exe)
    found = shutil.which("maturin")
    if found is None:
        pytest.skip("maturin isn't installed (pip install maturin)")
    return found


def test_a_release_wheel_installs_into_a_fresh_venv(tmp_path: Path) -> None:
    given = os.environ.get("FREEQRENT_WHEEL")
    if given:
        wheel = Path(given)
    else:
        out = tmp_path / "dist"
        subprocess.run(
            [maturin(), "build", "--release", "--out", str(out)],
            cwd=PROJECT,
            check=True,
            capture_output=True,
        )
        (wheel,) = out.glob("freeqrent-*.whl")
    assert "abi3" in wheel.name, wheel.name

    env_dir = tmp_path / "fresh"
    venv.create(env_dir, with_pip=True)
    bin_dir = env_dir / ("Scripts" if os.name == "nt" else "bin")
    python = bin_dir / ("python.exe" if os.name == "nt" else "python")
    subprocess.run(
        [str(python), "-m", "pip", "install", "--no-index", "--quiet", str(wheel)],
        check=True,
        capture_output=True,
    )
    done = subprocess.run(
        [str(python), "-c", SMOKE], cwd=tmp_path, capture_output=True, text=True
    )
    assert done.returncode == 0, done.stderr
    pyproject = (Path(__file__).resolve().parents[1] / "pyproject.toml").read_text()
    wanted = re.search(r'^version = "([^"]+)"', pyproject, re.M)
    assert wanted and done.stdout.strip() == wanted.group(1)
