// scripts/build-obsidian-release.js
//
// Bump the Obsidian plugin version, commit, tag (obsidian-vX.Y.Z), and push.
// The `obsidian-v*` tag triggers .github/workflows/obsidian-plugin-release.yml,
// which builds, mirrors the plugin into the standalone kenforthewin/obsidian-atomic
// repo, and creates a release there with manifest.json/main.js/styles.css.

import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import readline from 'readline';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = path.resolve(__dirname, '..');
const PLUGIN_DIR = path.join(PROJECT_ROOT, 'plugins', 'obsidian-plugin');

const MANIFEST_PATH = path.join(PLUGIN_DIR, 'manifest.json');
const PACKAGE_JSON_PATH = path.join(PLUGIN_DIR, 'package.json');
const VERSIONS_JSON_PATH = path.join(PLUGIN_DIR, 'versions.json');
const PACKAGE_LOCK_PATH = path.join(PLUGIN_DIR, 'package-lock.json');

const TOUCHED_FILES = [
  path.relative(PROJECT_ROOT, MANIFEST_PATH),
  path.relative(PROJECT_ROOT, PACKAGE_JSON_PATH),
  path.relative(PROJECT_ROOT, VERSIONS_JSON_PATH),
  path.relative(PROJECT_ROOT, PACKAGE_LOCK_PATH),
];

function log(msg) { console.log(msg); }
function error(msg) { console.error(`ERROR: ${msg}`); process.exit(1); }

function parseVersion(v) {
  const m = v.match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!m) return null;
  return { major: +m[1], minor: +m[2], patch: +m[3] };
}

function formatVersion(v) { return `${v.major}.${v.minor}.${v.patch}`; }

function bumpVersion(versionStr, type) {
  const v = parseVersion(versionStr);
  if (!v) error(`Invalid version: ${versionStr}`);
  if (type === 'major') { v.major++; v.minor = 0; v.patch = 0; }
  else if (type === 'minor') { v.minor++; v.patch = 0; }
  else if (type === 'patch') { v.patch++; }
  return formatVersion(v);
}

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, 'utf8'));
}

function writeJson(p, obj) {
  fs.writeFileSync(p, JSON.stringify(obj, null, 2) + '\n');
}

function exec(cmd, opts = {}) {
  execSync(cmd, { cwd: PROJECT_ROOT, stdio: 'inherit', ...opts });
}

function execIn(cwd, cmd) {
  execSync(cmd, { cwd, stdio: 'inherit' });
}

function execCapture(cmd) {
  return execSync(cmd, { cwd: PROJECT_ROOT, encoding: 'utf8' }).trim();
}

function askConfirmation(question) {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((resolve) => {
    rl.question(question, (answer) => { rl.close(); resolve(answer.trim().toLowerCase()); });
  });
}

function revertTouched() {
  log('\nReverting uncommitted release changes...');
  try {
    exec(`git checkout -- ${TOUCHED_FILES.join(' ')}`);
  } catch (err) {
    console.error(`  (failed to restore: ${err?.message || err})`);
  }
}

function preflight() {
  log('Running preflight checks...');

  const branch = execCapture('git rev-parse --abbrev-ref HEAD');
  if (branch !== 'main') {
    error(`Must be on 'main' branch to release (currently on '${branch}')`);
  }

  const status = execCapture('git status --porcelain');
  if (status) {
    error(`Working tree is not clean. Commit or stash changes first:\n${status}`);
  }

  log('Pulling latest from origin/main...');
  exec('git pull --ff-only origin main');
}

function updateVersion(newVersion) {
  log(`Updating Obsidian plugin version to ${newVersion}...`);

  const manifest = readJson(MANIFEST_PATH);
  manifest.version = newVersion;
  writeJson(MANIFEST_PATH, manifest);

  const pkg = readJson(PACKAGE_JSON_PATH);
  pkg.version = newVersion;
  writeJson(PACKAGE_JSON_PATH, pkg);

  const versions = readJson(VERSIONS_JSON_PATH);
  versions[newVersion] = manifest.minAppVersion;
  writeJson(VERSIONS_JSON_PATH, versions);

  log('Syncing plugin package-lock.json...');
  execIn(PLUGIN_DIR, 'npm install --package-lock-only --silent');

  log('Updated manifest.json, package.json, versions.json, package-lock.json');
}

function buildAndTest() {
  log('\nBuilding plugin...');
  execIn(PLUGIN_DIR, 'npm run build');
  log('\nRunning plugin tests...');
  execIn(PLUGIN_DIR, 'npm test');
}

function showHelp() {
  console.log(`
Usage: node scripts/build-obsidian-release.js <bump-type>

Bump the Obsidian plugin version, commit, tag (obsidian-vX.Y.Z), and push to
trigger the release workflow.

Arguments:
  patch    Bump patch version (0.1.0 -> 0.1.1)
  minor    Bump minor version (0.1.0 -> 0.2.0)
  major    Bump major version (0.1.0 -> 1.0.0)

Examples:
  npm run release:obsidian:patch
  npm run release:obsidian:minor
  npm run release:obsidian:major
  `);
}

async function main() {
  const bumpType = process.argv[2];

  if (!bumpType || bumpType === '--help' || bumpType === '-h') {
    showHelp();
    process.exit(bumpType ? 0 : 1);
  }

  if (!['patch', 'minor', 'major'].includes(bumpType)) {
    error(`Invalid bump type: ${bumpType}. Use patch, minor, or major.`);
  }

  preflight();

  const currentVersion = readJson(MANIFEST_PATH).version;
  const newVersion = bumpVersion(currentVersion, bumpType);
  const tagName = `obsidian-v${newVersion}`;

  log(`\nReleasing Obsidian plugin ${tagName} (${currentVersion} -> ${newVersion})\n`);

  updateVersion(newVersion);

  const sigintHandler = () => { revertTouched(); process.exit(130); };
  process.on('SIGINT', sigintHandler);

  try {
    buildAndTest();
  } catch (err) {
    revertTouched();
    error(`Build or tests failed: ${err?.message || err}`);
  }

  const answer = await askConfirmation(`\nProceed with release ${tagName}? [Y/n] `);
  process.off('SIGINT', sigintHandler);

  if (answer === 'n' || answer === 'no') {
    revertTouched();
    log('\nAborted by user. No commit, tag, or push.');
    process.exit(0);
  }

  log('\nCommitting version bump...');
  exec(`git add ${TOUCHED_FILES.join(' ')}`);
  exec(`git commit -m "Obsidian plugin ${newVersion}"`);

  log(`\nCreating tag ${tagName}...`);
  exec(`git tag -a ${tagName} -m "Obsidian plugin ${newVersion}"`);

  log('\nPushing to origin...');
  exec('git push && git push --tags');

  log(`\nDone! GitHub Actions will sync to kenforthewin/obsidian-atomic and release ${newVersion}.`);
  log('Watch progress at: https://github.com/kenforthewin/atomic/actions');
}

main().catch((err) => {
  error(err?.stack || err?.message || String(err));
});
