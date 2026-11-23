import copy
import hashlib
from dataclasses import replace

import pytest

from jam_e2e.cases import Case
from jam_e2e.coverage import validate


def example():
    return ({"behaviors": [{"id": "image", "reviewed": True, "cases": ["IMG-001"],
                             "assertions": ["Exact bytes on the target host"]}],
             "changed_files": [{"path": "app", "classification": "ui", "behaviors": ["image"],
                                "reviewed": True, "reason": "Adds image upload"}],
             "changed_paths_sha256": hashlib.sha256(b"app").hexdigest()},
            [Case("IMG-001", "Image paste", (("file_exists", "image.png"),),
                  behavior_ids=("image",))])


def test_reviewed_bidirectional_mapping():
    ledger, cases = example()
    assert validate(ledger, cases) == 1


@pytest.mark.parametrize("defect", ["unreviewed", "no-cases", "no-assertions", "unknown-case",
                                   "unknown-behavior", "reverse-mapping", "duplicate", "file", "missing-file",
                                   "file-behavior"])
def test_uncovered_behavior_never_counts_as_complete(defect):
    ledger, cases = example()
    row = ledger["behaviors"][0]
    if defect == "unreviewed": row["reviewed"] = False
    elif defect == "no-cases": row["cases"] = []
    elif defect == "no-assertions": row["assertions"] = []
    elif defect == "unknown-case": row["cases"] = ["MISSING"]
    elif defect == "unknown-behavior": cases = [replace(cases[0], behavior_ids=("missing",))]
    elif defect == "reverse-mapping": cases.append(replace(cases[0], id="IMG-002"))
    elif defect == "duplicate": ledger["behaviors"].append(copy.deepcopy(row))
    elif defect == "missing-file": ledger["changed_files"][0]["path"] = "different"
    elif defect == "file-behavior": ledger["changed_files"][0]["behaviors"] = []
    else: ledger["changed_files"][0]["reviewed"] = False
    with pytest.raises(ValueError):
        validate(ledger, cases)
