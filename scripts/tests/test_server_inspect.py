import importlib.util
import pathlib
import tempfile
import unittest
from unittest.mock import patch

SOURCE = pathlib.Path(__file__).resolve().parents[1] / 'server-inspect.py'
spec = importlib.util.spec_from_file_location('server_inspect', SOURCE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class InspectionTests(unittest.TestCase):
    def test_absent_socket_is_read_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            with patch.dict('os.environ', {'XDG_RUNTIME_DIR':temporary, 'XDG_DATA_HOME':temporary}):
                value = module.inspect()
                self.assertFalse(value['running'])
                self.assertEqual(list(pathlib.Path(temporary).iterdir()), [])

    def test_connection_failure_is_not_treated_as_absent(self):
        with patch('socket.socket') as socket:
            socket.return_value.__enter__.return_value.connect.side_effect = PermissionError('blocked')
            with self.assertRaises(PermissionError):
                module.inspect()
