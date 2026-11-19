// Private test tool using the official resident subprocess service.
const rawService = Symbol.for('cordis.original');
const spawnOriginal = Symbol.for('hgs.dsh.spawn.original');
const installations = new WeakMap();
let nextInstallation = 0;

function installationNumber(service) {
  const spawn = service.spawn;
  if (!installations.has(spawn)) installations.set(spawn, ++nextInstallation);
  return installations.get(spawn);
}

function spec(cwd) {
  return { argv: ['/usr/bin/true'], cwd, graceMs: 1000,
    stdio: { stdin: 'ignore', stdout: 'ignore', stderr: 'ignore' } };
}

function spawnUnderProbe(ctx, cwd) {
  return ctx.get('subprocess').spawn(spec(cwd));
}

export async function probeSpawns(ctx, cwd, action) {
  const service = ctx.get('subprocess')[rawService];
  if (action === 'damage') {
    // Install a recognizable chain of stale HGS layers, then observe the
    // real failure before returning control to the native tool hook.
    let wrapped = Object.getPrototypeOf(service).spawn;
    for (let i = 0; i < 12000; i++) {
      const next = wrapped;
      wrapped = function staleSpawn(...args) { return next.apply(this, args); };
      Object.defineProperty(wrapped, spawnOriginal, { value: next });
    }
    service.spawn = wrapped;
    try {
      await spawnUnderProbe(ctx, cwd).done;
      return { error: 'The stale wrapper chain unexpectedly spawned a process' };
    } catch (error) {
      return { error: String(error) };
    }
  }

  const count = action === 'bulk' ? 500 : 1;
  const installation = installationNumber(service);
  const depths = [];
  const select = service.selectContainmentMode;
  const originalDescriptor = Object.getOwnPropertyDescriptor(service, 'selectContainmentMode');
  let completed = 0;
  for (let i = 0; i < count; i++) {
    const stackLimit = Error.stackTraceLimit;
    let handle;
    // Observe exactly where the native implementation admits each spawn.
    // Count only the frames up to this dedicated synchronous call site.
    service.selectContainmentMode = function (...args) {
      const depth = new Error().stack.split('\n').findIndex(frame => frame.includes('at spawnUnderProbe '));
      depths.push(depth);
      return select.apply(this, args);
    };
    try {
      Error.stackTraceLimit = Infinity;
      handle = spawnUnderProbe(ctx, cwd);
    } catch (error) {
      return { completed, error: String(error) };
    } finally {
      Error.stackTraceLimit = stackLimit;
      if (originalDescriptor) Object.defineProperty(service, 'selectContainmentMode', originalDescriptor);
      else delete service.selectContainmentMode;
    }
    const result = await handle.done;
    if (result.exitCode !== 0) return { completed, error: `Spawn exit ${result.exitCode}` };
    completed++;
  }
  return { completed, installation, minDepth: Math.min(...depths), maxDepth: Math.max(...depths) };
}
