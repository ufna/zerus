"""Test-only observations after one original Enter; never approve or retry it."""
import re
import time


class Trace:
    def __init__(self, clock=time.monotonic):
        self.clock = clock
        self.started = clock()
        self.events = []

    def add(self, event, **data):
        if len(self.events) < 20:
            self.events.append(dict(event=event, elapsed_ms=round((self.clock() - self.started) * 1000, 3),
                                    wall_unix=time.time(), **data))


def selected_slot(screen):
    slots = re.findall(r'^\s*[❯›]\s+(\d+)\.\s+', screen, re.MULTILINE)
    return int(slots[0]) if len(slots) == 1 else None


def navigation_probe(trace, observe, send, completed, deadline, sleep=time.sleep):
    """Observe Down/Up once each within five seconds and the original deadline.

    Neither the Enter nor the test's success condition is retried. Only the
    original verified approval with Yes still selected admits the first arrow.
    Losing that original identity, worker or panel closes the probe.
    """
    stop = min(deadline, trace.clock() + 5)
    for key, before, after in [('Down', 1, 2), ('Up', 2, 1)]:
        if completed() or trace.clock() >= stop:
            return
        sample = observe(stop)
        trace.add('before_navigation', key=key, **sample)
        if not sample.get('same_prompt') or sample.get('selection') != before or completed():
            return
        if trace.clock() >= stop:
            return
        # The only keys this probe can submit; no Enter, literal text or retry.
        assert key in ('Down', 'Up')
        send(key, stop)
        trace.add('navigation_queued', key=key)
        changed = False
        until = min(stop, trace.clock() + 2)
        for _ in range(6):
            if completed() or trace.clock() >= until:
                return
            sample = observe(until)
            trace.add('after_navigation', key=key, **sample)
            if not sample.get('same_prompt'):
                return
            if sample.get('selection') == after:
                changed = True
                break
            sleep(min(.1, max(0, until - trace.clock())))
        if not changed:
            return
