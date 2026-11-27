"""A native settings fixture must never modify settings outside its profile."""

import pytest

from fixtures.native_preferences import reduced_motion


@pytest.mark.parametrize("escape", ["configuration", "gtk-symlink"])
def test_native_preferences_refuse_an_external_settings_directory(tmp_path, escape):
    owned = tmp_path / "owned"
    owned.mkdir()
    foreign = tmp_path / "foreign"
    foreign.mkdir()
    existing = foreign / "settings.ini"
    existing.write_text("[Settings]\ngtk-enable-animations=true\n")
    configuration = foreign
    if escape == "gtk-symlink":
        configuration = owned / "config"
        configuration.mkdir()
        (configuration / "gtk-4.0").symlink_to(foreign, target_is_directory=True)
    with pytest.raises(AssertionError, match="owned|external"):
        reduced_motion(owned, {"XDG_CONFIG_HOME": str(configuration)})
    assert existing.read_text() == "[Settings]\ngtk-enable-animations=true\n"
    assert sorted(path.name for path in foreign.iterdir()) == ["settings.ini"]
