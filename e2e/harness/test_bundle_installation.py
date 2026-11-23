import json

import pytest

from jam_e2e.bundle import installation_archive


def test_recipe_selects_only_a_prepackaged_private_installation(tmp_path):
    manifest = tmp_path / "fixtures/manifest.json"
    manifest.parent.mkdir()
    app = manifest.parent / "managed-test.zip"
    app.touch()
    manifest.write_text(json.dumps({
        "schema": 1, "recipes": {
            "default": {},
            "rollback": {"installations": {"macos": app.name}},
            "unsafe": {"installations": {"macos": "../outside.zip"}},
        },
    }))
    (tmp_path / "outside.zip").touch()
    assert installation_archive(manifest, "default", "macos") is None
    assert installation_archive(manifest, "rollback", "macos") == app
    with pytest.raises(ValueError, match="packaged linux"):
        installation_archive(manifest, "rollback", "linux")
    with pytest.raises(ValueError, match="private installation archive"):
        installation_archive(manifest, "unsafe", "macos")
