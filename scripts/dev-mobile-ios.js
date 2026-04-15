#!/usr/bin/env node

/**
 * Development helper for the Capacitor iOS app.
 *
 * Spawns the backend + web dev server (via dev-server.js), points Capacitor
 * at the running Vite instance via CAP_DEV_URL, builds the iOS app, installs
 * it in a booted simulator, and launches it. Edits to src/ hot-reload
 * through Vite HMR — no rebuild or re-sync needed for JS/TS/CSS changes.
 *
 * Usage:
 *   npm run dev:mobile:ios
 *   npm run dev:mobile:ios -- --device "iPhone 17 Pro"
 *   npm run dev:mobile:ios -- --no-build           # skip xcodebuild (reuse prior .app)
 *   npm run dev:mobile:ios -- --postgres           # forwarded to dev-server.js
 *
 * Options:
 *   --device NAME   Simulator device name (default: first booted, else "iPhone 16e")
 *   --no-build      Skip xcodebuild; reuse the previously built .app
 *
 * All unrecognised flags are forwarded to scripts/dev-server.js.
 * Ctrl+C stops the backend, Vite, and terminates the simulator app.
 */

import { spawn, execFileSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import http from 'node:http';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');

const DEV_URL = 'http://localhost:1420';
const BUNDLE_ID = 'com.atomic.mobile';
const SCHEME = 'App';
const PROJECT = resolve(root, 'mobile/ios/App/App.xcodeproj');
const DERIVED = resolve(root, 'mobile/ios/App/build');
const APP_PATH = resolve(DERIVED, 'Build/Products/Debug-iphonesimulator/App.app');
const DEFAULT_DEVICE = 'iPhone 16e';

let deviceName = null;
let skipBuild = false;
const forwardArgs = [];
const rawArgs = process.argv.slice(2);
for (let i = 0; i < rawArgs.length; i++) {
  const a = rawArgs[i];
  if (a === '--device') deviceName = rawArgs[++i];
  else if (a === '--no-build') skipBuild = true;
  else forwardArgs.push(a);
}

const children = [];
let shuttingDown = false;

function log(prefix, line, color = '\x1b[36m') {
  process.stdout.write(`${color}[${prefix.padEnd(6)}]\x1b[0m ${line}\n`);
}

function spawnChild(name, command, args, opts = {}) {
  const proc = spawn(command, args, {
    cwd: root,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, ...opts.env },
  });
  const color = opts.color || '\x1b[36m';
  const emit = (warn) => (data) => {
    for (const line of data.toString().split('\n').filter(Boolean)) {
      log(name, line, warn ? '\x1b[33m' : color);
    }
  };
  proc.stdout.on('data', emit(false));
  proc.stderr.on('data', emit(true));
  proc.on('exit', (code) => {
    log(name, `exited (${code})`, '\x1b[90m');
    if (!shuttingDown) cleanup(1);
  });
  children.push(proc);
  return proc;
}

function waitForVite(timeoutMs = 60_000) {
  const start = Date.now();
  return new Promise((resolvePromise, rejectPromise) => {
    const tick = () => {
      if (Date.now() - start > timeoutMs) {
        rejectPromise(new Error(`Vite did not respond on ${DEV_URL} within ${timeoutMs}ms`));
        return;
      }
      const req = http.get(DEV_URL, (res) => {
        res.resume();
        resolvePromise();
      });
      req.on('error', () => setTimeout(tick, 500));
      req.setTimeout(1000, () => {
        req.destroy();
        setTimeout(tick, 500);
      });
    };
    tick();
  });
}

function ensureSimulator() {
  const json = execFileSync('xcrun', ['simctl', 'list', 'devices', '--json'], { encoding: 'utf8' });
  const runtimes = JSON.parse(json).devices;

  // Prefer an already-booted device; if --device was given, require that specific one.
  for (const devices of Object.values(runtimes)) {
    for (const d of devices) {
      if (d.state === 'Booted' && (!deviceName || d.name === deviceName)) {
        return d.name;
      }
    }
  }

  const target = deviceName || DEFAULT_DEVICE;
  log('sim', `booting ${target}...`);
  execFileSync('xcrun', ['simctl', 'boot', target], { stdio: 'inherit' });
  try {
    execFileSync('open', ['-a', 'Simulator'], { stdio: 'ignore' });
  } catch {}
  return target;
}

function capSync() {
  log('cap', `cap sync ios (CAP_DEV_URL=${DEV_URL})`);
  execFileSync('npx', ['cap', 'sync', 'ios'], {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, CAP_DEV_URL: DEV_URL },
  });
}

function xcodeBuild(device) {
  log('cap', `xcodebuild for ${device}...`);
  execFileSync(
    'xcodebuild',
    [
      '-project', PROJECT,
      '-scheme', SCHEME,
      '-destination', `platform=iOS Simulator,name=${device}`,
      '-derivedDataPath', DERIVED,
      '-quiet',
      'build',
    ],
    { stdio: 'inherit' },
  );
}

function installAndLaunch() {
  log('sim', 'installing app...');
  execFileSync('xcrun', ['simctl', 'install', 'booted', APP_PATH], { stdio: 'inherit' });
  try { execFileSync('xcrun', ['simctl', 'terminate', 'booted', BUNDLE_ID]); } catch {}
  log('sim', 'launching app');
  execFileSync('xcrun', ['simctl', 'launch', 'booted', BUNDLE_ID], { stdio: 'inherit' });
}

function cleanup(code = 0) {
  if (shuttingDown) return;
  shuttingDown = true;
  try { execFileSync('xcrun', ['simctl', 'terminate', 'booted', BUNDLE_ID]); } catch {}
  for (const c of children) {
    if (!c.killed) c.kill('SIGTERM');
  }
  // Give children a moment to flush, then exit.
  setTimeout(() => process.exit(code), 500);
}

process.on('SIGINT', () => cleanup(0));
process.on('SIGTERM', () => cleanup(0));

(async () => {
  // 1. Backend + Vite via the existing dev-server.js (forwarding unknown flags).
  spawnChild('dev', 'node', [resolve(root, 'scripts/dev-server.js'), ...forwardArgs], {
    color: '\x1b[36m',
  });

  // 2. Wait for Vite to answer before syncing — cap sync doesn't care, but we
  //    don't want to launch the app before HMR is ready to serve.
  log('cap', `waiting for Vite on ${DEV_URL}...`);
  await waitForVite();
  log('cap', 'Vite is up');

  // 3. Simulator, sync, build, install, launch.
  const device = ensureSimulator();
  log('sim', `target: ${device}`);
  capSync();
  if (!skipBuild) xcodeBuild(device);
  installAndLaunch();

  log('cap', 'ready — edit src/ to hot-reload in the simulator (Ctrl+C to stop)');
})().catch((err) => {
  console.error(`\x1b[31m[error]\x1b[0m ${err.stack || err.message || err}`);
  cleanup(1);
});
