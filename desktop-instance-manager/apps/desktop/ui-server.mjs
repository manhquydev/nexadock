import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const UI_ROOT = path.resolve(__dirname, 'ui');
const REPO_ROOT = path.resolve(__dirname, '..', '..');
const HOST_SCRIPT = path.resolve(REPO_ROOT, 'scripts', 'dim-host.ps1');

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml'
};

function runHostCli(positionalArgs, options = {}, extraFlags = []) {
  return new Promise((resolve) => {
    const args = [
      '-NoProfile',
      '-ExecutionPolicy', 'Bypass',
      '-File', HOST_SCRIPT,
      '--cli',
      '--json'
    ];
    if (options.dataDir) {
      args.push('--data-dir', options.dataDir);
    }
    if (options.nodeFixture) {
      args.push('--node-fixture');
    }
    if (options.appPath) {
      args.push('--app-path', options.appPath);
    }
    if (extraFlags.length > 0) {
      args.push(...extraFlags);
    }
    args.push('--', ...positionalArgs);

    execFile('powershell.exe', args, {
      encoding: 'utf8',
      maxBuffer: 16 * 1024 * 1024,
      windowsHide: true
    }, (error, stdout, stderr) => {
      const trimmed = (stdout || '').trim();
      try {
        const parsed = trimmed ? JSON.parse(trimmed) : null;
        if (parsed) {
          resolve({ statusCode: parsed.ok === false ? 400 : 200, payload: parsed });
          return;
        }
      } catch {
        // Fall through to error payload
      }
      resolve({
        statusCode: 500,
        payload: {
          ok: false,
          error_code: 'HOST_EXEC_FAILED',
          message: (stderr || error?.message || 'Host command failed').trim(),
          data: null
        }
      });
    });
  });
}

function readJsonBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let total = 0;
    req.on('data', (chunk) => {
      total += chunk.length;
      if (total > 64 * 1024) {
        reject(new Error('Request body exceeds 64 KB limit'));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8').trim();
      if (!raw) {
        resolve({});
        return;
      }
      try {
        resolve(JSON.parse(raw));
      } catch {
        reject(new Error('Invalid JSON request body'));
      }
    });
    req.on('error', reject);
  });
}

function sendJson(res, statusCode, obj) {
  const body = JSON.stringify(obj);
  res.writeHead(statusCode, {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store',
    'X-Content-Type-Options': 'nosniff'
  });
  res.end(body);
}

function serveStaticFile(reqPath, res) {
  let decoded;
  try {
    decoded = decodeURIComponent(reqPath);
  } catch {
    sendJson(res, 400, { ok: false, error_code: 'BAD_REQUEST', message: 'Malformed URI path' });
    return;
  }
  const rel = decoded === '/' ? 'index.html' : decoded.replace(/^\/+/, '');
  if (rel.includes('\0') || rel.includes('..')) {
    sendJson(res, 403, { ok: false, error_code: 'PATH_TRAVERSAL_REJECTED', message: 'Forbidden path' });
    return;
  }
  const resolved = path.resolve(UI_ROOT, rel);
  const rootPrefix = UI_ROOT.endsWith(path.sep) ? UI_ROOT : UI_ROOT + path.sep;
  if (!resolved.startsWith(rootPrefix)) {
    sendJson(res, 403, { ok: false, error_code: 'PATH_TRAVERSAL_REJECTED', message: 'Forbidden path' });
    return;
  }
  if (!fs.existsSync(resolved) || fs.statSync(resolved).isDirectory()) {
    sendJson(res, 404, { ok: false, error_code: 'NOT_FOUND', message: 'Static asset not found' });
    return;
  }
  const ext = path.extname(resolved).toLowerCase();
  const mime = MIME_TYPES[ext] || 'application/octet-stream';
  res.writeHead(200, {
    'Content-Type': mime,
    'Cache-Control': 'no-store',
    'X-Content-Type-Options': 'nosniff',
    'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'"
  });
  fs.createReadStream(resolved).pipe(res);
}

function isAllowedLoopbackHost(hostHeader, boundPort) {
  if (!hostHeader || typeof hostHeader !== 'string') return false;
  const normalized = hostHeader.trim().toLowerCase();
  if (Number.isInteger(boundPort) && boundPort > 0) {
    return normalized === `127.0.0.1:${boundPort}` || normalized === `localhost:${boundPort}`;
  }
  return /^(127\.0\.0\.1|localhost)(:\d+)?$/.test(normalized);
}

function isAllowedLoopbackOrigin(originHeader, boundPort) {
  if (!originHeader || typeof originHeader !== 'string') return true;
  const normalized = originHeader.trim().toLowerCase();
  if (Number.isInteger(boundPort) && boundPort > 0) {
    return normalized === `http://127.0.0.1:${boundPort}` || normalized === `http://localhost:${boundPort}`;
  }
  return /^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?$/.test(normalized);
}

function isSafeProfileIdParam(id) {
  return Boolean(id) && !/[\0/\\]/.test(id) && !id.includes('..') && !/[\u0000-\u001f\u007f]/.test(id);
}

export function createUiServer(options = {}) {
  const hostOptions = {
    dataDir: options.dataDir || process.env.DIM_DATA_DIR || null,
    nodeFixture: Boolean(options.nodeFixture),
    appPath: options.appPath || null
  };

  const server = http.createServer(async (req, res) => {
    try {
      const boundPort = server.address()?.port;
      if (!isAllowedLoopbackHost(req.headers.host, boundPort)) {
        sendJson(res, 403, { ok: false, error_code: 'FORBIDDEN_HOST', message: 'Non-loopback Host header rejected' });
        return;
      }

      if (req.headers.origin !== undefined && !isAllowedLoopbackOrigin(req.headers.origin, boundPort)) {
        sendJson(res, 403, { ok: false, error_code: 'FORBIDDEN_ORIGIN', message: 'Cross-origin request rejected' });
        return;
      }

      const fetchSite = (req.headers['sec-fetch-site'] || '').toString().toLowerCase();
      if (fetchSite === 'cross-site') {
        sendJson(res, 403, { ok: false, error_code: 'FORBIDDEN_ORIGIN', message: 'Cross-site fetch rejected' });
        return;
      }

      const rawUrlPath = (req.url || '/').split('?')[0];
      if (rawUrlPath.includes('\0') || /(?:%00|\.\.|%2e%2e|%5c\.\.)/i.test(rawUrlPath)) {
        sendJson(res, 403, { ok: false, error_code: 'PATH_TRAVERSAL_REJECTED', message: 'Forbidden traversal path' });
        return;
      }

      const url = new URL(req.url || '/', 'http://127.0.0.1');
      const pathname = url.pathname;
      const method = (req.method || 'GET').toUpperCase();

      if (!pathname.startsWith('/api/')) {
        if (method !== 'GET' && method !== 'HEAD') {
          sendJson(res, 405, { ok: false, error_code: 'METHOD_NOT_ALLOWED', message: 'Method not allowed' });
          return;
        }
        serveStaticFile(pathname, res);
        return;
      }

      if (method === 'POST' || method === 'PUT' || method === 'PATCH' || method === 'DELETE') {
        const contentType = (req.headers['content-type'] || '').toString().toLowerCase();
        const mediaType = contentType.split(';')[0].trim();
        if (mediaType !== 'application/json') {
          sendJson(res, 415, {
            ok: false,
            error_code: 'UNSUPPORTED_MEDIA_TYPE',
            message: 'Content-Type: application/json is required for state-changing requests'
          });
          return;
        }
      }

      if (pathname === '/api/health' && method === 'GET') {
        sendJson(res, 200, { ok: true, bind: '127.0.0.1', host_script: HOST_SCRIPT });
        return;
      }

      if (pathname === '/api/status' && method === 'GET') {
        const flags = url.searchParams.get('withSize') === '1' ? ['--with-size'] : [];
        const out = await runHostCli(['status'], hostOptions, flags);
        sendJson(res, out.statusCode, out.payload);
        return;
      }

      if (pathname === '/api/discover' && method === 'GET') {
        const out = await runHostCli(['discover'], hostOptions);
        sendJson(res, out.statusCode, out.payload);
        return;
      }

      if (pathname === '/api/backups' && method === 'GET') {
        const out = await runHostCli(['backups'], hostOptions);
        sendJson(res, out.statusCode, out.payload);
        return;
      }

      if (pathname === '/api/diagnostics' && method === 'GET') {
        const out = await runHostCli(['diagnostics'], hostOptions);
        sendJson(res, out.statusCode, out.payload);
        return;
      }

      if (pathname === '/api/verify-isolation' && method === 'GET') {
        const out = await runHostCli(['verify-isolation'], hostOptions);
        sendJson(res, out.statusCode, out.payload);
        return;
      }

      if (pathname === '/api/profiles' && method === 'POST') {
        const body = await readJsonBody(req);
        const flags = body.notes ? ['--notes', String(body.notes)] : [];
        const out = await runHostCli(['create', String(body.name || '')], hostOptions, flags);
        sendJson(res, out.statusCode, out.payload);
        return;
      }

      if (pathname === '/api/backups/restore' && method === 'POST') {
        const body = await readJsonBody(req);
        const flags = body.name ? ['--name', String(body.name)] : [];
        const out = await runHostCli(['restore', String(body.backupDir || '')], hostOptions, flags);
        sendJson(res, out.statusCode, out.payload);
        return;
      }

      const profileMatch = pathname.match(/^\/api\/profiles\/([^/]+)(?:\/(launch|focus|stop|rename|backup))?$/);
      if (profileMatch) {
        const id = decodeURIComponent(profileMatch[1]);
        if (!isSafeProfileIdParam(id)) {
          sendJson(res, 400, { ok: false, error_code: 'INVALID_ID', message: 'Invalid profile identifier' });
          return;
        }
        const action = profileMatch[2] || null;

        if (!action && method === 'DELETE') {
          const body = await readJsonBody(req);
          const flags = body.confirm === true ? ['--confirm'] : [];
          const out = await runHostCli(['delete', id], hostOptions, flags);
          sendJson(res, out.statusCode, out.payload);
          return;
        }

        if (action === 'rename' && method === 'POST') {
          const body = await readJsonBody(req);
          const out = await runHostCli(['rename', id, String(body.newName || '')], hostOptions);
          sendJson(res, out.statusCode, out.payload);
          return;
        }

        if (action === 'backup' && method === 'POST') {
          const body = await readJsonBody(req);
          const flags = body.dest ? ['--dest', String(body.dest)] : [];
          const out = await runHostCli(['backup', id], hostOptions, flags);
          sendJson(res, out.statusCode, out.payload);
          return;
        }

        if ((action === 'launch' || action === 'focus' || action === 'stop') && method === 'POST') {
          const out = await runHostCli([action, id], hostOptions);
          sendJson(res, out.statusCode, out.payload);
          return;
        }
      }

      sendJson(res, 404, { ok: false, error_code: 'ROUTE_NOT_FOUND', message: `Unknown route ${method} ${pathname}` });
    } catch (err) {
      sendJson(res, 400, { ok: false, error_code: 'BAD_REQUEST', message: err.message });
    }
  });

  return server;
}

export function startUiServer(options = {}) {
  const server = createUiServer(options);
  const port = Number.isFinite(options.port) ? options.port : 0;
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => {
      const addr = server.address();
      resolve({
        server,
        port: addr.port,
        url: `http://127.0.0.1:${addr.port}/`
      });
    });
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const argv = process.argv.slice(2);
  const opts = { port: Number(process.env.DIM_PORT || 18420) };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--port' && argv[i + 1]) opts.port = Number(argv[++i]);
    else if (argv[i] === '--data-dir' && argv[i + 1]) opts.dataDir = argv[++i];
    else if (argv[i] === '--app-path' && argv[i + 1]) opts.appPath = argv[++i];
    else if (argv[i] === '--node-fixture') opts.nodeFixture = true;
  }

  startUiServer(opts).then(({ url }) => {
    console.log(JSON.stringify({ ok: true, url, bind: '127.0.0.1', data_dir: opts.dataDir || null }));
  }).catch((err) => {
    console.error(JSON.stringify({ ok: false, error: err.message }));
    process.exit(1);
  });
}
