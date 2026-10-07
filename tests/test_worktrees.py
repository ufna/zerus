#!/usr/bin/env python3
"""Catalogs and explicit worktree creation against isolated real Git repositories."""
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

REPO=Path(__file__).resolve().parents[1]
HGS=Path(os.environ.get('HGS_TEST_BIN',REPO/'target/debug/hgs')).resolve()
GIT=shutil.which('git')

class Worktrees(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='hgs-worktrees-');self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.home=self.root/'home';self.home.mkdir()
        self.env=dict(os.environ,HOME=str(self.home),XDG_CONFIG_HOME=str(self.home/'.config'),HGS_STATE_DIR=str(self.root/'state'),HGS_SELF='fixture',HGS_PEERS='',GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='/dev/null')
        for key in ('GIT_DIR','GIT_COMMON_DIR','GIT_WORK_TREE','GIT_INDEX_FILE','GIT_CONFIG_COUNT'):self.env.pop(key,None)
        self.repo=self.root/'проект repo';self.repo.mkdir();self.git('init','-q','-b','main',str(self.repo))
        self.git('-C',str(self.repo),'-c','user.name=Fixture','-c','user.email=fixture@example.test','commit','--allow-empty','-qm','initial')
    def git(self,*args):
        return subprocess.check_output([GIT,*args],env=self.env,stderr=subprocess.PIPE).decode().strip()
    def catalog(self,path=None,refresh=False):
        result=subprocess.run([str(HGS),'worktrees','--path',str(path or self.repo),'--json',*(['--refresh'] if refresh else [])],env=self.env,text=True,capture_output=True,timeout=7)
        self.assertEqual(result.returncode,0,result.stderr);return json.loads(result.stdout)
    def fake(self,body):
        file=self.home/'.local/bin/git';file.parent.mkdir(parents=True,exist_ok=True)
        file.write_text('#!'+sys.executable+'\nimport os,sys,time\n'+body);file.chmod(0o755)
    def linked(self,name='linked tree'):
        path=self.root/name;self.git('-C',str(self.repo),'worktree','add','-qb','feature/'+name.replace(' ','-'),str(path));return path
    def test_main_linked_subdirectory_symlink_and_identity(self):
        linked=self.linked();nested=linked/'nested';nested.mkdir();alias=self.root/'alias';alias.symlink_to(nested,target_is_directory=True)
        data=self.catalog(alias);self.assertEqual(data['state'],'ok');self.assertEqual(data['path'],str(nested.resolve()));self.assertEqual(data['selected_root'],str(linked.resolve()))
        self.assertEqual(data['common_dir'],str((self.repo/'.git').resolve()));self.assertEqual(data['machine'],'fixture')
        self.assertEqual([r['kind'] for r in data['worktrees']],['main','linked']);self.assertTrue(all(r['available'] for r in data['worktrees']))
        self.assertEqual(data['worktrees'][1]['branch'],'feature/linked-tree')
        self.assertEqual(self.catalog()['sampled_at'],data['sampled_at'])
    def test_plain_folder_unborn_bare_and_missing(self):
        plain=self.root/'plain';plain.mkdir();self.assertEqual(self.catalog(plain)['state'],'not_repo')
        self.assertFalse((plain/'.git').exists());self.assertEqual(self.catalog(self.root/'missing')['state'],'folder_unavailable')
        unborn=self.root/'unborn';self.git('init','-q',str(unborn));self.assertEqual(self.catalog(unborn)['state'],'ok')
        bare=self.root/'bare';self.git('clone','--bare','-q',str(self.repo),str(bare));data=self.catalog(bare)
        self.assertEqual(data['worktrees'][0]['kind'],'bare');self.assertIsNone(data['selected_root'])
    def test_locked_detached_missing_and_no_cleanup(self):
        linked=self.linked();self.git('-C',str(linked),'checkout','--detach','-q');self.git('-C',str(self.repo),'worktree','lock','--reason','keep this copy',str(linked))
        row=self.catalog()['worktrees'][1];self.assertTrue(row['detached']);self.assertTrue(row['locked']);self.assertEqual(row['locked_reason'],'keep this copy')
        self.git('-C',str(self.repo),'worktree','unlock',str(linked));shutil.rmtree(linked)
        row=self.catalog(refresh=True)['worktrees'][1];self.assertFalse(row['available']);self.assertTrue(row['prunable'])
        self.assertTrue((self.repo/'.git/worktrees/linked-tree').exists())
    def test_refresh_cache_and_independent_clones(self):
        before=self.catalog();self.linked();self.assertEqual(len(self.catalog()['worktrees']),1)
        self.assertEqual(len(self.catalog(refresh=True)['worktrees']),2)
        clone=self.root/'other'/'проект repo';clone.parent.mkdir();self.git('clone','-q',str(self.repo),str(clone))
        self.assertNotEqual(self.catalog(clone)['common_dir'],before['common_dir'])
    def test_submodule_identity_is_distinct_from_parent(self):
        source=self.root/'submodule-source';self.git('clone','-q',str(self.repo),str(source))
        self.git('-C',str(self.repo),'-c','protocol.file.allow=always','submodule','add','-q',str(source),'child')
        data=self.catalog(self.repo/'child');self.assertEqual(data['state'],'ok')
        self.assertEqual(data['worktrees'][0]['path'],str((self.repo/'child').resolve()))
        self.assertNotEqual(data['common_dir'],self.catalog()['common_dir'])
    def test_environment_cannot_redirect_discovery(self):
        self.env.update(GIT_DIR='/nonexistent',GIT_WORK_TREE='/nonexistent',GIT_COMMON_DIR='/nonexistent',GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='core.bare',GIT_CONFIG_VALUE_0='true')
        data=self.catalog();self.assertEqual(data['state'],'ok');self.assertEqual(data['worktrees'][0]['path'],str(self.repo.resolve()))
    def test_errors_are_distinct_and_keep_last_snapshot(self):
        before=self.catalog();self.fake("sys.stderr.write('fatal: permission denied\\n');sys.exit(1)\n")
        failed=self.catalog(refresh=True);self.assertEqual(failed['state'],'metadata_error');self.assertTrue(failed['stale']);self.assertEqual(failed['worktrees'],before['worktrees'])
        if not Path('/opt/homebrew/bin/git').exists() and not Path('/usr/local/bin/git').exists():
            (self.home/'.local/bin/git').unlink();self.env['PATH']='/nonexistent';self.assertEqual(self.catalog()['state'],'git_unavailable')
    def test_timeout_and_large_output_are_bounded(self):
        self.fake('time.sleep(20)\n');start=time.monotonic();self.assertEqual(self.catalog()['state'],'timeout');self.assertLess(time.monotonic()-start,5)
        self.fake("sys.stdout.write('x'*(2*1024*1024));sys.stdout.flush()\n");self.assertEqual(self.catalog()['state'],'too_large')
    def test_large_catalog_drains_pipes_and_marks_partial(self):
        common=str(self.repo/'.git')
        self.fake("if 'worktree' in sys.argv:\n for i in range(520):\n  sys.stdout.write('worktree /fixture/'+str(i)+'-'+('x'*150)+'\\0HEAD 0000000000000000000000000000000000000000\\0branch refs/heads/a\\0\\0')\nelse: print("+repr(common)+")\n")
        data=self.catalog();self.assertEqual(data['state'],'ok');self.assertTrue(data['partial']);self.assertEqual(len(data['worktrees']),512)
    def test_concurrent_catalog_requests_share_snapshot(self):
        linked=self.linked()
        with concurrent.futures.ThreadPoolExecutor() as pool:data=list(pool.map(self.catalog,[self.repo,linked]*3))
        self.assertTrue(all(v['state']=='ok' for v in data));self.assertEqual(len({v['sampled_at'] for v in data}),1)
    def create(self,destination=None,branch='feature/new',base='HEAD',extra=(),source=None):
        return subprocess.run([str(HGS),'worktrees','create','--path',str(source or self.repo),'--destination',str(destination or self.root/'new worktree'),
            '--branch',branch,'--base',base,'--request-id','test-create','--json',*extra],env=self.env,text=True,capture_output=True,timeout=10)
    def test_create_pins_base_preserves_source_and_refreshes_catalog(self):
        base=self.git('-C',str(self.repo),'rev-parse','HEAD')
        self.git('-C',str(self.repo),'-c','user.name=Fixture','-c','user.email=fixture@example.test','commit','--allow-empty','-qm','second')
        dirty=self.repo/'untracked.txt';dirty.write_text('keep my edits')
        self.catalog();destination=self.root/'новая копия'
        result=self.create(destination,base=base,extra=('--common-dir',str(self.repo/'.git')))
        self.assertEqual(result.returncode,0,result.stderr);data=json.loads(result.stdout)
        self.assertEqual(data['status'],'created');self.assertEqual(data['path'],str(destination.resolve()));self.assertEqual(data['request_id'],'test-create')
        self.assertEqual(self.git('-C',str(destination),'rev-parse','HEAD'),base)
        self.assertEqual(self.git('-C',str(destination),'branch','--show-current'),'feature/new')
        self.assertEqual(self.git('-C',str(self.repo),'branch','--show-current'),'main');self.assertEqual(dirty.read_text(),'keep my edits')
        self.assertEqual(len(self.catalog()['worktrees']),2)
    def test_create_rejects_collisions_symlinks_and_repository_change(self):
        destination=self.root/'new worktree'
        existing=self.create(branch='main');self.assertNotEqual(existing.returncode,0);self.assertFalse(destination.exists())
        for branch in ('main/child','HEAD'):
            self.assertNotEqual(self.create(branch=branch).returncode,0);self.assertFalse(destination.exists())
        self.git('-C',str(self.repo),'branch','topic/child');self.assertNotEqual(self.create(branch='topic').returncode,0);self.assertFalse(destination.exists())
        destination.mkdir();keep=destination/'keep';keep.write_text('unchanged')
        self.assertNotEqual(self.create().returncode,0);self.assertEqual(keep.read_text(),'unchanged')
        alias=self.root/'alias';alias.symlink_to(self.root/'missing')
        self.assertNotEqual(self.create(alias).returncode,0);self.assertTrue(alias.is_symlink())
        changed=self.create(self.root/'different',extra=('--common-dir',str(self.root)))
        self.assertNotEqual(changed.returncode,0);self.assertIn('Repository changed',changed.stderr);self.assertFalse((self.root/'different').exists())
    def test_create_validates_inputs_and_dry_run_before_any_mutation(self):
        for branch,base in [('-bad','HEAD'),('bad..branch','HEAD'),('@{-1}','HEAD'),('good','does-not-exist'),('good','--help')]:
            with self.subTest(branch=branch,base=base):
                result=self.create(branch=branch,base=base);self.assertNotEqual(result.returncode,0,result.stdout);self.assertFalse((self.root/'new worktree').exists())
        result=self.create(extra=('--dry-run',));self.assertNotEqual(result.returncode,0);self.assertFalse((self.root/'new worktree').exists())
        self.assertNotEqual(self.create(self.repo/'.git'/'inside').returncode,0)
        self.assertNotEqual(self.create(self.root/'missing'/'child').returncode,0)
        plain=self.root/'plain';plain.mkdir();self.assertNotEqual(self.create(source=plain).returncode,0);self.assertFalse((plain/'.git').exists())
        self.assertEqual(self.git('-C',str(self.repo),'for-each-ref','--format=%(refname)','refs/heads'),'refs/heads/main')
    def test_create_from_linked_and_bare_repositories(self):
        linked=self.linked();self.assertEqual(self.create(source=linked).returncode,0)
        bare=self.root/'bare';self.git('clone','--bare','-q',str(self.repo),str(bare))
        result=self.create(self.root/'bare-checkout',branch='from-bare',source=bare);self.assertEqual(result.returncode,0,result.stderr)
        self.assertTrue((self.root/'bare-checkout'/'.git').is_file())
    def test_create_failure_keeps_checkout_and_branch(self):
        hook=self.repo/'.git/hooks/post-checkout';hook.write_text('#!/bin/sh\nprintf "hook-created" > keep.txt\nexit 1\n');hook.chmod(0o755)
        result=self.create();self.assertNotEqual(result.returncode,0);self.assertIn('have been kept',result.stderr)
        self.assertEqual((self.root/'new worktree/keep.txt').read_text(),'hook-created')
        self.assertEqual(self.git('-C',str(self.root/'new worktree'),'branch','--show-current'),'feature/new')
        retry=self.create();self.assertNotEqual(retry.returncode,0);self.assertIn('already exists',retry.stderr)
    def test_concurrent_create_reserves_destination(self):
        with concurrent.futures.ThreadPoolExecutor() as pool:results=list(pool.map(lambda branch:self.create(branch=branch),['first','second']))
        self.assertEqual(sum(r.returncode==0 for r in results),1)
        self.assertEqual(len(self.catalog()['worktrees']),2)

if __name__=='__main__':unittest.main()
