#!/usr/bin/env python3
"""Isolated desktop-v1 fixture; never launches a real CLI, service or agent."""
import json
import os
from pathlib import Path
import sys
import time

root = Path(os.path.abspath(__file__)).parent
config = json.loads((root / 'fixture.json').read_text())
epoch = '00000000-0000-4000-8000-000000000001'
cursor = '00000000-0000-4000-8000-000000000002'
args = sys.argv[1:]
value = json.loads(sys.stdin.readline()) if args[:2] == ['desktop', 'request'] else {}
record = {'args': args, 'value': value}
with (root / 'calls').open('a') as log:
    log.write(json.dumps(record) + '\n')


def wait(path):
    until = time.monotonic() + 15
    while not path.exists():
        if time.monotonic() > until:
            raise SystemExit('fixture deadline')
        time.sleep(.005)


def status():
    counts = {'epoch': epoch, 'scope': 'shared_host_state', 'display_only': True, 'authority': False,
              'local_marks_included': False, 'computers': [{'computer': '', 'incarnation': 1, 'generation': 1,
              'online': True, 'complete': True, 'total': 2, 'attention': None}],
              'aggregate': {'complete': False, 'total': 2, 'attention': None, 'known_total': 2, 'known_attention': 0}}
    background = {name: {'ready': config.get('ready', False), 'code': 'fixture'}
                  for name in ['fleet', 'presence', 'reads', 'attention', 'approvals']}
    return {'abi': 'desktop-v1', 'configuration': 'a' * 64, 'epoch': epoch, 'generation': 1,
            'lifecycle': {'abi': 'desktop-lifecycle-v1', 'phase': 'serving', 'ipc_ready': True,
                          'configuration': 'a' * 64, 'epoch': epoch, 'generation': 1, 'background': background},
            'attention_counts': counts, 'automatic_approvals': False}


def frame(seq, kind, body):
    print(json.dumps({'abi': 'desktop-v1', 'epoch': epoch, 'seq': seq, 't': kind, 'value': body}), flush=True)


if args == ['--help']:
    print('Desktop service ABI: desktop-v1' if config.get('supported', True) else 'Older fixture CLI')
elif args == ['desktop', 'status', '--json']:
    print(json.dumps(status()))
elif args == ['desktop', 'watch', '--json']:
    frame(0, 'hello', status())
    if config.get('dirty_walk'):
        wait(root / 'page-started')
        for seq in [1, 2]:
            frame(seq, 'catalog_changed', {'computer': '', 'incarnation': 1, 'generation': 1,
                                           'attention_counts': status()['attention_counts']})
        (root / 'hinted').touch()
    sys.stdin.read()  # Detaches only when the owning subscriber closes stdin.
elif args == ['desktop', 'request', '--json']:
    op = value['op']
    request_id = value['request_id']
    if op == 'catalog_page':
        path = root / 'page-number'
        number = int(path.read_text()) + 1 if path.exists() else 1
        path.write_text(str(number))
        next_cursor = None
        if config.get('dirty_walk'):
            if number == 1:
                (root / 'page-started').touch()
                wait(root / 'hinted')
                next_cursor = cursor
            if number == 3:
                (root / 'refresh-started').touch()
                wait(root / 'finish-refresh')
        row = {'name': 'codex/example/row', 'agent': 'codex', 'state': 'running', 'status': 'ready'}
        print(json.dumps({'request_id': request_id, 'accepted': True, 'durable': False,
                          'abi': 'desktop-catalog-v1', 'epoch': epoch, 'computer': value['computer'],
                          'display_only': True, 'authority': False, 'complete': True,
                          'items': [{'key': 'first' if config.get('dirty_walk') and number == 1 else 'row', 'row': row}], 'next_cursor': next_cursor}))
    elif op == 'read_intent':
        path = root / 'read-seen'
        confirmed = path.exists()
        path.touch()
        print(json.dumps({'request_id': request_id, 'accepted': True, 'durable': confirmed, 'op': op}))
    elif op == 'delivery_result':
        print(json.dumps({'request_id': request_id, 'accepted': True, 'durable': False, 'op': op,
                          'original_request_id': value['original_request_id'], 'found': False, 'result': None}))
    elif op == 'catalog_close':
        print(json.dumps({'request_id': request_id, 'accepted': True, 'durable': False}))
    else:
        raise SystemExit('Unexpected fixture operation')
else:
    raise SystemExit('Forbidden fallback CLI call')
