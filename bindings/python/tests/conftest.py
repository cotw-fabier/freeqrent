import json
from pathlib import Path
from typing import Any, Dict, List

import pytest

ROOT = Path(__file__).resolve().parents[3]
CASES_PATH = ROOT / "conformance" / "cases.json"
SCHEMA_PATH = ROOT / "schema" / "freeqrent.schema.json"


def load_cases() -> List[Dict[str, Any]]:
    cases: List[Dict[str, Any]] = json.loads(CASES_PATH.read_text(encoding="utf-8"))["cases"]
    return cases


@pytest.fixture(scope="session")
def schema() -> Dict[str, Any]:
    data: Dict[str, Any] = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    return data
