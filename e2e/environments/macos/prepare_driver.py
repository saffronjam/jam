"""Apply the reviewed desktop-input patch to exactly Mac2 4.6.1's native driver."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def prepare(driver):
    root = Path(__file__).parent
    metadata = json.loads((root / 'desktop-input.json').read_text())
    assert json.loads((driver / 'package.json').read_text())['version'] == metadata['version']
    updates = []
    for patch in metadata['patches']:
        target = driver / patch['file']
        original = target.read_bytes()
        digest = hashlib.sha256(original).hexdigest()
        if digest == patch['patched_sha256']:
            continue
        assert digest == patch['original_sha256'], 'Pinned native Mac2 source changed; review the patch'
        with tempfile.TemporaryDirectory(prefix='je-mac2-patch-') as temporary:
            copy = Path(temporary) / patch['file']
            copy.parent.mkdir(parents=True)
            copy.write_bytes(original)
            subprocess.run(['git', 'apply', '--unidiff-zero', str((root / patch['patch']).resolve())],
                           cwd=temporary, check=True, capture_output=True, timeout=10)
            patched = copy.read_bytes()
            assert hashlib.sha256(patched).hexdigest() == patch['patched_sha256']
            updates.append((target, patched))
    # Validate every source before changing the installation.
    for target, patched in updates:
        target.write_bytes(patched)
    print('Verified pinned Mac2 desktop input and process attachment patches applied.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('driver', type=Path)
    prepare(parser.parse_args().driver.resolve())
