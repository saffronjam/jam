"""Cold-checkout toolchain bootstrap must reach checksum-verified download."""
import os
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch


class BootstrapTests(unittest.TestCase):
    def test_nested_cache_is_created_before_the_first_download(self):
        script = Path(__file__).resolve().parents[1] / 'bootstrap.py'
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary) / 'clean-checkout' / '.cache' / 'remote-architecture'
            self.assertFalse(cache.parent.exists())
            # Stop at the network boundary without replacing checksum or archive
            # validation. The original failure happened before this boundary.
            with patch.dict(os.environ, {'JAM_BOOTSTRAP_CACHE': str(cache)}), patch(
                'urllib.request.urlopen', side_effect=RuntimeError('download reached')
            ) as download:
                with self.assertRaisesRegex(RuntimeError, '^download reached$'):
                    runpy.run_path(str(script), run_name='__main__')
            self.assertTrue(cache.is_dir())
            download.assert_called_once()


if __name__ == '__main__':
    unittest.main()
