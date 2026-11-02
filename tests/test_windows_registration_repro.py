"""Credential and byte-identity refusals for the temporary crash diagnostic."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
import urllib.request
from unittest.mock import patch
import zipfile


SOURCE = Path(__file__).resolve().parents[1] / 'scripts/ci/windows-registration-package-repro.py'
spec = importlib.util.spec_from_file_location('registration_repro', SOURCE)
repro = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repro)


class WindowsRegistrationRepro(unittest.TestCase):
    def test_storage_redirect_drops_repository_authorization(self):
        request = urllib.request.Request('https://api.github.com/example',
                                         headers={'Authorization': 'Bearer synthetic'})
        redirected = repro.PublicRedirect().redirect_request(
            request, None, 302, 'redirect', {}, 'https://storage.example.invalid/object')
        self.assertFalse(redirected.has_header('Authorization'))
        with self.assertRaises(ValueError):
            repro.PublicRedirect().redirect_request(
                request, None, 302, 'redirect', {}, 'http://storage.example.invalid/object')

    def test_corrupt_selected_image_refuses_before_creating_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            portable = io.BytesIO()
            with zipfile.ZipFile(portable, 'w') as inner:
                inner.writestr('zerus-windows-x86_64/hgs.exe', b'corrupt synthetic PE')
            body = portable.getvalue()
            archive = root / 'artifact.zip'
            with zipfile.ZipFile(archive, 'w') as outer:
                outer.writestr('zerus-windows-x86_64.zip', body)
            with patch.object(repro, 'PORTABLE_SIZE', len(body)), \
                    patch.object(repro, 'PORTABLE_DIGEST', hashlib.sha256(body).hexdigest()), \
                    patch.object(repro, 'FILES', {'hgs.exe': hashlib.sha256(b'expected synthetic PE').hexdigest()}):
                with self.assertRaisesRegex(ValueError, 'exact retained image differs'):
                    repro.prepare(archive, root / 'not-created')
            self.assertFalse((root / 'not-created').exists())


if __name__ == '__main__':
    unittest.main()
