import type { E2EConfig } from 'e2e';
import { web } from '@e2e-dev/web';
import { googleChrome } from './e2e-army/chrome-provider';

export default {
  tests: 'tests/**/*.e2e.ts',
  workers: 1,
  retries: 0,
  timeout: 120_000,
  assertionTimeout: 10_000,
  cache: 'off',
  reporters: ['list', 'junit', 'markdown'],
  trace: 'retain-on-failure',
  targets: [{
    name: 'google-chrome',
    engine: web({ browser: googleChrome(), viewport: { width: 1600, height: 1000 } }),
    app: {
      url: `http://localhost:${process.env.SCREENSHOTS_E2E_PORT ?? '4312'}`,
      command: { executable: process.execPath, args: ['e2e-army/server.cjs', '{port}'], startupTimeout: 120_000, log: '.e2e/logs/app.log', env: { SCREENSHOTS_E2E_PRODUCTION: process.env.SCREENSHOTS_E2E_PRODUCTION ?? '0' } },
    },
  }],
} satisfies E2EConfig;
