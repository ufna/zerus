#!/usr/bin/env python3
"""Isolated desktop-v1 fixture; never launches a real CLI, service or agent."""
import json
import os
from pathlib import Path
import sys
import time
import uuid

root = Path(os.path.abspath(__file__)).parent
config = json.loads((root / 'fixture.json').read_text())
epoch = '00000000-0000-4000-8000-000000000001'
cursor = '00000000-0000-4000-8000-000000000002'
projection = 'a' * 32
abi = 'desktop-catalog-changes-v1'
args = sys.argv[1:]
value = json.loads(sys.stdin.readline()) if args[:2] == ['desktop', 'request'] else {}
with (root / 'calls').open('a') as log:
    log.write(json.dumps({'args': args, 'value': value}) + '\n')


def wait(path):
    until = time.monotonic() + 20
    while not path.exists():
        if time.monotonic() > until:
            raise SystemExit('fixture deadline')
        time.sleep(.005)


def counter(name):
    path = root / name
    number = int(path.read_text()) + 1 if path.exists() else 1
    path.write_text(str(number))
    return number


def status():
    online = not config.get('offline', False)
    counts = {'epoch': epoch, 'scope': 'shared_host_state', 'display_only': True, 'authority': False,
              'local_marks_included': False, 'computers': [{'computer': '', 'incarnation': 1, 'generation': 1,
              'online': online, 'complete': online, 'total': 2, 'attention': None}],
              'aggregate': {'complete': False, 'total': 2, 'attention': None, 'known_total': 2, 'known_attention': 0}}
    background = {name: {'ready': config.get('ready', False), 'code': 'fixture'}
                  for name in ['fleet', 'presence', 'reads', 'attention', 'approvals']}
    return {'abi': 'desktop-v1', 'configuration': 'a' * 64, 'epoch': epoch, 'generation': 1,
            'lifecycle': {'abi': 'desktop-lifecycle-v1', 'phase': 'serving', 'ipc_ready': True,
                          'configuration': 'a' * 64, 'epoch': epoch, 'generation': 1, 'background': background},
            'attention_counts': counts, 'automatic_approvals': False, 'catalog_reads': {'changes_abi': abi}}


def frame(seq, kind, body):
    print(json.dumps({'abi': 'desktop-v1', 'epoch': epoch, 'seq': seq, 't': kind, 'value': body}), flush=True)


def hint(seq, revision, selected=projection):
    frame(seq, 'catalog_changed', {'computer': '', 'incarnation': 1, 'generation': 1,
          'projection': selected, 'revision': revision, 'online': True, 'last_known': False,
          'complete': True, 'reset_required': False, 'changes_abi': abi, 'attention_counts': status()['attention_counts']})


def row(key):
    return {'key': key, 'row': {'name': 'codex/example/' + key, 'agent': 'codex', 'state': 'running', 'status': 'ready'}}


def change(key, revision, remove=False):
    return {'op': 'remove' if remove else 'upsert', 'key': key, 'revision': revision,
            'row': None if remove else row(key)['row']}


if args == ['--help']:
    print('Desktop service ABI: desktop-v1\nDesktop catalog changes ABI: ' + abi if config.get('supported', True) else 'Older fixture CLI')
elif args == ['desktop', 'status', '--json']:
    print(json.dumps(status()))
elif args == ['desktop', 'watch', '--json']:
    frame(0, 'hello', status())
    if config.get('dirty_walk'):
        wait(root / 'page-started')
        hint(1, '2')
        hint(2, '2')
        (root / 'hinted').touch()
        wait(root / 'delta-started')
        hint(3, '3')
        hint(4, '3')
        (root / 'delta-hinted').touch()
    if config.get('replace_projection'):
        wait(root / ('page-started' if config['replace_projection'] == 'early' else 'initial-complete'))
        hint(1, '0', 'b' * 32)
        (root / 'projection-replaced').touch()
    if config.get('large'):
        wait(root / 'initial-complete')
        hint(1, '3')
    sys.stdin.read()  # Detaches only when the owning subscriber closes stdin.
elif args == ['desktop', 'request', '--json']:
    op, request_id = value['op'], value['request_id']
    common = {'request_id': request_id, 'accepted': True, 'durable': False, 'abi': 'desktop-catalog-v1',
              'changes_abi': abi, 'epoch': epoch, 'computer': value.get('computer', ''), 'incarnation': 1,
              'generation': 1, 'projection': projection, 'display_only': True, 'authority': False,
              'complete': True, 'online': True, 'last_known': False, 'stale': False, 'next_cursor': None}
    if op == 'catalog_page':
        number = counter('page-number')
        page = dict(common, start_revision='0', revision='0', items=[row('row')])
        if config.get('dirty_walk'):
            page.update(start_revision='0', revision='2', stale=True, complete=False)
            if number == 1:
                (root / 'page-started').touch()
                wait(root / 'hinted')
                page.update(next_cursor=cursor, items=[row('first')])
        if config.get('replace_projection') == 'early' and number == 1:
            (root / 'page-started').touch()
            wait(root / 'projection-replaced')
            page.update(next_cursor=cursor)
        elif config.get('replace_projection') and (root / 'projection-replaced').exists():
            page.update(projection='b' * 32, items=[row('replacement')])
        if config.get('failure') and number > 1:
            page.update(start_revision='5', revision='5', items=[row('recovered')])
        if config.get('offline'):
            page.update(online=False, last_known=True, complete=False, start_revision=None)
        if config.get('large'):
            offset = (number - 1) * 128
            count = config['large']
            page.update(items=[row('row-' + str(i)) for i in range(offset, min(offset + 128, count))],
                        next_cursor=str(uuid.UUID(int=number + 100)) if offset + 128 < count else None)
        print(json.dumps(page))
    elif op == 'catalog_changes':
        number = counter('delta-number')
        page = dict(common, projection=value['projection'], since_revision=value['since_revision'],
                    through_revision=value['since_revision'], revision=value['since_revision'], items=[])
        if config.get('dirty_walk'):
            if number == 1:
                (root / 'delta-started').touch()
                wait(root / 'delta-hinted')
                page.update(through_revision='2', revision='3', stale=True, items=[change('inserted', '1'), change('first', '2', True)], next_cursor=cursor)
            elif number == 2:
                (root / 'delta-page-two').touch()
                wait(root / 'finish-delta')
                page.update(through_revision='2', revision='3', stale=True, items=[change('first', '2', True)])
            else:
                (root / 'refresh-started').touch()
                wait(root / 'finish-refresh')
                page.update(through_revision='3', revision='3', items=[change('fresh', '3')])
        if config.get('failure') == 'changed_through' and number <= 2:
            page.update(through_revision=str(number), revision=str(number), next_cursor=cursor if number == 1 else None)
        elif config.get('failure') and number == 1:
            if config['failure'] == 'unknown':
                raise SystemExit('uncertain fixture exchange')
            page = {'request_id': request_id, 'accepted': False, 'durable': False,
                    'code': 'catalog_changes_gap', 'reset_required': True}
        if config.get('replace_projection') and config['replace_projection'] != 'early' and number == 1:
            page.update(through_revision='1', revision='1', items=[change('obsolete', '1')], next_cursor=cursor)
            (root / 'initial-complete').touch()
            # The old final page races the new projection hint.
            wait(root / 'projection-replaced')
        if config.get('large'):
            if number == 1:
                (root / 'initial-complete').touch()
            else:
                page.update(through_revision='3', revision='3', items=[change('row-0', '1'), change('row-1', '2', True), change('new', '3')])
        print(json.dumps(page))
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
