"""PRIVATE one-run module/cache diagnostic. Never merge or use owner state."""
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import struct
import tempfile
import threading
import time
import unittest

WORDS = ['', 'a"b', 'back\\', 'two\\\\tail', '日本語', 'Пример', '$(literal);& data', "it's a word"]
VARIANTS = [('baseline', 'baseline'), ('explicit-import', 'import'),
            ('no-autoload-import', 'no-autoload'), ('dotnet-parser', 'dotnet'),
            ('cache-cold', 'baseline'), ('cache-warm', 'baseline')]


def script():
    text = Path(__file__).with_name('windows_module_cache_probe.ps1').read_text(encoding='utf-8')
    payload = base64.b64encode(json.dumps(WORDS, ensure_ascii=False).encode()).decode()
    expected = ','.join("'"+word.replace("'", "''")+"'" for word in WORDS)
    assert text.count('__PAYLOAD__') == text.count('__EXPECTED__') == 1
    text = text.replace('__PAYLOAD__', payload).replace('__EXPECTED__', expected)
    return '\n'.join(line.strip() for line in text.splitlines() if line.strip() and not line.strip().startswith('#'))


def environment(root):
    env = {k: os.environ[k] for k in ('SystemRoot', 'WINDIR', 'ComSpec', 'PATHEXT') if k in os.environ}
    system = Path(env['SystemRoot'])
    profile = root/'profile space 日本語'
    profile.mkdir()
    for name in ('tmp', 'appdata', 'localappdata'):
        (root/name).mkdir()
    env.update(HOME=str(profile), USERPROFILE=str(profile), APPDATA=str(root/'appdata'),
               LOCALAPPDATA=str(root/'localappdata'), TEMP=str(root/'tmp'), TMP=str(root/'tmp'),
               PATH=os.pathsep.join(map(str, (system/'System32', system/'System32/WindowsPowerShell/v1.0'))),
               MODULE_PROBE_OUT=str(root/'phases.txt'))
    return env


def snapshot(path):
    try:
        if not path.exists():
            return {'exists': False}
        info = path.stat()
        value = {'exists': True, 'bytes': info.st_size, 'mtime_ns': info.st_mtime_ns}
        if info.st_size <= 16 << 20:
            with path.open('rb') as file: data = file.read((16 << 20)+1)
            if len(data) > 16 << 20: return {'unavailable': 'cache changed beyond read bound'}
            value['sha256'] = hashlib.sha256(data).hexdigest()
        else:
            value['hash_unavailable'] = '16 MiB observation bound'
        return value
    except OSError as error:
        return {'unavailable': type(error).__name__, 'errno': error.errno}


def phases(path):
    if not path.exists():
        return []
    with path.open('rb') as file: data = file.read(65537)
    if len(data) > 65536:
        raise AssertionError('phase file exceeds bound')
    rows = []
    for line in data.decode('utf-8-sig').splitlines():
        key, ticks, value = line.split('|')
        rows.append({'key': key, 'ticks': int(ticks),
                     'value': base64.b64decode(value, validate=True).decode('utf-8')})
    assert len(rows) <= 32 and len({x['key'] for x in rows}) == len(rows)
    return rows


def tail(path):
    with path.open('rb') as file:
        file.seek(0, 2); size = file.tell(); file.seek(max(0, size-4096))
        return {'bytes': size, 'tail': file.read(4096).decode('utf-8', errors='replace')}


@unittest.skipUnless(os.name == 'nt', 'ONE authorized hosted Windows diagnostic only')
class ModuleCache(unittest.TestCase):
    def probe(self, label, mode, cache=None):
        with tempfile.TemporaryDirectory(prefix='module-probe-') as name:
            root = Path(name); env = environment(root); env['MODULE_PROBE_MODE'] = mode
            if cache is not None:
                env.update(PSModuleAnalysisCachePath=str(cache), MODULE_CACHE_PROOF='1')
            before = None if cache is None else snapshot(cache)
            ps = Path(env['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe'
            encoded = base64.b64encode(script().encode('utf-8')).decode()
            command = "& ([ScriptBlock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('"+encoded+"'))))"
            self.assertLess(len(command), 8000)
            files = {k: (root/k).open('wb', buffering=0) for k in ('stdout', 'stderr')}
            workers = []; errors = []; timed_out = False
            def capture(pipe, key):
                try:
                    with pipe, files[key] as out:
                        while True:
                            data = pipe.read1(4096)
                            if not data: break
                            out.write(data)
                except Exception as error:
                    errors.append({'stream': key, 'type': type(error).__name__, 'errno': getattr(error, 'errno', None)})
            started = time.monotonic()
            try:
                child = subprocess.Popen([str(ps), '-NoLogo', '-NoProfile', '-NonInteractive', '-Command', command],
                                         env=env, cwd=root, stdin=subprocess.PIPE,
                                         stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                         creationflags=subprocess.CREATE_NO_WINDOW)
            except BaseException:
                for file in files.values(): file.close()
                raise
            for key in files:
                worker = threading.Thread(target=capture, args=(getattr(child, key), key))
                worker.start(); workers.append(worker)
            try:
                # Preserve the prior probes' held-open physical stdin and
                # original keepalive, avoiding a new closed-stdin confounder.
                child.stdin.write(struct.pack('<I', 0xffffffff)); child.stdin.flush()
                try: child.wait(timeout=45)  # Diagnostic observation only; production budgets unchanged.
                except subprocess.TimeoutExpired: timed_out = True
            finally:
                if child.poll() is None:
                    child.terminate(); child.wait(timeout=5)  # Only THIS original child-free probe.
                child.stdin.close()
                for worker in workers: worker.join(timeout=3)
            try:
                rows = phases(root/'phases.txt'); phase_error = None
            except Exception as error:
                rows = []; phase_error = type(error).__name__
            values = {r['key']: r for r in rows}
            frequency = int(values['entry']['value']) if 'entry' in values else None
            def duration(a, b):
                return (values[b]['ticks']-values[a]['ticks'])/frequency if a in values and b in values and frequency else None
            first_call = duration('entry', 'parse-after')
            census = values.get('module-census', {}).get('value', '').split('|')
            census = dict(zip(('roots', 'directories', 'module_candidates', 'version_units', 'complete',
                               'errors', 'visited', 'elapsed_ms'), census)) if len(census) == 8 else None
            value = {'kind': 'windows-module-cache', 'label': label, 'mode': mode,
                     'edition': values.get('edition', {}).get('value'), 'version': values.get('version', {}).get('value'),
                     'first_call_seconds': first_call, 'import_seconds': duration('import-before', 'import-after'),
                     'assembly_seconds': duration('load-before', 'load-after'), 'parse_seconds': duration('parse-before', 'parse-after'),
                     'exceeds_6s': None if first_call is None else first_call > 6,
                     'exceeds_8s': None if first_call is None else first_call > 8,
                     'process_wall_seconds': time.monotonic()-started, 'original_exit': child.returncode,
                     'timed_out_45s': timed_out, 'reader_errors': errors,
                     'phase_error': phase_error,
                     'module_census': census,
                     'readers_settled': all(not w.is_alive() for w in workers), 'phases': rows,
                     'stdout': tail(root/'stdout'), 'stderr': tail(root/'stderr'),
                     'cache_before': before, 'cache_after': None if cache is None else snapshot(cache)}
            frame = json.dumps(value, ensure_ascii=True)
            self.assertLess(len(frame), 65536)
            print('WINDOWS_MODULE_CACHE '+frame, flush=True)
            self.assertFalse(timed_out, 'diagnostic observation ceiling reached')
            self.assertEqual(child.returncode, 0)
            self.assertTrue(value['readers_settled']); self.assertEqual(errors, [])
            self.assertEqual(value['stderr']['bytes'], 0)
            self.assertIsNotNone(first_call)
            self.assertTrue(value['edition']); self.assertTrue(value['version'])
            self.assertEqual(values.get('verified', {}).get('value'), str(len(WORDS)))
            self.assertIn('module-census', values)
            return value

    def test_six_first_calls_once(self):
        with tempfile.TemporaryDirectory(prefix='module-cache-proof-') as name:
            cache = Path(name)/'ModuleAnalysisCache'
            self.assertFalse(cache.exists())
            for label, mode in VARIANTS:
                with self.subTest(label=label):
                    self.probe(label, mode, cache if label.startswith('cache-') else None)
