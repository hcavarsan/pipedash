import { test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';
import { writeFile, mkdir } from 'node:fs/promises';
const run = promisify(execFile);
for (const script of ['api-bug-bash', 'ui-bug-bash', 'bug-bash']) {
  test(`shipped ${script} regression groups`, { timeout: 900_000 }, async ({ app }) => {
    const log = path.resolve('.e2e/logs', `${script}.log`);
    await mkdir(path.dirname(log), { recursive: true });
    const artifacts = path.resolve('.e2e/artifacts', script);
    await mkdir(artifacts, { recursive: true });
    try {
      const { stdout, stderr } = await run(process.execPath, [path.resolve(`tests/harness/${script}.cjs`), app.baseUrl!], { timeout: 850_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, PLAYWRIGHT_MODULE: path.resolve('node_modules/playwright'), BUG_BASH_FILTER: '', BUG_BASH_ARTIFACTS: artifacts } });
      await writeFile(log, stdout + stderr);
      const passed = stdout.match(/^PASS /gm) ?? [];
      await expect(passed.length).toBeGreaterThan(0);
      await expect(stdout + stderr).not.toContain('FAIL ');
      console.log(`${script}: ${passed.length} regression groups passed; evidence ${log}`);
    } catch (error) {
      const output = error as Error & { stdout?: string; stderr?: string };
      await writeFile(log, `${output.stdout ?? ''}${output.stderr ?? ''}\n${output.message}`);
      throw error;
    }
  });
}
