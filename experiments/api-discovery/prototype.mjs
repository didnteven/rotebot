// Research only. Nothing in the app imports this module.
// Candidate facts below represent HOST-VERIFIED evidence, never model assertions.
export function assess({ local = false, localOnly = false, stopped = false,
  connected = false, publicRead = false, api = null, adapterReady = false } = {}) {
  if (stopped) return { state: 'paused' };
  if (local) return { state: 'ready', route: 'local' };
  if (localOnly) return { state: 'blocked_local_only' };
  if (connected) return { state: 'ready', route: 'existing_connection' };
  if (publicRead) return { state: 'ready', route: 'public_get' };
  if (!api?.officialEvidence || !api?.operationConfirmed || !api?.coversTask) {
    return { state: 'needs_research' };
  }
  if (api.userEligible !== true) return { state: 'needs_access_check' };
  if (api.readOnly !== true || api.method !== 'GET') {
    return { state: 'needs_separate_adapter', reason: 'Not an approved GET read' };
  }
  if (!['bearer', 'header_key'].includes(api.auth)) {
    return { state: 'needs_separate_adapter', reason: 'Authentication unsupported in first slice' };
  }
  return { state: adapterReady ? 'needs_connection' : 'needs_implementation', route: 'custom_api' };
}

export function responseAction(status, retryAfter, now) {
  if (status === 401) return { state: 'needs_auth', until: Infinity };
  if (status === 403) return { state: 'blocked', until: Infinity };
  if (status === 429) {
    const seconds = /^\d+$/.test(retryAfter ?? '') ? Number(retryAfter) : null;
    const parsed = seconds === null ? Date.parse(retryAfter ?? '') : now + seconds * 1000;
    const until = Number.isFinite(parsed) ? Math.max(now + 1000, parsed) : now + 60_000;
    return { state: 'rate_limited', until };
  }
  if (status >= 300 && status < 400) return { state: 'redirect_blocked' };
  if (status < 200 || status >= 300) return { state: 'http_error' };
  return { state: 'ok' };
}

// This deliberately CANNOT fetch a real API. Only synthetic loopback servers
// are accepted. It tests a credential boundary without shipping a new transport.
// Production needs HTTPS, DNS/IP confinement, durable shared budgets, encrypted
// vault integration, a permissions model, and approval/revocation race tests.
export function loopbackProbe({ endpoint, secretRef, vault, audit, limiter = new Map(),
  now = Date.now, maxBytes = 65_536, timeoutMs = 1000, canRun = () => true }) {
  const target = new URL(endpoint);
  if (target.protocol !== 'http:' || target.hostname !== '127.0.0.1'
      || target.username || target.password || target.search || target.hash) {
    throw new Error('Only a credential-free synthetic loopback URL is accepted');
  }
  return async function read(requestedEndpoint = endpoint) {
    if (!canRun()) return { state: 'disabled' };
    // Bind secrets to the exact endpoint, including path and port.
    if (requestedEndpoint !== endpoint) return { state: 'unapproved_endpoint' };
    const prior = limiter.get(target.origin);
    if (prior && prior.until > now()) return { state: prior.state, suppressed: true };
    const secret = vault.get(secretRef);
    if (!secret) return { state: 'needs_auth' };
    const record = result => {
      audit.push({ origin: target.origin, path: target.pathname, state: result.state });
      return result;
    };
    try {
      const response = await fetch(endpoint, {
        method: 'GET', headers: { Authorization: `Bearer ${secret}` },
        redirect: 'manual', signal: AbortSignal.timeout(timeoutMs),
      });
      const result = responseAction(response.status, response.headers.get('retry-after'), now());
      if (result.until) limiter.set(target.origin, result);
      if (result.state !== 'ok') {
        await response.body?.cancel();
        return record(result);
      }
      const reader = response.body.getReader();
      const parts = [];
      let size = 0;
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.byteLength;
        if (size > maxBytes) {
          await reader.cancel();
          return record({ state: 'too_large' });
        }
        parts.push(value);
      }
      const text = Buffer.concat(parts).toString('utf8');
      // An exact echo canary, NOT a general secret-redaction guarantee.
      if (text.includes(secret)) return record({ state: 'secret_echo' });
      return record({ state: 'ok', data: JSON.parse(text) });
    } catch {
      // Never propagate a remote body, header, URL or raw exception into logs.
      return record({ state: 'invalid_or_failed_response' });
    }
  };
}
