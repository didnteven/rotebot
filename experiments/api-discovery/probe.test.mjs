import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { assess, loopbackProbe, responseAction } from './prototype.mjs';

// Hand-authored host facts. These are NOT outputs from the task model or search.
const api = { officialEvidence: true, operationConfirmed: true, coversTask: true,
  userEligible: true, method: 'GET', readOnly: true, auth: 'bearer' };

test('route scenarios: custom setup is a fallback and does not imply runnable', () => {
  const scenarios = [
    ['CSV task stays local', { local: true, api }, 'ready', 'local'],
    ['connected service reused', { connected: true, api }, 'ready', 'existing_connection'],
    ['public API needs no key box', { publicRead: true, api }, 'ready', 'public_get'],
    ['API exists but no host adapter', { api }, 'needs_implementation', 'custom_api'],
    ['supported API can offer setup', { api, adapterReady: true }, 'needs_connection', 'custom_api'],
    ['third-party blog alone is insufficient', { api: { ...api, officialEvidence: false } }, 'needs_research'],
    ['endpoint existence does not prove task coverage', { api: { ...api, coversTask: false } }, 'needs_research'],
    ['operation unverified', { api: { ...api, operationConfirmed: false } }, 'needs_research'],
    ['unknown plan or admin access', { api: { ...api, userEligible: null } }, 'needs_access_check'],
    ['write operation cannot masquerade as a read', { api: { ...api, readOnly: false } }, 'needs_separate_adapter'],
    ['POST GraphQL read requires its own adapter', { api: { ...api, method: 'POST' } }, 'needs_separate_adapter'],
    ['OAuth cannot be solved by a key field', { api: { ...api, auth: 'oauth2' } }, 'needs_separate_adapter'],
    ['query keys need a different security policy', { api: { ...api, auth: 'query_key' } }, 'needs_separate_adapter'],
    ['Local only blocks connected remote reads', { localOnly: true, connected: true }, 'blocked_local_only'],
    ['Local only still allows local work', { localOnly: true, local: true }, 'ready', 'local'],
    ['Emergency stop precedes all routes', { stopped: true, local: true }, 'paused'],
    ['no evidence stays unresolved', {}, 'needs_research'],
  ];
  for (const [name, input, state, route] of scenarios) {
    const actual = assess(input);
    assert.equal(actual.state, state, name);
    assert.equal(actual.route, route, name);
  }
});

test('Retry-After supports seconds, dates and a conservative invalid-header fallback', () => {
  const now = Date.parse('2026-10-31T00:00:00Z');
  assert.equal(responseAction(429, '120', now).until, now + 120_000);
  assert.equal(responseAction(429, 'Sat, 31 Oct 2026 00:02:00 GMT', now).until, now + 120_000);
  assert.equal(responseAction(429, 'bad value', now).until, now + 60_000);
  assert.equal(responseAction(403, null, now).state, 'blocked');
  assert.equal(responseAction(401, null, now).state, 'needs_auth');
});

test('local transport scenarios exercise real HTTP with synthetic credentials', async t => {
  const secret = 'SYNTHETIC-KEY-DO-NOT-USE-0001';
  const hits = [];
  let base;
  const server = createServer((req, res) => {
    hits.push({ path: req.url, method: req.method, auth: req.headers.authorization });
    switch (req.url) {
      case '/ok':
        if (req.headers.authorization !== `Bearer ${secret}`) {
          res.writeHead(401).end('not authorized');
        } else {
          res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify({ records: [{ id: 'example', total: 42 }] }));
        }
        break;
      case '/rate': res.writeHead(429, { 'Retry-After': '120' }).end(secret); break;
      case '/forbidden': res.writeHead(403).end(secret); break;
      case '/unauthorized': res.writeHead(401).end(secret); break;
      case '/redirect': res.writeHead(302, { Location: `${base}/trap` }).end(); break;
      case '/trap': res.end('{}'); break;
      case '/echo': res.end(JSON.stringify({ echo: secret })); break;
      case '/large': res.end('x'.repeat(70_000)); break;
      case '/malformed': res.end('{'); break;
      case '/dynamic':
        res.setHeader('Content-Type', 'text/html');
        res.end('<main id="result">Loading</main><script src="/client.js"></script>');
        break;
      case '/client.js':
        res.setHeader('Content-Type', 'text/javascript');
        res.end('document.getElementById("result").textContent = "Dynamic answer: 42";');
        break;
      case '/slow': break; // Timeout test; socket is closed during cleanup.
      default: res.writeHead(404).end();
    }
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  base = `http://127.0.0.1:${server.address().port}`;
  t.after(async () => {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  });
  const make = (path, overrides = {}) => {
    const audit = [];
    const probe = loopbackProbe({ endpoint: `${base}${path}`, secretRef: 'test-key',
      vault: new Map([['test-key', secret]]), audit, ...overrides });
    return { probe, audit };
  };

  await t.test('credential supplied by host, only JSON data and safe metadata returned', async () => {
    const { probe, audit } = make('/ok');
    const result = await probe();
    assert.deepEqual(result, { state: 'ok', data: { records: [{ id: 'example', total: 42 }] } });
    assert.equal(hits.at(-1).auth, `Bearer ${secret}`);
    assert.equal(hits.at(-1).method, 'GET');
    assert.equal(JSON.stringify({ result, audit }).includes(secret), false);
  });
  await t.test('exact endpoint binding rejects a changed path or port before vault access', async () => {
    const { probe } = make('/ok', { vault: { get() { throw Error('must not read vault'); } } });
    const count = hits.length;
    assert.equal((await probe(`${base}/trap`)).state, 'unapproved_endpoint');
    assert.equal((await probe('http://127.0.0.1:1/ok')).state, 'unapproved_endpoint');
    assert.equal(hits.length, count);
  });
  await t.test('disabled egress and missing keys make zero requests', async () => {
    const count = hits.length;
    assert.equal((await make('/ok', { canRun: () => false }).probe()).state, 'disabled');
    assert.equal((await make('/ok', { vault: new Map() }).probe()).state, 'needs_auth');
    assert.equal(hits.length, count);
  });
  await t.test('credentialed redirects stop without sending credentials to redirect target', async () => {
    const count = hits.length;
    assert.equal((await make('/redirect').probe()).state, 'redirect_blocked');
    assert.equal(hits.length, count + 1);
    assert.equal(hits.some(h => h.path === '/trap'), false);
  });
  await t.test('429 shares a cooldown across endpoints, never retries, drops error body', async () => {
    const limiter = new Map();
    const now = Date.parse('2026-10-31T00:00:00Z');
    const { probe, audit } = make('/rate', { limiter, now: () => now });
    const count = hits.length;
    const result = await probe();
    assert.equal(result.state, 'rate_limited');
    assert.equal(result.until, now + 120_000);
    assert.equal((await make('/ok', { limiter, now: () => now }).probe()).suppressed, true);
    assert.equal(hits.length, count + 1);
    assert.equal(JSON.stringify({ result, audit }).includes(secret), false);
  });
  for (const [path, expected] of [['/forbidden', 'blocked'], ['/unauthorized', 'needs_auth']]) {
    await t.test(`${path} does not trigger retry or browser fallback`, async () => {
      const { probe, audit } = make(path);
      const count = hits.length;
      const first = await probe();
      assert.equal(first.state, expected);
      assert.equal((await probe()).suppressed, true);
      assert.equal(hits.length, count + 1);
      assert.equal(JSON.stringify({ first, audit }).includes(secret), false);
    });
  }
  for (const [path, expected] of [['/echo', 'secret_echo'], ['/large', 'too_large'],
    ['/malformed', 'invalid_or_failed_response'], ['/slow', 'invalid_or_failed_response']]) {
    await t.test(`${path} is contained`, async () => {
      const { probe, audit } = make(path, { timeoutMs: 100 });
      const result = await probe();
      assert.equal(result.state, expected);
      assert.equal(JSON.stringify({ result, audit }).includes(secret), false);
    });
  }
  await t.test('plain GET sees only the JavaScript shell and never loads its script', async () => {
    const count = hits.length;
    const html = await (await fetch(`${base}/dynamic`)).text();
    assert.match(html, /Loading/);
    assert.equal(html.includes('Dynamic answer: 42'), false);
    assert.equal(hits.length, count + 1);
    assert.equal(hits.some(h => h.path === '/client.js'), false);
  });
  await t.test('probe cannot accidentally target external hosts or accept URL credentials', () => {
    for (const endpoint of ['https://example.com/api', 'http://localhost/api',
      `${base}/ok?api_key=example`, `${base}/ok#fragment`, `${base.replace('://', '://user@')}/ok`]) {
      assert.throws(() => loopbackProbe({ endpoint }), /synthetic loopback/);
    }
  });
});
