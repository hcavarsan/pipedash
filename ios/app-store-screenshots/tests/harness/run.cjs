const { spawnSync } = require('node:child_process');
const path = require('node:path');
const base = process.argv[2];
if (!base) throw new Error('Provide the URL of a disposable server (never your working editor).');
for (const script of ['api-bug-bash.cjs', 'ui-bug-bash.cjs', 'bug-bash.cjs']) {
  const result = spawnSync(process.execPath, [path.join(__dirname, script), base], { stdio: 'inherit' });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
