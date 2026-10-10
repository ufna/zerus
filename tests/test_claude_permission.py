#!/usr/bin/env python3
"""Claude auto-mode onboarding on an isolated terminal; no real settings change."""
import json
import os
import signal
import time
import unittest
import uuid
from pathlib import Path

import test_input as fixtures
import test_questions as questions
import test_claude_trust as trust

FAKE = trust.FAKE.replace("labels=['No, exit','Yes, I trust this folder']",
    "labels=['Yes, set auto mode as my default permission mode','No, keep bypass permissions']")
FAKE = FAKE.replace("'git config'", "'bypass permissions'")
FAKE = FAKE.replace("screen.replace('❯ No, exit','  No, exit').replace('  Yes, I trust','❯ Yes, I trust')",
    "screen.replace('❯ Yes, set','  Yes, set').replace('  No, keep','❯ No, keep')")
FAKE = FAKE.replace('     if selected==0:sys.exit(0)\n','')

class PermissionMode(unittest.TestCase):
    script = fixtures.InputTransport.script
    tmux = fixtures.InputTransport.tmux
    wait_for = fixtures.InputTransport.wait_for
    received = fixtures.InputTransport.received
    write_record = fixtures.InputTransport.write_record
    tearDown = fixtures.InputTransport.tearDown
    inspect = questions.Questions.inspect
    answer = questions.Questions.answer
    redraw = trust.FolderTrust.redraw

    def setUp(self):
        os.environ['CLAUDE_TRUST_FIXTURE']=str(Path(__file__).parent/'fixtures/claude-auto-mode.txt')
        self.addCleanup(lambda:os.environ.pop('CLAUDE_TRUST_FIXTURE',None))
        original=fixtures.FAKE_AGENT
        try:
            fixtures.FAKE_AGENT=FAKE;fixtures.InputTransport.setUp(self)
        finally:fixtures.FAKE_AGENT=original
        self.record.update(agent='claude',run_identity_version=1,supervisor={},
            launch_dir='/work/example',phase='idle',activity='idle',last_event_at=time.time())
        self.write_record();self.card=self.inspect()['pending_questions'][0]

    def payload(self,choice=1,**updates):
        p=dict(request_id=str(uuid.uuid4()),expected_run_id=self.run_id,
            expected_conversation_id=self.conversation_id,question_id=self.card['question_id'],
            expected_question_hash=self.card['question_hash'],
            answers=[dict(question_id='permission_mode',selected_option_ids=['mode_'+str(choice)],text='')])
        p.update(updates);return p

    def test_after_session_start_attention_without_automatic_choice(self):
        state=self.inspect();self.assertEqual(state['phase'],'approval')
        self.assertEqual(state['attention_id'],self.card['question_id'])
        self.assertTrue(self.card['can_answer']);self.assertEqual(self.card['conversation_id'],self.conversation_id)
        self.assertIn('blocks the rest',self.card['questions'][0]['body'])
        self.assertEqual([o['label'] for o in self.card['questions'][0]['options']],
            ['Yes, set auto mode as my default permission mode','No, keep bypass permissions'])
        self.assertFalse(any(o.get('selected') for o in self.card['questions'][0]['options']))
        self.assertIn('permission mode choice',fixtures.InputTransport.send(self,fixtures.InputTransport.payload(self),success=False))
        self.assertEqual(self.received(),b'')

    def test_explicit_keep_mode_is_submitted_once(self):
        p=self.payload();receipt=self.answer(p);self.assertEqual(receipt['status'],'answered')
        self.assertEqual((self.root/'submitted').read_text(),'No, keep bypass permissions')
        self.assertEqual(self.received().count(b'\r'),1)
        before=self.received();self.assertEqual(self.answer(p),receipt);self.assertEqual(self.received(),before)
        self.assertEqual(self.inspect()['pending_questions'],[])

    def test_home_abbreviation_keeps_explicit_choice_and_exact_folder(self):
        self.record['launch_dir']=str(self.root/'work/example');self.write_record()
        self.redraw('folder','~/work/example','~/work/example')
        self.card=self.inspect()['pending_questions'][0]
        self.assertEqual(self.card['source'],'claude_permission_mode')
        self.assertTrue(self.card['can_answer']);self.assertEqual(self.received(),b'')
        p=self.payload();receipt=self.answer(p)
        self.assertEqual(receipt['status'],'answered')
        self.assertEqual((self.root/'submitted').read_text(),'No, keep bypass permissions')
        before=self.received();self.assertEqual(self.answer(p),receipt);self.assertEqual(self.received(),before)
        self.assertEqual(self.inspect()['pending_questions'],[])

    def test_home_abbreviation_does_not_match_an_unrelated_absolute_folder(self):
        self.redraw('folder','~/work/example','~/work/example')
        self.assertEqual(self.inspect()['pending_questions'],[])
        self.answer(self.payload(),success=False);self.assertEqual(self.received(),b'')

    def test_explicit_auto_navigates_back_from_current_selection(self):
        self.redraw('selection','1','❯ No, keep')
        self.assertEqual(self.inspect()['pending_questions'][0]['question_hash'],self.card['question_hash'])
        self.answer(self.payload(0));self.assertEqual((self.root/'submitted').read_text(),'Yes, set auto mode as my default permission mode')
        self.assertEqual(self.received().count(b'\r'),1)

    def test_before_session_start_is_also_answerable(self):
        self.conversation_id='';self.record['conversation_id']=None;self.record['last_event_at']=0
        self.write_record();self.card=self.inspect()['pending_questions'][0]
        self.assertEqual(self.answer(self.payload())['status'],'answered')

    def test_changed_mode_or_run_rejects_without_terminal_input(self):
        self.answer(self.payload(expected_run_id='other'),success=False)
        self.answer(self.payload(expected_conversation_id='other'),success=False)
        self.redraw('target','default permissions','No, keep default permissions')
        self.assertNotEqual(self.inspect()['pending_questions'][0]['question_hash'],self.card['question_hash'])
        self.answer(self.payload(),success=False);self.assertEqual(self.received(),b'')

    def test_quoted_history_or_wrong_folder_is_not_actionable(self):
        self.record['turn_started']=time.time();self.write_record()
        self.assertEqual(self.inspect()['pending_questions'],[])
        self.record.pop('turn_started');self.write_record()
        self.redraw('folder','/other','/other');self.assertEqual(self.inspect()['pending_questions'],[])
        self.answer(self.payload(),success=False);self.assertEqual(self.received(),b'')

    def test_missing_acknowledgement_is_never_retried(self):
        (self.root/'hold-dialog').touch();p=self.payload()
        self.assertIn('delivery uncertain',self.answer(p,success=False));before=self.received()
        self.assertIn('delivery uncertain',self.answer(p,success=False));self.assertEqual(self.received(),before)

if __name__=='__main__':unittest.main()
