"""Linux Codex ordinary input with real product launch pins and a private PTY."""
import copy
from contextlib import closing
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import signal
import sqlite3
import subprocess
import sys
import unittest
import uuid

from test_pause import HGS, Harness, TMUX


@unittest.skipUnless(sys.platform.startswith('linux') and TMUX, 'private Linux tmux and kernel births')
class NativeReadiness(Harness):
    def setUp(self):
        super().setUp()
        self.env.update({f'XDG_{name}_HOME': str(self.root / name.lower())
                         for name in ('CONFIG', 'STATE', 'DATA', 'CACHE')})
        producer = self.bin / 'codex'
        old = producer.read_text()
        start = 'event("SessionStart")\nfor line in sys.stdin:'
        self.assertIn(start, old)
        producer.write_text(old.split(start, 1)[0] + r'''
event("SessionStart")
import tty
tty.setraw(0)
def paint(*_):
    sys.stdout.write("\x1b[2J\x1b[H\x1b[?2004h› " +
                     ("retained draft" if (home / "native-draft").exists() else ""))
    sys.stdout.flush()
def change_owner(*_):
    args = (home / "native-owner-command").read_text().splitlines()
    os.execv(args[0], args)
def call_from_agent(*_):
    args, payload = json.loads((home / "native-agent-call").read_bytes())
    result = subprocess.run(args, input=json.dumps(payload), capture_output=True, text=True, timeout=9)
    output = home / "native-agent-result-temp"
    output.write_text(json.dumps([result.returncode, result.stdout, result.stderr]))
    output.replace(home / "native-agent-result")
signal.signal(signal.SIGUSR1, paint)
signal.signal(signal.SIGUSR2, change_owner)
signal.signal(signal.SIGURG, call_from_agent)
(home / f"native-caller-ready-{os.getpid()}").touch()
paint()
with (home / "native-input").open("ab", buffering=0) as log:
    while data := os.read(0, 4096):
        log.write(data)
''')
        self.name = self.start('codex', 'native-clock-drift')
        self.wait(lambda: (self.root / 'native-input').exists())
        self.send_event(self.name, 'Stop', last_assistant_message='Private fixture only')
        record = self.binding(self.name)
        paths = [self.root / 'state' / (hashlib.sha256(self.name.encode()).hexdigest() + '.json'),
                 self.root / 'state' / 'sessions' / (record['session_id'] + '.json')]
        self.path = next(path for path in paths if path.is_file() and json.loads(path.read_bytes()) == record)
        self.original = record
        self.drifted = copy.deepcopy(record)
        pin = self.drifted['process_index_pin']
        self.assertEqual(self.stat(pin['child']['pid'])[1], str(pin['supervisor']['pid']))
        for role in ('child', 'supervisor'):
            self.assertEqual(int(self.stat(pin[role]['pid'])[19]), pin[role]['birth']['Linux'])
        for role, place, key in (('child_start', self.drifted, 'process_start'),
                                 ('supervisor_start', self.drifted['supervisor'], 'start')):
            start = datetime.strptime(place[key], '%a %b %d %H:%M:%S %Y')
            place[key] = pin[role] = (start - timedelta(seconds=1)).strftime('%a %b %e %H:%M:%S %Y')
        self.save(self.drifted)

    @staticmethod
    def stat(pid):
        return Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()

    def save(self, record):
        path = self.path.with_suffix('.tmp-fixture')
        path.write_text(json.dumps(record))
        path.chmod(0o600)
        path.replace(self.path)

    def bytes_received(self):
        return (self.root / 'native-input').read_bytes()

    def request(self, **change):
        return dict({'text': 'One private readiness message', 'request_id': str(uuid.uuid4()),
                     'expected_run_id': self.original['run_id'],
                     'expected_conversation_id': self.original['conversation_id']}, **change)

    def call(self, args, payload=None):
        return subprocess.run([str(HGS), *args], input=json.dumps(payload) if payload is not None else None,
                              env=self.env, capture_output=True, text=True, timeout=9)

    def observe(self, payload, command='send-verified'):
        result = self.call([command, self.name, '--observe', '--json'],
                           {key: payload[key] for key in ('expected_run_id','expected_conversation_id')})
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def inspect_pending(self):
        result = self.call(['inspect', self.name, '--after', str(2**63-1)])
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(value['process_state'], 'exited')
        self.assertNotIn('previous_message_pending', value)
        return value.get('verified_previous_message_pending')

    def refusal(self, request=None, *, reason='exact agent process'):
        request = request or self.request()
        before = self.path.read_bytes()
        offered = self.bytes_received()
        observed = self.observe(request)
        self.assertIs(observed['ready'], False, observed)
        self.assertIn(reason, observed['reason'])
        self.assertEqual(self.path.read_bytes(), before)
        result = self.call(['send-verified', self.name, '--json'], request)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(reason, result.stderr)
        marker = self.root / 'state' / 'input_receipts' / (request['request_id'] + '.json')
        if marker.exists():
            self.assertNotIn(json.loads(marker.read_bytes()).get('status'), ['submitted','in_progress'])
        self.assertEqual(self.bytes_received(), offered)
        self.assertEqual(self.path.read_bytes(), before)

    def test_original_launch_recovers_ordinary_send_and_observe_writes_nothing(self):
        request = self.request()
        before = self.path.read_bytes()
        receipts = self.root / 'state/input_receipts'
        self.assertFalse(receipts.exists())
        self.assertEqual(self.observe(request), {'ready':True,'reason':''})
        self.assertEqual(self.path.read_bytes(), before)
        self.assertFalse(receipts.exists())
        self.assertEqual(self.bytes_received(), b'')
        self.assertIs(self.inspect_pending(), False)

        send = self.call(['send-verified', self.name, '--json'], request)
        self.assertEqual(send.returncode, 0, send.stderr)
        receipt = json.loads(send.stdout)
        self.assertEqual((receipt['status'], receipt['transport']), ('submitted','terminal'))
        self.assertEqual(receipt['run_id'], request['expected_run_id'])
        self.assertEqual(receipt['conversation_id'], request['expected_conversation_id'])
        expected = b'\x1b[200~'+request['text'].encode()+b'\x1b[201~\r'
        self.wait(lambda: self.bytes_received() == expected)
        replay = self.call(['send-verified', self.name, '--json'], request)
        self.assertEqual(replay.returncode, 0, replay.stderr)
        self.assertEqual(json.loads(replay.stdout), receipt)
        self.assertEqual(self.bytes_received(), expected)

        next_request = self.request()
        self.assertIn('previous input has not been confirmed', self.observe(next_request)['reason'])
        self.assertIs(self.inspect_pending(), True)
        current = json.loads(self.path.read_bytes())
        transcript = self.root / 'accepted-native-prompt.jsonl'
        pending = current['input_pending_at']
        started, consumed = (datetime.fromtimestamp(pending+delta, timezone.utc).isoformat()
                             for delta in (.001, .002))
        rows = [
            {'type':'session_meta','payload':{'id':current['conversation_id']}},
            {'type':'event_msg','timestamp':started,'payload':{'type':'task_started','turn_id':'private-turn'}},
            {'type':'response_item','timestamp':consumed,'payload':{'type':'message','role':'user',
             'content':[{'type':'input_text','text':receipt['submitted_text']}]}},
        ]
        transcript.write_text(''.join(json.dumps(row)+'\n' for row in rows))
        current['transcript'] = str(transcript)
        self.save(current)
        receipt_path = receipts / (request['request_id'] + '.json')
        binding, original_receipt = self.path.read_bytes(), receipt_path.read_bytes()
        self.assertEqual(self.observe(next_request), {'ready':True,'reason':''})
        self.assertEqual(self.path.read_bytes(), binding, 'observed native acceptance must stay in memory')
        self.assertEqual(receipt_path.read_bytes(), original_receipt)
        self.assertEqual(self.bytes_received(), expected)
        self.assertIs(self.inspect_pending(), False)

    def test_identity_changes_and_unavailable_proof_refuse_without_bytes(self):
        for case in ('child-birth', 'supervisor-birth', 'boot', 'missing-pin', 'wrong-pane',
                     'non-private-record', 'rename-shadow', 'archived-binding'):
            with self.subTest(case=case):
                changed = copy.deepcopy(self.drifted)
                pin = changed['process_index_pin']
                if case in ('child-birth', 'supervisor-birth'):
                    pin[case.split('-')[0]]['birth']['Linux'] += 1
                if case == 'boot':
                    pin['boot'] = 'different boot or namespace'
                if case == 'missing-pin':
                    del changed['process_index_pin']
                if case == 'wrong-pane':
                    changed['pane'] = '%9999999'
                if case == 'rename-shadow':
                    changed.update(rename_shadow=True, rename_target='codex/p/current-label')
                if case == 'archived-binding':
                    changed['state'] = 'archived'
                self.save(changed)
                if case == 'non-private-record':
                    self.path.chmod(0o644)
                self.refusal()

        self.save(self.drifted)
        self.refusal(self.request(expected_run_id=str(uuid.uuid4())), reason='session identity changed')
        self.refusal(self.request(expected_conversation_id=str(uuid.uuid4())), reason='session identity changed')
        self.t('set-environment','-t','='+self.name,'HGS_SESSION_ID',str(uuid.uuid4()))
        self.refusal()

    def test_different_live_pid_cannot_stand_in_for_original_birth(self):
        with subprocess.Popen(['sleep','15']) as other:
            try:
                substitute = copy.deepcopy(self.drifted)
                birth = int(self.stat(other.pid)[19])
                self.assertNotEqual(birth, substitute['process_index_pin']['child']['birth']['Linux'])
                substitute['pid'] = substitute['process_index_pin']['child']['pid'] = other.pid
                self.save(substitute)
                self.refusal()
            finally:
                other.terminate()

    def test_empty_composer_and_independent_pause_gate_still_refuse(self):
        (self.root/'native-draft').touch()
        with os.fdopen(os.pidfd_open(self.drifted['pid']), 'rb') as child:
            signal.pidfd_send_signal(child.fileno(), signal.SIGUSR1)
        self.wait(lambda: 'retained draft' in self.t('capture-pane','-p','-t',self.drifted['pane']))
        self.refusal(reason='terminal has a draft')
        self.assertIn('retained draft', self.t('capture-pane','-p','-t',self.drifted['pane']))
        pause = self.call(['pause', self.name])
        self.assertNotEqual(pause.returncode, 0, pause.stdout)
        self.assertEqual(self.bytes_received(), b'')

    def private_machine(self):
        result = self.call(['swarm', '__mobile-peer-identity', '--json'])
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)['machine_id']

    def test_plain_desktop_teams_deferred_and_mobile_abis_keep_legacy_gate(self):
        # The product builders send exactly these plain observer and send forms
        # for Desktop resume and Teams, and the v2 form for deferred delivery.
        request = self.request()
        before = self.path.read_bytes()
        legacy = self.observe(request, command='send')
        self.assertIs(legacy['ready'], False, legacy)
        self.assertIn('exact agent process', legacy['reason'])
        machine = self.private_machine()
        ordinary = ['send', self.name, '--json']
        v2 = ['send', self.name, '--delivery-outcome-v2', '--json']
        for args in [ordinary, v2, ['__state',*ordinary],
                     ['--expect-machine',machine,*ordinary],
                     ['--expect-machine',machine,'__state',*ordinary],
                     ['send',self.name,'--session-id',self.original['session_id'],'--json'],
                     ['--client','local',*ordinary], ['@test',*ordinary]]:
            with self.subTest(args=args):
                ref = self.call(args, self.request())
                self.assertNotEqual(ref.returncode, 0, ref.stdout)
                self.assertIn('exact agent process', ref.stderr)
                if args == v2:
                    outcome = json.loads(ref.stdout)
                    self.assertEqual((outcome['schema'], outcome['status'], outcome['proof']),
                                     (2, 'not_offered', 'native_input_lock'))
        # Exercise the actual local mobile envelope, including its in-process
        # redispatch. Both mobile generations use the existing CLI schema.
        for argv in (ordinary, v2):
            with self.subTest(mobile=argv):
                envelope = dict(schema=1,target_machine_id=machine,argv=argv,payload=self.request())
                mobile = self.call(['swarm','__mobile-peer-local','--json'], envelope)
                self.assertNotEqual(mobile.returncode, 0, mobile.stdout)
                self.assertIn('exact agent process', mobile.stderr)
                if argv == v2:
                    outcome = json.loads(mobile.stdout)
                    self.assertEqual((outcome['schema'], outcome['status'], outcome['proof']),
                                     (2, 'not_offered', 'native_input_lock'))
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(self.bytes_received(), b'')

    def test_verified_route_refuses_aliases_and_mobile_envelope(self):
        machine = self.private_machine()
        command = ['send-verified',self.name,'--json']
        observe = ['send-verified',self.name,'--observe','--json']
        args_to_refuse = [
            ['__state',*command], ['__state',*observe],
            ['--expect-machine',machine,*command],
            ['--expect-machine',machine,'__state',*command],
            ['--client','local',*command], ['@test',*command], ['@no-such-peer',*observe],
            ['--dry-run',*command],
            ['send-verified',self.name,'--delivery-outcome-v2','--json'],
            ['send-verified',self.name,'--session-id',self.original['session_id'],'--json'],
            ['send-verified','codex/project/tag@peer','--json'],
            ['send-verified','zerus://session?name=codex%2Fproject','--json'],
        ]
        for args in args_to_refuse:
            with self.subTest(args=args):
                ref = self.call(args, self.request())
                self.assertNotEqual(ref.returncode, 0, ref.stdout)
                self.assertIn('as a direct command', ref.stderr)
        for argv in (command, observe):
            with self.subTest(mobile=argv):
                envelope = dict(schema=1,target_machine_id=machine,argv=argv,payload=self.request())
                mobile = self.call(['swarm','__mobile-peer-local','--json'], envelope)
                self.assertNotEqual(mobile.returncode, 0, mobile.stdout)
                self.assertIn('native operation is not allowed', mobile.stderr)
        for options in ({'only_when_idle':True}, {'expected_compaction_id':str(uuid.uuid4())}):
            ref = self.call(command, self.request(**options))
            self.assertNotEqual(ref.returncode, 0, ref.stdout)
            self.assertIn('Verified send accepts only an ordinary message', ref.stderr)
        self.assertEqual(self.bytes_received(), b'')

    def test_tracked_agent_cannot_skip_ordinary_sender_attribution(self):
        caller = self.start('codex', 'native-caller')
        origin = self.seed_native_child(caller)
        self.wait(lambda: (self.root / f"native-caller-ready-{origin['pid']}").exists())
        request = self.request()
        response = self.root / 'native-agent-result'
        ledger = self.root / 'state/graph/sends.sqlite3'

        def agent_call(command, payload):
            response.unlink(missing_ok=True)
            (self.root / 'native-agent-call').write_text(json.dumps([[str(HGS), *command], payload]))
            with os.fdopen(os.pidfd_open(origin['pid']), 'rb') as child:
                signal.pidfd_send_signal(child.fileno(), signal.SIGURG)
            self.wait(response.exists)
            return json.loads(response.read_bytes())

        refused = []
        for flags, payload in ((['--observe'], {key: request[key] for key in
                                                ('expected_run_id', 'expected_conversation_id')}),
                               ([], request)):
            status, stdout, error = agent_call(['send-verified', self.name, *flags, '--json'], payload)
            refused.append(status != 0)
            with self.subTest(observe=bool(flags)):
                self.assertNotEqual(status, 0, stdout)
                self.assertIn('unavailable from a tracked agent', error)
                self.assertEqual(self.bytes_received(), b'')
                self.assertFalse((self.root / 'state' / 'scan-cache').exists())
        self.assertFalse(ledger.exists())
        if not all(refused):
            return  # A failing candidate may already have offered this original.

        self.save(self.original)
        status, output, error = agent_call(['send', self.name, '--json'], request)
        self.assertEqual(status, 0, error)
        self.wait(lambda: self.bytes_received().endswith(b'\r'))
        with sqlite3.connect(f'{ledger.as_uri()}?mode=ro', uri=True) as db:
            events = [json.loads(row[0]) for row in db.execute('SELECT payload FROM sends')]
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]['status'], 'submitted')
        self.assertEqual(events[0]['source']['endpoint']['conversation'], origin['conversation_id'])
        self.assertEqual(events[0]['target'], json.loads(output)['graph_target'])

    def seed_native_child(self, name):
        child = str(uuid.uuid4())
        self.send_event(name, 'SubagentStart', agent_id=child, agent_type='Review')
        self.send_event(name, 'Stop', last_assistant_message='Private parent result')
        record = self.binding(name)
        self.assertEqual(record['subagents'][child]['state'], 'working')
        account = Path(record['agent_home']) if record['agent_home'] else self.root / '.codex'
        self.assertTrue(account.is_relative_to(self.root))
        rollout = account / 'private-child.jsonl'
        with closing(sqlite3.connect(account / 'state_5.sqlite')) as db, db:
            db.execute('CREATE TABLE threads(id TEXT PRIMARY KEY, rollout_path TEXT)')
            db.execute('INSERT INTO threads VALUES (?, ?)', (child, str(rollout)))
        header = dict(type='session_meta', payload=dict(id=child, parent_thread_id=record['conversation_id']))
        event = dict(type='event_msg', timestamp=datetime.now(timezone.utc).isoformat(),
                     payload=dict(type='task_complete', turn_id=str(uuid.uuid4()),
                                  last_agent_message='Private child result'))
        rollout.write_text(json.dumps(header) + '\n' + json.dumps(event) + '\n')
        self.assertFalse((self.root / 'state' / 'scan-cache').exists())
        return record

    def test_verified_observation_preserves_the_targets_cold_scan_cache(self):
        self.save(self.original)
        self.seed_native_child(self.name)
        before = self.path.read_bytes()
        self.assertEqual(self.observe(self.request()), {'ready': True, 'reason': ''})
        self.assertFalse((self.root / 'state' / 'scan-cache').exists())
        self.assertFalse((self.root / 'state' / 'input_receipts').exists())
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(self.bytes_received(), b'')

    def test_binding_loss_after_final_capture_refuses_paste_and_replay(self):
        self.assert_binding_loss_refuses_input(after_paste=False)

    def test_binding_loss_after_paste_never_presses_or_replays_enter(self):
        self.assert_binding_loss_refuses_input(after_paste=True)

    def assert_binding_loss_refuses_input(self, *, after_paste):
        request = self.request()
        self.assertIs(self.observe(request)['ready'], True)
        bad_terminal_session = str(uuid.uuid4())
        receipt_path = self.root / 'state/input_receipts' / (request['request_id'] + '.json')
        wrapper = self.bin / 'tmux'
        wrapper.write_text(f"""#!{sys.executable}
import os, subprocess, sys
from pathlib import Path
tmux = {[TMUX, '-S', self.socket, '-f', '/dev/null']!r}
args = sys.argv[1:]
after_paste = {after_paste!r}
trigger = ('paste-buffer' in args if after_paste
           else 'capture-pane' in args and Path({str(receipt_path)!r}).exists())
if trigger:
    subprocess.run([*tmux, *args], check=True)
    subprocess.run([*tmux, 'set-environment', '-t', {'='+self.name!r},
                    'HGS_SESSION_ID', {bad_terminal_session!r}], check=True)
else:
    os.execv(tmux[0], [*tmux, *args])
""")
        result = self.call(['send-verified', self.name, '--json'], request)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        stage = 'after paste' if after_paste else 'before paste'
        self.assertIn(f'delivery uncertain: agent changed {stage}', result.stderr)
        offered = b'\x1b[200~'+request['text'].encode()+b'\x1b[201~' if after_paste else b''
        self.wait(lambda: self.bytes_received() == offered)
        receipt = receipt_path.read_bytes()
        self.assertEqual(json.loads(receipt)['delivery'], 'uncertain')
        self.assertEqual(json.loads(receipt)['status'], 'in_progress')
        self.t('set-environment', '-t', '='+self.name, 'HGS_SESSION_ID', self.original['session_id'])
        replay = self.call(['send-verified', self.name, '--json'], request)
        self.assertNotEqual(replay.returncode, 0, replay.stdout)
        self.assertIn('uncertain', replay.stderr)
        self.assertEqual(receipt_path.read_bytes(), receipt)
        self.assertEqual(self.bytes_received(), offered)

    def test_same_birth_with_wrong_kernel_owner_refuses(self):
        commands = [shutil.which(name) for name in ('unshare','setpriv','sleep')]
        if not all(commands):
            self.skipTest('private subordinate UID fixture needs unshare, setpriv and sleep')
        unshare, setpriv, sleep = commands
        flags = ['--user','--map-auto','--map-root-user']
        probe = subprocess.run([unshare,*flags,'true'],capture_output=True,timeout=5)
        if probe.returncode:
            self.skipTest('private subordinate UID mapping unavailable')
        pid = self.drifted['pid']
        stem = Path(f'/proc/{pid}')
        start = self.stat(pid)[19]
        (self.root/'native-owner-command').write_text('\n'.join(
            [unshare,*flags,setpriv,'--reuid','1','--regid','1','--clear-groups',sleep,'4']))
        with os.fdopen(os.pidfd_open(pid), 'rb') as child:
            signal.pidfd_send_signal(child.fileno(), signal.SIGUSR2)
        self.wait(lambda: stem.exists() and stem.stat().st_uid != os.geteuid())
        try:
            self.assertEqual(self.stat(pid)[19], start)
            self.assertEqual(self.stat(pid)[1], str(self.drifted['supervisor']['pid']))
            self.refusal()
        finally:
            self.wait(lambda: not stem.exists())

    def lifecycle_request(self, **fields):
        return dict({"request_id": str(uuid.uuid4()), "expected_run_id": self.original["run_id"],
                     "expected_conversation_id": self.original["conversation_id"]}, **fields)

    def test_drifted_desktop_pause_rechecks_the_selected_launch(self):
        args = ["session-action", self.name, "--json"]
        before = self.path.read_bytes()
        stale = self.lifecycle_request(action="pause", expected_run_id=str(uuid.uuid4()))
        refused = self.call(args, stale)
        self.assertEqual(json.loads(refused.stdout)["status"], "failed", refused.stderr)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(self.bytes_received(), b"")
        with os.fdopen(os.pidfd_open(self.original["pid"]), "rb") as child:
            result = self.call(args, self.lifecycle_request(action="pause"))
            receipt = json.loads(result.stdout)
            self.assertEqual(receipt["status"], "completed", receipt)
            exited = select.poll()
            exited.register(child, select.POLLIN)
            self.assertTrue(exited.poll(0), "Pause must observe the original child exit")
        self.assertTrue(self.binding(self.name)["paused"])
        self.assertEqual(self.bytes_received(), b"")

    def test_drifted_desktop_stop_is_pinned_to_the_current_turn(self):
        self.send_event(self.name, "UserPromptSubmit", prompt="Synthetic turn only")
        current = self.binding(self.name)
        self.assertGreater(current["turn_started"], 0)
        self.assertEqual(current["activity"], "busy")
        args = ["interrupt", self.name, "--json"]
        before = self.path.read_bytes()
        outdated = self.lifecycle_request(expected_turn_started=current["turn_started"] - 1)
        result = self.call(args, outdated)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("turn changed", result.stderr)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(self.bytes_received(), b"")

        exact = self.lifecycle_request(expected_turn_started=current["turn_started"])
        result = self.call(args, exact)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "submitted")
        self.wait(lambda: self.bytes_received() == b"\x1b")
        self.assertNotEqual(self.call(args, exact).returncode, 0)
        self.assertEqual(self.bytes_received(), b"\x1b")
        self.assertEqual(self.stat(self.original["pid"])[19], str(self.drifted["process_index_pin"]["child"]["birth"]["Linux"]))

    def test_drifted_desktop_terminate_keeps_the_original_archive(self):
        request = self.lifecycle_request(action="terminate")
        with os.fdopen(os.pidfd_open(self.original["pid"]), "rb") as child:
            result = self.call(["session-action", self.name, "--json"], request)
            self.assertEqual(result.returncode, 0, result.stderr)
            receipt = json.loads(result.stdout)
            self.assertEqual(receipt["status"], "completed", receipt)
            exited = select.poll()
            exited.register(child, select.POLLIN)
            self.assertTrue(exited.poll(0), "Terminate must observe the original child exit")
        archived = [json.loads(path.read_bytes()) for path in (self.root / "state/archives").glob("*.json")]
        self.assertEqual(len(archived), 1, archived)
        self.assertEqual(archived[0]["conversation_id"], self.original["conversation_id"])
        self.assertEqual(archived[0]["run_id"], self.original["run_id"])
        self.assertEqual(self.bytes_received(), b"")

    def test_drifted_desktop_actions_refuse_substituted_births_without_effect(self):
        for role in ("child", "supervisor"):
            with self.subTest(role=role):
                changed = copy.deepcopy(self.drifted)
                changed["process_index_pin"][role]["birth"]["Linux"] += 1
                self.save(changed)
                before = self.path.read_bytes()
                pause = self.call(["session-action", self.name, "--json"], self.lifecycle_request(action="pause"))
                self.assertEqual(json.loads(pause.stdout)["status"], "failed")
                self.assertEqual(self.path.read_bytes(), before)
                end = self.call(["session-action", self.name, "--json"], self.lifecycle_request(action="terminate"))
                self.assertEqual(json.loads(end.stdout)["status"], "failed")
                self.assertEqual(self.path.read_bytes(), before)
                self.assertEqual(self.bytes_received(), b"")
                self.assertFalse(list((self.root / "state/archives").glob("*.json")))
                self.assertIn(self.name, self.t("list-sessions", "-F", "#{session_name}"))
        self.send_event(self.name, "UserPromptSubmit", prompt="Synthetic turn only")
        before = self.path.read_bytes()
        current = self.binding(self.name)
        stop = self.call(["interrupt", self.name, "--json"],
                         self.lifecycle_request(expected_turn_started=current["turn_started"]))
        self.assertNotEqual(stop.returncode, 0)
        self.assertIn("Session identity changed", stop.stderr)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(self.bytes_received(), b"")

    def test_drifted_desktop_terminate_keeps_binding_without_original_terminal(self):
        self.t("rename-session", "-t", "=" + self.name, "private-renamed")
        for pin_kind in ("original", "unavailable", "changed"):
            with self.subTest(pin=pin_kind):
                current = copy.deepcopy(self.drifted)
                if pin_kind == "unavailable":
                    del current["process_index_pin"]
                elif pin_kind == "changed":
                    current["process_index_pin"]["child"]["birth"]["Linux"] += 1
                self.save(current)
                before = self.path.read_bytes()
                result = self.call(["session-action", self.name, "--json"], self.lifecycle_request(action="terminate"))
                receipt = json.loads(result.stdout)
                self.assertEqual(receipt["status"], "failed", receipt)
                self.assertIn("binding kept", receipt["error"])
                self.assertEqual(self.path.read_bytes(), before)
                self.assertFalse(list((self.root / "state/archives").glob("*.json")))
                self.assertIn("private-renamed", self.t("list-sessions", "-F", "#{session_name}"))
                self.assertEqual(self.bytes_received(), b"")

    def test_drifted_desktop_terminate_keeps_respawned_pane(self):
        pane = self.original["pane"]
        self.t("set-option", "-t", pane, "-w", "remain-on-exit", "on")
        with os.fdopen(os.pidfd_open(self.original["pid"]), "rb") as child:
            signal.pidfd_send_signal(child.fileno(), signal.SIGKILL)
        self.wait(lambda: self.t("display-message", "-p", "-t", pane, "#{pane_dead}").strip() == "1")
        self.t("respawn-pane", "-k", "-t", pane, "exec /usr/bin/sleep 40")
        supervisor = self.drifted["process_index_pin"]["supervisor"]["pid"]
        def observe():
            fields = self.t("display-message", "-p", "-t", pane,
                            "#{pane_id} #{@hgs_run} #{pane_pid} #{HGS_SESSION_ID}").split()
            self.assertEqual(fields[:2], [pane, self.original["run_id"]])
            self.assertEqual(fields[3], self.original["session_id"])
            self.assertNotEqual(int(fields[2]), supervisor)
            return fields
        replacement = observe()
        self.save(self.drifted)
        before = self.path.read_bytes()
        request = self.lifecycle_request(action="terminate")
        result = self.call(["session-action", self.name, "--json"], request)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["status"], "failed", receipt)
        self.assertIn("terminal kept", receipt["error"])
        self.assertEqual(observe(), replacement)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertFalse(list((self.root / "state/archives").glob("*.json")))

    def test_drifted_desktop_pause_keeps_a_pane_respawned_during_cleanup(self):
        pane = self.original["pane"]
        supervisor = self.drifted["process_index_pin"]["supervisor"]["pid"]
        self.t("set-option", "-t", pane, "-w", "remain-on-exit", "on")

        # The private tmux swaps its dead pane immediately before the guarded
        # cleanup command. The original child has already settled.
        wrapper = self.bin / "tmux"
        wrapper.write_text(f"""#!{sys.executable}
import os, subprocess, sys
tmux = {[TMUX, '-S', self.socket, '-f', '/dev/null']!r}
args = sys.argv[1:]
if args[:1] == ["if-shell"] and "kill-pane -t {pane}" in args:
    subprocess.run([*tmux, "respawn-pane", "-k", "-t", {pane!r},
                    "exec /usr/bin/sleep 40"], check=True)
os.execv(tmux[0], [*tmux, *args])
""")
        args = ["session-action", self.name, "--json"]
        result = self.call(args, self.lifecycle_request(action="pause"))
        self.assertEqual(json.loads(result.stdout)["status"], "completed", result.stderr)
        self.assertTrue(self.binding(self.name)["paused"])
        pane_pid, dead = self.t("display-message", "-p", "-t", pane,
                               "#{pane_pid} #{pane_dead}").split()
        self.assertNotEqual(int(pane_pid), supervisor)
        self.assertEqual(dead, "0")
        self.assertEqual(self.bytes_received(), b"")
