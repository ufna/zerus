"""Verify /clear against real Claude and HGS hooks, with a localhost model only."""
import json, os, shlex, subprocess, time, unittest, uuid
import test_input
import test_claude_questions_native as native

@unittest.skipUnless(native.CLAUDE, 'Set HGS_CLAUDE_TEST_BIN for the isolated native clear test')
class NativeClearContext(unittest.TestCase):
    setUp = native.NativeClaudeQuestions.setUp
    script = native.NativeClaudeQuestions.script
    write_record = native.NativeClaudeQuestions.write_record
    wait_for = native.NativeClaudeQuestions.wait_for
    screen = native.NativeClaudeQuestions.screen
    hook = native.NativeClaudeQuestions.hook
    inspect = native.NativeClaudeQuestions.inspect
    key = native.NativeClaudeQuestions.key

    def tmux(self, *args, **kwargs):
        if args[0] == 'new-session':
            env = dict(HGS_SESSION=self.name, HGS_RUN_ID=self.run_id,
                       HGS_STATE_DIR=str(self.state), HGS_CONFIG_DIR=str(self.root / 'config'), PATH=self.env['PATH'])
            command = 'env ' + ' '.join(shlex.quote(k+'='+v) for k,v in env.items()) + ' ' + shlex.quote(str(test_input.HGS)) + ' __state hook'
            path = self.root / '.claude/settings.json'
            config = json.loads(path.read_text())
            config['hooks'] = {event: [{'hooks': [{'type':'command', 'command':command}]}] for event in getattr(self,'hook_events',['SessionStart'])}
            path.write_text(json.dumps(config))
        return test_input.InputTransport.tmux(self, *args, **kwargs)

    def test_native_clear_confirms_new_conversation_without_restarting_process(self):
        self.key('Escape'); self.key('Escape'); time.sleep(.5)
        if 'No, keep bypass permissions' in self.screen(): self.key('Enter'); time.sleep(.3)
        self.record.update(phase='idle', activity='idle', active_tools={}); self.write_record()
        request = dict(request_id=str(uuid.uuid4()), expected_run_id=self.run_id,
                       expected_conversation_id=self.conversation_id)
        result = subprocess.run([str(test_input.HGS), 'clear-context', self.name, '--json'],
            input=json.dumps(request), env=self.env, text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr + '\n' + self.screen())
        self.assertEqual(json.loads(result.stdout)['status'], 'confirmed')
        current = json.loads(self.record_path.read_text())
        self.assertNotEqual(current['conversation_id'], self.conversation_id)
        self.assertEqual(current['pid'], self.record['pid'])
        self.assertEqual(current['run_id'], self.run_id)
        self.assertNotIn('clear_context_request', current)
        self.assertNotIn('Which release should we prepare?', self.screen())
        self.assertNotIn('Ask questions with the tool now.', self.screen())
        fresh = self.inspect()
        self.assertFalse(fresh.get('provider_messages'))
        self.assertFalse(fresh.get('pending_questions'))
        self.assertFalse(fresh.get('cache_hint'))
        self.assertEqual(fresh['phase'], 'idle')

if __name__ == '__main__': unittest.main()
