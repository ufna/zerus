import unittest

from terminal_key_diagnostics import Trace, navigation_probe, selected_slot


class NavigationDiagnostics(unittest.TestCase):
    def run_probe(self, samples, complete=False, deadline=20, send_error=False):
        now = [0.]
        trace = Trace(clock=lambda: now[0])
        keys = []
        snapshots = iter(samples)

        def observe(stop):
            now[0] += .01
            return next(snapshots, dict(same_prompt=False, selection=None))

        def send(key, stop):
            keys.append(key)
            if send_error:
                raise RuntimeError('uncertain queue acknowledgement')

        try:
            navigation_probe(trace, observe, send, lambda: complete, deadline,
                             sleep=lambda delay: now.__setitem__(0, now[0] + delay))
        except RuntimeError:
            pass
        return keys, trace

    def test_only_two_non_approving_keys_and_observed_return_to_yes(self):
        keys, trace = self.run_probe([dict(same_prompt=True, selection=x) for x in (1, 2, 2, 1)])
        self.assertEqual(keys, ['Down', 'Up'])
        self.assertEqual(trace.events[-1]['selection'], 1)

    def test_changed_scope_or_highlight_never_receives_a_key(self):
        for sample in [dict(same_prompt=False, selection=1), dict(same_prompt=True, selection=4)]:
            self.assertEqual(self.run_probe([sample])[0], [])

    def test_changed_scope_after_down_never_gets_an_up_or_another_enter(self):
        self.assertEqual(self.run_probe([dict(same_prompt=True, selection=1),
                                         dict(same_prompt=False, selection=2)])[0], ['Down'])

    def test_completion_or_expired_original_deadline_never_gets_a_probe(self):
        self.assertEqual(self.run_probe([], complete=True)[0], [])
        self.assertEqual(self.run_probe([], deadline=0)[0], [])

    def test_missing_response_or_uncertain_queue_ack_never_repeats_a_key(self):
        samples = [dict(same_prompt=True, selection=1)] * 10
        self.assertEqual(self.run_probe(samples)[0], ['Down'])
        self.assertEqual(self.run_probe(samples, send_error=True)[0], ['Down'])

    def test_screen_selection_and_trace_have_finite_bounds(self):
        self.assertEqual(selected_slot('❯ 1. Yes\n  2. No'), 1)
        self.assertIsNone(selected_slot('❯ 1. Yes\n❯ 2. No'))
        trace = Trace()
        for _ in range(100):
            trace.add('bounded')
        self.assertEqual(len(trace.events), 20)
