#!/usr/bin/env python3
"""Run the real Machines editor with private fake profiles, D-Bus and Xvfb."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

CLIENT = Path(__file__).resolve().parent.parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--inner', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    app, output = args.app.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if not args.inner:
        private = Path(tempfile.mkdtemp(prefix='zerus-machines-smoke-'))
        private.chmod(0o700)
        env = {'PATH':'/usr/bin:/bin', 'LANG':'C.UTF-8', 'GDK_BACKEND':'x11', 'WAYLAND_DISPLAY':'zerus-nonexistent',
               'NO_AT_BRIDGE':'1', 'WEBKIT_DISABLE_DMABUF_RENDERER':'1', 'GSETTINGS_BACKEND':'memory'}
        for key, folder in [('HOME','home'),('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_CACHE_HOME','cache'),
                            ('XDG_STATE_HOME','state'),('XDG_RUNTIME_DIR','runtime'),('TMPDIR','tmp'),('TMUX_TMPDIR','tmux'),
                            ('HGS_STATE_DIR','hgs-state'),('HGS_CONFIG_DIR','hgs-config')]:
            path = private / folder;path.mkdir(mode=0o700);env[key] = str(path)
        with (output/'driver.log').open('w') as log:
            command = ['dbus-run-session','--','xvfb-run','-a','-s','-screen 0 1440x900x24',sys.executable,__file__,
                       '--app',str(app),'--output',str(output),'--inner']
            log.write('EXECUTED '+json.dumps(command)+'\nFresh allowlisted private HOME/XDG/TMPDIR/TMUX_TMPDIR/D-Bus/Xvfb\n');log.flush()
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=100)
            log.write('\nEXIT_CODE '+str(result.returncode)+'\n')
        # The driver already retained bounded call/profile evidence. Do not
        # copy browser caches, runtime sockets or an entire scratch tree.
        shutil.rmtree(private)
        print((output/'driver.log').read_text()[-2500:])
        return result.returncode

    scratch = Path(os.environ['TMPDIR'])/'machines-fixture';scratch.mkdir(mode=0o700)
    fake = scratch/'hgs';shutil.copy2(CLIENT/'src-tauri/tests/support/fake_hgs_machines.py',fake);fake.chmod(0o700)
    (scratch/'config.json').write_text(json.dumps({'machines_fixture':True}))
    helpers = scratch/'helpers';helpers.mkdir(mode=0o700)
    for name in ['hgs','systemctl','launchctl','xdg-open','gio','sensible-browser']:
        file=helpers/name;file.write_text('#!/bin/sh\nexit 1\n');file.chmod(0o700)
    env={key:os.environ[key] for key in ['PATH','LANG','DISPLAY','DBUS_SESSION_BUS_ADDRESS','XAUTHORITY','HOME','XDG_CONFIG_HOME',
        'XDG_DATA_HOME','XDG_STATE_HOME','XDG_CACHE_HOME','XDG_RUNTIME_DIR','TMPDIR','TMUX_TMPDIR','HGS_STATE_DIR','HGS_CONFIG_DIR',
        'GDK_BACKEND','WAYLAND_DISPLAY','NO_AT_BRIDGE','WEBKIT_DISABLE_DMABUF_RENDERER','GSETTINGS_BACKEND'] if key in os.environ}
    env.update(PATH=str(helpers)+':/usr/bin:/bin',ZERUS_CLIENT_HEADLESS='1',ZERUS_CLIENT_REPLAY=str(CLIENT/'fixtures/replay.ndjson'),
               ZERUS_CLIENT_HGS=str(fake),ZERUS_CLIENT_QUERY='machines-smoke=1',ZERUS_CLIENT_PERF='smoke')
    import gi
    gi.require_version('Gtk','3.0');gi.require_version('Gdk','3.0')
    from gi.repository import Gtk,Gdk
    Gtk.init([])
    captured=set()
    with (output/'app.log').open('w') as log:
        process=subprocess.Popen([str(app),'--sessions'],cwd=scratch,env=env,stdout=log,stderr=subprocess.STDOUT)
        try:
            deadline=time.monotonic()+70
            while process.poll() is None and time.monotonic()<deadline:
                text=(output/'app.log').read_text()
                for stage in ['EXISTING','UNSAVED','NEW','REMOVE']:
                    if stage not in captured and 'machines-smoke: '+stage in text:
                        # The log arrives before WebKit's next compositor frame.
                        # The page holds each stage for 700 ms.
                        time.sleep(.25)
                        root=Gdk.get_default_root_window()
                        pix=Gdk.pixbuf_get_from_window(root,0,0,1440,900);pix.savev(str(output/('app-'+stage.lower()+'.png')),'png',[],[])
                        captured.add(stage);print('CAPTURE',stage,flush=True)
                time.sleep(.05)
            code=process.poll()
            if code is None:raise RuntimeError('Machines smoke exceeded 70 seconds')
            text=(output/'app.log').read_text()
            assert code==0 and 'machines-smoke: PASS 12' in text,text[-5000:]
            assert len(captured)==4,captured
            calls=[json.loads(line) for line in (scratch/'log.ndjson').read_text().splitlines()]
            writes=[c for c in calls if c['argv'][:1]==['machine'] and c['argv'][1] in ['set','reset','remove']]
            assert len(writes)==4,writes
            assert all('--revision' in c['argv'] for c in writes)
            shutil.copy2(scratch/'log.ndjson',output/'calls.ndjson')
            (output/'results.json').write_text(json.dumps({'passed':12,'failed':0,'mutations':4,'captures':sorted(captured)},indent=2)+'\n')
            print('PASS 12; four exact revision-guarded writes; four screenshots',flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                try:process.wait(timeout=8)
                except subprocess.TimeoutExpired:process.kill();process.wait(timeout=8)
            for name in ['log.ndjson','machine-profiles.json']:
                if (scratch/name).exists():
                    shutil.copy2(scratch/name,output/name)
    return 0

if __name__=='__main__':raise SystemExit(main())
