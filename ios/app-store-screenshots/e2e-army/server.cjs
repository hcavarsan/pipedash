// Copy the complete product into a disposable directory: API writes/uploads
// resolve from cwd, and must never touch a user's project or generated assets.
const { cpSync, mkdtempSync, symlinkSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const source = path.resolve(__dirname, '..');
const workspace = mkdtempSync(path.join(tmpdir(), 'screenshots-e2e-'));
cpSync(source, workspace, { recursive: true, filter: p => {
  const first = path.relative(source, p).split(path.sep)[0];
  return !['node_modules', '.next', '.e2e', '.git', 'tests', 'e2e-army'].includes(first);
}});
symlinkSync(path.join(source, 'node_modules'), path.join(workspace, 'node_modules'), 'dir');
const production = process.env.SCREENSHOTS_E2E_PRODUCTION === '1';
if (production) cpSync(path.join(source, '.next'), path.join(workspace, '.next'), { recursive: true });
console.log(`Disposable editor workspace: ${workspace}`);
const child = spawn(process.execPath, [path.join(source, 'node_modules/next/dist/bin/next'), production ? 'start' : 'dev', ...(production ? [] : ['--webpack']), '--hostname', '0.0.0.0', '--port', process.argv[2] || '4312'], { cwd: workspace, stdio: 'inherit', env: { ...process.env, NEXT_TELEMETRY_DISABLED: '1' } });
let stopping = false;
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { stopping = true; child.kill(signal); });
child.on('exit', (code, signal) => { rmSync(workspace, { recursive: true, force: true }); process.exit(stopping ? 0 : code ?? (signal ? 1 : 0)); });
