"""Native Claude permissions, using only a localhost API and temporary files."""
import os, unittest
import test_claude_questions_native as native
import test_questions as questions

@unittest.skipUnless(native.CLAUDE, 'Set HGS_CLAUDE_TEST_BIN for the isolated native approval fixture')
class NativeClaudeApprovals(unittest.TestCase):
    tool_name = 'Bash'
    tool_input = {'command': 'printf approved > approved.txt', 'description': 'Create a fixture file after approval'}
    setUp = native.NativeClaudeQuestions.setUp
    script = native.NativeClaudeQuestions.script
    tmux = native.NativeClaudeQuestions.tmux
    write_record = native.NativeClaudeQuestions.write_record
    wait_for = native.NativeClaudeQuestions.wait_for
    screen = native.NativeClaudeQuestions.screen
    hook = native.NativeClaudeQuestions.hook
    inspect = native.NativeClaudeQuestions.inspect
    answer = questions.Questions.answer
    payload = questions.Questions.payload

    def decision(self, choice, **updates):
        return self.payload(answers=[dict(question_id='approval', selected_option_ids=[choice], text='')], **updates)

    def test_approve_once_does_not_change_permission_mode(self):
        self.assertTrue(self.card['approval'])
        self.assertEqual(self.card['questions'][0]['body'], self.tool_input['command'])
        self.answer(self.decision('allow'))
        self.wait_for(lambda: (self.root/'work/approved.txt').exists())
        self.assertEqual((self.root/'work/approved.txt').read_text(), 'approved')
        self.assertNotIn('bypassPermissions', (self.root/'.claude/settings.json').read_text())

    def test_deny_does_not_execute_command(self):
        self.answer(self.decision('deny'))
        self.assertFalse((self.root/'work/approved.txt').exists())

    def test_changed_command_and_hash_send_no_approval(self):
        self.answer(self.decision('allow', expected_question_hash='0'*64), success=False)
        self.assertIn('Do you want to proceed?', self.screen())
        self.assertFalse((self.root/'work/approved.txt').exists())
        self.hook(dict(hook_event_name='PermissionRequest', tool_name='Bash', tool_use_id='changed', tool_input={'command':'rm changed'}))
        self.answer(self.decision('allow'), success=False)
        self.assertIn('Do you want to proceed?', self.screen())
        self.assertFalse((self.root/'work/approved.txt').exists())

if __name__ == '__main__': unittest.main()
