#!/usr/bin/env python3
"""Synthetic one-hop CLI: never SSHs or starts a model/native agent."""
import json
from pathlib import Path
import sys
import time

root = Path(__file__).parent
inventory = json.loads((root / 'inventory.json').read_text())
argv = sys.argv[1:]
payload = json.loads(sys.stdin.read() or 'null')
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps({'argv': argv, 'payload': payload}) + '\n')
if argv[0] == 'swarm' and argv[1] in {'mobile-peers', 'mobile-peer', '__mobile-peer-local'}:
    argv = argv[1:]
local = inventory['local']['machine_id']
machine = local
if argv == ['mobile-peers', '--json']:
    if inventory.get('discovery_delay'): time.sleep(inventory['discovery_delay'])
    print(json.dumps({key: inventory[key] for key in ('schema', 'local', 'peers')}))
    sys.exit()
if argv in (['mobile-peer', '--json'], ['__mobile-peer-local', '--json']):
    if argv[0] == 'mobile-peer':
        rows = [row for row in inventory['peers'] if row['via'] == payload['via'] and row['online']]
        if len(rows) != 1 or rows[0]['machine_id'] != payload['target_machine_id']: sys.exit(8)
        machine = rows[0]['machine_id']
        if inventory.get('peer_delay'): time.sleep(inventory['peer_delay'])
    elif payload['target_machine_id'] != local:
        sys.exit(8)
    argv, payload = payload['argv'], payload['payload']
run = 'local-run' if machine == local else 'peer-run'
conversation = 'local-conversation' if machine == local else 'peer-conversation'
base = {'name': 'codex/example', 'run_id': run, 'conversation_id': conversation, 'machine': machine}
if argv == ['--help']:
    print('mobile-peers --json\nmobile-peer --json\nhistory <session> --json\n--launch-id\nswarm assign-launch --json')
elif argv[0] == 'ls':
    launch = root / ('launch-' + machine + '.json')
    sessions = [base] + ([json.loads(launch.read_text())] if launch.exists() else [])
    print(json.dumps({'sessions': sessions, 'peers': [{'via': 'never-export-third-peer'}]}))
elif argv[0] == 'inspect':
    print(json.dumps({**base, 'events': [], 'pending_questions': []}))
elif argv[:2] == ['account', 'ls']:
    print(json.dumps({'profiles': [{'id': 'account-' + run, 'provider': 'codex', 'label': run, 'installed': True}]}))
elif argv[:2] == ['account', 'inspect']:
    print(json.dumps({'id': argv[2], 'provider': 'codex', 'status': 'ok'}))
elif argv == ['swarm', 'get']:
    print(json.dumps({'schema': 1, 'initialized': True, 'node_id': machine, 'swarm_id': 'fixture-swarm',
        'organization': {'projects': [{'id': 'project-' + run, 'name': run, 'folders': [{'id': 'folder-' + run, 'path': '/example/' + run, 'machine_id': machine}]}]}}))
elif argv == ['swarm', 'assign-launch', '--json']:
    (root / ('assigned-' + machine + '.json')).write_text(json.dumps(payload))
    print(json.dumps({**base, **payload, 'run_id': run, 'conversation_id': conversation, 'status': 'assigned'}))
elif argv[0] == 'dirs':
    print(json.dumps({'path': '/example/' + run, 'directories': []}))
elif argv[0] == 'codex':
    identity = argv[argv.index('--launch-id') + 1]
    (root / ('launch-' + machine + '.json')).write_text(json.dumps({**base, 'name': 'codex/new', 'launch_id': identity}))
    print('Created')
elif argv[0] == 'history':
    print(json.dumps({**base, 'request_id': payload['request_id'], 'events': [], 'history_epoch': run, 'head': {}}))
else:
    with (root / 'native_effects.jsonl').open('a') as log:
        log.write(json.dumps({'argv': argv, 'machine': machine}) + '\n')
    print(json.dumps({**base, 'request_id': payload['request_id'], 'status': 'submitted'}))
