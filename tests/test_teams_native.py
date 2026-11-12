#!/usr/bin/env python3
"""Agent Teams milestone 1 against real agents (package 15).

Each class brings up one team of one agent kind (a lead and one member, launched by
a private `hgs desktop serve --background` from the team store) in a private root,
answers the members' startup cards as the person, and then walks the places where
the Teams service touches a real session, in order:

1. launch and identity: the session binds by its launch ID, its record carries the
   store's ownership nonce and worktree, the native process is the pinned image under
   the production supervisor, the role's model and effort reached its argv, and the
   member's parent link names the lead;
2. delivery and the member's shell: `hgs task team-deliver` puts the task prompt into
   the member exactly once; the member's own tool shell acknowledges with the pinned
   begin-attempt, passes the caller proof (`hgs team current` names it), is refused a
   lead's command, and reports with the pinned report-attempt;
3. the lead's doorbell: one `hgs send` naming the inbox, with none of the item's text;
   the lead's own shell reads the inbox as the lead;
4. the cancel notice: the lead cancels a delivered task from its shell and the member
   receives the existing cancel notice once;
5. stop and terminate: a person's team stop ends no agent; a person's Terminate ends
   the member's process and archives its run.

Steps 2 to 4 wait for zerus-63rru.51 (WAITING): they are kept as waiting_* methods
that the contract does not select. Folder trust and resume of an exited member are
added after zerus-63rru.48 and .49.
The model is a keyless loopback fixture; no account and no model quota are used.
Run through `scripts/ci/agent-compatibility.py --contract teams`, or directly with
HGS_TEST_BIN and HGS_CLAUDE_TEST_BIN or HGS_CODEX_TEST_BIN, outside any agent
session: the Teams caller proof refuses a person command below a live agent.
The Kimi class runs only in the hosted Kimi workflow.
"""
import json
import os
from pathlib import Path
import re
import unittest
import uuid

import teams_native_fixture as fixture


# Cases written against real agents that wait for a product fix before the contract
# selects them; each is a method of TeamsContract named waiting_*. Folder trust by the
# standing confirmation and resume of an exited member wait for zerus-63rru.48 and .49
# and are added with those fixes.
WAITING = {
    'waiting_2_delivery_and_member_shell': 'zerus-63rru.51 (no member is free: availability reads subagents_active)',
    'waiting_3_lead_doorbell': 'zerus-63rru.51 (the doorbell needs an idle lead and a report)',
    'waiting_4_cancel_notice': 'zerus-63rru.51 (a delivered task to cancel needs a hand-out)',
}


class TeamsContract:
    agent = None
    variable = None
    fx = None
    failed = None

    @classmethod
    def setUpClass(cls):
        cls.fx = fixture.TeamFixture(cls.agent, os.environ[cls.variable], cls.__name__)
        try:
            cls.fx.bring_up()
        except BaseException:
            problems = cls.fx.teardown()
            cls.fx.evidence.write('bring-up-teardown.json', problems)
            raise

    @classmethod
    def tearDownClass(cls):
        problems = cls.fx.teardown()
        if problems:
            raise AssertionError('Teams fixture teardown: ' + '; '.join(problems))

    @property
    def root(self):
        """The private root, declared for the hosted Kimi runner's custody records."""
        return self.fx.root if self.fx else None

    def setUp(self):
        if type(self).failed:
            self.fail(f'an earlier step failed: {type(self).failed}')

    def run(self, result=None):
        before = len(result.failures) + len(result.errors) if result is not None else 0
        outcome = super().run(result)
        if result is not None and len(result.failures) + len(result.errors) > before and not type(self).failed:
            type(self).failed = self._testMethodName
        return outcome

    # 1 -----------------------------------------------------------------------
    def test_1_launch_and_identity(self):
        fx = self.fx
        members = fx.members()
        for member_id in ('lead', 'm1'):
            stored = members[member_id]
            view = fx.inspect(member_id)
            launch = fx.sessions[member_id]['launch']
            self.assertEqual(stored['session']['session_id'], 'launch_id:' + launch)
            self.assertEqual(view.get('launch_id'), launch)
            self.assertTrue(view.get('tracked'))
            self.assertEqual(view.get('agent'), self.agent)
            self.assertEqual(os.path.realpath(view.get('cwd')), os.path.realpath(stored['worktree']))
            # The launch picks among the signed-in fixture accounts and spreads seats over
            # them (the always-listed native account is a configured fixture account too).
            self.assertIn(stored.get('account'), fx.fixture_accounts)
            record = fx.record(member_id)
            self.assertEqual((record.get('worktree_owner') or {}).get('id'), stored['ownership_nonce'])
            pid = record['pid']
            parent = int(open(f'/proc/{pid}/stat').read().rsplit(')', 1)[1].split()[1])
            self.assertEqual(parent, record['supervisor']['pid'], 'the native agent runs under the production supervisor')
            self.assertEqual(Path(f'/proc/{pid}/exe').resolve(), fx.native, 'the native process is the pinned image')
            argv = open(f'/proc/{pid}/cmdline', 'rb').read().split(b'\0')
            if member_id == 'm1' and self.agent == 'claude':
                self.assertIn(b'--effort', argv)
                self.assertIn(b'high', argv)
                self.assertIn(b'claude-sonnet-4-6', argv)
            if member_id == 'm1' and self.agent == 'codex':
                self.assertIn(b'model_reasoning_effort="high"', argv)
                self.assertIn(b'gpt-6.1-sol', argv)
        # The service writes the link on a pass after the lead has a conversation (it
        # records identity_pending until then); it never replaces one. Codex starts its
        # conversation at the first message, so a fresh Codex team has no link until the
        # person's first message to each session has started one.
        for member_id in ('lead', 'm1'):
            if not fx.inspect(member_id).get('conversation_id'):
                fx.send(member_id, 'Fixture first message.')
                fx.wait(f'{member_id} conversation started by its first message',
                        lambda m=member_id: fx.inspect(m).get('conversation_id'), fixture.BOUND['turn'], 2)
        def linked():
            member = fx.inspect('m1')
            read = fx.json('parent', fx.sessions['m1']['name'], '--json', ok=False, stdin=json.dumps(dict(
                expected_run_id=member.get('run_id'), expected_conversation_id=member.get('conversation_id') or '',
                action='read')))
            return read if read.get('parent') else None
        parent = fx.wait('member parent link written by the service', linked, fixture.BOUND['parent'], 3)
        fx.evidence.write('member-parent.json', parent)
        self.assertEqual((parent.get('parent') or {}).get('name'), fx.sessions['lead']['name'],
                         'the member is linked to its lead (manual link written after binding)')

    # 2 -----------------------------------------------------------------------
    def waiting_2_delivery_and_member_shell(self):
        fx = self.fx
        out = fx.root / 'out'
        fx.script('delivered', (
            'set -- "$@"; n=0; for x in ' + str(out) + '/delivered-*.json; do [ -e "$x" ] && n=$((n+1)); done; '
            'f=' + str(out) + '/delivered-$n.json\n'
            'begin=$("$H" task begin-attempt "$1" --attempt "$2" 2>&1); b=$?\n'
            'current=$("$H" team current --json 2>&1); c=$?\n'
            'inbox=$("$H" team inbox --team ' + fx.team + ' --json 2>&1); i=$?\n'
            'if [ -f ' + str(out) + '/begin-only ]; then report=""; r=-1; else\n'
            '  report=$(printf "Fixture report: nothing to change.\\n" | "$H" task report-attempt "$3" --attempt "$4" 2>&1); r=$?\n'
            'fi\n'
            'python3 -c \'import json,sys; json.dump(dict(begin=sys.argv[1], begin_exit=int(sys.argv[2]), current=sys.argv[3], '
            'current_exit=int(sys.argv[4]), inbox=sys.argv[5], inbox_exit=int(sys.argv[6]), report=sys.argv[7], '
            'report_exit=int(sys.argv[8])), open(sys.argv[9], "w"))\' "$begin" $b "$current" $c "$inbox" $i "$report" $r "$f"\n'
            'echo "fixture step done"\n'))
        ref, task_uuid = fx.add_task('Fixture task one', 'Fixture task one: report that nothing needs to change.')
        node = fx.wait('task node in the queue', lambda: fx.node_of(task_uuid), fixture.BOUND['handout'], 2)
        result = fx.wait('member ran the delivered task in its shell',
                         lambda: (out / 'delivered-0.json').is_file() and json.loads((out / 'delivered-0.json').read_text()),
                         fixture.BOUND['handout'] + fixture.BOUND['turn'], 2)
        fx.evidence.write('delivered-0.json', result)
        # Delivery: the prompt the member's model saw names this task and holds it once.
        prompts = fx.requests_with(lambda r: (r.get('prompt') or '').startswith('[Zerus task ' + ref))
        self.assertTrue(prompts, 'no model request carried the delivered prompt')
        delivered = prompts[0]['prompt']
        self.assertIn('hgs task begin-attempt', delivered)
        for request in prompts:
            self.assertEqual(sum(text == delivered for text in request['users']), 1,
                             'the delivered prompt appears once in the member conversation')
        # The member's own shell: acknowledgement, caller proof, refusal, report.
        self.assertEqual(result['begin_exit'], 0, result['begin'])
        current = json.loads(result['current'])
        self.assertEqual(result['current_exit'], 0, result['current'])
        self.assertEqual(current['actor'], dict(kind='session', member='m1', person='local', team=fx.team))
        self.assertIn(node['id'], [n['id'] for n in current.get('nodes') or []])
        refused = json.loads(result['inbox'])
        self.assertIs(refused.get('ok'), False, result['inbox'])
        self.assertEqual(refused['error']['code'], 'not_allowed', 'a member is refused the lead inbox')
        self.assertEqual(result['report_exit'], 0, result['report'])
        task = fx.json('task', 'get', ref, '--json')
        task = task.get('task') or task
        self.assertEqual(task.get('stage'), 'review', 'the report moved the task to In review')
        type(self).first = dict(ref=ref, uuid=task_uuid, node=node['id'])

    # 3 -----------------------------------------------------------------------
    def waiting_3_lead_doorbell(self):
        fx = self.fx
        bell = re.compile(r'(1 item waits|\d+ items wait) in your team inbox\. Read them with: hgs team inbox --team '
                          + re.escape(fx.team))
        rang = fx.wait('doorbell reached the lead', lambda: fx.requests_with(lambda r: bell.fullmatch(r.get('prompt') or '')),
                       fixture.BOUND['doorbell'], 2)
        text = rang[0]['prompt']
        latest = fx.requests_with(lambda r: text in r['users'])[-1]
        self.assertEqual(latest['users'].count(text), 1, 'one doorbell, not a repeat')
        self.assertNotIn('Fixture report', text, 'the doorbell carries none of the item')
        out = fx.root / 'out/lead-inbox.json'
        fx.script('lead-inbox', '"$H" team inbox --team ' + fx.team + ' --json > ' + str(out) + ' 2>&1; echo done\n')
        fx.send('lead', 'ZT-RUN lead-inbox')
        inbox = fx.wait('lead read its inbox in its own shell', lambda: out.is_file() and json.loads(out.read_text() or 'null'),
                        fixture.BOUND['turn'], 2)
        fx.evidence.write('lead-inbox.json', inbox)
        self.assertEqual(inbox['actor'].get('kind'), 'session')
        self.assertEqual(inbox['actor'].get('member'), 'lead')
        self.assertTrue(any(row.get('kind') == 'report' and row.get('node') == self.first['node'] for row in inbox['rows']))

    # 4 -----------------------------------------------------------------------
    def waiting_4_cancel_notice(self):
        fx = self.fx
        out = fx.root / 'out'
        (out / 'begin-only').write_text('the member acknowledges and keeps working\n')
        ref, task_uuid = fx.add_task('Fixture task two', 'Fixture task two: keep working until cancelled.')
        result = fx.wait('member acknowledged the second task',
                         lambda: (out / 'delivered-1.json').is_file() and json.loads((out / 'delivered-1.json').read_text()),
                         fixture.BOUND['handout'] + fixture.BOUND['turn'], 2)
        self.assertEqual(result['begin_exit'], 0, result['begin'])
        node = fx.node_of(task_uuid)
        cancel = out / 'lead-cancel.json'
        fx.script('lead-cancel', (
            'rev=$("$H" team status --team ' + fx.team + ' --json | python3 -c "import json,sys; print(json.load(sys.stdin)[\'team\'][\'revision\'])")\n'
            'printf \'{"request_id":"%s","team":"' + fx.team + '","expected_revision":%s,"node":' + str(node['id'])
            + ',"reason":"Fixture cancel"}\' "$(python3 -c "import uuid; print(uuid.uuid4())")" "$rev" | '
            '"$H" team cancel --json > ' + str(cancel) + ' 2>&1; echo done\n'))
        fx.send('lead', 'ZT-RUN lead-cancel')
        reply = fx.wait('lead cancelled the task in its own shell',
                        lambda: cancel.is_file() and json.loads(cancel.read_text() or 'null'), fixture.BOUND['turn'], 2)
        fx.evidence.write('lead-cancel.json', reply)
        self.assertTrue(reply.get('ok'), reply)
        self.assertEqual(reply['actor'].get('member'), 'lead')
        notice = re.compile(r'\[Zerus task ' + re.escape(ref) + r' cancelled\]')
        seen = fx.wait('member received the cancel notice',
                       lambda: fx.requests_with(lambda r: notice.match(r.get('prompt') or '')), fixture.BOUND['cancel'], 2)
        latest = fx.requests_with(lambda r: seen[0]['prompt'] in r['users'])[-1]
        self.assertEqual(latest['users'].count(seen[0]['prompt']), 1, 'the notice arrives once')
        task = fx.json('task', 'get', ref, '--json')
        task = task.get('task') or task
        self.assertTrue(task.get('cancelled_at'), 'the task is cancelled')

    # 5 -----------------------------------------------------------------------
    def test_5_stop_and_terminate(self):
        fx = self.fx
        rows = fx.json('team', 'queue', '--team', fx.team, '--limit', '200', '--json').get('rows') or []
        if not rows:
            # The stop must have an unfinished node to cancel.
            _, task_uuid = fx.add_task('Fixture task for the stop', 'Fixture task: unfinished when the team stops.')
            fx.wait('task node in the queue', lambda: fx.node_of(task_uuid), fixture.BOUND['handout'], 2)
        before = {m: fx.process(m) for m in ('lead', 'm1')}
        revision = fx.status()['team']['revision']
        reply = fx.json('team', 'stop', '--json', stdin=json.dumps(dict(
            request_id=str(uuid.uuid4()), team=fx.team, expected_revision=revision)))
        self.assertTrue(reply.get('ok'), reply)
        fx.wait('team stopped', lambda: fx.status()['team'].get('status') == 'stopped', fixture.BOUND['stop'], 2)
        # The service acts on the stop (it records the cancel of the unfinished node) and
        # ends no agent. The node's own state is the store's rule (package 10), not agent
        # behaviour, so this step reads the service's journal entry, not the queue.
        def acted():
            rows = fx.json('team', 'journal', '--team', fx.team, '--limit', '200', '--json').get('rows') or []
            stops = [r['seq'] for r in rows if r.get('kind') == 'stop']
            after = {r.get('kind') for r in rows if stops and r['seq'] > stops[-1]
                     and (r.get('actor') or {}).get('kind') == 'service'}
            return {'record_cancel', 'record_key_clear'} <= after or None
        fx.wait('service recorded the stopped team\'s cancel and key clear', acted, fixture.BOUND['stop'], 2)
        for member_id, (pid, ticks) in before.items():
            self.assertEqual(fx.start_ticks(pid), ticks, f'{member_id} still runs the same process after the stop')
        view = fx.inspect('m1')
        pid, _ = before['m1']
        fx.hgs('terminate', fx.sessions['m1']['name'], '--expected-run-id', view['run_id'], timeout=120)
        fx.wait('member process exited after Terminate', lambda: fx.start_ticks(pid) is None or None,
                fixture.BOUND['exit'], 1)
        archives = [json.loads(p.read_text()) for p in (Path(fx.env['HGS_STATE_DIR']) / 'archives').glob('*.json')]
        self.assertTrue(any(a.get('run_id') == view['run_id'] for a in archives), 'Terminate archived the run')


class TeamsFixtureProducers(unittest.TestCase):
    """Values another component writes come from that component (plan, 2026-11-12):
    the Kimi inspection stand-in carries only what src/accounts/kimi-read.mjs writes."""

    def test_kimi_inspection_stand_in_is_a_value_the_producer_writes(self):
        producer = (Path(__file__).resolve().parents[1] / 'src/accounts/kimi-read.mjs').read_text()
        base = re.search(r"const output = (\{.*?\});", producer)[1]
        self.assertEqual(base, "{identity:{},windows:[],source:'Kimi Code',status:'unavailable'}")
        self.assertEqual(fixture.KIMI_PRODUCER_BASE, dict(identity={}, windows=[], source='Kimi Code', status='unavailable'))
        identity = re.search(r"output\.identity=\{(.*?)\};", producer)[1]
        written = set(re.findall(r"(\w+):", identity))
        value = fixture.kimi_inspection()
        self.assertEqual(set(value), set(fixture.KIMI_PRODUCER_BASE), 'no key the producer does not write')
        self.assertEqual(set(value['identity']), written)
        self.assertIn("auth_method:'Kimi Code'", identity)
        self.assertEqual(value['identity']['auth_method'], 'Kimi Code')
        # The producer sets status ok only when it has windows; with none it stays unavailable.
        self.assertIn("if(output.windows.length)output.status='ok';", producer)
        self.assertEqual((value['windows'], value['status']), ([], 'unavailable'))


@unittest.skipUnless(os.environ.get('HGS_CLAUDE_TEST_BIN') and fixture.HGS, 'Set HGS_TEST_BIN and HGS_CLAUDE_TEST_BIN')
class NativeTeamsClaude(TeamsContract, unittest.TestCase):
    agent = 'claude'
    variable = 'HGS_CLAUDE_TEST_BIN'


@unittest.skipUnless(os.environ.get('HGS_CODEX_TEST_BIN') and fixture.HGS, 'Set HGS_TEST_BIN and HGS_CODEX_TEST_BIN')
class NativeTeamsCodex(TeamsContract, unittest.TestCase):
    agent = 'codex'
    variable = 'HGS_CODEX_TEST_BIN'


# Set only by scripts/ci/kimi-native.py --teams, which itself refuses to run outside
# a GitHub-hosted runner.
HOSTED = os.environ.get('ZERUS_TEAMS_KIMI_HOSTED') == '1'


@unittest.skipUnless(HOSTED and os.environ.get('HGS_KIMI_TEST_BIN') and fixture.HGS,
                     'Kimi Teams classes run only in the hosted Kimi workflow (no local Kimi staging); a skip is no evidence')
class NativeTeamsKimi(TeamsContract, unittest.TestCase):
    agent = 'kimi'
    variable = 'HGS_KIMI_TEST_BIN'


if __name__ == '__main__':
    unittest.main()
