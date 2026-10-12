import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync, fork, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { appendFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { builtinModules } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { build } from 'esbuild';
import { accountProxyWorker } from '../benchmark/account-proxy.mjs';

// Surface-state recovery journey (the "stuck tool card"): a tool call is in
// flight when the Session owner dies. Every API call is the curl executable
// against the normal ingress, Managed API and Egress workers on persisted
// workerd SQLite/R2. Owner loss is SIGKILL of the whole workerd process group
// while an external effect is held open; the provider and one effect endpoint
// are the only stubs. Before this protocol, the interrupted root call kept a
// tool.call without any terminal result, so clients rendered it as running.
const root = fileURLToPath(new URL('../../..', import.meta.url));
const owner = '11111111-1111-4111-8111-111111111177';
const settings = { model: 'gpt-6.1-sol', thinking: 'low', reasoning_mode: 'standard', fast_mode: false };
const flags = ['nodejs_compat', 'durable_object_io_tasks_prevent_eviction', 'enable_request_signal'];

const bootstrap = `import managed from './src/index.ts';export * from './src/index.ts';
import {ensureAccount,createApiKey} from './src/account-auth.ts';
export default {async fetch(request,env,ctx){const path=new URL(request.url).pathname;
if(path==='/__fixture/identity'){await ensureAccount(env,'${owner}',true);
 const auth=await(await env.NANOCODEX_USERS.getByName('${owner}').fetch('https://user.internal/authorization')).json();
 return Response.json(await createApiKey(env,{kind:'api_key',userId:'${owner}',...auth.grant,subjectId:'fixture:${owner}',credentialId:'fixture'},'synthetic-surface-recovery'));}
if(path==='/__fixture/chatgpt'){const expires_at=(Math.ceil(Date.now()/1000)+3600)*1000;
 const claims={exp:Math.ceil(expires_at/1000),'https://api.openai.com/auth':{chatgpt_account_id:'synthetic-account',chatgpt_account_is_fedramp:false}};
 const jwt=btoa(JSON.stringify({alg:'none'})).replaceAll('=','')+'.'+btoa(JSON.stringify(claims)).replaceAll('=','')+'.fixture';
 return env.NANOCODEX.fetch('https://broker.internal/users/${owner}/credentials/chatgpt',{method:'PUT',headers:{'content-type':'application/json'},
  body:JSON.stringify({access_token:jwt,refresh_token:'synthetic-refresh',account_id:'synthetic-account',expires_at,fedramp:false})});}
return managed.fetch(request,env,ctx);}};`;

const providerSource = `export default {async fetch(request, env) {
  const url = new URL(request.url);
  if (url.origin !== 'https://chatgpt.com' || request.headers.get('upgrade') !== 'websocket') return env.CONTROL.fetch(request);
  const [client, server] = Object.values(new WebSocketPair()); server.accept();
  let history = [];
  server.addEventListener('close', () => server.close(1000));
  server.addEventListener('message', async event => {
    const body = JSON.parse(event.data);
    if (body.type !== 'response.create') throw new Error('Unexpected provider frame ' + body.type);
    history = body.previous_response_id ? [...history, ...(body.input ?? [])] : [...(body.input ?? [])];
    const response = await env.CONTROL.fetch('https://control.internal/model', { method: 'POST',
      body: JSON.stringify({ model: body.model, previous_response_id: body.previous_response_id ?? null, history }) });
    for (const frame of await response.json()) server.send(JSON.stringify(frame));
  });
  return new Response(null, { status: 101, webSocket: client });
}};`;

async function bundle(output, name, source, cwd) {
  const wasm = new Set();
  const result = await build({ stdin: { contents: source, resolveDir: cwd }, bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
    external: ['cloudflare:*', 'node:*'], alias: { 'node-rsa': join(root, 'js/nanocodex/tools/browser/unsupportedNodeRsa.mjs') }, logLevel: 'silent',
    plugins: [{ name: 'compiled-wasm', setup(b) {
      b.onResolve({ filter: /\.\/just-bash-lazy\.mjs$/ }, a => ({ path: resolve(a.resolveDir, a.path), external: true }));
      b.onResolve({ filter: /^[a-z][a-z_]*(?:\/[a-z_]+)?$/ }, a => builtinModules.includes(a.path) ? { path: 'node:' + a.path, external: true } : undefined);
      b.onResolve({ filter: /\.wasm(?:\?module)?$|^nanocodex\/wasm$/ }, a => {
        const path = a.path === 'nanocodex/wasm' ? join(root, 'js/nanocodex/pkg-web/nanocodex_bg.wasm') : resolve(a.resolveDir, a.path.replace(/\?module$/, ''));
        wasm.add(path); return { path, external: true };
      });
    } }] });
  const code = result.outputFiles[0].text;
  const requires = [...new Set([...code.matchAll(/__require\("(node:[^"]+)"\)/g)].map(m => m[1]))];
  const prelude = requires.map((n, i) => 'import * as builtin' + i + ' from ' + JSON.stringify(n) + ';').join('\n')
    + '\nconst requireMap={' + requires.map((n, i) => JSON.stringify(n) + ':builtin' + i).join(',') + "};const require=name=>{if(!requireMap[name])throw new Error('Unexpected require '+name);return requireMap[name];};\n";
  const path = join(output, name + '.mjs'); await writeFile(path, prelude + code);
  const lazy = [...code.matchAll(/import\("([^"]*just-bash-lazy\.mjs)"\)/g)].map(m => m[1]);
  return [{ type: 'ESModule', path }, ...[...new Set(lazy)].map(path => ({ type: 'ESModule', path })), ...[...wasm].map(path => ({ type: 'CompiledWasm', path }))];
}

// A client renders a tool.call as running until its tool.result. Each call
// attempt must end: a call interrupted by a runtime epoch may reappear only as
// a fresh tool.call after that epoch (a replayed attempt), never as a second
// bare result.
function openCalls(rows) {
  const open = new Map(), violations = [];
  for (const row of rows) {
    const attempt = row.event?.payload?.attempt ?? 1;
    const key = (row.agent_id ?? 'root') + ':' + row.event?.payload?.call_id + (attempt > 1 ? '#' + attempt : '');
    if (row.event?.type === 'tool.call') open.set(key, row.cursor);
    if (row.event?.type === 'tool.result') {
      if (!open.has(key)) violations.push({ key, cursor: row.cursor, reason: 'result without an open attempt' });
      open.delete(key);
    }
  }
  assert.deepEqual(violations, [], 'every tool.result closes exactly one open attempt');
  return [...open.keys()];
}

test('a runtime restart mid-tool settles every open call as unknown, and clients reconnect from a surface snapshot', { timeout: 240_000 }, async () => {
  const output = join(root, 'output/surface-state-recovery', new Date().toISOString().replaceAll(':', '-') + '-' + randomUUID().slice(0, 8));
  await mkdir(join(output, 'curl'), { recursive: true });
  const git = args => { try { return execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' }).trim(); } catch { return 'unavailable'; } };
  await writeFile(join(output, 'run.json'), JSON.stringify({ command: [process.execPath, ...process.argv.slice(1)], cwd: process.cwd(), git_head: git(['rev-parse', 'HEAD']),
    git_status: git(['status', '--short']), node: process.version, started_at: new Date().toISOString() }, null, 2));
  const workers = [await accountProxyWorker(root),
    { name: 'managed', modulesRoot: '/', modules: await bundle(output, 'managed', bootstrap, join(root, 'js/managed')), compatibilityDate: '2026-07-29', compatibilityFlags: flags,
      bindings: { MANAGED_AGENT_DIRECT_CREDENTIALS: 'true' }, serviceBindings: { NANOCODEX: 'egress', NANOCODEX_SESSION_MODEL_EGRESS: { name: 'egress', entrypoint: 'SessionModelEgress' } },
      durableObjects: Object.fromEntries([['NANOCODEX_AUTH', 'NonceStorage'], ['NANOCODEX_USERS', 'UserAccount'], ['NANOCODEX_ORGANIZATIONS', 'Organization'], ['NANOCODEX_API_KEYS', 'ApiKeyRecord'],
        ['NANOCODEX_SESSIONS', 'DurableAgentSession'], ['NANOCODEX_ACCOUNT_TOOLS', 'AccountHostedTools'], ['NANOCODEX_VM_HOST_POOLS', 'VmHostPool'], ['NANOCODEX_MEMORY', 'MemoryScope']]
        .map(([binding, className]) => [binding, { className, useSQLite: true }])),
      r2Buckets: ['NANOCODEX_HISTORY', 'NANOCODEX_WORKSPACES'], outboundService: 'provider' },
    { name: 'egress', modulesRoot: '/', modules: await bundle(output, 'egress', "export * from './src/egress.ts';export {default} from './src/egress.ts';", join(root, 'js/egress')),
      compatibilityDate: '2026-07-29', compatibilityFlags: ['nodejs_compat', 'enable_request_signal'],
      bindings: { ENVIRONMENT: 'test', CREDENTIAL_ENCRYPTION_KEY: 'MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY' }, serviceBindings: { MANAGED_AGENT_OWNERSHIP: { name: 'managed', entrypoint: 'ManagedAgentOwnership' } },
      durableObjects: Object.fromEntries([['USER_CREDENTIALS', 'UserCredentialBroker'], ['AGENT_SUBJECTS', 'AgentSubjectDirectory'], ['USER_CONNECTORS', 'UserConnectorBroker'],
        ['MCP_CONNECTIONS', 'McpConnectionDirectory'], ['SPOTIFY_RATE_LIMITS', 'SpotifyRateLimit'], ['GMAIL_PUSH_MAILBOXES', 'GmailPushMailbox']].map(([binding, className]) => [binding, { className, useSQLite: true }])),
      outboundService: 'provider' },
    { name: 'provider', modules: true, script: providerSource, compatibilityDate: '2026-07-29' }];
  await writeFile(join(output, 'workers.json'), JSON.stringify(workers));

  // ---- External stub boundary: the parent owns every provider decision ----
  const modelCalls = [], effects = [], unexpected = [], kills = [];
  let processNumber = 0, fixture, base, token, dead = Promise.resolve();
  const respond = (items, end) => { const id = 'resp_' + randomUUID();
    return [{ type: 'response.created', response: { id, status: 'in_progress' } },
      ...items.filter(item => item.type === 'message').map(item => ({ type: 'response.output_text.delta', output_index: 0, delta: item.content[0].text })),
      { type: 'response.completed', response: { id, status: 'completed', ...(end ? { end_turn: true } : {}), output: items, usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2 } } }]; };
  const say = text => respond([{ type: 'message', role: 'assistant', content: [{ type: 'output_text', text }] }], true);
  const exec = (callId, source) => respond([{ type: 'custom_tool_call', name: 'exec', call_id: callId, input: source }], false);
  const effect = name => 'text(await tools.exec_command({cmd:"curl -s -X POST https://effects.example/effect/' + name + '"}));';
  // Most specific first: a root's later input can quote its child's task.
  const markers = ['SURFACE_CHILD_TASK', 'SURFACE_SPAWN', 'SURFACE_NEXT', 'SURFACE_ROOT'];
  const baselines = {};
  const decide = ({ history }) => {
    const user = JSON.stringify(history.filter(item => item.role === 'user'));
    const scenario = markers.find(marker => user.includes(marker));
    const outputs = history.filter(item => /_call_output$/.test(item.type ?? ''));
    const users = history.filter(item => item.role === 'user' || item.role === 'developer');
    const resumed = users.length > (baselines[scenario] ??= users.length);
    modelCalls.push({ process: processNumber, scenario, resumed, tool_outputs: outputs.map(item => item.call_id), at: new Date().toISOString() });
    switch (scenario) {
      case 'SURFACE_ROOT': return outputs.length === 0 ? exec('surface-root-cell', effect('S1')) : say('SURFACE_ROOT_DONE');
      case 'SURFACE_NEXT': return say('SURFACE_NEXT_DONE');
      case 'SURFACE_SPAWN': return [
        () => exec('surface-spawn', 'text(await tools.spawn_agent(' + JSON.stringify({ role: 'Surface child', task: 'SURFACE_CHILD_TASK: apply effect S2 once, then submit_result.', model: 'sol', thinking: 'low', output_contract: { kind: 'string' } }) + '));'),
        () => exec('surface-wait', 'text(await tools.wait_agent({agent_ids:[1],timeout_ms:20000}));'),
      ][outputs.length]?.() ?? say('SURFACE_SPAWN_DONE');
      case 'SURFACE_CHILD_TASK': {
        // A restored child is told which call was interrupted; it never repeats
        // S2 and moves on to new work (S3, held) before submitting.
        const done = outputs.map(item => item.call_id);
        if (resumed) return !done.includes('surface-child-moved-on') ? exec('surface-child-moved-on', effect('S3'))
          : !done.includes('surface-child-submit') ? exec('surface-child-submit', 'text(await tools.submit_result({output:"CHILD_RESUMED_WITHOUT_REPEAT"}));')
          : say('CHILD_DONE');
        return outputs.length === 0 ? exec('surface-child-cell', effect('S2')) : say('CHILD_UNEXPECTED');
      }
      default: unexpected.push({ kind: 'model', scenario: scenario ?? null }); return say('UNEXPECTED');
    }
  };
  const held = [], releases = new Map();
  const sockets = new Set();
  const control = createServer(async (req, res) => {
    let raw = ''; for await (const chunk of req) raw += chunk;
    const call = JSON.parse(raw), url = new URL(call.url);
    const send = (status, body, type = 'application/json') => { res.writeHead(status, { 'content-type': type }); res.end(typeof body === 'string' ? body : JSON.stringify(body)); };
    if (url.href === 'https://control.internal/model') return send(200, decide(JSON.parse(call.body)));
    if (url.hostname === 'effects.example') {
      const name = url.pathname.split('/').pop();
      effects.push({ process: processNumber, name, at: new Date().toISOString() });
      // The first dispatch reaches the external system and its response is
      // held: the call is visibly running when the owner dies.
      if (effects.filter(entry => entry.name === name).length === 1) {
        held.push(name);
        releases.set(name, () => send(200, 'EFFECT_' + name + '_APPLIED\n', 'text/plain'));
        return;
      }
      return send(200, 'EFFECT_' + name + '_APPLIED\n', 'text/plain');
    }
    if (url.origin === 'https://chatgpt.com' && call.body?.includes('"gpt-6-luna"'))
      return send(200, { id: 'title', output: [{ type: 'message', role: 'assistant', content: [{ type: 'output_text', text: 'Synthetic surface' }] }], usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2 } });
    if (call.method === 'POST' && /\/mcp\/?$|^https:\/\/mcp\./.test(call.url)) return send(503, { error: 'synthetic offline' });
    unexpected.push({ kind: 'http', method: call.method, url: call.url });
    send(404, { error: 'unexpected synthetic external request' });
  });
  control.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  await new Promise(r => control.listen(0, '127.0.0.1', r));
  const controlBase = 'http://127.0.0.1:' + control.address().port;
  const start = () => dead.then(() => new Promise((resolvePromise, reject) => {
    const number = ++processNumber;
    const child = fork(fileURLToPath(new URL('./fixtures/managed-curl-recovery-process.mjs', import.meta.url)), [output, controlBase], { detached: true, stdio: ['ignore', 'pipe', 'pipe', 'ipc'] });
    for (const stream of ['stdout', 'stderr']) child[stream].on('data', chunk => void appendFile(join(output, 'workerd-' + number + '.log'), chunk));
    child.once('error', reject); child.once('exit', code => reject(new Error('fixture exited before ready: ' + code)));
    child.once('message', message => { child.removeAllListeners('exit'); fixture = { child, number, exited: new Promise(r => child.once('exit', r)) }; base = message.ready; resolvePromise(); });
  }));
  async function kill(reason) {
    const current = fixture; if (!current) return; fixture = undefined;
    process.kill(-current.child.pid, 'SIGKILL'); dead = current.exited;
    kills.push({ process: current.number, reason, at: new Date().toISOString() }); await dead;
    // Held effect sockets belonged to the dead owner.
    for (const socket of sockets) socket.destroy();
  }
  const waitFor = async (label, predicate, timeout = 30_000, interval = 100) => {
    const deadline = Date.now() + timeout;
    for (;;) { const value = await predicate(); if (value) return value; if (Date.now() > deadline) throw new Error('timed out waiting for ' + label); await delay(interval); }
  };
  let sequence = 0;
  async function curl(label, path, { method = 'GET', body, headers = {}, expected, sse, maxTime = 30 } = {}) {
    const name = String(++sequence).padStart(3, '0') + '-' + label, file = join(output, 'curl', name);
    if (body !== undefined) await writeFile(file + '.request.json', JSON.stringify(body, null, 2));
    const args = ['--silent', '--show-error', '--no-buffer', '--max-time', String(maxTime), '--request', method, '--dump-header', file + '.headers', '--config', '-',
      '-H', 'Content-Type: application/json', ...Object.entries(headers).flatMap(([key, value]) => ['-H', key + ': ' + value]),
      ...(body === undefined ? [] : ['--data-binary', '@' + file + '.request.json']), new URL(path, base).href];
    const started = Date.now(), child = spawn('curl', args, { stdio: ['pipe', 'pipe', 'pipe'] });
    child.stdin.end('header = ' + JSON.stringify('Authorization: Bearer ' + token) + '\n');
    let stdout = '', stderr = '', buffer = ''; const frames = [];
    child.stdout.on('data', chunk => { stdout += chunk; if (!sse) return; buffer += chunk;
      for (let end; (end = buffer.indexOf('\n\n')) >= 0;) { const frame = buffer.slice(0, end); buffer = buffer.slice(end + 2);
        const data = frame.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
        if (data) { const value = JSON.parse(data); frames.push(value); if (sse(value)) child.kill('SIGTERM'); } } });
    child.stderr.on('data', chunk => { stderr += chunk; });
    const exit = await new Promise(r => child.on('close', (code, signal) => r({ code, signal })));
    const head = await readFile(file + '.headers', 'utf8').catch(() => '');
    const status = Number([...head.matchAll(/^HTTP\/\S+ (\d+)/gm)].at(-1)?.[1] ?? 0);
    await writeFile(file + '.response', stdout);
    let value; try { value = JSON.parse(stdout); } catch { value = stdout; }
    await appendFile(join(output, 'transcript.jsonl'), JSON.stringify({ name, process: fixture?.number, command: ['curl', ...args], stdin: 'header = "Authorization: Bearer <redacted synthetic API key>"',
      status, exit, stderr, duration_ms: Date.now() - started, request: body ?? null, response: sse ? { frames } : value }) + '\n');
    if (expected !== undefined) assert.equal(status, expected, name + ': ' + stderr + ' ' + stdout.slice(0, 2000));
    return { status, value, frames };
  }
  const terminal = (label, agent, turn) => waitFor(label + ' terminal', async () => {
    const value = (await curl(label, '/v1/agents/' + agent + '/turns/' + turn, { expected: 200 })).value;
    return ['completed', 'failed', 'cancelled'].includes(value.state) ? value : undefined;
  }, 60_000, 250);
  const history = async (label, agent, surface) => (await curl(label, '/v1/agents/' + agent + '/events/history?after=0&limit=256' + (surface ? '&surface=1' : ''), { expected: 200 })).value;
  const snapshot = async (label, agent) => (await curl(label, '/v1/agents/' + agent + '/surface', { expected: 200 })).value;
  const interruptedResults = rows => rows.filter(row => row.event?.type === 'tool.result' && row.event.payload.structured_result?.code === 'TOOL_CALL_INTERRUPTED');
  const summary = {};

  try {
    await start();
    token = (await (await fetch(new URL('/__fixture/identity', base), { method: 'POST' })).json()).token; assert.ok(token, 'synthetic API key');
    assert.equal((await fetch(new URL('/__fixture/chatgpt', base), { method: 'POST' })).status, 204);

    // 1. Root stuck card: the root Code Mode cell and its nested shell call are
    // running (effect S1 held) when the owner dies.
    const rootRun = (await curl('root-admit', '/v1/agent-runs', { method: 'POST', body: { input: 'SURFACE_ROOT: apply S1 once.', settings }, headers: { 'Idempotency-Key': randomUUID() }, expected: 201 })).value;
    await waitFor('S1 held', () => held.includes('S1'));
    const during = await snapshot('root-surface-during', rootRun.agent_id);
    summary.root_during = during;
    assert.equal(during.epoch, 0);
    assert.ok(during.tool_calls.every(call => call.attempt === 1));
    assert.deepEqual(during.turns.map(turn => [turn.id, turn.state]), [[rootRun.turn_id, 'accepted']]);
    assert.deepEqual(during.runs.map(run => [run.agent_id, run.turn_id]), [[null, rootRun.turn_id]], 'the root run is open');
    const openDuring = during.tool_calls.map(call => call.call_id);
    assert.ok(openDuring.includes('surface-root-cell'), JSON.stringify(during.tool_calls));
    const nested = during.tool_calls.find(call => call.parent_call_id === 'surface-root-cell');
    assert.ok(nested && nested.tool === 'exec_command', 'the nested shell call is open beneath its cell: ' + JSON.stringify(during.tool_calls));
    const beforeLoss = await history('root-history-before-loss', rootRun.agent_id, true);
    assert.equal(during.cursor, beforeLoss.latest_cursor, 'the snapshot is exact at the latest durable cursor');

    await kill('root effect S1 held');
    await start();
    // Any public request reconstructs the Session; recovery runs in its constructor.
    const after = await snapshot('root-surface-after-restart', rootRun.agent_id);
    summary.root_after = after;
    assert.equal(after.epoch, 1, 'the restart began runtime epoch 1');
    const rootDone = await terminal('root-terminal', rootRun.agent_id, rootRun.turn_id);
    assert.equal(rootDone.state, 'completed', JSON.stringify(rootDone));

    const surfaceRows = (await history('root-history-surface', rootRun.agent_id, true)).data;
    const legacyRows = (await history('root-history-legacy', rootRun.agent_id, false)).data;
    const epochIndex = surfaceRows.findIndex(row => row.type === 'runtime_epoch');
    assert.ok(epochIndex > 0, 'a surface reader sees the runtime epoch');
    const marker = surfaceRows[epochIndex];
    summary.root_epoch = marker;
    assert.equal(surfaceRows.filter(row => row.type === 'runtime_epoch').length, 1);
    assert.equal(marker.epoch, 1); assert.equal(marker.reason, 'runtime_restarted');
    assert.deepEqual(marker.interrupted.tool_calls.map(call => call.call_id).sort(), [...openDuring].sort(), 'the epoch names exactly the calls that were open');
    assert.deepEqual(marker.interrupted.turns, [rootRun.turn_id]);
    assert.deepEqual(marker.interrupted.runs.map(run => run.turn_id), [rootRun.turn_id]);
    // The unknown-outcome results follow the epoch immediately, nested call first.
    const settled = surfaceRows.slice(epochIndex + 1, epochIndex + 1 + openDuring.length);
    assert.deepEqual(settled.map(row => row.event?.payload?.call_id), [...marker.interrupted.tool_calls].reverse().map(call => call.call_id));
    for (const row of settled) {
      assert.equal(row.event.type, 'tool.result'); assert.equal(row.event.payload.status, 'failed');
      assert.equal(row.event.payload.outcome_unknown, true);
      assert.equal(typeof row.event.payload.duration_ns, 'number');
      assert.deepEqual([row.event.payload.structured_result.outcome, row.event.payload.structured_result.reason, row.event.payload.structured_result.epoch], ['unknown', 'runtime_restarted', 1]);
      assert.match(row.event.payload.result, /runtime restarted before a terminal result/);
      assert.equal(row.turn_id, rootRun.turn_id);
    }
    assert.deepEqual(openCalls(surfaceRows), [], 'no root call is left running after the restart');
    // The replay after the epoch is attempt 2 of the same call ids, call and result alike.
    const replayed = surfaceRows.slice(epochIndex + 1 + openDuring.length)
      .filter(row => (row.event?.type === 'tool.call' || row.event?.type === 'tool.result') && openDuring.includes(row.event.payload.call_id));
    summary.root_replayed = replayed.map(row => [row.cursor, row.event.type, row.event.payload.call_id, row.event.payload.attempt ?? 1]);
    assert.ok(replayed.length >= 2, 'recovery replayed the interrupted cell');
    assert.ok(replayed.every(row => row.event.payload.attempt === 2), JSON.stringify(summary.root_replayed));
    assert.ok(settled.every(row => row.event.payload.attempt === undefined && row.event.payload.structured_result.attempt === 1));
    assert.ok(!legacyRows.some(row => row.type === 'runtime_epoch'), 'legacy readers never receive runtime_epoch');
    assert.equal(interruptedResults(legacyRows).length, openDuring.length, 'legacy readers still receive the terminal unknown results');
    assert.deepEqual(openCalls(legacyRows), []);
    assert.equal(effects.filter(entry => entry.name === 'S1').length, 1, 'recovery never repeats the unproved effect');

    // Reconnect: an SSE client from cursor 0 sees the same settlement; only an
    // opted-in reader receives the epoch frame.
    const stop = value => value.type === 'turn_completed' && value.turn_id === rootRun.turn_id;
    const legacySse = await curl('root-sse-legacy', '/v1/agents/' + rootRun.agent_id + '/events?cursor=0', { headers: { Accept: 'text/event-stream' }, sse: stop, maxTime: 20 });
    const surfaceSse = await curl('root-sse-surface', '/v1/agents/' + rootRun.agent_id + '/events?cursor=0&surface=1', { headers: { Accept: 'text/event-stream' }, sse: stop, maxTime: 20 });
    assert.ok(legacySse.frames.some(stop) && surfaceSse.frames.some(stop), 'both streams reached the terminal');
    assert.equal(legacySse.frames.filter(frame => frame.type === 'runtime_epoch').length, 0);
    assert.equal(surfaceSse.frames.filter(frame => frame.type === 'runtime_epoch').length, 1);
    assert.equal(interruptedResults(legacySse.frames).length, openDuring.length);
    // Live WebSocket transport: runtime_epoch only for a socket opened with
    // surface=1. curl performs the upgrade; server frames are unmasked text.
    const socketFrames = (label, surface) => new Promise((resolvePromise, reject) => {
      const url = new URL('/v1/agents/' + rootRun.agent_id + '/ws?cursor=0' + (surface ? '&surface=1' : ''), base).href;
      const child = spawn('curl', ['--silent', '--no-buffer', '--max-time', '20', '--config', '-', '-H', 'Connection: Upgrade', '-H', 'Upgrade: websocket',
        '-H', 'Sec-WebSocket-Version: 13', '-H', 'Sec-WebSocket-Key: c3VyZmFjZS1zdGF0ZS1wcm9iZQ==', url], { stdio: ['pipe', 'pipe', 'pipe'] });
      child.stdin.end('header = ' + JSON.stringify('Authorization: Bearer ' + token) + '\n');
      let buffer = Buffer.alloc(0), done = false; const frames = [];
      child.stdout.on('data', chunk => {
        buffer = Buffer.concat([buffer, chunk]);
        for (;;) {
          if (buffer.length < 2) return;
          let length = buffer[1] & 0x7f, offset = 2;
          if (length === 126) { if (buffer.length < 4) return; length = buffer.readUInt16BE(2); offset = 4; }
          else if (length === 127) { if (buffer.length < 10) return; length = Number(buffer.readBigUInt64BE(2)); offset = 10; }
          if (buffer.length < offset + length) return;
          const opcode = buffer[0] & 0x0f, payload = buffer.subarray(offset, offset + length);
          buffer = buffer.subarray(offset + length);
          if (opcode !== 1) continue;
          const value = JSON.parse(payload.toString('utf8')); frames.push(value);
          if (stop(value) && !done) { done = true; child.kill('SIGTERM'); }
        }
      });
      child.on('close', () => { void appendFile(join(output, 'transcript.jsonl'), JSON.stringify({ name: label, command: ['curl', url], frames: frames.length, types: frames.map(frame => frame.type) }) + '\n');
        done ? resolvePromise(frames) : reject(new Error(label + ' ended before the terminal frame: ' + JSON.stringify(frames.map(frame => frame.type)))); });
    });
    const legacySocket = await socketFrames('ws-legacy', false), surfaceSocket = await socketFrames('ws-surface', true);
    await appendFile(join(output, 'websocket.jsonl'), JSON.stringify({ legacy: legacySocket, surface: surfaceSocket }) + '\n');
    assert.equal(legacySocket[0].type, 'ready'); assert.equal(legacySocket[0].surface, undefined);
    assert.deepEqual(surfaceSocket[0].surface?.epoch, 1, 'a surface socket learns the epoch in ready');
    assert.equal(legacySocket.filter(frame => frame.type === 'runtime_epoch').length, 0);
    assert.equal(surfaceSocket.filter(frame => frame.type === 'runtime_epoch').length, 1);
    assert.equal(interruptedResults(legacySocket).length, openDuring.length);

    const settledSnapshot = await snapshot('root-surface-settled', rootRun.agent_id);
    summary.root_settled = settledSnapshot;
    assert.deepEqual([settledSnapshot.epoch, settledSnapshot.turns, settledSnapshot.runs, settledSnapshot.tool_calls], [1, [], [], []]);
    // Resume from the snapshot cursor: nothing is missed or repeated.
    const tail = (await curl('root-history-after-snapshot', '/v1/agents/' + rootRun.agent_id + '/events/history?after=' + settledSnapshot.cursor + '&limit=16&surface=1', { expected: 200 })).value;
    assert.deepEqual(tail.data, []);
    const next = (await curl('root-next-turn', '/v1/agents/' + rootRun.agent_id + '/turns', { method: 'POST', body: { input: 'SURFACE_NEXT: still usable.' }, headers: { 'Idempotency-Key': randomUUID() }, expected: 202 })).value;
    assert.equal((await terminal('root-next-terminal', rootRun.agent_id, next.turn_id)).state, 'completed');
    const delta = (await curl('root-delta-from-snapshot', '/v1/agents/' + rootRun.agent_id + '/events/history?after=' + settledSnapshot.cursor + '&limit=256&surface=1', { expected: 200 })).value;
    assert.ok(delta.data.length > 0 && delta.data.every(row => BigInt(row.cursor) > BigInt(settledSnapshot.cursor)), 'the delta starts after the snapshot cursor');
    assert.ok(delta.data.some(row => row.type === 'turn_completed' && row.turn_id === next.turn_id));

    // 2. Child stuck card: the child's shell call (effect S2) and the root's
    // wait_agent call are both running when the owner dies.
    const spawnRun = (await curl('child-admit', '/v1/agent-runs', { method: 'POST', body: { input: 'SURFACE_SPAWN: delegate S2 to one child.', settings }, headers: { 'Idempotency-Key': randomUUID() }, expected: 201 })).value;
    await waitFor('S2 held', () => held.includes('S2'));
    const childDuring = await waitFor('child call visible', async () => {
      const value = await snapshot('child-surface-during', spawnRun.agent_id);
      return value.tool_calls.some(call => call.agent_id === 1 && call.call_id === 'surface-child-cell') ? value : undefined;
    });
    summary.child_during = childDuring;
    assert.ok(childDuring.agents.some(agent => agent.agent_id === 1 && agent.parent_agent_id === null && agent.role === 'Surface child'), JSON.stringify(childDuring.agents));
    assert.ok(childDuring.runs.some(run => run.agent_id === 1), 'the child run is open');
    const childOpen = childDuring.tool_calls.map(call => (call.agent_id ?? 'root') + ':' + call.call_id);
    await kill('child effect S2 held');
    await start();
    // Any public request reconstructs the Session (production also fires its alarm).
    const childRestart = await snapshot('child-surface-after-restart', spawnRun.agent_id);
    summary.child_restart = childRestart;
    assert.equal(childRestart.epoch, 1);
    assert.ok(!childRestart.tool_calls.some(call => call.call_id.startsWith('surface-child-cell')), 'the lost child call is settled at reconstruction');
    // The stuck-card moment: the restored child has moved on to new work (S3
    // is held, its new call is running), and the call that died with the old
    // owner already has a terminal result for every reader, legacy included.
    await waitFor('restored child moved on', () => held.includes('S3'));
    const movedOnRows = (await history('child-history-moved-on-legacy', spawnRun.agent_id, false)).data;
    const openAfterMove = openCalls(movedOnRows);
    summary.child_open_while_moved_on = openAfterMove;
    assert.ok(openAfterMove.includes('1:surface-child-moved-on'), 'the restored child is running new work: ' + JSON.stringify(openAfterMove));
    assert.ok(!openAfterMove.some(key => key.startsWith('1:surface-child-cell')), 'the lost child call is not left running: ' + JSON.stringify(openAfterMove));
    const childAfter = await snapshot('child-surface-moved-on', spawnRun.agent_id);
    summary.child_after = childAfter;
    assert.equal(childAfter.epoch, 1);
    assert.ok(childAfter.tool_calls.some(call => call.agent_id === 1 && call.call_id === 'surface-child-moved-on'));
    assert.ok(!childAfter.tool_calls.some(call => call.call_id.startsWith('surface-child-cell')));
    releases.get('S3')();
    const spawnDone = await terminal('child-root-terminal', spawnRun.agent_id, spawnRun.turn_id);
    assert.equal(spawnDone.state, 'completed', JSON.stringify(spawnDone));
    const childRows = (await history('child-history-surface', spawnRun.agent_id, true)).data;
    const childEpoch = childRows.find(row => row.type === 'runtime_epoch');
    summary.child_epoch = childEpoch;
    assert.deepEqual(childEpoch.interrupted.tool_calls.map(call => (call.agent_id ?? 'root') + ':' + call.call_id).sort(), [...childOpen].sort());
    const childSettled = interruptedResults(childRows).map(row => (row.agent_id ?? 'root') + ':' + row.event.payload.call_id);
    assert.ok(childSettled.includes('1:surface-child-cell'), 'the child call settled as unknown: ' + JSON.stringify(childSettled));
    assert.deepEqual(openCalls(childRows), [], 'no root or child call is left running');
    assert.equal(effects.filter(entry => entry.name === 'S2').length, 1, 'the restored child never repeats S2');
    // The child's completion may wake the idle root for one more turn; once
    // that settles, nothing is open.
    const finalChild = await waitFor('child agent settled', async () => {
      const value = await snapshot('child-surface-settled', spawnRun.agent_id);
      return value.turns.length === 0 && value.runs.length === 0 && value.tool_calls.length === 0 ? value : undefined;
    }, 30_000, 250);
    summary.child_settled = finalChild;
    assert.ok(finalChild.agents.some(agent => agent.agent_id === 1 && agent.state === 'completed'), JSON.stringify(finalChild.agents));
    assert.deepEqual(openCalls((await history('child-history-final', spawnRun.agent_id, true)).data), []);

    // 3. A restart with no open work (idle eviction) is not a visible epoch.
    await kill('idle owner loss');
    await start();
    const beforeIdle = await snapshot('root-surface-before-idle-restart', rootRun.agent_id);
    await kill('idle owner loss 2');
    await start();
    const idle = await snapshot('root-surface-after-idle-restart', rootRun.agent_id);
    summary.idle = { before: beforeIdle.runtime_generation, after: idle.runtime_generation };
    assert.equal(idle.epoch, 1, 'an idle restart does not begin a new epoch');
    assert.ok(idle.runtime_generation > beforeIdle.runtime_generation, 'but it does change the runtime generation');
    assert.equal((await history('root-history-after-idle', rootRun.agent_id, true)).data.filter(row => row.type === 'runtime_epoch').length, 1);
    assert.deepEqual(unexpected, []);
  } finally {
    await writeFile(join(output, 'trace.json'), JSON.stringify({ summary, kills, model_calls: modelCalls, effects, unexpected }, null, 2));
    await kill('test cleanup');
    for (const socket of sockets) socket.destroy();
    await new Promise(r => control.close(r));
    console.log(JSON.stringify({ evidence: output, kills: kills.length, effects: effects.map(entry => entry.name), model_calls: modelCalls.length }));
  }
});
