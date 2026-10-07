let result;
try {
  const call = await accountClient(request.url);
  const ref = await apiKeyReference(call);
  const before = (await call('credentials/describe', {refs: [ref]}))[ref];
  if (!before?.writable) {
    result = {ok: false, error: 'DeepSeek uses a read-only API key from its launch environment. Remove that override before saving a key here.'};
  } else {
    await call('credentials/set', {ref, value: request.api_key});
    request.api_key = '';
    const after = (await call('credentials/describe', {refs: [ref]}))[ref];
    if (!after?.configured) throw new Error('verify');
    result = {ok: true};
  }
} catch {
  result = {ok: false, error: 'Could not save the key through the native DeepSeek credentials service. Refresh the account and retry.'};
}
process.stdout.write(JSON.stringify(result));
