"""Portable checks for the disposable native file-trace reader only."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('archive_trace', Path(__file__).parent / 'support/archive_reference_file_trace.py')
TRACE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRACE)


class ScopedTrace(unittest.TestCase):
    def test_names_objects_and_redaction_keep_other_folders_out(self):
        xml = r'''<Events>
<Event><System><EventID>12</EventID><Execution ProcessID="41" ThreadID="42"/></System><EventData><Data Name="FileObject">0x123</Data><Data Name="FileName">\Device\Volume\fixture\state\archive-reference-writes-v1\.receipt.stage</Data></EventData></Event>
<Event><System><EventID>13</EventID></System><EventData><Data Name="FileObject">0x123</Data><Data Name="Status">0xc0000043</Data></EventData></Event>
<Event><EventData><Data Name="FileObject">0x456</Data><Data Name="FileName">\outside\unrelated.txt</Data></EventData></Event>
<Event><EventData><Data Name="FileObject">0x123</Data><Data Name="FileName">\outside\reused-pointer.txt</Data></EventData></Event>
</Events>'''
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'trace.xml'
            path.write_text(xml)
            rows = TRACE.selected_events(path, Path('fixture'))
        self.assertEqual(len(rows), 3)
        self.assertEqual(rows[0]['ProcessID'], '41')
        self.assertEqual(rows[1]['Status'], '0xc0000043')
        self.assertEqual(rows[2]['FileName'], '<outside-private-scope>')
        self.assertNotIn('unrelated.txt', json.dumps(rows))
        self.assertNotIn('reused-pointer.txt', json.dumps(rows))

    def test_a_null_object_cannot_expand_scope_to_unrelated_events(self):
        xml = r'''<Events>
<Event><EventData><Data Name="FileObject">0x0000</Data><Data Name="FileName">\fixture\state\archive-reference-writes-v1\receipt.json</Data></EventData></Event>
<Event><EventData><Data Name="FileObject">0x0000</Data><Data Name="FileName">\outside\unrelated.txt</Data></EventData></Event>
</Events>'''
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'trace.xml'
            path.write_text(xml)
            rows = TRACE.selected_events(path, Path('fixture'))
        self.assertEqual(len(rows), 1)
