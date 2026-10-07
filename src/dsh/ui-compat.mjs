// Compatibility with already-running 0.17 hosts. Use the released web API and
// its normal launch-token cookie exchange; credentials arrive only on stdin.
// No messages, model calls, or settings changes are made here.
let input = '';
for await (const chunk of process.stdin) input += chunk;
try {
  const { url, sessionId, cwd } = JSON.parse(input);
  const origin = new URL(url).origin;
  const auth = await fetch(url, { redirect: 'manual', signal: AbortSignal.timeout(8000) });
  if (auth.status !== 303) throw new Error('auth');
  const cookie = auth.headers.getSetCookie().map(value => value.split(';')[0]).join('; ');
  if (!cookie) throw new Error('cookie');
  async function call(method, request) {
    const response = await fetch(origin + '/api/' + method, { method: 'POST', redirect: 'error',
      headers: { Cookie: cookie, Origin: origin, 'Content-Type': 'application/json' },
      body: JSON.stringify({ type: 'client-request', rpcId: 'hgs-native-workspace', method,
        payload: { args: method === 'session/list' ? { _request: request } : { request } } }),
      signal: AbortSignal.timeout(8000) });
    if (!response.ok) throw new Error('transport');
    const result = (await response.json()).result;
    if (!result?.ok) throw new Error('registration');
    return result.value;
  }
  const list = await call('session/list', {});
  const session = list.items.find(item => item.sessionId === sessionId);
  // Do not resume a paused legacy agent just to show the web workspace.
  // New adapters attach stored sessions directly without starting an agent.
  if (session?.agentAvailable) {
    const { workspace } = await call('workspace/create', { path: cwd });
    // Existing membership is authoritative. Re-adoption would unnecessarily
    // compare literal cwd strings (e.g. /tmp vs /private/tmp on macOS).
    if (!workspace.sessionIds.includes(sessionId))
      await call('session/create', { workspaceId: workspace.workspaceId, sessionId });
  }
} catch {
  process.stderr.write('Could not register the session in the native DeepSeek workspace.');
  process.exitCode = 1;
}
