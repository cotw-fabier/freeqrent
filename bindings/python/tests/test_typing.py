"""The package type-checks under mypy --strict, and the typed styles catch
bad names (tests/typecheck.py's ``type: ignore`` lines must be needed)."""

import subprocess
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent


def test_mypy_strict() -> None:
    pytest.importorskip("mypy", reason="mypy isn't installed (pip install mypy)")
    done = subprocess.run(
        [sys.executable, "-m", "mypy", "--strict", "--warn-unused-ignores", "python/freeqrent", "tests/typecheck.py"],
        cwd=HERE.parent,
        capture_output=True,
        text=True,
    )
    assert done.returncode == 0, done.stdout + done.stderr
