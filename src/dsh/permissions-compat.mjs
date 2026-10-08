// Add account-mode support to a resident legacy bridge using the released web
// API. Keep its owned agents, questions, socket and host generation intact.
// account-client.mjs supplies request and the normal launch-token exchange.
try {
  const { url, sessionId, operation, pending } = request;
  if (!/^session-[0-9a-f-]{36}$/.test(sessionId)) throw new Error('identity');
  const call = await accountClient(url);
  const catalog = await call('permissionPresets/catalog');
  if (!catalog.options?.some(option => option.value === 'danger-full-access')) throw new Error('preset');
  const list = await call('session/list', { _request: {} });
  const session = list.items.find(item => item.sessionId === sessionId);
  if (operation === 'check') {
    process.stdout.write(JSON.stringify({ ok: true, needs_setup: !session?.agentAvailable || pending === true }));
  } else if (operation === 'apply') {
    if (!session?.agentAvailable || session.running) throw new Error('state');
    const current = await call('session/projections', { request: { sessionId } });
    // Read the durable result first: an earlier uncertain acknowledgement must
    // not repeat a permission command that already succeeded.
    if (current?.values.permissions?.currentValue !== 'danger-full-access') {
      const commands = await call('commands/list', { agentId: sessionId });
      if (!commands.some(command => command.name === 'permission'
          && command.definitionId === '@deepseek-ai/dsh-permission-presets')) throw new Error('command');
      const result = await call('commands/execute', {
        agentId: sessionId, line: '/permission danger-full-access', submittedAttachments: [],
      });
      if (result?.result.kind !== 'success') throw new Error('result');
    }
    const confirmed = await call('session/projections', { request: { sessionId } });
    if (confirmed?.values.permissions?.currentValue !== 'danger-full-access') throw new Error('confirmation');
    process.stdout.write(JSON.stringify({ ok: true }));
  } else throw new Error('operation');
} catch {
  // Never print launch tokens, cookies, native RPC payloads or credential data.
  process.stdout.write(JSON.stringify({ ok: false }));
  process.exitCode = 1;
}
