import io
from types import SimpleNamespace

import pytest
from PIL import Image

from jam_e2e.clipboard import image_fixture, verify_image


@pytest.mark.parametrize("kind", ["png", "tiff"])
def test_local_image_receipt_checks_exact_payload_and_private_file(tmp_path, kind):
    temporary = tmp_path / "tmp"
    directory = temporary / "jam-clipboard-123"
    directory.mkdir(parents=True)
    image = directory / "image.png"
    profile = SimpleNamespace(repo=tmp_path, temporary=temporary)
    payload, _ = image_fixture(kind)
    with Image.open(io.BytesIO(payload)) as original:
        if kind == "png":
            image.write_bytes(payload)
        else:
            original.save(image, format="PNG")
    image.chmod(0o600)
    (tmp_path / "image-path.txt").write_text(f"{image}\nqueued-after-image\n")
    verify_image(profile, kind)
    image.chmod(0o644)
    with pytest.raises(AssertionError):
        verify_image(profile, kind)
