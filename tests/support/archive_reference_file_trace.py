#!/usr/bin/env python3
"""Disposable Windows archive-writer diagnostic; file I/O only, no contents.

The exact successful start owns one random ETW session. Stop only that session.
The raw circular trace stays in the private fixture; print only scoped events.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import xml.etree.ElementTree as ET

PROVIDER = 'Microsoft-Windows-Kernel-File'


def local(tag):
    return tag.rsplit('}', 1)[-1]


def selected_events(path, scope):
    if path.stat().st_size > 128 * 1024 * 1024:
        raise ValueError('trace XML exceeds diagnostic cap')
    rows = []
    objects = set()
    needle = (scope.name + r'\state\archive-reference-writes-v1').casefold()
    for _, event in ET.iterparse(path, events=['end']):
        if local(event.tag) != 'Event':
            continue
        fields = {}
        for node in event.iter():
            name = node.get('Name') or local(node.tag)
            if node.text and node.text.strip() and len(node) == 0:
                fields[name] = node.text.strip()[:1024]
            if local(node.tag) in ('Execution', 'TimeCreated'):
                fields.update(node.attrib)
        if any(needle in value.casefold() for value in fields.values()):
            for key, value in fields.items():
                if any(part in key.casefold() for part in ('fileobject', 'filekey')):
                    if value.lower().strip("0x"):
                        objects.add(value)
        rows.append(fields)
        event.clear()
        if len(rows) > 100000:
            raise ValueError('trace event count exceeds diagnostic cap')
    result = []
    for row in rows:
        named = any(needle in value.casefold() for value in row.values())
        linked = any(value in objects for key, value in row.items()
                     if any(part in key.casefold() for part in ('fileobject', 'filekey')))
        if not (named or linked):
            continue
        row = dict(row)
        for key, value in row.items():
            if ('name' in key.casefold() or 'path' in key.casefold()) and ('\\' in value or ':/' in value) and scope.name.casefold() not in value.casefold():
                row[key] = '<outside-private-scope>'
        result.append(row)
    return result



def process_identity(pid):
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    kernel.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        return {'pid': pid, 'unavailable': ctypes.get_last_error()}
    try:
        times = [wintypes.FILETIME() for _ in range(4)]
        if not kernel.GetProcessTimes(handle, *(ctypes.byref(value) for value in times)):
            return {'pid': pid, 'unavailable': ctypes.get_last_error()}
        birth = (times[0].dwHighDateTime << 32) | times[0].dwLowDateTime
        size = wintypes.DWORD(1024)
        image = ctypes.create_unicode_buffer(size.value)
        if not kernel.QueryFullProcessImageNameW(handle, 0, image, ctypes.byref(size)):
            return {'pid': pid, 'birth': birth, 'image_unavailable': ctypes.get_last_error()}
        return {'pid': pid, 'birth': birth, 'image': image.value}
    finally:
        kernel.CloseHandle(handle)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode', choices=['start', 'stop'])
    p.add_argument('--session', required=True)
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--scope', type=Path, required=True)
    args = p.parse_args()
    if os.name != 'nt' or not args.session.startswith('hgs-archive-'):
        p.error('private Windows diagnostic only')
    loader = importlib.util.spec_from_file_location('outside_job', Path(__file__).resolve().parents[2] / 'scripts/ci/outside-job.py')
    utility = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(utility)
    system = Path(os.environ['SystemRoot']) / 'System32'
    etl = args.root / 'file.etl'
    def run(argv, seconds=8):
        result = utility.bounded_run([str(x) for x in argv], timeout=seconds,
                                     capture_output=True, text=True)
        print(json.dumps({'archive_trace_command': [str(x) for x in argv],
                          'code': result.returncode, 'out': result.stdout[:4096],
                          'error': result.stderr[:4096]}), flush=True)
        return result.returncode
    if args.mode == 'start':
        return run([system / 'logman.exe', 'start', args.session, '-p', PROVIDER,
                    '0xffffffffffffffff', '5', '-o', etl, '-f', 'bincirc', '-max', '16', '-ets'])
    code = run([system / 'logman.exe', 'stop', args.session, '-ets'])
    if code:
        return code
    xml = args.root / 'file.xml'
    summary = args.root / 'file-summary.txt'
    code = run([system / 'tracerpt.exe', etl, '-of', 'XML', '-o', xml, '-summary', summary, '-y'], 12)
    if code:
        return code
    if summary.exists():
        raw = summary.read_bytes()[:8192]
        text = raw.decode("utf-16" if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig", errors="replace")
        print("archive_trace_summary " + json.dumps(text), flush=True)
    rows = selected_events(xml, args.scope)
    print(json.dumps({'archive_trace_scoped_count': len(rows), 'printed_cap': 160}), flush=True)
    pids = set()
    for row in rows:
        for key, value in row.items():
            if key.casefold() in ('processid', 'pid', 'process_id'):
                try:
                    pid = int(value, 0)
                    if 0 < pid < (1 << 32):
                        pids.add(pid)
                except ValueError:
                    pass
    for pid in sorted(pids)[:16]:
        print('archive_trace_process ' + json.dumps(process_identity(pid)), flush=True)
    for row in rows[-160:]:
        print('archive_file_event ' + json.dumps(row), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
