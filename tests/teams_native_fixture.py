"""Isolated real-agent fixture for Agent Teams (milestone 1, package 15).

One private root per test class holds HOME, every XDG folder, the runtime folder,
the hgs state and configuration, the agents' account homes, TMPDIR and an existing
0700 TMUX_TMPDIR. A tmux wrapper first on PATH refuses any call that names no
server inside the root; ssh, scp, systemd-run and hgs are refusing shims, so the
worktree build runs only by its absolute path (HGS_TEST_BIN) and nothing reaches
another computer, the user manager or an installed hgs. The real agent is the
pinned image the runner selected (HGS_<AGENT>_TEST_BIN). Its model is a keyless
loopback fixture that answers text or a scripted shell step, so every place where
the Teams service touches a session runs against the real agent without any
account or model quota.

A private `hgs desktop serve --background` with the Teams preview on launches the
lead and the member; the person answers their startup cards (folder trust in this
round; the standing confirmations come after zerus-63rru.48 and .49). Every wait
names what it waits for, has an explicit bound and logs its measured duration as
`ZERUS_TEAMS_WAIT=` lines and in the evidence folder. A socket census samples the
fixture's own processes and fails on any peer that is neither loopback nor an
allow-listed host of the agent under test.
"""
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import uuid

import model_api_fixture as api

HGS = os.environ.get('HGS_TEST_BIN', '')
REAL_TMUX = shutil.which('tmux') or '/usr/bin/tmux'
SYSTEM_PATH = '/usr/local/bin:/usr/bin:/bin' if sys.platform != 'darwin' else '/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin'
FIXTURE_KEY = 'fixture-key'
DONE = 'Fixture complete.'
REPLY = 'Fixture reply.'
# Hosts each agent contacts whatever its switches say (measured 2026-11-12 with Claude
# Code 2.1.322 and Codex 0.167.2; docs/ci-and-aur.md lists them with the switches set).
# Any other non-loopback peer fails the class.
ALLOWED_HOSTS = {
    'claude': ('downloads.claude.ai',),
    'codex': ('raw.githubusercontent.com', 'ab.chatgpt.com'),
    'kimi': (),
}
# Bounds are about three times the duration measured on a loaded development host
# (the upstream canary has no Teams step yet); the logged durations tighten them.
BOUND = dict(service=60, bound=420, listed=90, card=180, ready=180, parent=200, handout=240, turn=180,
             doorbell=240, cancel=240, stop=180, exit=60, teardown=60)

TMUX_WRAPPER = r'''#!/bin/bash
# Teams fixture tmux wrapper: logs every call; passes it to the real tmux only when it
# names a server inside the private root (-S inside it, $TMUX inside it, or no socket
# with the existing private TMUX_TMPDIR). Anything else is refused.
ROOT=@ROOT@
REAL=@REAL@
args=("$@"); n=${#args[@]}; sock=""; idx=0; version_only=0
while (( idx < n )); do
  a=${args[idx]}
  case "$a" in
    --) break ;;
    -S) sock=${args[idx+1]}; idx=$((idx+2)) ;;
    -S*) sock=${a#-S}; idx=$((idx+1)) ;;
    -L|-f|-c|-T) idx=$((idx+2)) ;;
    -V) version_only=1; idx=$((idx+1)) ;;
    -*) idx=$((idx+1)) ;;
    *) break ;;
  esac
done
inside() { local p; p=$(realpath -m -- "$1") || return 1; [[ "$p" == "$ROOT"/* ]]; }
verdict=refused
if [[ -n "$sock" ]]; then inside "$sock" && verdict=private
elif [[ -n "${TMUX:-}" ]]; then inside "${TMUX%%,*}" && verdict=private
elif [[ -n "${TMUX_TMPDIR:-}" && -d "$TMUX_TMPDIR" ]] && inside "$TMUX_TMPDIR"; then verdict=private
elif (( version_only == 1 && n == 1 )); then verdict=version
fi
printf '%s\t%s\t%s\n' "$(date +%s.%N)" "$verdict" "${1:-}" >> "$ROOT/log/tmux-calls.log"
if [[ $verdict == refused ]]; then
  echo "Teams fixture: refused a tmux call that names no server inside the private root" >&2
  exit 1
fi
exec "$REAL" "$@"
'''

REFUSE = r'''#!/bin/bash
# Teams fixture: no other computer, no new unit, no hgs from PATH.
printf '%s\t%s\n' "$(date +%s.%N)" "$(basename -- "$0")" >> @ROOT@/log/refused-calls.log
echo "Teams fixture: $(basename -- "$0") is refused in this private root" >&2
exit 97
'''

# The hosted Kimi workflow only: `hgs account inspect` signs a Kimi account in only
# through api.kimi.com. With an expiring token hgs runs `node` with its bundled module
# src/accounts/kimi-read.mjs, the producer of this value; this stand-in answers that
# one call. Its value is built by kimi_inspection() from the producer's own starting
# object, changing named keys only to what the producer writes for a signed-in profile
# with no usage windows; a guard test compares both with kimi-read.mjs. Account choice
# for Kimi is therefore not evidence here; everything after it is.
KIMI_PRODUCER_BASE = {'identity': {}, 'windows': [], 'source': 'Kimi Code', 'status': 'unavailable'}


def kimi_inspection():
    value = json.loads(json.dumps(KIMI_PRODUCER_BASE))
    value['identity'] = {'name': 'Fixture', 'email': '', 'plan': '', 'auth_method': 'Kimi Code'}
    return value


KIMI_NODE = r'''#!/bin/bash
printf '%s\tnode %s\n' "$(date +%s.%N)" "${1:-}" >> @ROOT@/log/kimi-inspection-stand-in.log
if [[ "${1:-}" == "--input-type=module" && "${2:-}" == "-e" && -n "${KIMI_CODE_HOME:-}" ]]; then
  printf '%s\n' @VALUE@
  exit 0
fi
exit 97
'''


def now():
    return time.monotonic()


class Evidence:
    """Bounded evidence: wait timings, requests, census rows. Written as files."""

    def __init__(self, folder):
        self.folder = Path(folder)
        self.folder.mkdir(parents=True, exist_ok=True)
        self.lock = threading.Lock()

    def append(self, name, row):
        with self.lock, (self.folder / name).open('a', encoding='utf-8') as f:
            f.write(json.dumps(row, ensure_ascii=False, default=str) + '\n')

    def write(self, name, value):
        (self.folder / name).write_text(value if isinstance(value, str) else json.dumps(value, indent=2, default=str),
                                        encoding='utf-8')


class Model(api.Handler):
    """The shared loopback fixture plus the Teams steps: a delivered task prompt and a
    typed `ZT-RUN <step>` each answer with one shell call that runs a fixture script."""

    def do_POST(self):
        route = api.urlsplit(self.path).path.rstrip('/')
        try:
            body = json.loads(self.rfile.read(int(self.headers.get('Content-Length') or 0)) or b'{}')
        except ValueError:
            body = {}
        body = body if isinstance(body, dict) else {}
        number = next(self.server.numbers)
        answer = self.server.plan(body)
        self.server.log_request(route, number, body, answer)
        try:
            if route.endswith('/count_tokens'):
                return self.reply(200, {'input_tokens': 100})
            if route.endswith('/messages'):
                message = api.anthropic_message(body, answer, number)
                return self.stream(api.anthropic_events(message)) if body.get('stream') else self.reply(200, message)
            if route.endswith('/responses'):
                return self.stream(api.responses_events(body, answer, number))
            if route.endswith('/chat/completions'):
                return self.chat(answer, number)
            if '/kimi-platform' in route:
                return self.reply(404, {'error': 'no platform in the fixture'})
            self.reply(404, {'error': {'type': 'not_found_error', 'message': 'no such route'}})
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass

    def do_GET(self):
        route = api.urlsplit(self.path).path.rstrip('/')
        if '/kimi-platform' in route:
            return self.reply(404, {'error': 'no platform in the fixture'})
        if route.endswith('/models'):
            return self.reply(200, {'object': 'list', 'data': [{'id': 'fixture', 'object': 'model'}], 'has_more': False})
        self.reply(404, {'error': {'type': 'not_found_error', 'message': 'no such route'}})

    def chat(self, answer, number):
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()

        def chunk(delta, finish=None):
            data = dict(id=f'chat_fixture_{number}', object='chat.completion.chunk', model='fixture',
                        choices=[dict(index=0, delta=delta, finish_reason=finish)])
            self.wfile.write(b'data: ' + json.dumps(data).encode() + b'\n\n')
        if answer[0] == 'tool':
            chunk(dict(role='assistant', content=None, tool_calls=[dict(index=0, id=f'call_fixture_{number}', type='function',
                  function=dict(name=answer[1], arguments=json.dumps(answer[2])))]))
            chunk({}, 'tool_calls')
        else:
            chunk(dict(role='assistant', content=answer[1]))
            chunk({}, 'stop')
        self.wfile.write(b'data: [DONE]\n\n')
        self.wfile.flush()


def chat_tools(body):
    return [t.get('function', {}).get('name') for t in body.get('tools') or [] if isinstance(t, dict)]


def chat_results(body):
    found = []
    for item in reversed(body.get('messages') or []):
        if isinstance(item, dict) and item.get('role') == 'tool':
            found.append(item.get('content'))
        elif isinstance(item, dict) and item.get('role') == 'user':
            break
    return found[::-1]


def user_texts(body):
    """Every typed user text of the request, oldest first."""
    return [text for item in api.items(body) for text in api.typed_parts(item)]


class ModelServer(api.Server):
    def __init__(self, root, scripts, evidence):
        super().__init__(root, '')
        self.RequestHandlerClass = Model
        self.scripts = Path(scripts)
        self.evidence = evidence
        self.requests = []

    def shell(self, offered, command):
        if 'Bash' in offered:
            return ('tool', 'Bash', {'command': command, 'description': 'Zerus Teams fixture step'})
        if 'exec_command' in offered or 'exec' in offered:
            # Codex's native shell subtool, called directly (tests/test_busy_input_native.py).
            return ('tool', 'exec_command', {'cmd': command, 'yield_time_ms': 30000, 'max_output_tokens': 4000})
        return ('text', 'Fixture: no shell tool offered: ' + ','.join(offered))

    def plan(self, body):
        offered = api.tool_names(body) or chat_tools(body)
        prompt = (api.typed(body) or '').strip()
        if api.results(body) or chat_results(body):
            return ('text', DONE)
        step = re.fullmatch(r'ZT-RUN ([a-z0-9-]{1,40})', prompt)
        if step and (self.scripts / (step.group(1) + '.sh')).is_file():
            return self.shell(offered, '/bin/bash ' + shlex.quote(str(self.scripts / (step.group(1) + '.sh'))))
        if prompt.startswith('[Zerus task ') and not prompt.split('\n', 1)[0].rstrip().endswith('cancelled]'):
            begin = re.search(r"`hgs task begin-attempt '?([^' `]+)'? --attempt ([0-9a-f-]{36})`", prompt)
            report = re.search(r"`hgs task report-attempt '?([^' `]+)'? --attempt ([0-9a-f-]{36})", prompt)
            if begin and report:
                script = self.scripts / 'delivered.sh'
                return self.shell(offered, ' '.join(shlex.quote(x) for x in (
                    '/bin/bash', str(script), begin.group(1), begin.group(2), report.group(1), report.group(2))))
            return ('text', 'Fixture: the delivered prompt has no pinned commands')
        return ('text', REPLY)

    def log_request(self, route, number, body, answer):
        row = dict(n=number, at=time.time(), route=route, prompt=api.typed(body), users=user_texts(body),
                   tools=(api.tool_names(body) or chat_tools(body))[:40],
                   results=[str(r)[:4000] for r in (api.results(body) or chat_results(body))], answer=list(answer[:2]))
        with self.lock:
            self.requests.append(row)
        self.evidence.append('model-requests.jsonl', row)


class Census(threading.Thread):
    """Samples the fixture's own processes (descendants of this test process and of the
    private tmux server) and their TCP/UDP peers once a second. A peer that is neither
    loopback nor an address an allow-listed host resolves to fails the class. Sampled:
    a connection opened and closed between two samples is not seen."""

    def __init__(self, evidence, hosts):
        super().__init__(daemon=True)
        self.evidence, self.hosts = evidence, tuple(hosts)
        self.roots = {os.getpid()}
        self.stop_event = threading.Event()
        self.allowed = {}  # ip -> host
        self.seen, self.violations, self.counts, self.samples = set(), [], {}, 0
        self.resolved_at = 0

    def resolve(self):
        if now() - self.resolved_at < 20:
            return
        self.resolved_at = now()
        for host in self.hosts:
            try:
                for info in socket.getaddrinfo(host, 443, proto=socket.IPPROTO_TCP):
                    self.allowed[info[4][0]] = host
            except OSError:
                pass

    @staticmethod
    def table():
        parents = {}
        for name in os.listdir('/proc'):
            if name.isdigit():
                try:
                    text = open(f'/proc/{name}/stat').read()
                    parents[int(name)] = int(text[text.rindex(')') + 2:].split()[1])
                except (OSError, ValueError):
                    pass
        return parents

    def owned(self):
        parents = self.table()
        children = {}
        for pid, parent in parents.items():
            children.setdefault(parent, []).append(pid)
        out, todo = set(), [p for p in self.roots if p in parents]
        while todo:
            pid = todo.pop()
            if pid not in out:
                out.add(pid)
                todo += children.get(pid, [])
        return out

    @staticmethod
    def peers(pid):
        table = {}
        for proto in ('tcp', 'tcp6', 'udp', 'udp6'):
            try:
                lines = open(f'/proc/{pid}/net/{proto}').read().splitlines()[1:]
            except OSError:
                continue
            for line in lines:
                f = line.split()
                host, port = f[2].split(':')
                if int(port, 16) == 0:
                    continue
                raw = bytes.fromhex(host)
                if len(raw) == 16:
                    raw = b''.join(raw[i:i + 4][::-1] for i in range(0, 16, 4))
                    ip = socket.inet_ntop(socket.AF_INET6, raw)
                    if ip.startswith('::ffff:'):
                        ip = ip[7:]
                else:
                    ip = socket.inet_ntop(socket.AF_INET, raw[::-1])
                table[int(f[9])] = (proto, ip, int(port, 16))
        return table

    def sample(self):
        self.samples += 1
        self.resolve()
        table = None
        for pid in self.owned():
            try:
                inodes = {int(os.readlink(f'/proc/{pid}/fd/{fd}')[8:-1]) for fd in os.listdir(f'/proc/{pid}/fd')
                          if os.readlink(f'/proc/{pid}/fd/{fd}').startswith('socket:[')}
            except OSError:
                continue
            if not inodes:
                continue
            if table is None:
                table = self.peers(pid)
            for inode in inodes & table.keys():
                if (pid, inode) in self.seen:
                    continue
                self.seen.add((pid, inode))
                proto, ip, port = table[inode]
                loopback = ip.startswith('127.') or ip == '::1'
                host = 'loopback' if loopback else self.allowed.get(ip)
                try:
                    comm = open(f'/proc/{pid}/comm').read().strip()
                except OSError:
                    comm = '?'
                row = dict(at=time.time(), pid=pid, comm=comm, proto=proto, peer=f'{ip}:{port}', host=host)
                self.evidence.append('census.jsonl', row)
                self.counts[host or 'not allowed'] = self.counts.get(host or 'not allowed', 0) + 1
                if host is None:
                    self.violations.append(row)

    def run(self):
        while not self.stop_event.wait(1.0):
            try:
                self.sample()
            except Exception as error:  # A census failure is evidence too, never silent.
                self.violations.append(dict(error=str(error)[:300]))

    def finish(self):
        self.stop_event.set()
        self.join(10)
        self.sample()
        summary = dict(samples=self.samples, allow_list=list(self.hosts), counts=self.counts,
                       violations=self.violations[:50])
        self.evidence.write('census-summary.json', summary)
        return summary


class TeamFixture:
    """One private root, one private service, one team (lead and member m1) of one agent."""

    def __init__(self, agent, native, case):
        self.agent, self.native, self.case = agent, Path(native).resolve(), case
        base = os.environ.get('TMPDIR') or tempfile.gettempdir()
        self.root = Path(tempfile.mkdtemp(prefix='hgs-teams-native-', dir=base)).resolve()
        folder = os.environ.get('ZERUS_TEAMS_EVIDENCE') or str(self.root / 'evidence')
        self.evidence = Evidence(Path(folder) / case)
        self.waits = []
        self.processes = []
        self.service = None
        self.team = None
        self.sessions = {}
        self.census = None
        self.model = None

    # ---- environment -----------------------------------------------------------
    def build(self):
        r = self.root
        for sub in ('bin', 'log', 'home/.local/bin', 'home/.config', 'home/.local/share', 'home/.cache',
                    'home/.local/state', 'run', 'tmux', 'tmp', 'state', 'cfg', 'scripts', 'out', 'model', 'work'):
            (r / sub).mkdir(parents=True, exist_ok=True)
        os.chmod(r / 'run', 0o700)
        os.chmod(r / 'tmux', 0o700)
        for name, text in (('tmux', TMUX_WRAPPER), ('ssh', REFUSE), ('scp', REFUSE), ('systemd-run', REFUSE), ('hgs', REFUSE)):
            path = r / 'bin' / name
            path.write_text(text.replace('@ROOT@', shlex.quote(str(r))).replace('@REAL@', shlex.quote(REAL_TMUX)))
            path.chmod(0o755)
        if self.agent == 'kimi':
            path = r / 'bin/node'
            path.write_text(KIMI_NODE.replace('@ROOT@', shlex.quote(str(r))).replace('@VALUE@', shlex.quote(json.dumps(kimi_inspection()))))
            path.chmod(0o755)
        for log in ('tmux-calls.log', 'refused-calls.log'):
            (r / 'log' / log).touch()
        (r / 'home/.local/bin' / self.agent).symlink_to(self.native)
        home = r / 'home'
        self.env = {
            'PATH': f'{r}/bin:{home}/.local/bin:{SYSTEM_PATH}', 'HOME': str(home), 'USER': os.environ.get('USER', 'fixture'),
            'LOGNAME': os.environ.get('LOGNAME', 'fixture'), 'SHELL': '/bin/bash', 'LANG': 'C.UTF-8', 'TERM': 'xterm-256color',
            'XDG_CONFIG_HOME': f'{home}/.config', 'XDG_DATA_HOME': f'{home}/.local/share', 'XDG_CACHE_HOME': f'{home}/.cache',
            'XDG_STATE_HOME': f'{home}/.local/state', 'XDG_RUNTIME_DIR': f'{r}/run', 'HGS_STATE_DIR': f'{r}/state',
            'HGS_CONFIG_DIR': f'{r}/cfg', 'HGS_PEERS': '', 'TMUX_TMPDIR': f'{r}/tmux', 'TMPDIR': f'{r}/tmp',
            'DBUS_SESSION_BUS_ADDRESS': f'unix:path={r}/run/no-bus', 'DISABLE_AUTOUPDATER': '1',
            'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1', 'KIMI_CODE_NO_AUTO_UPDATE': '1', 'KIMI_DISABLE_TELEMETRY': '1',
            'GIT_CONFIG_NOSYSTEM': '1', 'PYTHONDONTWRITEBYTECODE': '1', 'HGS_TEST_BIN': HGS,
        }

    def preflight(self):
        r, env, failures = str(self.root), self.env, []
        def inside(p):
            return os.path.realpath(p).startswith(r + '/')
        for key in ('HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME', 'XDG_RUNTIME_DIR',
                    'HGS_STATE_DIR', 'HGS_CONFIG_DIR', 'TMUX_TMPDIR', 'TMPDIR'):
            if not inside(env[key]):
                failures.append(key)
        for key in ('TMUX', 'TMUX_PANE', 'DISPLAY', 'WAYLAND_DISPLAY', 'ZERUS_CLIENT_DESKTOP_SERVICE',
                    'HGS_SESSION', 'HGS_RUN_ID', 'HGS_SESSION_ID'):
            if key in env:
                failures.append('present ' + key)
        for key in ('TMUX_TMPDIR', 'XDG_RUNTIME_DIR'):
            if stat.S_IMODE(os.stat(env[key]).st_mode) != 0o700:
                failures.append('mode ' + key)
        if os.path.exists(os.path.join(env['XDG_RUNTIME_DIR'], 'systemd/private')):
            failures.append('user manager in the runtime folder')
        for name in ('tmux', 'ssh', 'scp', 'systemd-run', 'hgs'):
            if shutil.which(name, path=env['PATH']) != f'{r}/bin/{name}':
                failures.append('resolves ' + name)
        sock = Path(env['HGS_STATE_DIR']) / 'desktop/service.sock'
        if len(str(sock).encode()) >= 104:
            failures.append('service socket path length')
        if not (os.path.isabs(HGS) and os.access(HGS, os.X_OK)):
            failures.append('HGS_TEST_BIN is not an absolute executable')
        self.evidence.write('preflight.json', dict(root=r, failures=failures))
        if failures:
            raise AssertionError('Teams fixture preflight failed: ' + ', '.join(failures))

    # ---- commands --------------------------------------------------------------
    def hgs(self, *argv, stdin=None, timeout=60, ok=True):
        run = subprocess.run([HGS, *argv], input=stdin, env=self.env, capture_output=True, text=True, timeout=timeout)
        self.evidence.append('hgs-calls.jsonl', dict(at=time.time(), argv=list(argv), exit=run.returncode,
                                                     stdout=run.stdout[:8000], stderr=run.stderr[:2000]))
        if ok and run.returncode != 0:
            raise AssertionError(f'hgs {" ".join(argv[:3])} exited {run.returncode}: {run.stdout[-600:]} {run.stderr[-600:]}')
        return run

    def json(self, *argv, **kw):
        return json.loads(self.hgs(*argv, **kw).stdout)

    def wait(self, label, predicate, bound, step=1.0):
        start = now()
        last = None
        while True:
            last = predicate()
            if last:
                break
            if now() - start > bound:
                break
            time.sleep(step)
        row = dict(wait=label, case=self.case, seconds=round(now() - start, 2), bound=bound, ok=bool(last))
        self.waits.append(row)
        self.evidence.append('waits.jsonl', row)
        print('ZERUS_TEAMS_WAIT=' + json.dumps(row), flush=True)
        if not last:
            raise AssertionError(f'{label}: not reached within {bound} s')
        return last

    def script(self, name, body):
        path = self.root / 'scripts' / (name + '.sh')
        path.write_text('#!/bin/bash\nset -u\nH="$HGS_EXECUTABLE"\n' + body)
        return path

    # ---- bring-up --------------------------------------------------------------
    def start_model(self):
        self.model = ModelServer(self.root / 'model', self.root / 'scripts', self.evidence)
        threading.Thread(target=self.model.serve_forever, daemon=True).start()
        self.base = f'http://127.0.0.1:{self.model.server_address[1]}'

    def project(self):
        proj = self.root / 'work/proj'
        proj.mkdir(parents=True)
        git = ['git', '-C', str(proj), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid']
        subprocess.run(git[:3] + ['init', '-q', '-b', 'main'], env=self.env, check=True)
        (proj / 'README.md').write_text('Teams fixture project.\n')
        subprocess.run(git + ['add', 'README.md'], env=self.env, check=True)
        subprocess.run(git + ['commit', '-q', '-m', 'Fixture start'], env=self.env, check=True)
        self.commit = subprocess.check_output(git[:3] + ['rev-parse', 'HEAD'], env=self.env, text=True).strip()
        self.machine = self.json('swarm', 'hello')['node_id']
        self.hgs('swarm', 'initialize', stdin=json.dumps({'projects': [dict(
            id='teams-fixture', name='Teams fixture', color='#123456', sessions=[],
            folders=[dict(id='teams-fixture-folder', machine=self.machine, path=str(proj), name='')])]}))
        self.proj = proj

    def claude_home(self, home):
        home.mkdir(parents=True, exist_ok=True)
        (home / '.claude.json').write_text(json.dumps(dict(
            hasCompletedOnboarding=True, hasSeenAutoDefaultNudge=True, theme='dark', bypassPermissionsModeAccepted=True,
            autoUpdates=False, customApiKeyResponses=dict(approved=[FIXTURE_KEY], rejected=[]))))
        (home / 'settings.json').write_text(json.dumps({'env': {
            'ANTHROPIC_BASE_URL': self.base, 'ANTHROPIC_API_KEY': FIXTURE_KEY, 'DISABLE_AUTOUPDATER': '1',
            'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1'}}))

    def codex_common(self):
        return ('check_for_update_on_startup=false\ncli_auth_credentials_store="file"\n'
                f'chatgpt_base_url="{self.base}/backend-api/"\n[features]\nplugins=false\n')

    def accounts(self):
        self.account = 'zt-' + self.agent
        self.hgs('account', 'add', self.account, '--provider', self.agent, '--label', 'Teams fixture')
        home = Path(self.env['HGS_CONFIG_DIR']) / 'accounts' / self.account
        home.mkdir(parents=True, exist_ok=True)
        own = self.root / 'home'
        if self.agent == 'claude':
            self.claude_home(home)
            # The always-listed native account (HOME/.claude) is inspected too.
            self.claude_home(own / '.claude')
            (own / '.claude.json').write_text((own / '.claude/.claude.json').read_text())
        elif self.agent == 'codex':
            (home / 'config.toml').write_text(
                'model="gpt-6.1-sol"\nmodel_provider="fixture"\n' + self.codex_common().replace('[features]', '[features]', 1) +
                '[model_providers.fixture]\nname="Fixture"\n'
                f'base_url="{self.base}/v1"\nwire_api="responses"\nrequires_openai_auth=true\n'
                'request_max_retries=0\nstream_max_retries=0\n')
            native_home = own / '.codex'
            native_home.mkdir(parents=True, exist_ok=True)
            (native_home / 'config.toml').write_text(self.codex_common())
            # A key login the way a person makes one: Codex's own command.
            login = subprocess.run([str(self.native), 'login', '--with-api-key'], input=FIXTURE_KEY + '\n',
                                   env=dict(self.env, CODEX_HOME=str(home)), capture_output=True, text=True, timeout=60)
            if login.returncode != 0:
                raise AssertionError('codex login --with-api-key failed: ' + login.stderr[-500:])
        else:
            for kimi_home in (home, own / '.kimi-code'):
                kimi_home.mkdir(parents=True, exist_ok=True)
                (kimi_home / 'config.toml').write_text(
                    'default_model = "fixture"\ntelemetry = false\n'
                    f'[providers.fixture]\ntype = "openai"\nbase_url = "{self.base}/v1"\napi_key = "{FIXTURE_KEY}"\n'
                    '[models.fixture]\nprovider = "fixture"\nmodel = "fixture"\nmax_context_size = 32768\n'
                    '[loop_control]\nmax_attempts_per_step = 1\n')
                (kimi_home / 'tui.toml').write_text('[upgrade]\nauto_install = false\n')
            (home / 'credentials').mkdir(exist_ok=True)
            (home / 'credentials/kimi-code.json').write_text(json.dumps(dict(access_token='fixture-token', expires_at=1)))
            self.env['KIMI_CODE_BASE_URL'] = self.base + '/kimi-platform'
        self.account_home = home
        # Accounts a launch may choose: those whose home holds the fixture's key.
        self.fixture_accounts = {self.account} | ({'native-claude'} if self.agent == 'claude' else set())
        self.evidence.write('account-inspect.json', self.json('account', 'inspect', self.account, timeout=90))

    def bundle(self):
        role = dict(id='zt-dev', name='Fixture member', agent=self.agent, prompt='Fixture member; run the fixture steps.')
        if self.agent == 'claude':
            role.update(model='claude-sonnet-4-6', effort='high')
        elif self.agent == 'codex':
            role.update(model='gpt-6.1-sol', effort='high')
        return {'template': {'id': 'zt-' + self.agent, 'name': 'Teams fixture ' + self.agent, 'description': 'One member.',
                             'lead': {'agent': self.agent}, 'roster': [{'role': 'zt-dev', 'count': 1}],
                             'default_pipeline': 'zt-direct', 'settings': {}},
                'roles': [role], 'pipelines': [{'id': 'zt-direct', 'name': 'Direct', 'implementer': 'zt-dev'}]}

    def create_team(self):
        preview = self.json('team', 'preview', 'on', '--json', ok=False)
        if preview.get('ok') is not True:
            raise AssertionError('The Teams preview was refused; run the suite outside any agent session '
                                 '(for example in a systemd-run --user unit): ' + json.dumps(preview)[:800])
        self.json('team', 'template', 'import', '--json', stdin=json.dumps(self.bundle()))
        self.team = str(uuid.uuid4())
        reply = self.json('team', 'create', '--template', 'zt-' + self.agent, '--json', stdin=json.dumps({
            'request_id': str(uuid.uuid4()), 'team': self.team, 'expected_revision': 0, 'project': 'teams-fixture',
            'name': 'Teams fixture', 'goal': 'Real-agent contract.', 'home': self.machine,
            'integration_branch': 'zerus/team/fixture-' + self.agent, 'base_commit': self.commit,
            # Standing confirmations stay off in this round (zerus-63rru.48 and .49).
            'delegations': {'accept_results': False, 'answer_members': False, 'confirm_trust': False,
                            'merge_accepted': False, 'resume_exited': False}}))
        if reply.get('ok') is not True:
            raise AssertionError('team create: ' + json.dumps(reply)[:800])

    def start_service(self):
        log = (self.evidence.folder / 'desktop-serve.log').open('w')
        self.service = subprocess.Popen([HGS, 'desktop', 'serve', '--background'], env=self.env,
                                        stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        self.wait('desktop service running', lambda: self.hgs('desktop', 'status', '--json', ok=False).returncode == 0
                  and json.loads(self.hgs('desktop', 'status', '--json', ok=False).stdout).get('running'), BOUND['service'])

    def status(self):
        return self.json('team', 'status', '--team', self.team, '--json')

    def members(self):
        return {m['id']: m for m in self.status().get('members') or []}

    def rows(self):
        found = []
        def walk(value):
            if isinstance(value, dict):
                if 'run_id' in value and 'name' in value:
                    found.append(value)
                for child in value.values():
                    walk(child)
            elif isinstance(value, list):
                for child in value:
                    walk(child)
        walk(self.json('ls', '--json'))
        return found

    def session_of(self, member):
        key = (member.get('session') or {}).get('session_id') or ''
        launch = key.split(':', 1)[1] if key.startswith('launch_id:') else ''
        for row in self.rows():
            if launch and (row.get('launch_id') == launch or str(row.get('name', '')).endswith('/team-' + launch)):
                return dict(name=row['name'], launch=launch)
        return None

    def bring_up(self):
        self.build()
        self.preflight()
        self.census = Census(self.evidence, ALLOWED_HOSTS[self.agent])
        self.census.start()
        self.start_model()
        self.project()
        self.accounts()
        self.create_team()
        self.start_service()
        bound = self.wait('lead and member bound by launch ID', lambda: (lambda m: m if len(m) == 2 and all(
            (x.get('session') or {}).get('session_id') for x in m.values()) else None)(self.members()), BOUND['bound'], 3)
        for member_id, member in bound.items():
            self.sessions[member_id] = self.wait(f'{member_id} session listed', lambda m=member: self.session_of(m),
                                                 BOUND['listed'], 2)
            self.sessions[member_id]['member'] = member
        self.census.roots.add(self.tmux_server())
        for member_id in bound:
            self.answer_startup(member_id)

    def tmux_server(self):
        out = subprocess.run(['tmux', 'display-message', '-p', '#{pid}'], env=self.env, capture_output=True, text=True)
        return int(out.stdout.strip()) if out.stdout.strip().isdigit() else os.getpid()

    def inspect(self, member_id):
        return self.json('inspect', self.sessions[member_id]['name'], timeout=60)

    def answer_startup(self, member_id):
        """The person answers each startup card of this member until it is ready. Claude
        and Kimi folder trust and Codex's hooks review go through `hgs answer`; Codex's
        Folder access is only answerable in Terminal (zerus-63rru.49), where the person
        presses Enter on its highlighted first choice."""
        name = self.sessions[member_id]['name']
        answered = []

        def step():
            view = self.inspect(member_id)
            pending = view.get('pending_questions') or []
            live = view.get('runtime_state') == 'live' and view.get('process_state') == 'running'
            started = view.get('conversation_id') and view.get('activity') == 'idle'
            if not pending and live and (started or view.get('first_message_can_send')):
                return view
            for card in pending:
                if card.get('question_id') in answered:
                    continue
                items = card.get('questions') or []
                options = items[0].get('options') if len(items) == 1 else []
                if card.get('source') == 'startup_terminal_choice' and options and options[0].get('label') == 'Trust and continue':
                    subprocess.run(['tmux', 'send-keys', '-t', '=' + name + ':', 'Enter'], env=self.env, check=True)
                    answered.append(card.get('question_id'))
                    self.evidence.append('person-answers.jsonl', dict(member=member_id, source=card.get('source'),
                                                                       terminal=True, label=options[0]['label']))
                    continue
                accept = [o for o in options or [] if re.match(r'(yes|trust)\b', o.get('label', ''), re.I)]
                if card.get('can_answer') and len(accept) == 1:
                    payload = dict(request_id=str(uuid.uuid4()), expected_run_id=view.get('run_id'),
                                   expected_conversation_id=view.get('conversation_id') or '',
                                   question_id=card['question_id'], expected_question_hash=card['question_hash'],
                                   answers=[dict(question_id=items[0]['id'], selected_option_ids=[accept[0]['id']], text='')])
                    reply = self.hgs('answer', name, '--json', stdin=json.dumps(payload), ok=False)
                    answered.append(card.get('question_id'))
                    self.evidence.append('person-answers.jsonl', dict(member=member_id, source=card.get('source'),
                                                                       label=accept[0]['label'], exit=reply.returncode))
            return None
        view = self.wait(f'{member_id} ready after the person answered its startup cards', step, BOUND['card'] + BOUND['ready'], 2)
        self.sessions[member_id].update(run=view.get('run_id'), conversation=view.get('conversation_id') or '')
        return view

    # ---- person actions --------------------------------------------------------
    def send(self, member_id, text):
        view = self.inspect(member_id)
        request = dict(request_id=str(uuid.uuid4()), expected_run_id=view.get('run_id'),
                       expected_conversation_id=view.get('conversation_id') or '', text=text)
        reply = self.json('send', self.sessions[member_id]['name'], '--json', stdin=json.dumps(request))
        if reply.get('status') != 'submitted':
            raise AssertionError('send: ' + json.dumps(reply)[:600])
        return reply

    def add_task(self, title, text):
        reply = self.json('task', 'add', '--project', 'teams-fixture', '--team', self.team, '--title', title, '--json',
                          stdin=text)
        task = reply.get('task') or reply
        return task['ref'], task['uuid']

    def node_of(self, uuid_):
        rows = self.json('team', 'queue', '--team', self.team, '--limit', '200', '--json').get('rows') or []
        return next((r for r in rows if (r.get('task') or {}).get('uuid') == uuid_), None)

    def requests_with(self, predicate):
        with self.model.lock:
            return [r for r in self.model.requests if predicate(r)]

    def process(self, member_id):
        """The native agent's PID and kernel start time from its session record."""
        record = self.record(member_id)
        pid = record.get('pid')
        return pid, self.start_ticks(pid)

    @staticmethod
    def start_ticks(pid):
        try:
            text = open(f'/proc/{pid}/stat').read()
            return int(text[text.rindex(')') + 2:].split()[19])
        except (OSError, ValueError, TypeError):
            return None

    def record(self, member_id):
        run = self.inspect(member_id).get('run_id')
        for path in Path(self.env['HGS_STATE_DIR']).rglob('*.json'):
            try:
                data = json.loads(path.read_text())
            except (OSError, ValueError):
                continue
            if isinstance(data, dict) and data.get('run_id') == run and data.get('pid'):
                return data
        raise AssertionError(f'no session record for {member_id} run {run}')

    # ---- teardown --------------------------------------------------------------
    def owned_left(self):
        parents = Census.table()
        left = []
        root = str(self.root) + '/'
        for pid in parents:
            if pid == os.getpid():
                continue
            hit = False
            for key in ('cwd', 'exe'):
                try:
                    hit = hit or (os.readlink(f'/proc/{pid}/{key}') + '/').startswith(root)
                except OSError:
                    pass
            try:
                hit = hit or root.encode() in open(f'/proc/{pid}/environ', 'rb').read()
            except OSError:
                pass
            if hit:
                left.append(pid)
        return left

    def diagnose(self):
        """Bounded reads that explain a failed step: owner cards, waiting rows, members."""
        if not self.team:
            return
        for name, argv in (('cards', ('team', 'cards', '--team', self.team, '--limit', '50', '--json')),
                           ('waiting', ('team', 'waiting', '--limit', '50', '--json')),
                           ('status', ('team', 'status', '--team', self.team, '--json')),
                           ('journal', ('team', 'journal', '--team', self.team, '--limit', '100', '--json'))):
            try:
                self.evidence.write(f'diagnose-{name}.json', self.hgs(*argv, ok=False, timeout=30).stdout)
            except Exception as error:
                self.evidence.write(f'diagnose-{name}.json', str(error)[:500])

    def fleet_rows(self):
        """Read-only: the private service's fleet rows of this team's runs, the fields the
        teams actor reads to decide that a member is free (actor/availability.rs)."""
        import sqlite3
        path = Path(self.env['HGS_STATE_DIR']) / 'desktop/state.sqlite3'
        if not path.is_file():
            return {}
        out = {}
        db = sqlite3.connect(f'file:{path}?mode=ro', uri=True, timeout=1)
        try:
            for member_id, session in self.sessions.items():
                run = session.get('run')
                if not run:
                    continue
                rows = db.execute('SELECT generation, json FROM desktop_catalog_rows WHERE key=? ORDER BY generation DESC LIMIT 2',
                                  ('run:' + run,)).fetchall()
                out[member_id] = []
                for generation, text in rows:
                    row = json.loads(text)
                    out[member_id].append(dict(generation=generation, keys=sorted(row), **{k: row.get(k) for k in (
                        'state', 'activity', 'phase', 'process_work', 'subagents_active', 'optional_questions',
                        'launch_id', 'run_id', 'tracked', 'process_state', 'recovery', 'subagent_active_count')}))
        finally:
            db.close()
        return out

    def teardown(self):
        problems = []
        self.diagnose()
        try:
            self.evidence.write('diagnose-fleet-rows.json', self.fleet_rows())
        except Exception as error:
            self.evidence.write('diagnose-fleet-rows.json', str(error)[:500])
        for name in ('teams/member-launch-v1.sqlite3',):
            try:
                import sqlite3
                path = Path(self.env['HGS_STATE_DIR']) / name
                if path.is_file():
                    db = sqlite3.connect(f'file:{path}?mode=ro', uri=True, timeout=1)
                    tables = {}
                    for (table,) in db.execute("SELECT name FROM sqlite_master WHERE type='table'"):
                        cursor = db.execute(f'SELECT * FROM "{table}" LIMIT 40')
                        cols = [c[0] for c in cursor.description]
                        tables[table] = [dict(zip(cols, [v if not isinstance(v, bytes) else v.hex()[:120] for v in r])) for r in cursor]
                    db.close()
                    self.evidence.write('diagnose-' + name.replace('/', '-') + '.json', tables)
            except Exception as error:
                self.evidence.write('diagnose-ledger-error.txt', str(error)[:500])
        for member_id, session in list(self.sessions.items()):
            try:
                view = self.inspect(member_id)
                if view.get('runtime_state') == 'live':
                    self.hgs('terminate', session['name'], '--expected-run-id', view.get('run_id'), ok=False, timeout=90)
            except Exception as error:
                problems.append(f'terminate {member_id}: {error}')
        if self.service is not None and self.service.poll() is None:
            self.service.send_signal(signal.SIGTERM)
            try:
                code = self.service.wait(timeout=30)
                if code != 0:
                    problems.append(f'desktop service exited {code}')
            except subprocess.TimeoutExpired:
                self.service.kill()
                problems.append('desktop service did not stop within 30 s')
        subprocess.run(['tmux', 'kill-server'], env=self.env, capture_output=True)
        if self.model is not None:
            self.model.shutdown()
            self.model.server_close()
        try:
            left = self.wait('no owned process left', lambda: not self.owned_left() or None, BOUND['teardown'], 1)
        except AssertionError:
            left = None
            problems.append('owned processes left: ' + ','.join(map(str, self.owned_left()[:20])))
        census = self.census.finish() if self.census else {}
        for name in ('tmux-calls.log', 'refused-calls.log'):
            shutil.copy(self.root / 'log' / name, self.evidence.folder / name)
        calls = [l.split('\t') for l in (self.root / 'log/tmux-calls.log').read_text().splitlines()]
        refused = (self.root / 'log/refused-calls.log').read_text().splitlines()
        self.evidence.write('isolation.json', dict(tmux_calls=len(calls), tmux_refused=sum(c[1] == 'refused' for c in calls if len(c) > 1),
                                                    shim_calls=len(refused), census=census, problems=problems))
        if any(len(c) > 1 and c[1] == 'refused' for c in calls):
            problems.append('a tmux call named no private server')
        if refused:
            problems.append(f'{len(refused)} refused ssh/scp/systemd-run/hgs calls')
        if census.get('violations'):
            problems.append('non-allow-listed network peers: ' + json.dumps(census['violations'][:5]))
        if left is not None and not problems:
            shutil.rmtree(self.root, ignore_errors=True)
        return problems
