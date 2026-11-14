"""Portable sampling controls; no scheduler, native transport or process starts."""
import contextlib
import io
import unittest
from unittest import mock

import windows_flake_samples as samples


class Sampling(unittest.TestCase):
    def test_round_count_is_explicit_bounded_and_disabled_by_default(self):
        self.assertEqual(samples.round_count({}), 0)
        self.assertEqual(samples.round_count({'ZERUS_WINDOWS_FLAKE_ROUNDS':'40'}), 40)
        for value in ('-1', '41', '1.5', '1e1', '', 'true'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                samples.round_count({'ZERUS_WINDOWS_FLAKE_ROUNDS':value})

    def test_serial_choices_use_fresh_requests_only_after_settlement(self):
        grant = mock.Mock()
        n = iter(range(4))
        choose = mock.Mock(side_effect=lambda desired: dict(ok=True, running_mode=desired,
                                                          request_id='request-'+str(next(n))))
        with contextlib.redirect_stderr(io.StringIO()):
            samples.mode_rounds(2, grant, choose)
        self.assertEqual(grant.call_args_list,
                         [mock.call('ownership_grant'), mock.call('ownership_rearm')])
        self.assertEqual(choose.call_args_list, [mock.call('background'), mock.call('passive')]*2)

    def test_unknown_choice_stops_sampling_without_retry_or_later_choice(self):
        for failed_mode in ('background', 'passive'):
            with self.subTest(failed_mode=failed_mode):
                grant = mock.Mock()
                def choice(desired):
                    if desired == failed_mode:
                        return dict(ok=False, code='mode_transition_unconfirmed',
                                    stop_timing={'budget_ms':2000})
                    return dict(ok=True, running_mode=desired, request_id='fresh-request')
                choose = mock.Mock(side_effect=choice)
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaisesRegex(
                        AssertionError, 'mode_transition_unconfirmed'):
                    samples.mode_rounds(40, grant, choose)
                grant.assert_called_once_with('ownership_grant')
                expected = [mock.call('background')]
                if failed_mode == 'passive': expected.append(mock.call('passive'))
                self.assertEqual(choose.call_args_list, expected)

    def test_reused_request_cannot_qualify_another_choice(self):
        choose = mock.Mock(side_effect=lambda desired: dict(ok=True, running_mode=desired,
                                                          request_id='same-request'))
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaisesRegex(AssertionError, 'fresh request'):
            samples.mode_rounds(40, mock.Mock(), choose)
        self.assertEqual(choose.call_count, 2)


class WorkflowSampling(unittest.TestCase):
    def test_repeat_is_dispatch_only_and_keeps_the_ordinary_transport_command(self):
        from pathlib import Path
        import re
        root = Path(__file__).resolve().parents[1]
        entry = (root/'.github/workflows/ci.yml').read_text()
        checks = (root/'.github/workflows/checks.yml').read_text()
        self.assertIn("windows_flake_rounds: ${{ github.event_name == 'workflow_dispatch' && fromJSON(format('{0}', inputs.windows_flake_rounds || 0)) || 0 }}", entry)
        selected = [('client-native-windows', 'Loaded native Windows task-mode samples'),
                    ('windows-desktop', 'Loaded native Windows transport samples')]
        for name, label in selected:
            job = re.search(r'^  '+name+r':\n.*?(?=^  [a-zA-Z0-9_-]+:|\Z)', checks, re.M|re.S).group()
            self.assertNotIn('ZERUS_WINDOWS_FLAKE_ROUNDS', job.split('    steps:', 1)[0])
            step = re.search(r'^      - name: '+label+r'\n.*?(?=^      - |\Z)', job, re.M|re.S).group()
            if name == 'client-native-windows':
                self.assertIn("if: ${{ !cancelled() && steps.windows-shell.outcome == 'success' && github.event_name == 'workflow_dispatch' && inputs.windows_flake_rounds != 0 }}", step)
                self.assertIn('id: windows-shell', job)
            else:
                self.assertIn("if: ${{ github.event_name == 'workflow_dispatch' && inputs.windows_flake_rounds != 0 }}", step)
            self.assertIn('ZERUS_WINDOWS_FLAKE_ROUNDS: ${{ inputs.windows_flake_rounds }}', step)
        normal = re.search(r'^      - name: Native Windows bootstrap and framed transport\n.*?(?=^      - |\Z)', checks, re.M|re.S).group()
        self.assertIn('timeout-minutes: 6', normal)
        self.assertIn('--custody-only --timeout 300', normal)
        self.assertIn('python -m unittest -v test_windows_bootstrap test_remote_windows.WindowsNativeTransport', normal)

    def test_manual_string_count_becomes_a_number_before_the_reusable_call(self):
        from pathlib import Path
        from types import SimpleNamespace
        import json
        import re
        workflow = (Path(__file__).resolve().parents[1]/'.github/workflows/ci.yml').read_text()
        expression = re.search(r'^      windows_flake_rounds: (\$\{\{.*\}\})$', workflow, re.M)[1]
        expression = expression.removeprefix('${{ ').removesuffix(' }}')
        expression = expression.replace('&&', ' and ').replace('||', ' or ')
        for event, raw, expected in [('workflow_dispatch', '40', 40), ('workflow_dispatch', 40, 40),
                                     ('workflow_dispatch', '0', 0), ('push', '40', 0),
                                     ('pull_request', '40', 0)]:
            with self.subTest(event=event, raw=raw):
                value = eval(expression, {'__builtins__':{}}, {
                    'github':SimpleNamespace(event_name=event),
                    'inputs':SimpleNamespace(windows_flake_rounds=raw),
                    'format':lambda template, *args: template.format(*args),
                    'fromJSON':json.loads})
                self.assertIs(type(value), int, 'called workflow requires a number, not a manual-input string')
                self.assertEqual(value, expected)
