#!/usr/bin/env python3
"""Actual supervisor/CLI startup controls with a private scripted Claude TUI.

This is not official Claude compatibility evidence. The fake never emits native
hooks or a conversation.
"""
import json
import os
import signal
import subprocess
import sys
import time
import unittest
import uuid

import test_pause

Base = type('ThemeFixture', (unittest.TestCase,), {
    name: value for name, value in test_pause.Harness.__dict__.items()
    if name not in ('__dict__', '__weakref__') and not name.startswith('test_')})

AGENT = r'''#!PYTHON
import os, pathlib, signal, sys, termios, tty
root=pathlib.Path.home()
if sys.argv[1:]==['--help']:
    print('Usage: claude [options] [prompt]');sys.exit(0)
if sys.argv[1:]==['--version']:
    print('2.1.322 (Claude Code)');sys.exit(0)
fd=sys.stdin.fileno();original=termios.tcgetattr(fd);tty.setraw(fd)
def draw(text):
    os.write(1,('\x1b[?2004h\x1b[2J\x1b[H'+text.replace('\n','\r\n')).encode())
def repaint(*_):
    draw((root/'theme-screen').read_text())
repaint()
signal.signal(signal.SIGUSR1, repaint)
(root/'theme-agent-ready').touch()
try:
    while True:
        value=os.read(fd,4096)
        if not value:break
        if value==b'\x04':break
        with (root/'theme-received').open('ab') as log:log.write(value)
        if value==b'\r' and not (root/'theme-remains').exists():
            draw("Welcome to Claude Code v2.1.322\n\nSelect login method:\n\n❯ 1. Claude account with subscription\n  2. Anthropic Console account\n  3. 3rd-party platform\n")
finally:termios.tcsetattr(fd,termios.TCSANOW,original)
'''


class ClaudeTheme(Base):
    def setUp(self):
        super().setUp()
        self.script('claude', AGENT.replace('#!PYTHON', '#!'+sys.executable))
        self.screen = (test_pause.REPO/'tests/fixtures/claude-theme-first-run.txt').read_text()
        # Only the committed isolated official native interface fixture enters fake controls.
        self.screen = self.screen[:self.screen.index('\n')+1] + self.screen[self.screen.index(" Let's get started."):]
        # This scripted raw TUI has no native responsive renderer: fit only
        # decorative rules to its80-column pane and omit captured blank padding.
        # Semantic labels/preview/footer and the committed raw native fixture
        # remain unchanged. The separate native suite never reflows captures.
        self.screen = '\n'.join(' '+('╌'*64) if row.strip() and set(row.strip())=={'╌'} else row
                                for row in self.screen.splitlines()).rstrip()+'\n'
        (self.root/'theme-screen').write_text(self.screen)

    def launch(self, tag='theme', width=80, height=24):
        self.name='claude/p/'+tag
        config = self.root/'.tmux.conf'
        config.write_text(f'set -g exit-empty off\nset -g default-size {width}x{height}\n')
        subprocess.run([test_pause.TMUX, '-S', self.socket, '-f', str(config), 'start-server'],
                       env=self.env, capture_output=True, check=True, timeout=5)
        self.hgs('claude','p','-n',tag,'-d')
        self.wait(lambda:(self.root/'theme-agent-ready').exists())
        self.wait(lambda:self.binding(self.name).get('pid'))
        return self.binding(self.name)

    def claims(self):
        directory=self.root/'state/claude-theme-confirmations'
        return [json.loads(p.read_text()) for p in directory.glob('*.json') if not p.name.endswith('.result.json')]

    def result(self):
        paths=list((self.root/'state/claude-theme-confirmations').glob('*.result.json'))
        return json.loads(paths[0].read_text()) if len(paths)==1 else {}

    def received(self):
        path=self.root/'theme-received'
        return path.read_bytes() if path.exists() else b''

    def assert_manual_card(self):
        output=json.loads(self.hgs('inspect',self.name))
        cards=output.get('pending_questions',[])
        self.assertTrue(cards,output)
        self.assertTrue(all(not q.get('can_answer') for q in cards),cards)
        self.assertEqual(self.received(),b'')
        self.assertEqual(self.claims(),[])

    def test_security_notes_fit_the_default_compact_terminal_without_art(self):
        screen = (test_pause.REPO/'tests/fixtures/claude-security-notes-first-run.txt').read_text()
        header, _, rest = screen.partition('\n')
        compact = header + '\n\n Security notes:' + rest.split(' Security notes:', 1)[1]
        (self.root/'theme-screen').write_text(compact)
        record = self.launch('compact-security-notes')
        state = json.loads(self.hgs('inspect', self.name))
        card, = state['pending_questions']
        self.assertEqual(card['questions'][0]['question'], 'Security notes:')
        self.assertEqual(card['run_id'], record['run_id'])
        self.assertFalse(state['first_message_can_send'])
        self.assertEqual(self.received(), b'')

    def test_security_notes_are_one_manual_card_until_the_live_screen_advances(self):
        screen = (test_pause.REPO/'tests/fixtures/claude-security-notes-first-run.txt').read_text()
        (self.root/'theme-screen').write_text(screen)
        record = self.launch('security-notes', height=40)

        def inspect():
            return json.loads(self.hgs('inspect', self.name))

        def repaint(frame, marker):
            (self.root/'theme-screen').write_text(frame)
            os.kill(record['pid'], signal.SIGUSR1)
            self.wait(lambda: marker in subprocess.check_output(
                [test_pause.TMUX, '-S', self.socket, 'capture-pane', '-p', '-t', record['pane']],
                env=self.env, text=True, timeout=5))

        def notice(state):
            return [card for card in state['pending_questions']
                    if card['questions'][0]['question'] == 'Security notes:']

        first = inspect()
        self.assertEqual(len(first['pending_questions']), 1, first['pending_questions'])
        card, = notice(first)
        self.assertEqual(card['run_id'], record['run_id'])
        self.assertEqual((card['source'], card['can_answer'], card['answer_transport']),
                         ('startup_terminal_choice', False, ''))
        self.assertEqual(card['questions'][0]['id'], 'terminal')
        self.assertEqual(card['questions'][0]['options'], [])
        self.assertEqual((first['phase'], first['attention_id']), ('input', card['question_id']))
        self.assertFalse(first['first_message_can_send'])
        self.assertTrue(first['terminal_supported'])
        answer = subprocess.run(
            [str(test_pause.HGS), 'answer', self.name, '--json'], env=self.env,
            input=json.dumps(dict(request_id=str(uuid.uuid4()), expected_run_id=record['run_id'],
                                  expected_conversation_id='', question_id=card['question_id'],
                                  expected_question_hash=card['question_hash'], answers=[])),
            text=True, capture_output=True, timeout=5)
        self.assertNotEqual(answer.returncode, 0, answer.stdout)
        self.assertIn('refresh before answering', answer.stderr)
        self.assertEqual(self.received(), b'')
        listing = json.loads(self.hgs('ls', '--local', '--json'))
        row = next(row for row in listing['sessions'] if row['name'] == self.name)
        self.assertEqual((row['phase'], row['attention_id']), ('input', card['question_id']))

        # Repaint artwork and wrap the same words at a different column.
        redraw = screen.replace('█████▓▓░', '░░░░')
        redraw = redraw.replace('only use it with code you trust', 'only use it with\n    code you trust')
        repaint(redraw, '\n    code you trust')
        self.assertEqual(notice(inspect()), [card])

        # Lookalikes before a conversation: even the whole notice embedded in
        # a log, a different footer, or a login page must not mint this card.
        for frame, marker in [
            ('Previous terminal output:\n'+screen, 'Previous terminal output:'),
            (screen.replace('Learn more:', 'Unrelated:'), 'Unrelated:'),
            (screen.replace('Press Enter to continue', 'Press Enter to open settings'), 'open settings'),
            ('Welcome to Claude Code v2.1.332\n\nSelect login method:\n\n❯ 1. Claude account\n  2. Anthropic Console',
             'Select login method:'),
        ]:
            repaint(frame, marker)
            self.assertEqual(notice(inspect()), [], marker)

        # The old notice can remain visible above a real empty composer. It
        # is no longer the active frame, and a verified composer is Ready.
        rule = '─' * 60
        composer = f'\n{rule}\n❯ \n{rule}\n  ? for shortcuts\x1b[2A\x1b[3G'
        repaint(screen+composer, '? for shortcuts')
        after = inspect()
        self.assertEqual(notice(after), [])
        self.assertNotEqual(after.get('attention_id'), card['question_id'])
        self.assertTrue(after['first_message_can_send'], after.get('first_message_reason'))
        self.assertEqual(self.received(), b'')
        self.assertEqual(self.claims(), [])

    def test_original_supervisor_confirms_dark_once_then_leaves_login_manual(self):
        before=self.launch()
        self.wait(lambda:self.result().get('status')=='advanced')
        self.assertEqual(self.received(),b'\r')
        claim,=self.claims()
        self.assertEqual(claim['label'],'Dark mode')
        for key in ('session_id','run_id','pid','process_start','pane'):
            self.assertEqual(claim[key],before[key])
        self.assertEqual(claim['status'],'reserved')
        self.assertEqual(self.binding(self.name)['conversation_id'],None)
        for _ in range(3):
            self.hgs('inspect',self.name)
        time.sleep(.5)
        self.assertEqual(self.received(),b'\r')
        # Ordinary backend still works in the SAME resident process when its
        # actual child exits: finish must record exit rather than sticky refusal.
        subprocess.run([test_pause.TMUX,'-S',self.socket,'send-keys','-t',before['pane'],'C-d'],env=self.env,capture_output=True,check=True)
        self.wait(lambda:self.binding(self.name).get('completion_source')=='supervisor')
        self.assertEqual(self.binding(self.name)['exit_code'],0)

    def test_auto_preserves_the_focused_checked_default_without_navigation(self):
        screen=self.screen.replace('     Auto (match terminal)',' ❯ ✔ Auto (match terminal)').replace(' ❯ ✔ Dark mode','     Dark mode')
        (self.root/'theme-screen').write_text(screen)
        self.launch('auto')
        self.wait(lambda:self.result().get('status')=='advanced')
        self.assertEqual(self.received(),b'\r')
        self.assertEqual(self.claims()[0]['label'],'Auto (match terminal)')

    def test_light_and_mismatched_check_stay_visible_and_never_submit(self):
        screen=self.screen.replace(' ❯ ✔ Dark mode','   ✔ Dark mode').replace('     Light mode\n',' ❯   Light mode\n')
        (self.root/'theme-screen').write_text(screen)
        self.launch('light')
        time.sleep(.5)
        self.assert_manual_card()

    def test_unknown_after_enter_keeps_one_durable_claim_and_never_retries(self):
        (self.root/'theme-remains').touch()
        self.launch('unknown')
        self.wait(lambda:bool(self.claims()))
        time.sleep(3.5)
        self.assertEqual(self.received(),b'\r')
        self.assertEqual(len(self.claims()),1)
        for _ in range(3):self.hgs('inspect',self.name)
        self.assertEqual(self.received(),b'\r')
        self.assertIn(self.result().get('status'),('unknown',None))


if __name__=='__main__':unittest.main()
