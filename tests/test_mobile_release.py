import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
SCRIPTS=Path(__file__).resolve().parents[1]/'scripts';sys.path.insert(0,str(SCRIPTS))
spec=importlib.util.spec_from_file_location('mobile_seal',SCRIPTS/'prepare-mobile-release.py');seal=importlib.util.module_from_spec(spec);spec.loader.exec_module(seal)

class MobileSealTest(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name);self.apk=self.root/'zerus.apk';self.apk.write_bytes(b'apk');self.source=self.root/'zerus-source.tar.gz';self.source.write_bytes(b'exact source snapshot')
        self.badging="package: name='app.zerus.mobile' versionCode='8' versionName='0.1.7'\nminSdkVersion:'26'\n"
        self.signature='Signer #1 certificate SHA-256 digest: '+'b'*64+'\n'
    def tearDown(self):self.tmp.cleanup()
    def tool(self,argv):return self.badging if argv[1]=='dump' else self.signature
    def test_exact_snapshot_seal_with_authoritative_metadata(self):
        out=self.root/'candidate';result=seal.seal(self.apk,self.source,out,'a'*40,'dev',True,Path('aapt2'),Path('apksigner'),self.tool)
        self.assertEqual(result['version_code'],8);self.assertTrue(result['source_dirty']);self.assertEqual(result['source_snapshot']['name'],self.source.name)
        sums=(out/'SHA256SUMS').read_text();self.assertIn('  release-info.json\n',sums);self.assertNotIn('SHA256SUMS\n',sums)
    def test_wrong_package_unsigned_multisigner_and_long_version(self):
        for issue in ('package','unsigned','multiple','version'):
            badging=self.badging;signature=self.signature
            if issue=='package':badging=badging.replace('app.zerus.mobile','other.app')
            elif issue=='unsigned':signature=''
            elif issue=='multiple':signature+=signature.replace('#1','#2')
            else:badging=badging.replace('0.1.7','a'*81)
            with self.subTest(issue=issue),self.assertRaises(ValueError):seal.apk_metadata(self.apk,Path('a'),Path('b'),lambda argv:badging if argv[1]=='dump' else signature)
    def test_legacy_sdk_debug_split_conflicting_sdk(self):
        legacy=self.badging.replace('minSdkVersion','sdkVersion');self.assertEqual(seal.apk_metadata(self.apk,Path('a'),Path('b'),lambda argv:legacy if argv[1]=='dump' else self.signature)['min_sdk'],26)
        for suffix in ("application-debuggable\n", "package: split='feature'\n", "sdkVersion:'27'\n"):
            with self.subTest(suffix=suffix),self.assertRaises(ValueError):seal.apk_metadata(self.apk,Path('a'),Path('b'),lambda argv:self.badging+suffix if argv[1]=='dump' else self.signature)
    def test_no_dirty_stable_and_no_replace(self):
        with self.assertRaises(ValueError):seal.seal(self.apk,self.source,self.root/'candidate','a'*40,'stable',True,Path('a'),Path('b'),self.tool)
        existing=self.root/'existing';existing.mkdir()
        with self.assertRaises(ValueError):seal.seal(self.apk,self.source,existing,'a'*40,'dev',True,Path('a'),Path('b'),self.tool)

if __name__=='__main__':unittest.main()
