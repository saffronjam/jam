import io
import json
import tarfile

import pytest

from fixtures.build_bundle import build


def archive(path, role, version, protocol):
    payload = json.dumps(dict(arch="aarch64", server_build=role, release_version=version,
                              protocol=protocol)).encode()
    with tarfile.open(path, "w:gz") as out:
        entry = tarfile.TarInfo("manifest.json")
        entry.size = len(payload)
        out.addfile(entry, io.BytesIO(payload))
    return path


def test_remote_bundle_routes_compatible_update_and_legacy_takeover_separately(tmp_path):
    trust = tmp_path / "trust"
    trust.mkdir()
    current = archive(tmp_path / "current.tar.gz", "current", "0.0.4", "jam/8:test")
    legacy = archive(tmp_path / "legacy.tar.gz", "legacy", "0.0.4", "jam/7:test")
    previous = archive(tmp_path / "previous.tar.gz", "previous", "0.0.3", "jam/8:test")
    limited = archive(tmp_path / "limited.tar.gz", "limited", "0.0.4", "jam/8:test")
    out = tmp_path / "bundle"
    build(out, trust, ssh_image="fixture", engine="/bin/true", remote_archive=current,
          remote_previous_archive=previous, remote_legacy_archive=legacy,
          remote_limited_archive=limited)
    recipes = json.loads((out / "manifest.json").read_text())["recipes"]
    for recipe, role in (("remote-update-available", "previous"),
                         ("remote-legacy", "legacy"), ("remote-limited", "limited")):
        assert recipes[recipe]["environment"]["JAM_E2E_REMOTE_ARCHIVE"] == (
            "{bundle}/remote/" + role + ".tar.gz")
        assert (out / f"remote/{role}.tar.gz").read_bytes() == (
            tmp_path / f"{role}.tar.gz").read_bytes()
    with pytest.raises(AssertionError, match="previous must use the current protocol"):
        build(tmp_path / "wrong-role", trust, ssh_image="fixture", engine="/bin/true",
              remote_archive=current, remote_previous_archive=legacy)
