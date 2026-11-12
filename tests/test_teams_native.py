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
import pwd
import re
import shutil
import socket
import subprocess
import tempfile
import unittest
from unittest import mock
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


class ClassFixture:
    """One team per class, brought up once; the ordered steps share it, and a failed
    step fails the later ones with its name."""
    agent = None
    variable = None
    fx = None
    failed = None
    delegations = None
    service_answers = ()
    after_refusal = None

    @classmethod
    def setUpClass(cls):
        cls.fx = fixture.TeamFixture(cls.agent, os.environ[cls.variable], cls.__name__,
                                     delegations=cls.delegations, service_answers=cls.service_answers,
                                     after_refusal=cls.after_refusal)
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


class TeamsContract(ClassFixture):
    """Delegations off: the person answers every startup card."""

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


class TrustContract(ClassFixture):
    """Delegation confirm_trust on: the standing confirmation answers the members' startup
    folder-trust cards (zerus-63rru.48), and a prompt that is not a startup prompt stays
    the person's. The agent's own trust record proves the answer reached it."""
    delegations = {'confirm_trust': True}
    trust_source = None

    def test_1_the_service_answers_the_startup_folder_trust(self):
        fx = self.fx
        answered = [json.loads(line) for line in (fx.evidence.folder / 'person-answers.jsonl').read_text().splitlines()] \
            if (fx.evidence.folder / 'person-answers.jsonl').is_file() else []
        for member_id in ('lead', 'm1'):
            with self.subTest(member=member_id):
                seen = [v for k, v in fx.cards_seen.items() if k[0] == member_id and v['source'] == self.trust_source]
                self.assertEqual(len(seen), 1, 'the real startup trust card appeared once')
                self.assertIsNone(seen[0]['card'].get('conversation_id'), 'the real card carries a null conversation')
                self.assertFalse([a for a in answered if a['member'] == member_id and a['source'] == self.trust_source],
                                 'the person did not answer it')
                name = fx.sessions[member_id]['name']
                rows = fx.audit_rows(name)
                fx.evidence.write(f'audit-{member_id}.json', rows)
                grants = [r for r in rows if json.loads(r['question']).get('source') == self.trust_source]
                self.assertEqual(len(grants), 1, 'one audited grant for the card')
                self.assertNotIn(grants[0]['status'], ('prepared', 'cancelled', 'unknown'), grants[0])
                self.assert_agent_trusted(member_id)
        refusals = [c for c in fx.cards() if 'needs your attention' in c.get('text', '')]
        fx.evidence.write('cards-after-trust.json', refusals)
        self.assertFalse(refusals, 'no member card was left for the person')

    def test_2_a_prompt_that_is_not_a_startup_prompt_stays_the_persons(self):
        fx = self.fx
        fx.send('m1', 'Ask the fixture questions now.')
        card = fx.wait('m1 asked a question mid-conversation', lambda: next(iter(fx.inspect('m1').get('pending_questions') or []), None),
                       fixture.BOUND['turn'], 2)
        fx.evidence.write('mid-conversation-card.json', card)
        refused = fx.wait('the service refused the mid-conversation prompt with a card for the person',
                          lambda: [c for c in fx.cards() if 'Member m1 needs your attention' in c.get('text', '')
                                   and 'not a startup prompt' in c.get('text', '')], fixture.BOUND['doorbell'], 3)
        fx.evidence.write('mid-conversation-refusal.json', refused)
        still = [q.get('question_id') for q in fx.inspect('m1').get('pending_questions') or []]
        self.assertIn(card.get('question_id'), still, 'the prompt is still pending for the person')
        grants = [r for r in fx.audit_rows(fx.sessions['m1']['name'])
                  if json.loads(r['question']).get('question_id') == card.get('question_id')]
        self.assertFalse(grants, 'no audited grant for a prompt that is not a startup prompt')

    def assert_agent_trusted(self, member_id):
        raise NotImplementedError


class ClaudeTrust:
    trust_source = 'claude_folder_trust'
    service_answers = ('claude_folder_trust',)

    def assert_agent_trusted(self, member_id):
        home = self.fx.agent_home(member_id)
        record = json.loads((home / '.claude.json').read_text())
        worktree = self.fx.members()[member_id]['worktree']
        self.assertTrue((record.get('projects') or {}).get(worktree, {}).get('hasTrustDialogAccepted'),
                        'Claude recorded the trust for exactly the member worktree')


class CodexTrustContract(ClassFixture):
    """Delegation confirm_trust on, Codex: every member worktree is a linked worktree, so
    Codex's Folder access panel carries its Git root note and asks to trust the repository
    root. The standing confirmation refuses that scope (zerus-63rru.49 and the owner's
    decision); the panel, wrapped or not, is now recognized, so the person answers it
    through hgs answer, not only in Terminal."""
    delegations = {'confirm_trust': True}
    after_refusal = {'codex_folder_trust': 'grants a different scope'}

    def test_1_the_repository_scope_is_refused_and_the_person_answers_it(self):
        fx = self.fx
        answered = [json.loads(line) for line in (fx.evidence.folder / 'person-answers.jsonl').read_text().splitlines()]
        for member_id in ('lead', 'm1'):
            with self.subTest(member=member_id):
                seen = [v for k, v in fx.cards_seen.items() if k[0] == member_id and v['source'] == 'codex_folder_trust']
                self.assertEqual(len(seen), 1, 'the Folder access panel is recognized as Codex folder trust')
                card = seen[0]['card']
                item = card['questions'][0]
                self.assertEqual(item['question'], 'Trust this Git repository?')
                self.assertTrue(card.get('can_answer'), 'the person can answer it through hgs answer')
                worktree = fx.members()[member_id]['worktree']
                self.assertEqual(item['body'].split('\n\n', 1)[0].replace('\n', ''), worktree,
                                 'the panel names the member worktree, wrapped as the terminal shows it')
                rows = [r for r in fx.audit_rows(fx.sessions[member_id]['name'])
                        if json.loads(r['question']).get('source') == 'codex_folder_trust']
                self.assertFalse(rows, 'the standing confirmation granted nothing')
                refusal = [c for c in fx.cards() if f'Member {member_id} needs your attention' in c.get('text', '')
                           and 'grants a different scope' in c.get('text', '')]
                self.assertTrue(refusal, 'the service refused the repository scope with a card for the person')
                person = [a for a in answered if a['member'] == member_id and a['source'] == 'codex_folder_trust']
                self.assertEqual([(a.get('terminal'), a.get('exit')) for a in person], [(None, 0)],
                                 'the person answered it once through hgs answer')
        fx.evidence.write('codex-trust-cards.json', fx.cards())


class TeamsFixtureIsolation(unittest.TestCase):
    """The fixture's own isolation, without an agent: the tmux wrapper judges resolved
    paths, and the preflight refuses the runner's account name in a child's environment."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='hgs-teams-isolation-')
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.marker = self.base / 'real-tmux-called'
        stand_in = self.base / 'real-tmux'
        stand_in.write_text('#!/bin/sh\necho "$@" >> ' + str(self.marker) + '\n')
        stand_in.chmod(0o755)
        native = self.base / 'agent'
        native.write_text('#!/bin/sh\n')
        native.chmod(0o755)
        patches = [mock.patch.object(fixture, 'REAL_TMUX', str(stand_in)), mock.patch.object(fixture, 'HGS', str(native)),
                   mock.patch.dict(os.environ, ZERUS_TEAMS_EVIDENCE=str(self.base / 'evidence'))]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        # The root goes straight into TMPDIR, as in a run, so its socket paths stay short.
        self.fx = fixture.TeamFixture('claude', native, 'Isolation')
        self.addCleanup(shutil.rmtree, self.fx.root, True)
        self.fx.build()
        self.outside = self.base / 'outside'
        self.outside.mkdir()
        server = socket.socket(socket.AF_UNIX)
        self.addCleanup(server.close)
        server.bind(str(self.outside / 'default'))

    def tmux(self, *argv, **env):
        self.marker.unlink(missing_ok=True)
        run = subprocess.run([str(self.fx.root / 'bin/tmux'), *argv], env=dict(self.fx.env, **env),
                             capture_output=True, text=True, timeout=30)
        return run.returncode, self.marker.exists()

    def test_tmux_wrapper_refuses_a_link_out_of_the_root_before_tmux_runs(self):
        private = self.fx.root / 'tmux'
        self.assertEqual(self.tmux('-S', str(private / 'own.sock'), 'ls'), (0, True), 'a private socket passes')
        self.assertEqual(self.tmux('ls'), (0, True), 'a fresh private server passes before tmux-<uid> exists')
        (private / 'link.sock').symlink_to(self.outside / 'default')
        (private / 'dangling.sock').symlink_to(self.outside / 'missing')
        for name, argv, env in (
                ('-S link', ('-S', str(private / 'link.sock'), 'ls'), {}),
                ('$TMUX link', ('ls',), dict(TMUX=str(private / 'link.sock') + ',1,0')),
                ('dangling link', ('-S', str(private / 'dangling.sock'), 'ls'), {})):
            with self.subTest(case=name):
                self.assertEqual(self.tmux(*argv, **env), (1, False), name + ' never reaches tmux')
        (private / ('tmux-%d' % os.getuid())).symlink_to(self.outside, target_is_directory=True)
        for name, argv in (('TMUX_TMPDIR route', ('ls',)), ('labelled server', ('-L', 'other', 'ls'))):
            with self.subTest(case=name):
                self.assertEqual(self.tmux(*argv), (1, False), name + ' through a linked tmux-<uid> never reaches tmux')
        log = (self.fx.evidence.folder / 'tmux-calls.log').read_text().splitlines()
        self.assertEqual([line.split('\t')[1] for line in log], ['private', 'private'] + ['refused'] * 5)

    def test_preflight_refuses_the_runner_account_name(self):
        account = pwd.getpwuid(os.getuid()).pw_name
        self.assertEqual((self.fx.env['USER'], self.fx.env['LOGNAME']), (fixture.FIXTURE_USER, fixture.FIXTURE_USER))
        self.fx.preflight()
        for key, value, refusal in (('USER', account, 'account name'), ('LOGNAME', account, 'account name'),
                                    ('HGS_PEERS', account, 'runner account name is the value of HGS_PEERS'),
                                    ('USER', 'someone-else', 'not the fixture account name'),
                                    ('LOGNAME', 'someone-else', 'not the fixture account name')):
            with self.subTest(variable=key, value=value):
                saved = self.fx.env[key]
                self.fx.env[key] = value
                with self.assertRaisesRegex(AssertionError, refusal):
                    self.fx.preflight()
                self.fx.env[key] = saved


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


@unittest.skipUnless(os.environ.get('HGS_CLAUDE_TEST_BIN') and fixture.HGS, 'Set HGS_TEST_BIN and HGS_CLAUDE_TEST_BIN')
class NativeTeamsTrustClaude(ClaudeTrust, TrustContract, unittest.TestCase):
    agent = 'claude'
    variable = 'HGS_CLAUDE_TEST_BIN'


@unittest.skipUnless(os.environ.get('HGS_CODEX_TEST_BIN') and fixture.HGS, 'Set HGS_TEST_BIN and HGS_CODEX_TEST_BIN')
class NativeTeamsTrustCodex(CodexTrustContract, unittest.TestCase):
    agent = 'codex'
    variable = 'HGS_CODEX_TEST_BIN'


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
