#!/usr/bin/env python3
"""Late direct-peer dispatch cannot orphan its target-owned Git checkout."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import unittest

spec=importlib.util.spec_from_file_location('peer_fixture',Path(__file__).with_name('test_mobile_peers.py'))
fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)
class WorktreeTransport(unittest.TestCase):
    call=fixture.MobilePeers.call
    request=fixture.MobilePeers.request
    running=staticmethod(fixture.MobilePeers.running)
    def setUp(self):fixture.MobilePeers.setUp(self)
    def test_late_dispatch_target_cleans_after_gateway_deadline(self):
        repo=self.root/'repo';subprocess.run([shutil.which('git'),'init','-q','-b','main',str(repo)],check=True)
        subprocess.run([shutil.which('git'),'-C',str(repo),'-c','user.name=Fixture','-c','user.email=fixture@example.test','commit','--allow-empty','-qm','initial'],check=True)
        marker=self.root/'hook.pid';hook=repo/'.git/hooks/post-checkout'
        hook.write_text('#!'+sys.executable+'\nimport os,time\nopen('+repr(str(marker))+',"w").write(str(os.getpid()))\ntime.sleep(60)\n');hook.chmod(0o700)
        ssh=self.root/'home/.local/bin/ssh';source=ssh.read_text()
        replacement='''if args[:2] == ['swarm', '__mobile-peer-local']:
    data=sys.stdin.buffer.read()
    time.sleep(20)
    child=subprocess.Popen([os.environ['MOBILE_PEERS_TEST_HGS'], *args], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    (root/'remote-native.pid').write_text(str(child.pid))
    out,err=child.communicate(data)
    sys.stdout.buffer.write(out);sys.stderr.buffer.write(err);sys.exit(child.returncode)
'''
        source=source.replace("os.execve(os.environ['MOBILE_PEERS_TEST_HGS']",replacement+"os.execve(os.environ['MOBILE_PEERS_TEST_HGS']")
        ssh.write_text(source)
        destination=self.root/'linked'
        argv=['worktrees','create','--path',str(repo),'--common-dir',str((repo/'.git').resolve()),'--destination',str(destination),'--branch','feature','--base','HEAD','--request-id','11111111-1111-4111-8111-111111111111','--json','--mobile']
        started=time.monotonic();result=self.call('a','swarm','mobile-peer','--json',data=self.request(argv),ok=False,timeout=33)
        self.assertLess(time.monotonic()-started,32);self.assertTrue(marker.exists(),result.stderr)
        owned=[int(marker.read_text()),int((self.root/'remote-native.pid').read_text())]
        deadline=time.monotonic()+9
        while time.monotonic()<deadline and any(self.running(pid) for pid in owned):time.sleep(.05)
        self.assertFalse(any(self.running(pid) for pid in owned),'late target Git/native process survived its own mobile budget')
        self.assertTrue(destination.is_dir())
if __name__=='__main__':unittest.main()
